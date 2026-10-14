//! アプリが持つ DB 接続。
//!
//! DB を開けなくてもアプリは落とさない。開けなかった理由を保持しておき、DB に触る
//! コマンドが呼ばれるたびにそのエラーを返す（フロントはそれを見てエラー画面を出す）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::Connection;

use crate::error::AppError;

/// DB ファイルの名前（`app_local_data_dir()` の直下に置く）。
pub const DB_FILE_NAME: &str = "library.sqlite3";
/// マイグレーション前のバックアップを置くフォルダの名前（`app_local_data_dir()` の直下）。
pub const BACKUP_DIR_NAME: &str = "backups";

pub struct Database {
    inner: Result<Inner, AppError>,
}

struct Inner {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    /// DB を開く（マイグレーションと端末 ID の生成まで行う）。失敗しても panic せず、
    /// 理由をログに出して `Database` の中に保持する。
    pub fn open(db_path: &Path, backup_dir: &Path) -> Self {
        match bibcontainer_core::open(db_path, backup_dir) {
            Ok((conn, report)) => {
                tracing::info!(
                    db_path = %db_path.display(),
                    schema_version = report.to_version,
                    applied = ?report.applied,
                    backup = ?report.backup_path,
                    "DB を開きました"
                );
                Self {
                    inner: Ok(Inner {
                        conn: Mutex::new(conn),
                        path: db_path.to_owned(),
                    }),
                }
            }
            Err(err) => {
                tracing::error!(db_path = %db_path.display(), error = %err, "DB を開けませんでした");
                Self::failed(AppError::from_open(err))
            }
        }
    }

    /// 開けなかった状態を作る。
    pub fn failed(error: AppError) -> Self {
        Self { inner: Err(error) }
    }

    /// 開けなかった場合の理由。
    pub fn failure(&self) -> Option<&AppError> {
        self.inner.as_ref().err()
    }

    /// WAL の内容を DB 本体へ書き戻し、WAL ファイルを空にする（終了時に呼ぶ）。
    ///
    /// 終了時に接続を閉じないため、これをしないと `-wal` ファイルに未反映の内容が残る。
    /// そのままでも次の起動で復旧されるが、`library.sqlite3` だけをコピーしても最新にならない。
    pub fn checkpoint(&self) {
        let Ok(inner) = &self.inner else { return };
        let conn = inner.conn.lock().unwrap_or_else(PoisonError::into_inner);
        match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())) {
            Ok(()) => tracing::debug!("WAL を DB 本体へ書き戻しました"),
            Err(err) => tracing::warn!(error = %err, "WAL の書き戻しに失敗しました"),
        }
    }

    /// 接続を使う処理を、メインスレッドとは別のスレッドで実行する。
    ///
    /// SQLite の呼び出しはブロックするので、async コマンドからはこれ経由で使う。
    /// 接続は 1 本を Mutex で共有するため、処理は 1 つずつ順に実行される。
    /// 開けなかった場合は、保持しているエラーをそのまま返す。
    pub async fn run<T, F>(self: &Arc<Self>, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&mut Connection, &Path) -> Result<T, AppError> + Send + 'static,
        T: Send + 'static,
    {
        if let Some(err) = self.failure() {
            return Err(err.clone());
        }
        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let inner = this.inner.as_ref().map_err(Clone::clone)?;
            // 処理の途中で panic しても、SQLite 側はトランザクションが巻き戻る。
            // 以後の呼び出しまで巻き込まないよう、poison は無視して続ける。
            let mut conn = inner.conn.lock().unwrap_or_else(PoisonError::into_inner);
            f(&mut conn, &inner.path)
        })
        .await
        .map_err(|err| AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use bibcontainer_core::migrate;
    use tauri::async_runtime::block_on;

    fn paths(dir: &Path) -> (PathBuf, PathBuf) {
        (dir.join(DB_FILE_NAME), dir.join(BACKUP_DIR_NAME))
    }

    #[test]
    fn opens_the_db_and_runs_work_off_the_calling_thread() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        assert!(db.failure().is_none());

        let caller = std::thread::current().id();
        let (version, ran_on) = block_on(db.run(|conn, path| {
            assert_eq!(path.file_name().unwrap(), DB_FILE_NAME);
            Ok((migrate::user_version(conn)?, std::thread::current().id()))
        }))
        .unwrap();
        assert_eq!(version, migrate::LATEST_VERSION);
        assert_ne!(ran_on, caller);
    }

    #[test]
    fn a_db_from_a_newer_app_is_kept_as_an_error_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let (conn, _) = bibcontainer_core::open(&db_path, &backup_dir).unwrap();
        conn.pragma_update(None, "user_version", 999).unwrap();
        drop(conn);

        let db = Arc::new(Database::open(&db_path, &backup_dir));
        assert!(matches!(
            db.failure(),
            Some(AppError::SchemaTooNew { found: 999, .. })
        ));
        // 何度呼んでも同じエラーが返る。
        for _ in 0..2 {
            let err = block_on(db.run(|_, _| Ok(()))).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::SchemaTooNew);
        }

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        assert_eq!(migrate::user_version(&conn).unwrap(), 999);
        assert!(!backup_dir.exists(), "バックアップも作らない");
    }

    #[test]
    fn failing_to_open_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        // DB のパスがフォルダだと開けない。
        let db_path = dir.path().join(DB_FILE_NAME);
        std::fs::create_dir(&db_path).unwrap();

        let db = Arc::new(Database::open(&db_path, &dir.path().join(BACKUP_DIR_NAME)));
        assert_eq!(
            db.failure().map(AppError::kind),
            Some(ErrorKind::DbUnavailable)
        );
        let err = block_on(db.run(|_, _| Ok(()))).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DbUnavailable);
    }

    #[test]
    fn checkpoint_empties_the_wal_file() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        let wal = dir.path().join(format!("{DB_FILE_NAME}-wal"));
        assert!(
            std::fs::metadata(&wal).unwrap().len() > 0,
            "開いた直後は WAL に書き込みがある"
        );

        db.checkpoint();
        assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0);
        // 書き戻したあとも使える。
        block_on(db.run(|conn, _| Ok(migrate::user_version(conn)?))).unwrap();
    }

    #[test]
    fn a_panic_in_work_is_reported_and_later_work_still_runs() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));

        let err = block_on(db.run::<(), _>(|_, _| panic!("テスト用の panic"))).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Internal);
        let version = block_on(db.run(|conn, _| Ok(migrate::user_version(conn)?))).unwrap();
        assert_eq!(version, migrate::LATEST_VERSION);
    }
}
