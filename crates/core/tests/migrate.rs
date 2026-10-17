//! マイグレーションのテスト（ダミーの一覧を使う）。

use std::fs;
use std::path::{Path, PathBuf};

mod common;

use bibcontainer_core::migrate::{self, LATEST_VERSION, MIGRATIONS, Migration, user_version};
use bibcontainer_core::{Error, open_with_migrations};
use rusqlite::Connection;
use tempfile::{TempDir, tempdir};

const M1: Migration = Migration {
    version: 1,
    name: "create t",
    sql: "CREATE TABLE t (x INTEGER NOT NULL);",
};
const M2: Migration = Migration {
    version: 2,
    name: "add t.y",
    sql: "ALTER TABLE t ADD COLUMN y TEXT; CREATE TABLE u (z INTEGER);",
};
/// 2 文目で失敗する。1 文目の変更も残ってはいけない。
const M2_BAD: Migration = Migration {
    version: 2,
    name: "broken",
    sql: "CREATE TABLE half_done (a INTEGER); INSERT INTO no_such_table VALUES (1);",
};

struct Env {
    _dir: TempDir,
    db_path: PathBuf,
    backups: PathBuf,
}

fn env() -> Env {
    let dir = tempdir().unwrap();
    Env {
        db_path: dir.path().join("test.sqlite3"),
        backups: dir.path().join("backups"),
        _dir: dir,
    }
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

fn files_in(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir) {
        Ok(rd) => {
            let mut v: Vec<String> = rd
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        }
        Err(_) => Vec::new(),
    }
}

#[test]
fn the_real_migration_list_is_consistent() {
    assert_eq!(LATEST_VERSION as usize, MIGRATIONS.len());
    for (i, m) in MIGRATIONS.iter().enumerate() {
        assert_eq!(m.version as usize, i + 1);
        assert!(!m.sql.trim().is_empty());
    }
}

#[test]
fn fresh_db_is_migrated_to_the_latest_without_a_backup() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    let report = migrate::migrate(&mut conn, &[M1, M2], &e.backups).unwrap();
    assert_eq!(report.from_version, 0);
    assert_eq!(report.to_version, 2);
    assert_eq!(report.applied, vec![1, 2]);
    assert!(report.backup_path.is_none());
    assert!(!e.backups.exists());
    assert_eq!(user_version(&conn).unwrap(), 2);
}

#[test]
fn migrating_again_does_nothing() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    migrate::migrate(&mut conn, &[M1, M2], &e.backups).unwrap();
    let report = migrate::migrate(&mut conn, &[M1, M2], &e.backups).unwrap();
    assert!(report.applied.is_empty());
    assert_eq!(report.from_version, 2);
    assert_eq!(report.to_version, 2);
    assert!(report.backup_path.is_none());
    assert!(files_in(&e.backups).is_empty());
}

#[test]
fn upgrading_v1_to_v2_takes_a_backup_of_v1_first() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    migrate::migrate(&mut conn, &[M1], &e.backups).unwrap();
    conn.execute("INSERT INTO t(x) VALUES (42)", []).unwrap();
    assert!(
        files_in(&e.backups).is_empty(),
        "v0→v1 ではバックアップしない"
    );

    let report = migrate::migrate(&mut conn, &[M1, M2], &e.backups).unwrap();
    assert_eq!(report.from_version, 1);
    assert_eq!(report.to_version, 2);
    assert_eq!(report.applied, vec![2]);

    let backup_path = report.backup_path.expect("バックアップのパス");
    assert_eq!(backup_path.parent().unwrap(), e.backups);
    let name = backup_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with("pre-migration-v1-") && name.ends_with("Z.sqlite3"),
        "{name}"
    );
    // pre-migration-v1-20261014T083000Z.sqlite3
    assert_eq!(
        name.len(),
        "pre-migration-v1-20261014T083000Z.sqlite3".len()
    );
    assert_eq!(files_in(&e.backups), vec![name]);

    // 本体は v2
    assert_eq!(user_version(&conn).unwrap(), 2);
    assert!(has_table(&conn, "u"));

    // バックアップは適用前（v1）の状態で、データを含む
    let backup = Connection::open(&backup_path).unwrap();
    assert_eq!(user_version(&backup).unwrap(), 1);
    assert!(!has_table(&backup, "u"));
    let x: i64 = backup
        .query_row("SELECT x FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(x, 42);
}

