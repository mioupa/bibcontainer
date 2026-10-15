//! 同期エンジンの `SyncDb` を、アプリの `Database` につなぐ。
//!
//! エンジンは、同期フォルダの読み書きの間は DB の接続を持たず、DB に触る短い処理だけを `with_conn` に渡す。
//! ここでは、その短い処理のたびに `Database` の接続の Mutex を借りる（`Database::run` と同じ Mutex）。
//! 復元・置き換えは同じ Mutex を持ったまま DB のファイルを入れ替えるので、その間は待たされ、
//! 終われば新しい接続で実行される。エンジンは `spawn_blocking` の中で呼ぶこと（`with_conn` はブロックする）。

use std::sync::Arc;

use bibcontainer_sync::{SyncDb, SyncError};
use rusqlite::Connection;

use crate::database::Database;

/// `Database` の接続を借りる `SyncDb`。
#[derive(Clone)]
pub struct AppSyncDb {
    db: Arc<Database>,
}

impl AppSyncDb {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl SyncDb for AppSyncDb {
    fn with_conn<T, F>(&self, f: F) -> bibcontainer_sync::Result<T>
    where
        F: FnOnce(&mut Connection) -> bibcontainer_core::Result<T>,
    {
        // DB を開けていないとき（`schema_too_new` など）は、その理由をそのまま伝える。
        self.db
            .with_connection(|conn, _| f(conn))
            .map_err(|err| SyncError::Internal(err.to_string()))?
            .map_err(SyncError::from)
    }
}

#[cfg(test)]
mod tests {
    use bibcontainer_core::sync_state;

    use super::*;
    use crate::database::{BACKUP_DIR_NAME, DB_FILE_NAME};
    use crate::error::AppError;

    #[test]
    fn borrows_the_same_connection_as_database_run() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(
            &dir.path().join(DB_FILE_NAME),
            &dir.path().join(BACKUP_DIR_NAME),
        ));
        let sync_db = AppSyncDb::new(Arc::clone(&db));

        // SyncDb 経由で書いたものが、Database::run からも見える
        sync_db
            .with_conn(|conn| sync_state::set_library_id(conn, "lib-1"))
            .unwrap();
        let seen =
            tauri::async_runtime::block_on(db.run(|conn, _| Ok(sync_state::library_id(conn)?)))
                .unwrap();
        assert_eq!(seen.as_deref(), Some("lib-1"));
        // 逆も
        tauri::async_runtime::block_on(db.run(|conn, _| Ok(sync_state::clear_library_id(conn)?)))
            .unwrap();
        assert_eq!(
            sync_db
                .with_conn(|conn| sync_state::library_id(conn))
                .unwrap(),
            None
        );
    }

    #[test]
    fn a_core_error_comes_back_as_a_sync_error_and_a_closed_db_as_internal() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(
            &dir.path().join(DB_FILE_NAME),
            &dir.path().join(BACKUP_DIR_NAME),
        ));
        let sync_db = AppSyncDb::new(Arc::clone(&db));
        let err = sync_db
            .with_conn(|conn| sync_state::set_library_id(conn, ""))
            .unwrap_err();
        assert!(matches!(err, SyncError::Db(_)), "{err}");

        let failed = AppSyncDb::new(Arc::new(Database::failed(AppError::DbUnavailable {
            reason: "開けない".into(),
        })));
        let err = failed.with_conn(|_| Ok(())).unwrap_err();
        assert!(matches!(err, SyncError::Internal(_)), "{err}");
        assert!(err.to_string().contains("開けない"), "{err}");
    }

    #[test]
    fn work_waits_while_another_thread_holds_the_connection() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(
            &dir.path().join(DB_FILE_NAME),
            &dir.path().join(BACKUP_DIR_NAME),
        ));
        let sync_db = AppSyncDb::new(Arc::clone(&db));
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let holder_db = Arc::clone(&db);
        let holder = std::thread::spawn(move || {
            holder_db
                .with_connection(|_, _| {
                    entered_tx.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(150));
                })
                .unwrap();
        });
        entered_rx.recv().unwrap();
        let started = std::time::Instant::now();
        sync_db.with_conn(|_| Ok(())).unwrap();
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(100),
            "接続を借りている間は待たされる"
        );
        holder.join().unwrap();
    }
}
