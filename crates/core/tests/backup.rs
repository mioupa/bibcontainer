//! バックアップ（作成・ローテーション・検証・復元）のテスト。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod common;
mod lib_common;

use std::fs;
use std::path::{Path, PathBuf};

use bibcontainer_core::backup::{
    self, BackupKind, DAILY_KEEP, Keep, WEEKLY_KEEP, create_backup, daily_backup_due, list_backups,
    manual_backup, restore_backup, rotate_backups, run_scheduled_backup,
    run_scheduled_backup_keeping, verify_backup,
};
use bibcontainer_core::library::{self, Env, FieldChange};
use bibcontainer_core::{
    Error, LATEST_VERSION, MIGRATIONS, ManualClock, meta, open, open_with_migrations, pending_ops,
    search_books,
};
use rusqlite::Connection;
use serde_json::json;
use tempfile::{TempDir, tempdir};

use lib_common::{T0, TODAY};

const DAY: i64 = 24 * 60 * 60 * 1000;

struct Dirs {
    root: TempDir,
}

impl Dirs {
    fn new() -> Self {
        Dirs {
            root: tempdir().unwrap(),
        }
    }
    fn db(&self) -> PathBuf {
        self.root.path().join("data").join("library.sqlite3")
    }
    fn backups(&self) -> PathBuf {
        self.root.path().join("backups")
    }
    fn open(&self) -> Connection {
        open(&self.db(), &self.backups()).unwrap().0
    }
}