#[test]
fn a_failing_migration_rolls_back_and_keeps_the_version() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    migrate::migrate(&mut conn, &[M1], &e.backups).unwrap();

    let err = migrate::migrate(&mut conn, &[M1, M2_BAD], &e.backups).unwrap_err();
    assert!(
        matches!(err, Error::MigrationFailed { version: 2, .. }),
        "{err:?}"
    );
    assert_eq!(user_version(&conn).unwrap(), 1, "版は変わらない");
    assert!(!has_table(&conn, "half_done"), "途中までの変更も残らない");
    assert!(has_table(&conn, "t"));
    // 失敗しても、適用前のバックアップは残っている
    assert_eq!(files_in(&e.backups).len(), 1);

    // 直した版で再試行できる。同じ秒に 2 回目のバックアップを取っても、前のものを壊さない。
    let report = migrate::migrate(&mut conn, &[M1, M2], &e.backups).unwrap();
    assert_eq!(report.applied, vec![2]);
    assert_eq!(files_in(&e.backups).len(), 2);
}

#[test]
fn a_failing_first_migration_leaves_an_empty_unversioned_db() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    let bad_first = Migration {
        version: 1,
        name: "broken",
        sql: "CREATE TABLE half_done (a INTEGER); INSERT INTO no_such_table VALUES (1);",
    };
    let err = migrate::migrate(&mut conn, &[bad_first], &e.backups).unwrap_err();
    assert!(matches!(err, Error::MigrationFailed { version: 1, .. }));
    assert_eq!(user_version(&conn).unwrap(), 0);
    assert!(!has_table(&conn, "half_done"));
}

#[test]
fn a_db_newer_than_the_app_is_rejected_with_a_dedicated_error() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    migrate::migrate(&mut conn, &[M1, M2], &e.backups).unwrap();

    let err = migrate::migrate(&mut conn, &[M1], &e.backups).unwrap_err();
    assert!(
        matches!(
            err,
            Error::SchemaTooNew {
                found: 2,
                supported: 1
            }
        ),
        "{err:?}"
    );
    assert_eq!(user_version(&conn).unwrap(), 2, "DB には何も書かない");
    assert!(files_in(&e.backups).is_empty(), "バックアップも取らない");
}

#[test]
fn open_reports_schema_too_new_for_a_db_from_a_newer_app() {
    let e = env();
    // 新しい版のアプリが作った DB（本物の一覧より版が大きい）
    let mut fake: Vec<Migration> = MIGRATIONS.to_vec();
    fake.extend(
        (LATEST_VERSION + 1..=LATEST_VERSION + 3).map(|v| Migration {
            version: v,
            name: "future",
            sql: "SELECT 1;",
        }),
    );
    let (conn, _) = open_with_migrations(&e.db_path, &e.backups, &fake).unwrap();
    drop(conn);

    let err = bibcontainer_core::open(&e.db_path, &e.backups).unwrap_err();
    assert!(
        matches!(err, Error::SchemaTooNew { found, supported }
            if found == LATEST_VERSION + 3 && supported == LATEST_VERSION),
        "{err:?}"
    );
}

