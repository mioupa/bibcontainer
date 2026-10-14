//! マイグレーションのテスト（ダミーの一覧を使う）。

use std::fs;
use std::path::{Path, PathBuf};

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