fn add_book(conn: &mut Connection, clock: &ManualClock, title: &str) -> String {
    library::create_book(
        conn,
        &Env::new(clock).with_today(TODAY),
        &[FieldChange::manual("title", json!(title))],
    )
    .unwrap()
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn count_books(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM books", [], |r| r.get(0))
        .unwrap()
}

fn integrity(conn: &Connection) -> String {
    conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap()
}

// ---------- 作成・名前・ローテーション ----------

#[test]
fn backups_are_made_with_vacuum_into_and_named_by_kind_and_utc_time() {
    let d = Dirs::new();
    let mut conn = d.open();
    let clock = ManualClock::new(T0);
    add_book(&mut conn, &clock, "A");

    let daily = create_backup(&conn, &d.backups(), BackupKind::Daily, &clock).unwrap();
    assert_eq!(daily.file_name, "daily-20261014T083000Z.sqlite3");
    assert_eq!(daily.kind, BackupKind::Daily);
    assert_eq!(daily.created_at.as_deref(), Some("2026-10-14T08:30:00Z"));
    assert!(daily.size_bytes > 0);
    let manual = manual_backup(&conn, &d.backups(), &clock).unwrap();
    assert_eq!(manual.file_name, "manual-20261014T083000Z.sqlite3");
    // 同じ秒に重なったときは連番
    let again = manual_backup(&conn, &d.backups(), &clock).unwrap();
    assert_eq!(again.file_name, "manual-20261014T083000Z-1.sqlite3");
    let third = manual_backup(&conn, &d.backups(), &clock).unwrap();
    assert_eq!(third.file_name, "manual-20261014T083000Z-2.sqlite3");

    // 作成途中の一時ファイルは残らない
    assert_eq!(file_names(&d.backups()).len(), 4);
    assert!(file_names(&d.backups()).iter().all(|n| !n.starts_with('.')));

    // バックアップは単体で開ける完全な DB
    let copy = Connection::open(&daily.path).unwrap();
    assert_eq!(integrity(&copy), "ok");
    assert_eq!(count_books(&copy), 1);
    let v: u32 = copy
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(v, LATEST_VERSION);
}

#[test]
fn daily_backups_run_when_24_hours_have_passed_and_weekly_every_7_days() {
    let d = Dirs::new();
    let mut conn = d.open();
    let clock = ManualClock::new(T0);
    add_book(&mut conn, &clock, "A");

    assert!(
        daily_backup_due(&d.backups(), &clock).unwrap(),
        "最初は取る"
    );
    let first = run_scheduled_backup(&conn, &d.backups(), &clock)
        .unwrap()
        .unwrap();
    assert_eq!(first.kind, BackupKind::Daily);
    let names = file_names(&d.backups());
    assert_eq!(
        names,
        [
            "daily-20261014T083000Z.sqlite3",
            "weekly-20261014T083000Z.sqlite3"
        ]
    );

    // 24 時間たつまでは取らない
    assert!(!daily_backup_due(&d.backups(), &clock).unwrap());
    assert!(
        run_scheduled_backup(&conn, &d.backups(), &clock)
            .unwrap()
            .is_none()
    );
    clock.advance(DAY - 1);
    assert!(
        run_scheduled_backup(&conn, &d.backups(), &clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(file_names(&d.backups()).len(), 2);

    // 24 時間たったら日次だけ（週次はまだ）
    clock.advance(1);
    let second = run_scheduled_backup(&conn, &d.backups(), &clock)
        .unwrap()
        .unwrap();
    assert_eq!(second.file_name, "daily-20261015T083000Z.sqlite3");
    assert_eq!(file_names(&d.backups()).len(), 3);

    // 週次は 7 日後
    for _ in 0..5 {
        clock.advance(DAY);
        run_scheduled_backup(&conn, &d.backups(), &clock)
            .unwrap()
            .unwrap();
    }
    assert_eq!(
        list_backups(&d.backups())
            .unwrap()
            .iter()
            .filter(|b| b.kind == BackupKind::Weekly)
            .count(),
        1
    );
    clock.advance(DAY);
    run_scheduled_backup(&conn, &d.backups(), &clock)
        .unwrap()
        .unwrap();
    let weeklies: Vec<String> = list_backups(&d.backups())
        .unwrap()
        .into_iter()
        .filter(|b| b.kind == BackupKind::Weekly)
        .map(|b| b.file_name)
        .collect();
    assert_eq!(
        weeklies,
        [
            "weekly-20261021T083000Z.sqlite3",
            "weekly-20261014T083000Z.sqlite3"
        ]
    );
}

#[test]
fn rotation_keeps_7_daily_and_4_weekly_generations_and_never_touches_other_backups() {
    let d = Dirs::new();
    let mut conn = d.open();
    let clock = ManualClock::new(T0);
    add_book(&mut conn, &clock, "A");
    let manual = manual_backup(&conn, &d.backups(), &clock).unwrap();
    // 移行前のバックアップ（migrate が作るもの）と作成途中のファイル
    fs::write(
        d.backups()
            .join("pre-migration-v1-20200101T000000Z.sqlite3"),
        b"x",
    )
    .unwrap();
    fs::write(d.backups().join(".daily-20200101T000000Z.tmp"), b"x").unwrap();
    fs::write(d.backups().join("memo.txt"), b"x").unwrap();

    // 40 日間、毎日起動する
    for _ in 0..40 {
        run_scheduled_backup(&conn, &d.backups(), &clock).unwrap();
        clock.advance(DAY);
    }
    let all = list_backups(&d.backups()).unwrap();
    let of = |k: BackupKind| -> Vec<String> {
        all.iter()
            .filter(|b| b.kind == k)
            .map(|b| b.file_name.clone())
            .collect()
    };
    let daily = of(BackupKind::Daily);
    let weekly = of(BackupKind::Weekly);
    assert_eq!(daily.len(), DAILY_KEEP);
    assert_eq!(weekly.len(), WEEKLY_KEEP);
    // 日次は直近の 7 日（11/22〜11/28）、週次は 7 日おきの新しい 4 つ（10/14 から 7 日おきの最後の 4 つ）
    assert_eq!(
        daily.first().map(String::as_str),
        Some("daily-20261122T083000Z.sqlite3")
    );
    assert_eq!(
        daily.last().map(String::as_str),
        Some("daily-20261116T083000Z.sqlite3")
    );
    assert_eq!(
        weekly,
        [
            "weekly-20261118T083000Z.sqlite3",
            "weekly-20261111T083000Z.sqlite3",
            "weekly-20261104T083000Z.sqlite3",
            "weekly-20261028T083000Z.sqlite3",
        ]
    );
    // 手動・移行前は消えない。無関係なファイルも触らない
    assert_eq!(of(BackupKind::Manual), [manual.file_name]);
    assert_eq!(
        of(BackupKind::PreMigration),
        ["pre-migration-v1-20200101T000000Z.sqlite3"]
    );
    let names = file_names(&d.backups());
    assert!(names.contains(&".daily-20200101T000000Z.tmp".to_owned()));
    assert!(names.contains(&"memo.txt".to_owned()));
    // もう一度ローテーションしても何も消えない
    assert!(rotate_backups(&d.backups()).unwrap().is_empty());
    // 一覧は新しい順
    let times: Vec<_> = all.iter().filter_map(|b| b.created_at.clone()).collect();
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(times, sorted);
}

#[test]
fn the_number_of_generations_kept_can_be_chosen() {
    let d = Dirs::new();
    let mut conn = d.open();
    let clock = ManualClock::new(T0);
    add_book(&mut conn, &clock, "A");
    let keep = Keep {
        daily: 2,
        weekly: 1,
    };
    for _ in 0..30 {
        run_scheduled_backup_keeping(&conn, &d.backups(), &clock, keep).unwrap();
        clock.advance(DAY);
    }
    let all = list_backups(&d.backups()).unwrap();
    let count = |k: BackupKind| all.iter().filter(|b| b.kind == k).count();
    assert_eq!(count(BackupKind::Daily), 2);
    assert_eq!(count(BackupKind::Weekly), 1);
    assert_eq!(Keep::default().daily, DAILY_KEEP);
    assert_eq!(Keep::default().weekly, WEEKLY_KEEP);
}

#[test]
fn list_backups_of_a_missing_folder_is_empty() {
    let d = Dirs::new();
    assert!(list_backups(&d.backups()).unwrap().is_empty());
    assert!(backup::daily_backup_due(&d.backups(), &ManualClock::new(T0)).unwrap());
}

// ---------- 検証 ----------

#[test]
fn verify_accepts_a_good_backup_and_rejects_bad_files() {
    let d = Dirs::new();
    let mut conn = d.open();
    let clock = ManualClock::new(T0);
    add_book(&mut conn, &clock, "A");
    add_book(&mut conn, &clock, "B");
    let good = create_backup(&conn, &d.backups(), BackupKind::Manual, &clock).unwrap();

    let check = verify_backup(Path::new(&good.path)).unwrap();
    assert_eq!(check.schema_version, LATEST_VERSION);
    assert_eq!(check.book_count, 2);
    assert_eq!(
        check.device_id.as_deref(),
        Some(meta::device_id(&conn).unwrap().as_str())
    );

    let bad_dir = d.root.path().join("bad");
    fs::create_dir_all(&bad_dir).unwrap();
    let invalid = |p: &Path| matches!(verify_backup(p), Err(Error::InvalidBackup(_)));

    // 存在しない
    assert!(invalid(&bad_dir.join("missing.sqlite3")));
    // SQLite でないファイル
    let garbage = bad_dir.join("garbage.sqlite3");
    fs::write(&garbage, b"this is not a database at all, just some text").unwrap();
    assert!(invalid(&garbage));
    // 空のファイル（開けるが版が 0）
    let empty = bad_dir.join("empty.sqlite3");
    fs::write(&empty, b"").unwrap();
    assert!(invalid(&empty));
    // 途中で切れたファイル
    let bytes = fs::read(&good.path).unwrap();
    let truncated = bad_dir.join("truncated.sqlite3");
    fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
    assert!(invalid(&truncated));
    // 壊れたファイル（ページの途中を壊す）
    let mut broken = bytes.clone();
    for b in broken.iter_mut().skip(8192).step_by(7).take(4000) {
        *b ^= 0xFF;
    }
    let broken_path = bad_dir.join("broken.sqlite3");
    fs::write(&broken_path, &broken).unwrap();
    assert!(invalid(&broken_path));
    // bibcontainer の DB でない SQLite（版はあるがテーブルがない）
    let other = bad_dir.join("other.sqlite3");
    {
        let c = Connection::open(&other).unwrap();
        c.execute_batch("CREATE TABLE x(a); PRAGMA user_version = 1;")
            .unwrap();
    }
    assert!(invalid(&other));
    // 版が新しすぎる
    let newer = bad_dir.join("newer.sqlite3");
    fs::copy(&good.path, &newer).unwrap();
    {
        let c = Connection::open(&newer).unwrap();
        c.pragma_update(None, "user_version", LATEST_VERSION + 1)
            .unwrap();
    }
    assert!(matches!(
        verify_backup(&newer),
        Err(Error::SchemaTooNew { found, supported }) if found == LATEST_VERSION + 1 && supported == LATEST_VERSION
    ));
    // 検証しても、元のファイルは変わらず、作業用のコピーも残らない
    assert_eq!(fs::read(&good.path).unwrap(), bytes);
}

// ---------- 復元 ----------

#[test]
fn restore_replaces_the_db_assigns_a_new_device_id_and_restarts_seq() {
    let d = Dirs::new();
    let clock = ManualClock::new(T0);
    let (old_dev, backup_path, pending_at_backup, extra_id);
    {
        let mut conn = d.open();
        add_book(&mut conn, &clock, "バックアップ前の本 1");
        add_book(&mut conn, &clock, "バックアップ前の本 2");
        // 1 つは書き出し済み、残りは未書き出し
        conn.execute("UPDATE outbox SET flushed = 1 WHERE seq = 1", [])
            .unwrap();
        old_dev = meta::device_id(&conn).unwrap();
        pending_at_backup = pending_ops(&conn).unwrap();
        backup_path = create_backup(&conn, &d.backups(), BackupKind::Manual, &clock)
            .unwrap()
            .path;
        // バックアップのあとに本を足す（復元で消える）
        extra_id = add_book(&mut conn, &clock, "バックアップ後の本");
        assert_eq!(count_books(&conn), 3);
    }
    assert_eq!(pending_at_backup.len(), 1);

    clock.advance(1000);
    let report = restore_backup(Path::new(&backup_path), &d.db(), &d.backups(), &clock).unwrap();
    assert_eq!(report.schema_version, LATEST_VERSION);
    assert_eq!(report.old_device_id.as_deref(), Some(old_dev.as_str()));
    assert_ne!(report.new_device_id, old_dev);
    assert_eq!(report.moved_unflushed_ops, 1);
    let pre = report.pre_restore_path.clone().expect("元の DB を残す");
    assert!(pre.contains("pre-restore-20261014T083001Z"));

    // 復元前の DB は、別の場所にそのまま残っている
    {
        let kept = Connection::open(&pre).unwrap();
        assert_eq!(integrity(&kept), "ok");
        assert_eq!(count_books(&kept), 3, "バックアップ後の本も入っている");
    }

    // 復元後の DB
    let mut conn = d.open();
    assert_eq!(integrity(&conn), "ok");
    assert_eq!(count_books(&conn), 2);
    let ids: Vec<String> = bibcontainer_core::list_rows(&conn)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert!(!ids.contains(&extra_id));
    assert_eq!(search_books(&conn, "バックアップ前").unwrap().len(), 2);
    assert!(search_books(&conn, "バックアップ後").unwrap().is_empty());
    // 新しい端末 ID と、0 に戻した seq のカウンタ
    assert_eq!(meta::device_id(&conn).unwrap(), report.new_device_id);
    assert_eq!(meta::load_op_seq(&conn).unwrap(), 0);
    // outbox は空で、古い端末 ID の未書き出しの Op は別の場所に移っている
    assert!(pending_ops(&conn).unwrap().is_empty());
    let moved: Vec<(String, i64, String)> = conn
        .prepare("SELECT dev, seq, op_json FROM outbox_restored ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].0, old_dev);
    assert_eq!(moved[0].1, pending_at_backup[0].seq);
    assert_eq!(moved[0].2, pending_at_backup[0].to_json_line().unwrap());

    // 次の変更は、新しい端末 ID で seq 1 から始まる（outbox の主キーと重ならない）
    let id = add_book(&mut conn, &clock, "復元後の本");
    let ops = pending_ops(&conn).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].dev, report.new_device_id);
    assert_eq!(ops[0].seq, 1);
    assert!(ops[0].hlc.ends_with(&report.new_device_id));
    assert_eq!(ops[0].id, id);
    // HLC は復元前に使った値より進む（古い端末が書いた Op より新しい）
    let last_old_hlc = &pending_at_backup[0].hlc;
    assert!(&ops[0].hlc > last_old_hlc);
}

#[test]
fn restore_discards_a_stale_wal_and_leaves_no_working_files() {
    let d = Dirs::new();
    let clock = ManualClock::new(T0);
    let backup_path;
    {
        let mut conn = d.open();
        add_book(&mut conn, &clock, "バックアップの本");
        backup_path = create_backup(&conn, &d.backups(), BackupKind::Manual, &clock)
            .unwrap()
            .path;
    }
    // 今の DB の WAL に、チェックポイント前の書き込みが残っている状態を作る
    let stale_wal = d.db().with_file_name("library.sqlite3-wal");
    {
        let mut conn = d.open();
        conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        for i in 0..30 {
            add_book(&mut conn, &clock, &format!("WAL にだけある本 {i}"));
        }
        fs::copy(&stale_wal, d.root.path().join("saved-wal")).unwrap();
        // 接続を閉じると WAL は DB に取り込まれて消える
    }
    assert!(!stale_wal.exists());
    fs::copy(d.root.path().join("saved-wal"), &stale_wal).unwrap();
    fs::write(
        d.db().with_file_name("library.sqlite3-shm"),
        vec![0u8; 32768],
    )
    .unwrap();

    restore_backup(Path::new(&backup_path), &d.db(), &d.backups(), &clock).unwrap();
    assert!(!stale_wal.exists(), "古い WAL は残さない");
    assert!(!d.db().with_file_name("library.sqlite3-shm").exists());

    let conn = d.open();
    assert_eq!(integrity(&conn), "ok");
    assert_eq!(count_books(&conn), 1, "古い WAL の内容は重ならない");
    // 作業用のファイルは残らない
    let names = file_names(d.db().parent().unwrap());
    assert!(
        names.iter().all(|n| !n.contains("restore-tmp")),
        "{names:?}"
    );
    // 復元前の DB（WAL つき）は、そのまま開ける
    let pre = list_backups(&d.backups())
        .unwrap()
        .into_iter()
        .find(|b| b.kind == BackupKind::PreRestore)
        .unwrap();
    let kept = Connection::open(&pre.path).unwrap();
    assert_eq!(count_books(&kept), 31);
}

#[test]
fn a_failed_validation_leaves_the_current_db_untouched() {
    let d = Dirs::new();
    let clock = ManualClock::new(T0);
    {
        let mut conn = d.open();
        add_book(&mut conn, &clock, "守られる本");
    }
    let garbage = d.root.path().join("garbage.sqlite3");
    fs::write(&garbage, b"not a database, definitely not a database......").unwrap();
    let before = file_names(d.db().parent().unwrap());
    let r = restore_backup(&garbage, &d.db(), &d.backups(), &clock);
    assert!(matches!(r, Err(Error::InvalidBackup(_))), "{r:?}");
    assert!(
        restore_backup(
            &d.root.path().join("none.sqlite3"),
            &d.db(),
            &d.backups(),
            &clock
        )
        .is_err()
    );
    assert_eq!(
        file_names(d.db().parent().unwrap()),
        before,
        "作業用のファイルも残さない"
    );
    assert!(
        !d.backups().exists() || list_backups(&d.backups()).unwrap().is_empty(),
        "復元前のコピーも作らない"
    );

    let conn = d.open();
    assert_eq!(count_books(&conn), 1);
    assert_eq!(integrity(&conn), "ok");
    drop(conn);

    // 自分自身は復元元にできない
    let r = restore_backup(&d.db(), &d.db(), &d.backups(), &clock);
    assert!(matches!(r, Err(Error::InvalidBackup(_))), "{r:?}");
    // 版が新しすぎるバックアップ
    let newer = d.root.path().join("newer.sqlite3");
    fs::copy(d.db(), &newer).unwrap();
    Connection::open(&newer)
        .unwrap()
        .pragma_update(None, "user_version", LATEST_VERSION + 5)
        .unwrap();
    let r = restore_backup(&newer, &d.db(), &d.backups(), &clock);
    assert!(matches!(r, Err(Error::SchemaTooNew { .. })), "{r:?}");
    assert_eq!(count_books(&d.open()), 1);
}

#[test]
fn restoring_onto_a_machine_without_a_db_works() {
    let src = Dirs::new();
    let clock = ManualClock::new(T0);
    let path;
    {
        let mut conn = src.open();
        add_book(&mut conn, &clock, "持ってきた本");
        path = create_backup(&conn, &src.backups(), BackupKind::Manual, &clock)
            .unwrap()
            .path;
    }
    let dst = Dirs::new();
    let report = restore_backup(Path::new(&path), &dst.db(), &dst.backups(), &clock).unwrap();
    assert!(report.pre_restore_path.is_none());
    assert!(
        !dst.backups().exists(),
        "残すものがないので、フォルダも作らない"
    );
    let conn = dst.open();
    assert_eq!(count_books(&conn), 1);
    assert_eq!(meta::device_id(&conn).unwrap(), report.new_device_id);
}

#[test]
fn restoring_an_older_schema_migrates_it_and_keeps_unflushed_ops() {
    // フェーズ 0 の DB（0001 だけ）のバックアップを、今のアプリで復元する
    let src = Dirs::new();
    let clock = ManualClock::new(T0);
    let backup_path = src.root.path().join("old-backup.sqlite3");
    let old_dev;
    {
        let (conn, _) = open_with_migrations(&src.db(), &src.backups(), &MIGRATIONS[..1]).unwrap();
        old_dev = meta::device_id(&conn).unwrap();
        conn.execute(
            "INSERT INTO books(id, title, authors_json) VALUES (?1, '旧版の本', '[{\"name\":\"旧著者\"}]')",
            [common::BOOK_1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO outbox(seq, hlc, op_json, flushed) VALUES (1, 'h1', '{\"v\":1}', 0), (2, 'h2', '{\"v\":2}', 1)",
            [],
        )
        .unwrap();
        meta::save_op_seq(&conn, 2).unwrap();
        conn.execute("VACUUM INTO ?1", [backup_path.to_str().unwrap()])
            .unwrap();
    }
    assert_eq!(verify_backup(&backup_path).unwrap().schema_version, 1);

    let dst = Dirs::new();
    let report = restore_backup(&backup_path, &dst.db(), &dst.backups(), &clock).unwrap();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.moved_unflushed_ops, 1);
    let (conn, migration) = open_with_migrations(&dst.db(), &dst.backups(), MIGRATIONS).unwrap();
    assert_eq!(
        migration.from_version, LATEST_VERSION,
        "復元の時点で最新の版にそろえてある"
    );
    assert_eq!(integrity(&conn), "ok");
    // 検索用の派生データは open が作る
    assert_eq!(search_books(&conn, "旧著者").unwrap(), [common::BOOK_1]);
    let moved: Vec<(String, i64)> = conn
        .prepare("SELECT dev, seq FROM outbox_restored")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(moved, [(old_dev, 1)]);
}

// ---------- 別の読み取り専用の接続でのバックアップ（アプリの接続を塞がない） ----------

/// `books` に、大きめの行を `rows` 行入れる（`VACUUM INTO` に時間がかかるようにする）。
fn fill_books(conn: &Connection, rows: i64) {
    conn.execute(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
         INSERT INTO books(id, title, description)
         SELECT printf('fill-%08d', i), 'タイトル ' || i, printf('%.*c', 400, 'x') FROM n",
        [rows],
    )
    .unwrap();
}

#[test]
fn a_reader_connection_makes_a_complete_backup_and_cannot_write() {
    let d = Dirs::new();
    let mut conn = d.open();
    let clock = ManualClock::new(T0);
    add_book(&mut conn, &clock, "A");
    add_book(&mut conn, &clock, "B");

    let reader = backup::open_reader(&d.db()).unwrap();
    // WAL に書いたばかりで、本体にまだ反映していない内容も、読み取りの接続から見える
    let info = create_backup(&reader, &d.backups(), BackupKind::Manual, &clock).unwrap();
    let copy = Connection::open(&info.path).unwrap();
    assert_eq!(integrity(&copy), "ok");
    assert_eq!(count_books(&copy), 2);
    assert_eq!(verify_backup(Path::new(&info.path)).unwrap().book_count, 2);
    // 日次の自動実行も、読み取りの接続で動く
    let daily = run_scheduled_backup(&reader, &d.backups(), &clock)
        .unwrap()
        .expect("日次を作る");
    assert_eq!(daily.kind, BackupKind::Daily);

    // 読み取り専用なので、書き込めない
    assert!(
        reader
            .execute("INSERT INTO books(id, title) VALUES ('x', 'x')", [])
            .is_err()
    );
    assert_eq!(count_books(&conn), 2);
    // アプリの接続は、読み取りの接続を開いたまま書ける
    add_book(&mut conn, &clock, "C");
    assert_eq!(count_books(&conn), 3);
}

#[test]
fn a_backup_taken_while_writes_continue_is_a_consistent_snapshot_and_does_not_block_them() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    let d = Dirs::new();
    let mut conn = d.open();
    fill_books(&conn, 40_000);
    let initial = count_books(&conn);
    let clock = ManualClock::new(T0);

    // 書き込みの側: 1 回のトランザクションで 2 行ずつ足す（どの時点のスナップショットでも、行数は偶数ぶん増える）
    let stop = Arc::new(AtomicBool::new(false));
    let commits = Arc::new(AtomicU64::new(0));
    let slowest_ms = Arc::new(AtomicU64::new(0));
    let writer = {
        let (stop, commits, slowest_ms) = (stop.clone(), commits.clone(), slowest_ms.clone());
        std::thread::spawn(move || {
            let mut n = 0_i64;
            while !stop.load(Ordering::Relaxed) {
                let t = Instant::now();
                let tx = bibcontainer_core::write_tx(&mut conn).unwrap();
                tx.execute(
                    "INSERT INTO books(id, title) VALUES (?1, 'pair'), (?2, 'pair')",
                    [format!("w-{n:08}-a"), format!("w-{n:08}-b")],
                )
                .unwrap();
                tx.commit().unwrap();
                let ms = u64::try_from(t.elapsed().as_millis()).unwrap();
                slowest_ms.fetch_max(ms, Ordering::Relaxed);
                commits.fetch_add(1, Ordering::Relaxed);
                n += 1;
            }
            conn
        })
    };

    // バックアップの側: 別の読み取りの接続で、何回か取る
    let reader = backup::open_reader(&d.db()).unwrap();
    let mut during = 0;
    let mut paths = Vec::new();
    for _ in 0..4 {
        let before = commits.load(Ordering::Relaxed);
        let info = create_backup(&reader, &d.backups(), BackupKind::Manual, &clock).unwrap();
        during += commits.load(Ordering::Relaxed) - before;
        paths.push(info.path);
        std::thread::sleep(Duration::from_millis(30));
    }
    stop.store(true, Ordering::Relaxed);
    let mut conn = writer.join().unwrap();

    // どのバックアップも、ある時点のスナップショット: 完全な DB で、2 行ずつ足された状態（半端な行数にならない）
    for path in &paths {
        let copy = Connection::open(path).unwrap();
        assert_eq!(integrity(&copy), "ok");
        let n = count_books(&copy) - initial;
        assert!(n >= 0 && n % 2 == 0, "{path}: 足された行数 {n}");
    }
    // バックアップ中も書き込みが進んだ（アプリの接続が待たされない）。待たされても、バックアップの
    // 長さではなく、1 回の書き込みが数十ミリ秒の範囲。busy_timeout（5 秒）に達していない。
    assert!(during > 0, "バックアップの間に 1 回も書き込めていない");
    assert!(
        slowest_ms.load(Ordering::Relaxed) < 2000,
        "1 回の書き込みに {}ms かかった",
        slowest_ms.load(Ordering::Relaxed)
    );
    // バックアップを取ったあとも、元の DB は無傷
    assert_eq!(integrity(&conn), "ok");
    assert!(count_books(&conn) > initial);
    add_book(&mut conn, &clock, "最後の本");
}
