//! アプリが持つ DB 接続。
//!
//! DB を開けなくてもアプリは落とさない。開けなかった理由を保持しておき、DB に触る
//! コマンドが呼ばれるたびにそのエラーを返す（フロントはそれを見てエラー画面を出す）。
//!
//! 接続は 1 本を Mutex で共有する。バックアップからの復元（`restore`）は、その Mutex を持ったまま
//! 接続を閉じ、DB のファイルを置き換えて開き直す。復元中の `run` は、終わるまで待ってから新しい
//! 接続で実行される。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bibcontainer_core::backup::RestoreReport;
use rusqlite::Connection;

use crate::error::AppError;

/// DB ファイルの名前（`app_local_data_dir()` の直下に置く）。
pub const DB_FILE_NAME: &str = "library.sqlite3";
/// マイグレーション前のバックアップを置くフォルダの名前（`app_local_data_dir()` の直下）。
pub const BACKUP_DIR_NAME: &str = "backups";

/// DB のファイルの場所。`app_local_data_dir()` を決められなかったときは持たない。
struct Paths {
    db: PathBuf,
    /// マイグレーション前のバックアップを置くフォルダ（`app_local_data_dir()` の `backups`）。
    migration_backups: PathBuf,
}

/// 接続の状態。開けなかったとき（と、復元の途中で開き直せなかったとき）は理由を持つ。
enum Slot {
    Open(Connection),
    Failed(AppError),
}

pub struct Database {
    paths: Option<Paths>,
    slot: Mutex<Slot>,
}

impl Database {
    /// DB を開く（マイグレーションと端末 ID の生成まで行う）。失敗しても panic せず、
    /// 理由をログに出して `Database` の中に保持する。
    pub fn open(db_path: &Path, backup_dir: &Path) -> Self {
        Self {
            paths: Some(Paths {
                db: db_path.to_owned(),
                migration_backups: backup_dir.to_owned(),
            }),
            slot: Mutex::new(open_slot(db_path, backup_dir)),
        }
    }

    /// 開けなかった状態を作る。
    pub fn failed(error: AppError) -> Self {
        Self {
            paths: None,
            slot: Mutex::new(Slot::Failed(error)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Slot> {
        // 処理の途中で panic しても、SQLite 側はトランザクションが巻き戻る。
        // 以後の呼び出しまで巻き込まないよう、poison は無視して続ける。
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 開けなかった場合の理由（処理中の呼び出しがあれば、終わるまで待つ）。テストで状態を確かめるのに使う。
    #[cfg(test)]
    pub fn failure(&self) -> Option<AppError> {
        match &*self.lock() {
            Slot::Open(_) => None,
            Slot::Failed(err) => Some(err.clone()),
        }
    }

    /// WAL の内容を DB 本体へ書き戻し、WAL ファイルを空にする（終了時に呼ぶ）。
    ///
    /// 終了時に接続を閉じないため、これをしないと `-wal` ファイルに未反映の内容が残る。
    /// そのままでも次の起動で復旧されるが、`library.sqlite3` だけをコピーしても最新にならない。
    pub fn checkpoint(&self) {
        let slot = self.lock();
        let Slot::Open(conn) = &*slot else { return };
        match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())) {
            Ok(()) => tracing::debug!("WAL を DB 本体へ書き戻しました"),
            Err(err) => tracing::warn!(error = %err, "WAL の書き戻しに失敗しました"),
        }
    }

    /// 接続を使う処理を、メインスレッドとは別のスレッドで実行する。
    ///
    /// SQLite の呼び出しはブロックするので、async コマンドからはこれ経由で使う。
    /// 接続は 1 本を Mutex で共有するため、処理は 1 つずつ順に実行される
    /// （バックアップからの復元中は、復元が終わるまで待たされる）。
    /// 開けなかった場合は、保持しているエラーをそのまま返す。
    pub async fn run<T, F>(self: &Arc<Self>, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&mut Connection, &Path) -> Result<T, AppError> + Send + 'static,
        T: Send + 'static,
    {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let mut slot = this.lock();
            match &mut *slot {
                Slot::Failed(err) => Err(err.clone()),
                Slot::Open(conn) => {
                    let path = this.paths.as_ref().map_or_else(|| Path::new(""), |p| &p.db);
                    f(conn, path)
                }
            }
        })
        .await
        .map_err(|err| AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}")))?
    }