#[test]
fn invalid_migration_lists_are_rejected() {
    let e = env();
    let mut conn = Connection::open(&e.db_path).unwrap();
    for bad in [&[M2][..], &[M1, M1][..]] {
        let err = migrate::migrate(&mut conn, bad, &e.backups).unwrap_err();
        assert!(matches!(err, Error::InvalidMigrations(_)), "{err:?}");
    }
    assert_eq!(user_version(&conn).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// 0003: field_clocks を WITHOUT ROWID にする
// ---------------------------------------------------------------------------

fn field_clocks_sql(conn: &Connection) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'field_clocks'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn freelist_count(conn: &Connection) -> i64 {
    conn.pragma_query_value(None, "freelist_count", |r| r.get(0))
        .unwrap()
}

/// 2026-10-14T08:30:00Z 付近
const T0: i64 = 1_791_966_600_000;

/// 版 2 の DB（0003 の前）を作り、本を `books` 冊入れる。
///
/// 今のコード（`library`、`dump_canonical`、`apply_op`）は `books` の最新の列（0004 の `work_id`）を
/// 読み書きするので、古い版の DB には使えない。古い版のアプリが書いた状態を、SQL で直接作る。
fn v2_db_with_books(e: &Env, books: usize) -> Connection {
    use bibcontainer_core::Hlc;

    let (mut conn, report) =
        open_with_migrations(&e.db_path, &e.backups, &MIGRATIONS[..2]).unwrap();
    assert_eq!(report.to_version, 2);
    let tx = conn.transaction().unwrap();
    {
        let mut insert_book = tx
            .prepare(
                "INSERT INTO books(id, title, authors_json, publisher, memo, tags_json, created_at)
                 VALUES (?1, ?2, ?3, '出版社', 'メモ', '[\"研究/SiC\",\"小説\"]', '2026-10-14T08:30:00.000Z')",
            )
            .unwrap();
        let mut insert_clock = tx
            .prepare(
                "INSERT INTO field_clocks(entity, entity_id, field, hlc) VALUES ('book', ?1, ?2, ?3)",
            )
            .unwrap();
        for i in 0..books {
            let id = format!("0192f3a0-6c1e-7cc3-9b1a-{i:012}");
            let authors = format!(r#"[{{"name":"著者 {i}","role":"著"}}]"#);
            insert_book
                .execute(rusqlite::params![id, format!("書名 {i}"), authors])
                .unwrap();
            let hlc = Hlc::new(T0 + i64::try_from(i).unwrap(), 0, "d-aaaaaaaaaaaa")
                .unwrap()
                .to_string();
            for field in [
                "title",
                "authors",
                "publisher",
                "memo",
                "tags",
                "ownership",
                "read_status",
                "meta_status",
                "deleted",
                "purged",
                "created_at",
            ] {
                insert_clock
                    .execute(rusqlite::params![id, field, hlc])
                    .unwrap();
            }
        }
    }
    tx.commit().unwrap();
    conn
}

/// テーブルの全行（第 1 列の順）。移行の前後で中身が変わらないことの確認に使う。
fn snapshot(conn: &Connection, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
        .unwrap();
    let columns = stmt.column_count();
    stmt.query_map([], |r| {
        (0..columns)
            .map(|i| r.get::<_, rusqlite::types::Value>(i))
            .collect()
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

#[test]
fn the_real_migration_list_has_field_clocks_without_rowid_as_the_third() {
    let m = &MIGRATIONS[2];
    assert_eq!((m.version, m.name), (3, "field_clocks_without_rowid"));
}

#[test]
fn upgrading_v2_to_v3_keeps_every_clock_and_the_whole_library() {
    let e = env();
    let conn = v2_db_with_books(&e, 120);
    assert!(
        !field_clocks_sql(&conn).contains("WITHOUT ROWID"),
        "0002 までは通常のテーブル"
    );
    let books_before = snapshot(&conn, "books");
    let rows_before: i64 = conn
        .query_row("SELECT count(*) FROM field_clocks", [], |r| r.get(0))
        .unwrap();
    assert!(rows_before > 120 * 5, "{rows_before}");
    // 行ごとの突き合わせ用に、移行前の中身を別のテーブルに写しておく
    conn.execute_batch("CREATE TABLE clocks_before AS SELECT * FROM field_clocks;")
        .unwrap();
    drop(conn);

    let (conn, report) = open_with_migrations(&e.db_path, &e.backups, &MIGRATIONS[..3]).unwrap();
    assert_eq!((report.from_version, report.to_version), (2, 3));
    assert_eq!(report.applied, vec![3]);
    assert_eq!(user_version(&conn).unwrap(), 3);

    // 作り直したテーブル: 列・主キーは同じで、WITHOUT ROWID
    let sql = field_clocks_sql(&conn);
    assert!(sql.contains("WITHOUT ROWID"), "{sql}");
    let columns: Vec<(String, i64)> = conn
        .prepare("SELECT name, pk FROM pragma_table_info('field_clocks') ORDER BY cid")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        columns,
        [
            ("entity".to_owned(), 1),
            ("entity_id".to_owned(), 2),
            ("field".to_owned(), 3),
            ("hlc".to_owned(), 0),
        ],
        "主キーは (entity, entity_id, field) のまま"
    );
    assert!(
        conn.prepare("SELECT rowid FROM field_clocks").is_err(),
        "rowid を持たない"
    );

    // 中身は 1 行も変わらない
    let rows_after: i64 = conn
        .query_row("SELECT count(*) FROM field_clocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows_after, rows_before);
    for (a, b) in [
        ("clocks_before", "field_clocks"),
        ("field_clocks", "clocks_before"),
    ] {
        let diff: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM (SELECT * FROM {a} EXCEPT SELECT * FROM {b})"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(diff, 0, "{a} にあって {b} にない行がある");
    }
    assert_eq!(snapshot(&conn, "books"), books_before);

    // 移行前のバックアップは v2 のまま残る
    let backup = Connection::open(report.backup_path.expect("移行前のバックアップ")).unwrap();
    assert_eq!(user_version(&backup).unwrap(), 2);
    assert!(!field_clocks_sql(&backup).contains("WITHOUT ROWID"));

    // 作り直したあとも、時計の更新（ON CONFLICT ... DO UPDATE）と、新しい変更の記録が動く
    // （最新の版まで上げる。`update_book` は最新の列を読む）
    drop(conn);
    let (mut conn, _) = open_with_migrations(&e.db_path, &e.backups, MIGRATIONS).unwrap();
    conn.execute_batch("DROP TABLE clocks_before;").unwrap();
    let id: String = conn
        .query_row("SELECT id FROM books ORDER BY id LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let memo_clock = |conn: &Connection| -> String {
        conn.query_row(
            "SELECT hlc FROM field_clocks WHERE entity = 'book' AND entity_id = ?1 AND field = 'memo'",
            [&id],
            |r| r.get(0),
        )
        .unwrap()
    };
    let memo_before = memo_clock(&conn);
    let clock = bibcontainer_core::ManualClock::new(1_800_000_000_000);
    let env = bibcontainer_core::library::Env::new(&clock);
    bibcontainer_core::library::update_book(
        &mut conn,
        &env,
        &id,
        &[bibcontainer_core::library::FieldChange::manual(
            "memo",
            serde_json::json!("更新後"),
        )],
        bibcontainer_core::library::UpdateOptions::default(),
    )
    .unwrap();
    assert!(memo_clock(&conn) > memo_before, "新しい HLC で上書きされる");
    let rows_final: i64 = conn
        .query_row("SELECT count(*) FROM field_clocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        rows_final, rows_before,
        "同じ行を更新するだけで、行は増えない"
    );
}

#[test]
fn upgrading_to_v3_shrinks_the_file_when_the_old_table_left_free_pages() {
    let e = env();
    // field_clocks を数万行にする（通常のテーブルだと、本体と主キーの索引で十数 MB）
    let conn = v2_db_with_books(&e, 100);
    conn.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 60000)
         INSERT INTO field_clocks(entity, entity_id, field, hlc)
         SELECT 'book', printf('0192f3a0-6c1e-7cc3-9b1a-%012d', i / 40), 'field_' || (i % 40),
                '000001791966600:00000:d-aaaaaaaaaaaa'
         FROM n;",
    )
    .unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    drop(conn);
    let size_before = fs::metadata(&e.db_path).unwrap().len();

    let (conn, _) = open_with_migrations(&e.db_path, &e.backups, &MIGRATIONS[..3]).unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    drop(conn);
    let size_after = fs::metadata(&e.db_path).unwrap().len();

    assert!(
        size_after * 10 < size_before * 8,
        "VACUUM で 2 割以上は小さくなる: {size_before} → {size_after}"
    );
    let (conn, _) = open_with_migrations(&e.db_path, &e.backups, &MIGRATIONS[..3]).unwrap();
    assert_eq!(freelist_count(&conn), 0, "空きページを残さない");
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
}

#[test]
fn field_clocks_in_a_fresh_db_is_without_rowid_and_the_convergence_rules_still_hold() {
    use bibcontainer_core::{Entity, Hlc, Op, apply_op, db, schema::Fields};
    use serde_json::json;

    let mut conn = db::open_in_memory().unwrap();
    assert!(field_clocks_sql(&conn).contains("WITHOUT ROWID"));
    let op = |ms: i64, title: &str| {
        let set: Fields = [("title".to_owned(), json!(title))].into_iter().collect();
        Op {
            v: 1,
            dev: "d-aaaaaaaaaaaa".to_owned(),
            seq: 1,
            hlc: Hlc::new(ms, 0, "d-aaaaaaaaaaaa").unwrap().to_string(),
            ent: Entity::Book,
            id: "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned(),
            set,
        }
    };
    for o in [op(2000, "新しい"), op(1000, "古い"), op(2000, "新しい")] {
        let tx = db::write_tx(&mut conn).unwrap();
        apply_op(&tx, &o).unwrap();
        tx.commit().unwrap();
    }
    let title: String = conn
        .query_row("SELECT title FROM books", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "新しい", "古い Op は時計に負ける");
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM field_clocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1);
}

// ---------------------------------------------------------------------------
// 0004: books.work_id（作品の手動の指定）
// ---------------------------------------------------------------------------

const WORK: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
const OTHER_WORK: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d02";

#[test]
fn the_real_migration_list_has_book_work_as_the_fourth() {
    let m = &MIGRATIONS[3];
    assert_eq!((m.version, m.name), (4, "book_work"));
}

fn book_columns(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT name FROM pragma_table_info('books')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn work_clock(conn: &Connection, id: &str) -> Option<String> {
    bibcontainer_core::store::field_clock(conn, bibcontainer_core::Entity::Book, id, "work_id")
        .unwrap()
}

fn work_column(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row("SELECT work_id FROM books WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .unwrap()
}

fn extra_json(conn: &Connection, id: &str) -> serde_json::Value {
    let text: String = conn
        .query_row("SELECT extra_json FROM books WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// 版 3 の DB に、`work_id` を未知の項目として受け取った状態の本を入れる。
/// この項目を知らない版のアプリは、届いた値を `extra_json` に入れ、時計を `field_clocks` に書く。
/// 今のコード（`apply_op`）は `work_id` を知っているので、その状態は SQL で直接作る。
fn insert_book_with_extra(conn: &Connection, id: &str, extra: &serde_json::Value, hlc: &str) {
    conn.execute(
        "INSERT INTO books(id, title, extra_json) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, format!("本 {id}"), extra.to_string()],
    )
    .unwrap();
    if extra.get("work_id").is_some() {
        conn.execute(
            "INSERT INTO field_clocks(entity, entity_id, field, hlc) VALUES ('book', ?1, 'work_id', ?2)",
            rusqlite::params![id, hlc],
        )
        .unwrap();
    }
}

#[test]
fn upgrading_v3_to_v4_moves_a_received_work_id_from_extra_json_into_the_column() {
    use bibcontainer_core::{Entity, Hlc};
    use serde_json::json;

    const WITH_TEXT: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a01";
    const WITH_NULL: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a02";
    const WITH_NUMBER: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a03";
    const WITHOUT: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a04";

    let e = env();
    let (conn, report) = open_with_migrations(&e.db_path, &e.backups, &MIGRATIONS[..3]).unwrap();
    assert_eq!(report.to_version, 3);
    assert!(!book_columns(&conn).contains(&"work_id".to_owned()));

    let remote_hlc = Hlc::new(T0 + 10_000, 0, common::DEV_B).unwrap().to_string();
    for (id, extra) in [
        (WITH_TEXT, json!({ "future_field": "x", "work_id": WORK })),
        (WITH_NULL, json!({ "future_field": "x", "work_id": null })),
        (WITH_NUMBER, json!({ "future_field": "x", "work_id": 5 })),
        (WITHOUT, json!({})),
    ] {
        insert_book_with_extra(&conn, id, &extra, &remote_hlc);
    }
    let clocks_before = snapshot(&conn, "field_clocks");
    assert_eq!(clocks_before.len(), 3);
    drop(conn);

    let (mut conn, report) = open_with_migrations(&e.db_path, &e.backups, MIGRATIONS).unwrap();
    assert_eq!((report.from_version, report.to_version), (3, 4));
    assert_eq!(report.applied, vec![4]);
    assert_eq!(user_version(&conn).unwrap(), 4);
    assert!(book_columns(&conn).contains(&"work_id".to_owned()));

    // 文字列と null は列へ移り、extra_json からキーが消える（ほかのキーは残る）
    assert_eq!(work_column(&conn, WITH_TEXT).as_deref(), Some(WORK));
    assert_eq!(extra_json(&conn, WITH_TEXT), json!({ "future_field": "x" }));
    assert_eq!(work_column(&conn, WITH_NULL), None);
    assert_eq!(extra_json(&conn, WITH_NULL), json!({ "future_field": "x" }));
    // ほかの型の値は動かさない
    assert_eq!(work_column(&conn, WITH_NUMBER), None);
    assert_eq!(
        extra_json(&conn, WITH_NUMBER),
        json!({ "future_field": "x", "work_id": 5 })
    );
    // 受け取っていない本は、どちらも空
    assert_eq!(work_column(&conn, WITHOUT), None);
    assert_eq!(extra_json(&conn, WITHOUT), json!({}));

    // 時計は 1 行も変わらない（0004 の決定: 受け取ったときの時計をそのまま使う）
    assert_eq!(snapshot(&conn, "field_clocks"), clocks_before);
    assert_eq!(work_clock(&conn, WITH_TEXT), Some(remote_hlc.clone()));
    assert_eq!(work_clock(&conn, WITHOUT), None);
    let dump = bibcontainer_core::dump_canonical(&conn).unwrap();
    let row = common::dump_row(&dump, "books", WITH_TEXT);
    assert_eq!(row["work_id"], json!(WORK));
    assert_eq!(row["extra"], json!({ "future_field": "x" }));
    assert_eq!(
        common::dump_row(&dump, "books", WITH_NULL)["work_id"],
        json!(null)
    );

    // 以後は、時計の比べ方が通常の項目と同じ: 古い Op は負け、新しい Op は勝つ
    for id in [WITH_TEXT, WITH_NULL] {
        let older = common::remote_op(
            common::DEV_C,
            T0 + 5_000,
            0,
            Entity::Book,
            id,
            json!({ "work_id": OTHER_WORK }),
        );
        let outcome = common::apply(&mut conn, &older);
        assert_eq!((outcome.applied, outcome.stale), (0, 1), "古い Op は負ける");
    }
    assert_eq!(work_column(&conn, WITH_TEXT).as_deref(), Some(WORK));
    assert_eq!(work_column(&conn, WITH_NULL), None);
    for id in [WITH_TEXT, WITH_NULL] {
        let newer = common::remote_op(
            common::DEV_C,
            T0 + 20_000,
            0,
            Entity::Book,
            id,
            json!({ "work_id": OTHER_WORK }),
        );
        let outcome = common::apply(&mut conn, &newer);
        assert_eq!((outcome.applied, outcome.stale), (1, 0), "新しい Op は勝つ");
        assert_eq!(work_column(&conn, id).as_deref(), Some(OTHER_WORK));
        assert!(
            extra_json(&conn, id).get("work_id").is_none(),
            "extra_json には入らない"
        );
    }
    let dump = bibcontainer_core::dump_canonical(&conn).unwrap();
    assert_eq!(
        common::dump_row(&dump, "books", WITH_TEXT)["work_id"],
        json!(OTHER_WORK)
    );

    // 移行前のバックアップは v3 のまま残る
    let backup = Connection::open(report.backup_path.expect("移行前のバックアップ")).unwrap();
    assert_eq!(user_version(&backup).unwrap(), 3);
    assert!(!book_columns(&backup).contains(&"work_id".to_owned()));
}

#[test]
fn a_corrupt_extra_json_does_not_stop_the_work_id_migration() {
    let e = env();
    let (conn, _) = open_with_migrations(&e.db_path, &e.backups, &MIGRATIONS[..3]).unwrap();
    // 壊れた JSON の行（他端末の書き込みではまず起きないが、移行が止まらないことを確かめる）
    conn.execute(
        "INSERT INTO books(id, extra_json) VALUES (?1, 'not json')",
        ["0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a01"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO books(id, extra_json) VALUES (?1, ?2)",
        rusqlite::params![
            "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a02",
            format!(r#"{{"work_id":"{WORK}"}}"#)
        ],
    )
    .unwrap();
    drop(conn);

    let (conn, report) = open_with_migrations(&e.db_path, &e.backups, MIGRATIONS).unwrap();
    assert_eq!(report.applied, vec![4]);
    assert_eq!(
        work_column(&conn, "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a01"),
        None
    );
    assert_eq!(
        work_column(&conn, "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a02").as_deref(),
        Some(WORK)
    );
}
