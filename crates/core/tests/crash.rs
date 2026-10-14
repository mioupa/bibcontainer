//! AT-15: 保存中にプロセスを強制終了しても、再起動後に DB が壊れていない。
//!
//! テストバイナリ自身を子プロセスとして起動する（環境変数 `BIBCONTAINER_CRASH_TEST_DB` を付け、
//! `crash_child_writer` だけを実行させる）。子は DB に書き込み続け、親は途中で SIGKILL 相当で止めて、
//! 開き直して検べる。これを何度か繰り返す。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod lib_common;

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use bibcontainer_core::library::{self, Env, FieldChange, UpdateOptions};
use bibcontainer_core::{Op, SystemClock, meta, open};
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;

const ENV_DB: &str = "BIBCONTAINER_CRASH_TEST_DB";
const BATCH: usize = 5;

/// 子プロセスの本体。環境変数がなければ何もしない（通常のテスト実行ではすぐ終わる）。
#[test]
fn crash_child_writer() {
    let Some(db) = std::env::var_os(ENV_DB) else {
        return;
    };
    let db = Path::new(&db);
    let (mut conn, _) = open(db, &db.with_file_name("backups")).unwrap();
    let clock = SystemClock;
    let env = Env::new(&clock);
    let stdout = std::io::stdout();
    let tell = |conn: &Connection| {
        let mut out = stdout.lock();
        writeln!(out, "committed {}", meta::load_op_seq(conn).unwrap()).unwrap();
        out.flush().unwrap();
    };

    let start = u64::try_from(
        conn.query_row("SELECT count(*) FROM books", [], |r| r.get::<_, i64>(0))
            .unwrap(),
    )
    .unwrap();
    for n in start.. {
        // 5 冊を 1 つのトランザクションで作る（途中で止まれば、全部あるか全部ない）
        let batch: Vec<Vec<FieldChange>> = (0..BATCH)
            .map(|i| {
                vec![
                    FieldChange::provider("title", json!(format!("本 {n}-{i}")), "openbd"),
                    FieldChange::manual("memo", json!(format!("batch-{n}"))),
                    FieldChange::manual("tags", json!([format!("タグ{}", n % 7), "共通"])),
                    FieldChange::manual(
                        "authors",
                        json!([{"name": format!("著者{n}"), "kana": "ちょしゃ", "role": "著"}]),
                    ),
                ]
            })
            .collect();
        let ids = library::create_books(&mut conn, &env, &batch).unwrap();
        tell(&conn);
        // 同じ変更を 5 冊まとめて（これも 1 つのトランザクション）
        library::update_books(
            &mut conn,
            &env,
            &ids,
            &[FieldChange::manual("rating", json!(3))],
            UpdateOptions::default(),
        )
        .unwrap();
        tell(&conn);
        if n % 3 == 0 {
            library::trash_books(&mut conn, &env, &ids[..2]).unwrap();
            tell(&conn);
        }
        if n % 5 == 4 {
            library::purge_books(&mut conn, &env, &ids[..1]).unwrap();
            tell(&conn);
        }
    }
}

/// 子を起動して、`min_commits` 回のコミットを確認してから `extra` だけ待って強制終了する。
/// 子が最後に知らせたコミットの seq を返す。
fn run_and_kill(db: &Path, min_commits: usize, extra: Duration) -> i64 {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "crash_child_writer",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ENV_DB, db)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("子プロセスを起動できる");
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<i64>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let committed = line
                .strip_prefix("committed ")
                .and_then(|s| s.trim().parse().ok());
            if let Some(n) = committed
                && tx.send(n).is_err()
            {
                break;
            }
        }
    });

    let mut last = 0;
    let mut seen = 0;
    while seen < min_commits {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(n) => {
                last = n;
                seen += 1;
            }
            Err(e) => {
                let _ = child.kill();
                panic!("子プロセスが書き込みを始めない: {e}");
            }
        }
    }
    std::thread::sleep(extra);
    child.kill().expect("強制終了する");
    child.wait().unwrap();
    // 強制終了までに届いた分も数える
    reader.join().unwrap();
    while let Ok(n) = rx.try_recv() {
        last = last.max(n);
    }
    last
}