    /// バックアップから復元する。接続を閉じ、DB のファイルを置き換え、開き直す。
    ///
    /// 復元の間は接続の Mutex を持ち続けるので、ほかの `run` は終わるまで待たされ、開き直した
    /// 新しい接続を使う。復元に失敗したとき（検証で弾かれたときなど）は、今の DB のままで開き直す。
    /// 開き直せなかったときは、その理由を保持して（以後のコマンドが同じエラーを返す）、エラーを返す。
    /// 今の DB は `safety_dir` に `pre-restore-…` として残る。
    ///
    /// DB を最初から開けなかった場合（スキーマ版が新しすぎるなど）も、バックアップから復元できる。
    pub async fn restore(
        self: &Arc<Self>,
        backup: PathBuf,
        safety_dir: PathBuf,
    ) -> Result<RestoreReport, AppError> {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let paths = this.paths.as_ref().ok_or_else(|| AppError::DbUnavailable {
                reason: "データベースの場所を決められないため、復元できません".to_owned(),
            })?;
            let mut slot = this.lock();
            // 接続を閉じる。閉じている間に別の処理が来ても、ロックしているので入れない。
            let closing = std::mem::replace(
                &mut *slot,
                Slot::Failed(AppError::DbUnavailable {
                    reason: "バックアップから復元しています".to_owned(),
                }),
            );
            if let Slot::Open(conn) = closing
                && let Err((_, err)) = conn.close()
            {
                tracing::warn!(error = %err, "復元の前に接続を閉じられませんでした");
            }

            let restored = bibcontainer_core::backup::restore_backup(
                &backup,
                &paths.db,
                &safety_dir,
                &bibcontainer_core::SystemClock,
            );
            match &restored {
                Ok(report) => tracing::info!(
                    backup = %backup.display(),
                    new_device_id = %report.new_device_id,
                    "バックアップから復元しました。DB を開き直します"
                ),
                Err(err) => tracing::warn!(
                    backup = %backup.display(),
                    error = %err,
                    "復元できませんでした。今の DB を開き直します"
                ),
            }

            // 結果によらず開き直す。
            *slot = open_slot(&paths.db, &paths.migration_backups);
            if let Slot::Failed(err) = &*slot {
                return Err(err.clone());
            }
            restored.map_err(AppError::from)
        })
        .await
        .map_err(|err| AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}")))?
    }
}

