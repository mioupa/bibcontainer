//! `PRAGMA user_version` によるスキーマのマイグレーション。

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior};

use crate::error::{Error, Result};

/// マイグレーション 1 件。
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// 適用後の `user_version`。一覧の中で 1, 2, 3… と連番にする。
    pub version: u32,
    /// ログとエラーメッセージに出す名前。
    pub name: &'static str,
    /// 適用する SQL。トランザクションはランナーが張るので `BEGIN` / `COMMIT` は書かない。
    pub sql: &'static str,
}

/// アプリが知るマイグレーションの一覧。
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "local_settings_and_search",
        sql: include_str!("../migrations/0002_local_settings_and_search.sql"),
    },
    Migration {
        version: 3,
        name: "field_clocks_without_rowid",
        sql: include_str!("../migrations/0003_field_clocks_without_rowid.sql"),
    },
];

/// アプリが知る最新のスキーマ版。
pub const LATEST_VERSION: u32 = MIGRATIONS[MIGRATIONS.len() - 1].version;

/// マイグレーションの結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// 適用前の版（新規 DB は 0）。
    pub from_version: u32,
    /// 適用後の版。
    pub to_version: u32,
    /// 今回適用した版。
    pub applied: Vec<u32>,
    /// 適用前に取ったバックアップ。新規 DB や適用なしのときは `None`。
    pub backup_path: Option<PathBuf>,
}

/// 現在の `user_version` を読む。
pub fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

/// 未適用のマイグレーションを順に適用する。
///
/// - DB の版が `migrations` の最新より新しければ `SchemaTooNew`（DB には何も書かない）。
/// - 未適用があり、DB が新規でない（`user_version > 0`）場合は、適用前に
///   `backup_dir/pre-migration-v{旧版}-{UTC時刻}.sqlite3` へ `VACUUM INTO` でバックアップする。
/// - 各マイグレーションは 1 つのトランザクションで実行し、`user_version` の更新も同じ
///   トランザクションに入れる。失敗したらロールバックされ、版は変わらない。
/// - テーブルを作り直すマイグレーションで空きページが増えたときは、適用のあとに `VACUUM` して
///   ファイルを縮める（`reclaim_space`）。
pub fn migrate(
    conn: &mut Connection,
    migrations: &[Migration],
    backup_dir: &Path,
) -> Result<MigrationReport> {
    run(conn, migrations, Some(backup_dir))
}

/// `migrate` と同じだが、適用前のバックアップを取らない。バックアップの復元で、作業用のコピーを
/// 最新の版にそろえるときに使う（元のバックアップがそのまま残るので、取り直す必要がない）。
pub fn migrate_without_backup(
    conn: &mut Connection,
    migrations: &[Migration],
) -> Result<MigrationReport> {
    run(conn, migrations, None)
}

fn run(
    conn: &mut Connection,
    migrations: &[Migration],
    backup_dir: Option<&Path>,
) -> Result<MigrationReport> {
    check_list(migrations)?;
    let latest = migrations.last().map_or(0, |m| m.version);
    let current = user_version(conn)?;
    let mut report = MigrationReport {
        from_version: current,
        to_version: current,
        applied: Vec::new(),
        backup_path: None,
    };

    if current > latest {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: latest,
        });
    }
    if current == latest {
        return Ok(report);
    }

    if let (Some(dir), true) = (backup_dir, current > 0) {
        report.backup_path = Some(backup(conn, dir, current)?);
    }

    for m in migrations.iter().filter(|m| m.version > current) {
        // 即時トランザクションにして、別プロセスとの競合では後から来た側が待つようにする。
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if user_version(&tx)? >= m.version {
            // 待っている間に、別プロセスが適用を済ませた。
            continue;
        }
        let fail = |source| Error::MigrationFailed {
            version: m.version,
            name: m.name,
            source,
        };
        tx.execute_batch(m.sql).map_err(fail)?;
        tx.pragma_update(None, "user_version", m.version)
            .map_err(fail)?;
        tx.commit().map_err(fail)?;
        tracing::info!(
            version = m.version,
            name = m.name,
            "マイグレーションを適用した"
        );
        report.applied.push(m.version);
        report.to_version = m.version;
    }
    if !report.applied.is_empty() {
        reclaim_space(conn);
    }
    Ok(report)
}

/// 空きページがこの数以上あり、かつファイルの 2 割以上を占めるとき、`VACUUM` で縮める。
const RECLAIM_MIN_FREE_PAGES: i64 = 1000;

/// マイグレーションのあとに、空きページが多ければ `VACUUM` してファイルを縮める。
///
/// テーブルを作り直して古いものを消すマイグレーション（0003 など）は、古い領域を空きページとして
/// ファイルに残す。そのままだと、DB のファイルは小さくならない。マイグレーションのトランザクションの
/// 外で（`VACUUM` はトランザクションの中では動かない）、適用がすべて終わったあとに 1 回だけ行う。
/// 失敗しても、マイグレーションは成功している。警告を出すだけにする（ファイルが大きいままになる）。
fn reclaim_space(conn: &Connection) {
    let pragma = |name: &str| conn.pragma_query_value(None, name, |r| r.get::<_, i64>(0));
    let (Ok(free), Ok(total)) = (pragma("freelist_count"), pragma("page_count")) else {
        return;
    };
    if free < RECLAIM_MIN_FREE_PAGES || free * 5 < total {
        return;
    }
    let started = std::time::Instant::now();
    match conn.execute_batch("VACUUM") {
        Ok(()) => {
            // WAL モードの VACUUM は、書き直した DB 全体をいったん WAL に書く。DB 本体へ書き戻して WAL を空にし、
            // ファイルを縮めた分のディスクを、実際に返す。
            let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
            tracing::info!(
                free_pages = free,
                total_pages = total,
                elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "マイグレーションのあとに VACUUM して、空きページを返した"
            );
        }
        Err(err) => tracing::warn!(error = %err, "マイグレーションのあとの VACUUM に失敗した"),
    }
}

fn check_list(migrations: &[Migration]) -> Result<()> {
    for (i, m) in migrations.iter().enumerate() {
        let expected = u32::try_from(i + 1).unwrap_or(u32::MAX);
        if m.version != expected {
            return Err(Error::InvalidMigrations(format!(
                "{} 番目の版は {expected} のはずが {}（{}）",
                i + 1,
                m.version,
                m.name
            )));
        }
    }
    Ok(())
}

/// `VACUUM INTO` でバックアップを取り、そのパスを返す。
fn backup(conn: &Connection, backup_dir: &Path, from_version: u32) -> Result<PathBuf> {
    fs::create_dir_all(backup_dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    // 同じ秒に再試行しても、既存のバックアップを上書きも失敗もしないよう連番を付ける。
    let base = format!("pre-migration-v{from_version}-{stamp}");
    let mut path = backup_dir.join(format!("{base}.sqlite3"));
    for n in 1.. {
        if !path.exists() {
            break;
        }
        path = backup_dir.join(format!("{base}-{n}.sqlite3"));
    }
    let path_str = path
        .to_str()
        .ok_or_else(|| Error::InvalidPath(path.clone()))?;
    conn.execute("VACUUM INTO ?1", [path_str])?;
    tracing::info!(path = %path.display(), from_version, "マイグレーション前のバックアップを作った");
    Ok(path)
}