fn check(db: &Path, acknowledged: i64, round: usize) {
    let (conn, _) = open(db, &db.with_file_name("backups")).unwrap();
    let ctx = |what: &str| format!("{round} 回目: {what}");

    // 1. SQLite の整合性
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(result, "ok", "{}", ctx("integrity_check"));

    // 2. 書かれた本には、必ず対応する outbox の行がある
    let outbox: Vec<(i64, String)> = conn
        .prepare("SELECT seq, op_json FROM outbox ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let ops: Vec<Op> = outbox
        .iter()
        .map(|(_, j)| Op::from_json_line(j).unwrap())
        .collect();
    let ops_by_entity: HashSet<&str> = ops.iter().map(|o| o.id.as_str()).collect();
    let books: Vec<String> = conn
        .prepare("SELECT id FROM books")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for id in &books {
        assert!(
            ops_by_entity.contains(id.as_str()),
            "{}: {id}",
            ctx("outbox に行がない本がある")
        );
    }
    // 逆に、記録された Op の本は DB にある（Op とその適用が同じトランザクション）
    let book_set: HashSet<&str> = books.iter().map(String::as_str).collect();
    for op in &ops {
        assert!(
            book_set.contains(op.id.as_str()),
            "{}",
            ctx("Op だけあって本がない")
        );
    }

    // 3. seq は欠番なしの 1..=N で、カウンタと一致し、知らせを受けたコミットは失われていない
    let seqs: Vec<i64> = outbox.iter().map(|(s, _)| *s).collect();
    let expected: Vec<i64> = (1..=i64::try_from(seqs.len()).unwrap()).collect();
    assert_eq!(seqs, expected, "{}", ctx("seq に欠番がある"));
    assert_eq!(
        meta::load_op_seq(&conn).unwrap(),
        i64::try_from(seqs.len()).unwrap(),
        "{}",
        ctx("seq のカウンタ")
    );
    assert!(
        i64::try_from(seqs.len()).unwrap() >= acknowledged,
        "{}: 知らせを受けた {acknowledged} に対して {}",
        ctx("コミット済みの変更が失われた"),
        seqs.len()
    );
    // HLC のカウンタは、記録した Op のどれよりも進んでいる
    let hlc_last = meta::load_hlc_state(&conn).unwrap();
    let max_op = ops
        .iter()
        .map(|o| bibcontainer_core::Hlc::parse(&o.hlc).unwrap().state())
        .max()
        .unwrap();
    assert!(hlc_last >= max_op, "{}", ctx("HLC が逆行している"));

    eprintln!(
        "{round} 回目: 知らせを受けたコミット {acknowledged}、outbox {}、本 {}",
        seqs.len(),
        books.len()
    );

    // 4. バッチは 1 つのトランザクション: 全部あるか、全部ない
    let mut per_batch: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut stmt = conn
        .prepare("SELECT memo, rating FROM books WHERE memo LIKE 'batch-%'")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
        })
        .unwrap();
    for row in rows {
        let (memo, rating) = row.unwrap();
        let e = per_batch.entry(memo).or_default();
        e.0 += 1;
        e.1 += usize::from(rating == Some(3));
    }
    for (memo, (created, rated)) in &per_batch {
        // 完全削除した本は memo が消える（purge は 1 冊）ので、作成数は BATCH か BATCH - 1
        assert!(
            *created == BATCH || *created == BATCH - 1,
            "{}: {memo} は {created} 冊",
            ctx("作成のバッチが途中まで")
        );
        assert!(
            *rated == 0 || *rated == *created,
            "{}: {memo} の評価は {rated}/{created}",
            ctx("一括変更が途中まで")
        );
    }

    // 5. 検索用の派生データも、本と食い違っていない
    let idx: i64 = conn
        .query_row("SELECT count(*) FROM book_idx", [], |r| r.get(0))
        .unwrap();
    let fts: i64 = conn
        .query_row("SELECT count(*) FROM books_fts", [], |r| r.get(0))
        .unwrap();
    let live: i64 = conn
        .query_row("SELECT count(*) FROM books WHERE purged = 0", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        (idx, fts),
        (live, live),
        "{}",
        ctx("派生データが本と合わない")
    );
    conn.execute(
        "INSERT INTO books_fts(books_fts) VALUES('integrity-check')",
        [],
    )
    .unwrap_or_else(|e| panic!("{}: {e}", ctx("FTS の整合性")));
    let tags: i64 = conn
        .query_row("SELECT count(*) FROM book_tags", [], |r| r.get(0))
        .unwrap();
    assert!(tags >= 0);
}

#[test]
fn killing_the_writer_mid_transaction_never_corrupts_the_db() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("crash.sqlite3");
    // コミット数と、確認してから強制終了までの待ち時間（マイクロ秒）を変えて、止まる位置をばらす
    let plan: [(usize, u64); 14] = [
        (3, 0),
        (10, 1_000),
        (25, 3_000),
        (6, 0),
        (40, 7_000),
        (15, 250),
        (30, 0),
        (50, 5_000),
        (8, 130),
        (20, 470),
        (12, 90),
        (33, 900),
        (5, 40),
        (60, 1_700),
    ];
    let mut acknowledged = 0;
    for (round, (commits, extra_us)) in plan.into_iter().enumerate() {
        let last = run_and_kill(&db, commits, Duration::from_micros(extra_us));
        acknowledged = acknowledged.max(last);
        check(&db, acknowledged, round + 1);
    }
    // 何回も止めたあとも、普通に使える
    let (mut conn, _) = open(&db, &dir.path().join("backups")).unwrap();
    let clock = bibcontainer_core::ManualClock::new(lib_common::T0);
    let before = bibcontainer_core::meta::load_op_seq(&conn).unwrap();
    let id = library::create_book(
        &mut conn,
        &Env::new(&clock),
        &[FieldChange::manual("title", json!("復旧後の本"))],
    )
    .unwrap();
    assert!(!id.is_empty());
    assert_eq!(
        bibcontainer_core::meta::load_op_seq(&conn).unwrap(),
        before + 1
    );
}
