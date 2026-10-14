//! DB を開く。

use std::fs;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::derived;
use crate::error::Result;
use crate::meta;
use crate::migrate::{self, MIGRATIONS, Migration, MigrationReport};

/// `busy_timeout`（ミリ秒）。
pub const BUSY_TIMEOUT_MS: u64 = 5000;

/// DB を開き、PRAGMA を設定し、マイグレーションを適用し、端末 ID がなければ作る。
///
/// `db_path` は端末ローカルの場所（同期フォルダの中ではない）で、アプリ側が決める。
/// 親ディレクトリがなければ作る。マイグレーション前のバックアップは `backup_dir` に置く。
pub fn open(db_path: &Path, backup_dir: &Path) -> Result<(Connection, MigrationReport)> {
    open_with_migrations(db_path, backup_dir, MIGRATIONS)
}

/// `open` のマイグレーション一覧を差し替えられる版（テスト用）。
/// 一覧には端末 ID の保存先である `local_meta` を作るものが含まれている必要がある。
pub fn open_with_migrations(
    db_path: &Path,
    backup_dir: &Path,
    migrations: &[Migration],
) -> Result<(Connection, MigrationReport)> {
    if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(db_path)?;
    configure(&conn)?;
    let report = migrate::migrate(&mut conn, migrations, backup_dir)?;
    meta::ensure_device_id(&conn)?;
    // 検索用の派生データは、マイグレーションでは入れ物しか作らない。中身はここで埋める。
    derived::ensure_built(&mut conn)?;
    Ok((conn, report))
}

/// メモリ上の DB を開く（テストや一時的な利用向け。WAL にはならない）。
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    // 新規 DB なのでバックアップは取られない。
    migrate::migrate(&mut conn, MIGRATIONS, Path::new(""))?;
    meta::ensure_device_id(&conn)?;
    derived::ensure_built(&mut conn)?;
    Ok(conn)
}

/// 書き込みトランザクションを始める。
///
/// 最初から書き込みロックを取るので、読み取りから書き込みへの昇格による `SQLITE_BUSY` を避けられる。
pub fn write_tx(conn: &mut Connection) -> Result<Transaction<'_>> {
    Ok(conn.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

/// 接続の PRAGMA を設定する。
///
/// - `journal_mode=WAL`、`synchronous=FULL`（書き込み頻度が低いので安全側）、`busy_timeout=5000`。
/// - 外部キー制約は使わない。同期対象テーブルの行は他端末の Op によって順不同で作られ、
///   参照先がまだ無いことがあるため。
fn configure(conn: &Connection) -> Result<()> {
    // WAL への切り替えにもロックが要るので、先に busy_timeout を設定する。
    conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
    let mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") && !mode.eq_ignore_ascii_case("memory") {
        tracing::warn!(mode, "WAL モードにできなかった");
    }
    conn.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}
