//! DB を開く処理（PRAGMA、端末 ID、FTS5）のテスト。

mod common;

use bibcontainer_core::{Entity, LATEST_VERSION, ids, meta, open};
use rusqlite::Connection;
use tempfile::tempdir;

use common::mem_db;

#[test]
fn open_creates_the_latest_schema_with_the_decided_pragmas() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("data").join("bibcontainer.sqlite3");
    let backups = dir.path().join("backups");

    let (conn, report) = open(&db_path, &backups).unwrap();

    assert_eq!(report.from_version, 0);
    assert_eq!(report.to_version, LATEST_VERSION);
    assert_eq!(report.applied, vec![1]);
    assert!(
        report.backup_path.is_none(),
        "新規 DB ではバックアップしない"
    );
    assert!(!backups.exists());

    let user_version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(user_version, LATEST_VERSION);

    let journal_mode: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(journal_mode, "wal");
    let synchronous: i64 = conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .unwrap();
    assert_eq!(synchronous, 2, "FULL");
    let busy_timeout: i64 = conn
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))
        .unwrap();
    assert_eq!(busy_timeout, 5000);
}

#[test]
fn schema_has_the_expected_tables_and_no_foreign_keys() {
    let conn = mem_db();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        tables,
        [
            "applied_segments",
            "books",
            "field_clocks",
            "local_meta",
            "locations",
            "outbox",
            "provider_cache",
            "series",
            "shelves",
            "tags",
        ],
        "books_fts と book_tags はフェーズ 1 で作る"
    );

    for ent in Entity::ALL {
        let fks: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_foreign_key_list(?1)",
                [ent.table()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fks, 0, "{} に外部キーがない", ent.table());
    }

    // tags.name は UNIQUE ではない
    conn.execute_batch("INSERT INTO tags(id, name) VALUES ('t1', '小説'), ('t2', '小説');")
        .unwrap();
}

#[test]
fn id_only_rows_can_be_created_for_every_synced_table() {
    let conn = mem_db();
    for ent in Entity::ALL {
        conn.execute(
            &format!("INSERT OR IGNORE INTO {}(id) VALUES ('x')", ent.table()),
            [],
        )
        .unwrap_or_else(|e| panic!("{}: {e}", ent.table()));
    }
}

#[test]
fn second_open_applies_nothing_and_keeps_the_device_id() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("bibcontainer.sqlite3");
    let backups = dir.path().join("backups");

    let (conn, _) = open(&db_path, &backups).unwrap();
    let device_id = meta::device_id(&conn).unwrap();
    drop(conn);

    let (conn, report) = open(&db_path, &backups).unwrap();
    assert_eq!(report.from_version, LATEST_VERSION);
    assert_eq!(report.to_version, LATEST_VERSION);
    assert!(report.applied.is_empty());
    assert!(report.backup_path.is_none());
    assert!(!backups.exists(), "適用がなければバックアップも作らない");
    assert_eq!(meta::device_id(&conn).unwrap(), device_id);
}

#[test]
fn device_id_is_generated_once_in_the_expected_format() {
    let conn = mem_db();
    let id = meta::device_id(&conn).unwrap();
    assert!(ids::is_valid_device_id(&id));
    assert_eq!(id.len(), 14);
    assert!(id.starts_with("d-"));
    // ensure は既存の値を返すだけで、書き換えない
    assert_eq!(meta::ensure_device_id(&conn).unwrap(), id);
    assert_eq!(meta::ensure_device_id(&conn).unwrap(), id);

    // 別の DB は別の ID を持つ
    assert_ne!(meta::device_id(&mem_db()).unwrap(), id);
}

#[test]
fn device_id_is_missing_until_ensured() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE local_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
        .unwrap();
    assert!(matches!(
        meta::device_id(&conn),
        Err(bibcontainer_core::Error::MissingMeta("device_id"))
    ));
    let id = meta::ensure_device_id(&conn).unwrap();
    assert_eq!(meta::device_id(&conn).unwrap(), id);
}

#[test]
fn fts5_trigram_tokenizer_is_available() {
    let conn = mem_db();
    conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x, tokenize='trigram');")
        .unwrap();
    conn.execute(
        "INSERT INTO t(x) VALUES (?1), (?2)",
        ["日本語の蔵書管理アプリ", "Rust and SQLite"],
    )
    .unwrap();
    let hits = |q: &str| -> i64 {
        conn.query_row("SELECT count(*) FROM t WHERE t MATCH ?1", [q], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(hits("蔵書管"), 1, "3 文字以上の部分一致");
    assert_eq!(hits("sqlite"), 1, "大文字小文字を区別しない");
    assert_eq!(hits("ない文字列"), 0);
}