/// DB を開いて、接続の状態にする。失敗は理由を保持する。
fn open_slot(db_path: &Path, backup_dir: &Path) -> Slot {
    match bibcontainer_core::open(db_path, backup_dir) {
        Ok((conn, report)) => {
            tracing::info!(
                db_path = %db_path.display(),
                schema_version = report.to_version,
                applied = ?report.applied,
                backup = ?report.backup_path,
                "DB を開きました"
            );
            Slot::Open(conn)
        }
        Err(err) => {
            tracing::error!(db_path = %db_path.display(), error = %err, "DB を開けませんでした");
            Slot::Failed(AppError::from_open(err))
        }
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
            db.failure().as_ref().map(AppError::kind),
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

    fn add_book(db: &Arc<Database>, title: &'static str) -> String {
        block_on(db.run(move |conn, _| {
            let env = bibcontainer_core::library::Env::new(&bibcontainer_core::SystemClock);
            Ok(bibcontainer_core::library::create_book(
                conn,
                &env,
                &[bibcontainer_core::library::FieldChange::manual(
                    "title",
                    serde_json::json!(title),
                )],
            )?)
        }))
        .unwrap()
    }

    fn titles(db: &Arc<Database>) -> Vec<String> {
        block_on(db.run(|conn, _| {
            Ok(bibcontainer_core::list_rows(conn)?
                .into_iter()
                .map(|r| r.title)
                .collect())
        }))
        .unwrap()
    }

    fn device_id(db: &Arc<Database>) -> String {
        block_on(db.run(|conn, _| Ok(bibcontainer_core::meta::device_id(conn)?))).unwrap()
    }

    fn manual_backup(db: &Arc<Database>, dir: PathBuf) -> PathBuf {
        let info = block_on(db.run(move |conn, _| {
            Ok(bibcontainer_core::backup::manual_backup(
                conn,
                &dir,
                &bibcontainer_core::SystemClock,
            )?)
        }))
        .unwrap();
        PathBuf::from(info.path)
    }

    #[test]
    fn restore_replaces_the_db_and_reopens_it() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "バックアップ時点の本");
        let backup = manual_backup(&db, backup_dir.clone());
        add_book(&db, "あとから足した本");
        let old_device = device_id(&db);
        assert_eq!(titles(&db).len(), 2);

        let safety = dir.path().join("safety");
        let report = block_on(db.restore(backup, safety.clone())).unwrap();

        assert_eq!(titles(&db), ["バックアップ時点の本"]);
        // 新しい端末 ID が振られ、開き直した接続でも書き込める（0005 §2）
        assert_ne!(device_id(&db), old_device);
        assert_eq!(device_id(&db), report.new_device_id);
        add_book(&db, "復元後の本");
        assert_eq!(titles(&db).len(), 2);
        // 今の DB は、置き換える前に残される
        let kept = report.pre_restore_path.expect("置き換え前の DB を残す");
        assert!(Path::new(&kept).starts_with(&safety), "{kept}");
        assert!(db.failure().is_none());
    }

    #[test]
    fn a_rejected_backup_leaves_the_current_db_open_and_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "いまの本");
        let before = device_id(&db);

        let garbage = dir.path().join("garbage.sqlite3");
        std::fs::write(&garbage, "SQLite ではないファイル").unwrap();
        let err = block_on(db.restore(garbage, dir.path().join("safety"))).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidBackup);

        let missing = dir.path().join("none.sqlite3");
        let err = block_on(db.restore(missing, dir.path().join("safety"))).unwrap_err();
        assert!(
            matches!(err.kind(), ErrorKind::InvalidBackup | ErrorKind::Io),
            "{err}"
        );

        assert!(db.failure().is_none(), "開き直して使える");
        assert_eq!(titles(&db), ["いまの本"]);
        assert_eq!(device_id(&db), before, "端末 ID も変わらない");
    }

    #[test]
    fn a_db_that_failed_to_open_can_be_replaced_by_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        // 別の場所で作った、正常なバックアップ
        let good_dir = dir.path().join("good");
        let good = Arc::new(Database::open(
            &good_dir.join(DB_FILE_NAME),
            &good_dir.join(BACKUP_DIR_NAME),
        ));
        add_book(&good, "正常なデータ");
        let backup = manual_backup(&good, good_dir.join(BACKUP_DIR_NAME));

        // 新しすぎるスキーマ版の DB は開けない
        let (db_path, backup_dir) = paths(&dir.path().join("app"));
        let (conn, _) = bibcontainer_core::open(&db_path, &backup_dir).unwrap();
        conn.pragma_update(None, "user_version", 999).unwrap();
        drop(conn);
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        assert_eq!(
            db.failure().as_ref().map(AppError::kind),
            Some(ErrorKind::SchemaTooNew)
        );

        block_on(db.restore(backup, dir.path().join("safety"))).unwrap();
        assert!(db.failure().is_none());
        assert_eq!(titles(&db), ["正常なデータ"]);
    }

    #[test]
    fn restoring_without_a_known_db_location_is_an_error() {
        let db = Arc::new(Database::failed(AppError::DbUnavailable {
            reason: "場所がない".into(),
        }));
        let err = block_on(db.restore("x".into(), "y".into())).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DbUnavailable);
    }
}
