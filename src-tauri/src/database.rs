//! アプリが持つ DB 接続。
//!
//! DB を開けなくてもアプリは落とさない。開けなかった理由を保持しておき、DB に触る
//! コマンドが呼ばれるたびにそのエラーを返す（フロントはそれを見てエラー画面を出す）。
//!
//! 接続は 1 本を Mutex で共有する。バックアップからの復元（`restore`）は、その Mutex を持ったまま
//! 接続を閉じ、DB のファイルを置き換えて開き直す。復元中の `run` は、終わるまで待ってから新しい
//! 接続で実行される。
//!
//! バックアップ（`VACUUM INTO`。5,000 冊で 0.3 秒、2 万冊で 1.2 秒ほどかかる）は、この Mutex を持たず、
//! 別に開いた読み取り専用の接続で行う（`run_backup`）。WAL なので、バックアップ中も検索・一覧・保存は
//! 待たされない。バックアップの接続は DB のファイルを直接開くので、復元（ファイルの置き換え）とは
//! `backup_gate` で重ならないようにする。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bibcontainer_core::backup::{self, RestoreReport};
use rusqlite::Connection;
use serde_json::Value;

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
    /// バックアップ（別の接続で DB のファイルを読む）と復元（ファイルを置き換える）、バックアップどうしが
    /// 重ならないようにする。ふつうの `run` は取らない。取る順は、この門 → `slot`。
    backup_gate: Mutex<()>,
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
            backup_gate: Mutex::new(()),
        }
    }

    /// 開けなかった状態を作る。
    pub fn failed(error: AppError) -> Self {
        Self {
            paths: None,
            slot: Mutex::new(Slot::Failed(error)),
            backup_gate: Mutex::new(()),
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
        tauri::async_runtime::spawn_blocking(move || this.run_blocking(f))
            .await
            .map_err(|err| {
                AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}"))
            })?
    }

    /// `run` の、呼んだスレッドでそのまま実行する版。接続の Mutex を借りて `f` を実行する。
    ///
    /// ブロックするので、async の中からは呼ばない。すでに別のスレッドで動いている処理（同期エンジンの
    /// `spawn_blocking`）が、DB の接続を短く借りるために使う（`sync::db::AppSyncDb`）。
    /// 開けなかった場合は、保持しているエラーをそのまま返す。
    pub fn run_blocking<T, F>(&self, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&mut Connection, &Path) -> Result<T, AppError>,
    {
        self.with_connection(f)?
    }

    /// 接続を借りて `f` を実行し、`f` の値をそのまま返す（`f` が失敗を返す型でなくてもよい）。
    /// 開けなかった場合だけ、保持しているエラーを返す。
    pub fn with_connection<T>(
        &self,
        f: impl FnOnce(&mut Connection, &Path) -> T,
    ) -> Result<T, AppError> {
        let mut slot = self.lock();
        match &mut *slot {
            Slot::Failed(err) => Err(err.clone()),
            Slot::Open(conn) => {
                let path = self.paths.as_ref().map_or_else(|| Path::new(""), |p| &p.db);
                Ok(f(conn, path))
            }
        }
    }

    /// バックアップを、アプリの接続とは別に開いた読み取り専用の接続で行う。
    ///
    /// `f` には、その読み取り専用の接続を渡す（`create_backup` などに渡す。設定の読み出しにも使える）。
    /// 接続の Mutex は持たないので、`f` の間も `run` は待たされない（WAL。5,000 冊で約 0.3 秒、
    /// 2 万冊で約 1.2 秒の `VACUUM INTO` が、検索や一覧の取り直しを止めない）。`VACUUM INTO` は 1 つの
    /// 読み取りトランザクションなので、バックアップは始まった時点のスナップショットになる。
    ///
    /// 復元・ほかのバックアップとは重ならない（先に始まったほうが終わるまで待つ）。
    /// DB を開けなかったときは、`run` と同じく保持しているエラーを返す。
    pub async fn run_backup<T, F>(self: &Arc<Self>, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
        T: Send + 'static,
    {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let _gate = this
                .backup_gate
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            // 開けているかだけを見る。接続の Mutex は、この確認の間しか持たない。
            if let Slot::Failed(err) = &*this.lock() {
                return Err(err.clone());
            }
            let paths = this.paths.as_ref().ok_or_else(|| AppError::DbUnavailable {
                reason: "データベースの場所を決められないため、バックアップできません".to_owned(),
            })?;
            let reader = backup::open_reader(&paths.db)?;
            f(&reader)
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
            // 実行中のバックアップが終わるのを待ち、復元の間はバックアップを始めさせない。
            let _gate = this
                .backup_gate
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
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

/// `Database::reset_keeping_settings` の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetReport {
    /// 作り直す前の DB のバックアップ（手動バックアップ）。
    pub backup_path: String,
    /// 作り直す前の端末 ID。
    pub old_device_id: String,
    /// 新しい DB の端末 ID。
    pub new_device_id: String,
    /// 引き継いだ設定（`local_settings` の行）の数。
    pub kept_settings: usize,
}

impl Database {
    /// 今の DB を手動バックアップしてから、新しい空の DB（新しい端末 ID）に作り直す。同期の「置き換えて参加」で使う
    /// （0016）。端末ごとの設定（`local_settings` の全行）は新しい DB に引き継ぐ。API キーは OS のキーチェーンなので
    /// 影響しない。書影のフォルダはそのまま（使われなくなった分は、書影の整理で消せる）。
    ///
    /// 接続の Mutex を持ったまま、バックアップ → 接続を閉じる → 新しい DB を別のファイルに作る → 置き換える →
    /// 開き直す、を行う（復元と同じ。実行中の `run` は待たされ、新しい接続で動く）。バックアップに失敗したら、
    /// 何も変えない。途中で失敗しても、結果によらず開き直す（古い DB のままのことが多い）。
    ///
    /// ブロックするので、async の中からは呼ばない（同期エンジンの `spawn_blocking` から呼ぶ）。
    pub fn reset_keeping_settings(&self, backup_dir: &Path) -> Result<ResetReport, AppError> {
        let paths = self.paths.as_ref().ok_or_else(|| AppError::DbUnavailable {
            reason: "データベースの場所を決められないため、作り直せません".to_owned(),
        })?;
        let _gate = self
            .backup_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut slot = self.lock();
        // 先に、今の接続でバックアップと設定の読み出しを済ませる。ここで失敗したら何も変えない。
        let (backup_path, old_device_id, settings) = match &*slot {
            Slot::Failed(err) => return Err(err.clone()),
            Slot::Open(conn) => {
                let info =
                    backup::manual_backup(conn, backup_dir, &bibcontainer_core::SystemClock)?;
                (
                    info.path,
                    bibcontainer_core::meta::device_id(conn)?,
                    bibcontainer_core::settings::all(conn)?,
                )
            }
        };
        let closing = std::mem::replace(
            &mut *slot,
            Slot::Failed(AppError::DbUnavailable {
                reason: "データベースを作り直しています".to_owned(),
            }),
        );
        if let Slot::Open(conn) = closing
            && let Err((_, err)) = conn.close()
        {
            tracing::warn!(error = %err, "作り直しの前に接続を閉じられませんでした");
        }

        let rebuilt = replace_with_empty_db(paths, &settings);
        match &rebuilt {
            Ok(new_id) => tracing::info!(
                backup = %backup_path,
                old_device_id = %old_device_id,
                new_device_id = %new_id,
                kept_settings = settings.len(),
                "DB を作り直しました（置き換えて参加）。開き直します"
            ),
            Err(err) => {
                tracing::warn!(error = %err, "DB を作り直せませんでした。今の DB を開き直します")
            }
        }
        // 結果によらず開き直す。
        *slot = open_slot(&paths.db, &paths.migration_backups);
        if let Slot::Failed(err) = &*slot {
            return Err(err.clone());
        }
        Ok(ResetReport {
            backup_path,
            old_device_id,
            new_device_id: rebuilt?,
            kept_settings: settings.len(),
        })
    }
}

/// 新しい空の DB を別のファイルに作って、設定を入れ、今の DB のファイルと置き換える。新しい端末 ID を返す。
/// 呼び出し側は、DB の接続をすべて閉じてから呼ぶ。
fn replace_with_empty_db(
    paths: &Paths,
    settings: &BTreeMap<String, Value>,
) -> Result<String, AppError> {
    let scratch = sidecar(&paths.db, ".reset-tmp");
    remove_db_files(&scratch)?;
    let built = (|| -> Result<String, AppError> {
        let (conn, _) = bibcontainer_core::open(&scratch, &paths.migration_backups)?;
        for (key, value) in settings {
            bibcontainer_core::settings::set(&conn, key, value)?;
        }
        let id = bibcontainer_core::meta::device_id(&conn)?;
        // 1 つのファイルにまとめて閉じる（`open` が開くときに WAL に切り替える）。
        conn.pragma_update(None, "journal_mode", "DELETE")
            .map_err(bibcontainer_core::Error::from)?;
        conn.close()
            .map_err(|(_, err)| bibcontainer_core::Error::from(err))?;
        Ok(id)
    })();
    let new_id = match built {
        Ok(id) => id,
        Err(err) => {
            let _ = remove_db_files(&scratch);
            return Err(err);
        }
    };
    // 古い WAL を新しい DB に重ねないよう、置き換える前に消す。
    for suffix in ["-wal", "-shm"] {
        remove_if_exists(&sidecar(&paths.db, suffix))?;
    }
    std::fs::rename(&scratch, &paths.db)?;
    Ok(new_id)
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_if_exists(path: &Path) -> Result<(), AppError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// DB のファイルと、WAL・共有メモリのファイルを消す（なければ何もしない）。
fn remove_db_files(path: &Path) -> Result<(), AppError> {
    remove_if_exists(path)?;
    remove_if_exists(&sidecar(path, "-wal"))?;
    remove_if_exists(&sidecar(path, "-shm"))
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

    // ---------- バックアップは、アプリの接続とは別の接続で行う ----------

    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    const WAIT: Duration = Duration::from_secs(20);

    fn sql<T>(result: rusqlite::Result<T>) -> Result<T, AppError> {
        Ok(result.map_err(bibcontainer_core::Error::from)?)
    }

    /// `run_backup` の中の処理（`f`）を別のスレッドで走らせ、`f` が始まるまで待つ。
    fn start_backup_thread(
        db: &Arc<Database>,
        f: impl FnOnce(&Connection) -> Result<(), AppError> + Send + 'static,
    ) -> std::thread::JoinHandle<Result<(), AppError>> {
        let (started_tx, started_rx) = mpsc::channel();
        let db = Arc::clone(db);
        let handle = std::thread::spawn(move || {
            block_on(db.run_backup(move |conn| {
                started_tx.send(()).unwrap();
                f(conn)
            }))
        });
        started_rx
            .recv_timeout(WAIT)
            .expect("バックアップの処理が始まらない");
        handle
    }

    #[test]
    fn work_on_the_app_connection_is_not_blocked_while_a_backup_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "最初の本");

        // バックアップの処理が動いている間に、アプリの接続での処理（読み取りも書き込みも）が終わる。
        // 終わるまでバックアップの処理は待つので、もしバックアップが接続の Mutex を持っていると、
        // `run` が終わらず、タイムアウトで失敗する。
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (checked_tx, checked_rx) = mpsc::channel::<()>();
        let backup = start_backup_thread(&db, move |conn| {
            // バックアップの接続は、DB を読めるが、書けない
            let n: i64 = sql(conn.query_row("SELECT count(*) FROM books", [], |r| r.get(0)))?;
            assert_eq!(n, 1);
            assert!(conn.execute("DELETE FROM books", []).is_err());
            checked_tx.send(()).unwrap();
            release_rx
                .recv_timeout(WAIT)
                .map_err(|_| AppError::Internal("アプリの接続の処理が待たされた".to_owned()))
        });
        // 上の数え上げより先に本を足すと 2 冊に見えるので、確認が済むまで待つ
        // （読み取りの時点は接続を開いたときではなく、文ごとに決まる）
        checked_rx
            .recv_timeout(WAIT)
            .expect("バックアップの接続での確認が終わらない");

        let started = Instant::now();
        assert_eq!(titles(&db), ["最初の本"]);
        add_book(&db, "バックアップ中に足した本");
        assert_eq!(titles(&db).len(), 2);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "待たされている: {:?}",
            started.elapsed()
        );
        release_tx.send(()).unwrap();
        backup.join().unwrap().unwrap();
    }

    #[test]
    fn a_backup_on_its_own_connection_is_a_complete_snapshot_and_other_work_keeps_running() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "最初の本");
        // `VACUUM INTO` に時間がかかるよう、大きめの行を足す
        block_on(db.run(|conn, _| {
            sql(conn.execute(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 40000)
                 INSERT INTO books(id, title, description)
                 SELECT printf('fill-%08d', i), 'タイトル ' || i, printf('%.*c', 400, 'x') FROM n",
                [],
            ))?;
            Ok(())
        }))
        .unwrap();

        let (done_tx, done_rx) = mpsc::channel();
        let dir_for_backup = backup_dir.clone();
        let started = Instant::now();
        let backup = {
            let db = Arc::clone(&db);
            std::thread::spawn(move || {
                let result = block_on(db.run_backup(move |conn| {
                    Ok(backup::manual_backup(
                        conn,
                        &dir_for_backup,
                        &bibcontainer_core::SystemClock,
                    )?)
                }));
                done_tx.send(()).unwrap();
                result
            })
        };

        // バックアップが終わるまで、アプリの接続で保存を続ける（1 回の待ちの長さを測る）
        let (mut writes, mut slowest) = (0_u32, Duration::ZERO);
        while done_rx.try_recv().is_err() {
            let t = Instant::now();
            add_book(&db, "バックアップ中に足した本");
            slowest = slowest.max(t.elapsed());
            writes += 1;
            assert!(started.elapsed() < WAIT, "バックアップが終わらない");
        }
        let backup_took = started.elapsed();
        let info = backup.join().unwrap().unwrap();

        assert!(writes > 0, "バックアップの間に 1 回も保存できなかった");
        // 接続を塞ぐ実装だと、最初の 1 回がバックアップが終わるまで待たされ、バックアップの長さに近くなる
        assert!(
            slowest * 2 < backup_took,
            "保存 1 回の最長 {slowest:?}、バックアップ全体 {backup_took:?}"
        );
        // バックアップは完全な DB（保存の途中で切れた状態ではない）
        let check = bibcontainer_core::backup::verify_backup(Path::new(&info.path)).unwrap();
        assert_eq!(check.schema_version, migrate::LATEST_VERSION);
        // いつの時点のスナップショットでも、足した本（1 冊 1 トランザクション）が欠けず、本の行は整合する
        let snapshot = rusqlite::Connection::open(&info.path).unwrap();
        let (books, indexed): (i64, i64) = snapshot
            .query_row(
                "SELECT (SELECT count(*) FROM books), (SELECT count(*) FROM book_idx)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(books >= 40_001, "{books}");
        // 検索用の派生データは、本を作るのと同じトランザクションで書く。40,000 行はそれを通さず入れたので、その分だけ少ない
        assert!(indexed <= books && indexed >= 1, "{indexed} / {books}");
    }

    #[test]
    fn a_restore_waits_for_a_running_backup_and_a_backup_waits_for_a_restore() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "いまの本");

        let (release_tx, release_rx) = mpsc::channel::<()>();
        let backup = start_backup_thread(&db, move |_| {
            release_rx
                .recv_timeout(WAIT)
                .map_err(|_| AppError::Internal("復元がバックアップを待たなかった".to_owned()))
        });

        // 復元は、実行中のバックアップが終わるまで始まらない
        let restore = {
            let db = Arc::clone(&db);
            let garbage = dir.path().join("garbage.sqlite3");
            std::fs::write(&garbage, "SQLite ではないファイル").unwrap();
            let safety = dir.path().join("safety");
            std::thread::spawn(move || block_on(db.restore(garbage, safety)))
        };
        std::thread::sleep(Duration::from_millis(300));
        assert!(!restore.is_finished(), "バックアップの途中で復元が進んだ");
        // 復元が待っている間も、ふつうの処理は止まらない（復元は、門を取ってから接続の Mutex を取る）
        assert_eq!(titles(&db), ["いまの本"]);

        release_tx.send(()).unwrap();
        backup.join().unwrap().unwrap();
        let err = restore.join().unwrap().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidBackup);
        assert!(db.failure().is_none());

        // 復元が終われば、バックアップはまた取れる
        let info = block_on(db.run_backup(move |conn| {
            Ok(backup::manual_backup(
                conn,
                &backup_dir,
                &bibcontainer_core::SystemClock,
            )?)
        }))
        .unwrap();
        assert_eq!(
            bibcontainer_core::backup::verify_backup(Path::new(&info.path))
                .unwrap()
                .book_count,
            1
        );
    }

    #[test]
    fn a_backup_of_a_db_that_failed_to_open_returns_the_stored_error() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let (conn, _) = bibcontainer_core::open(&db_path, &backup_dir).unwrap();
        conn.pragma_update(None, "user_version", 999).unwrap();
        drop(conn);
        let db = Arc::new(Database::open(&db_path, &backup_dir));

        let err = block_on(db.run_backup(|_| Ok(()))).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::SchemaTooNew);
        let unknown = Arc::new(Database::failed(AppError::DbUnavailable {
            reason: "場所がない".into(),
        }));
        let err = block_on(unknown.run_backup(|_| Ok(()))).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DbUnavailable);
    }

    #[test]
    fn restoring_without_a_known_db_location_is_an_error() {
        let db = Arc::new(Database::failed(AppError::DbUnavailable {
            reason: "場所がない".into(),
        }));
        let err = block_on(db.restore("x".into(), "y".into())).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DbUnavailable);
    }
    fn set_setting(db: &Arc<Database>, key: &'static str, value: Value) {
        block_on(db.run(move |conn, _| Ok(bibcontainer_core::settings::set(conn, key, &value)?)))
            .unwrap();
    }

    fn get_setting(db: &Arc<Database>, key: &'static str) -> Option<Value> {
        block_on(db.run(move |conn, _| Ok(bibcontainer_core::settings::get(conn, key)?))).unwrap()
    }

    #[test]
    fn reset_backs_up_rebuilds_with_a_new_device_id_and_keeps_every_setting() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "古い本");
        set_setting(&db, "scan", serde_json::json!({ "muted": true }));
        set_setting(
            &db,
            "sync",
            serde_json::json!({ "deviceName": "書斎の Mac" }),
        );
        let old_id = device_id(&db);
        let safety = dir.path().join("reset-backups");

        let report = db.reset_keeping_settings(&safety).unwrap();

        assert_eq!(report.old_device_id, old_id);
        assert_ne!(report.new_device_id, old_id);
        assert_eq!(device_id(&db), report.new_device_id);
        assert_eq!(report.kept_settings, 2);
        assert!(titles(&db).is_empty(), "本は空になる");
        assert_eq!(
            get_setting(&db, "scan"),
            Some(serde_json::json!({ "muted": true }))
        );
        assert_eq!(
            get_setting(&db, "sync"),
            Some(serde_json::json!({ "deviceName": "書斎の Mac" }))
        );
        // 作り直す前の DB が、手動バックアップとして残っている
        let info = backup::list_backups(&safety).unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].kind, backup::BackupKind::Manual);
        assert_eq!(report.backup_path, info[0].path);
        assert_eq!(
            backup::verify_backup(Path::new(&info[0].path))
                .unwrap()
                .book_count,
            1
        );
        // 作業用のファイルは残らず、開き直した DB は書ける
        assert!(!sidecar(&db_path, ".reset-tmp").exists());
        add_book(&db, "新しい本");
        assert_eq!(titles(&db), ["新しい本"]);
        // 作り直しのあとの再起動でも、新しい端末 ID のまま開く
        drop(db);
        let reopened = Arc::new(Database::open(&db_path, &backup_dir));
        assert_eq!(device_id(&reopened), report.new_device_id);
        assert_eq!(titles(&reopened), ["新しい本"]);
    }

    #[test]
    fn reset_changes_nothing_when_the_backup_cannot_be_made() {
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let db = Arc::new(Database::open(&db_path, &backup_dir));
        add_book(&db, "古い本");
        let old_id = device_id(&db);
        // 保存先のパスが、フォルダではなくファイル
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();

        let err = db.reset_keeping_settings(&blocker.join("sub")).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Io);
        assert_eq!(titles(&db), ["古い本"]);
        assert_eq!(device_id(&db), old_id);
        assert!(db.failure().is_none());
    }

    #[test]
    fn reset_without_a_usable_db_is_an_error() {
        let unknown = Database::failed(AppError::DbUnavailable {
            reason: "場所がない".into(),
        });
        let err = unknown
            .reset_keeping_settings(Path::new("backups"))
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DbUnavailable);

        // DB を開けなかった（スキーマ版が新しい）ときは、バックアップできないので、作り直さない
        let dir = tempfile::tempdir().unwrap();
        let (db_path, backup_dir) = paths(dir.path());
        let (conn, _) = bibcontainer_core::open(&db_path, &backup_dir).unwrap();
        conn.pragma_update(None, "user_version", 999).unwrap();
        drop(conn);
        let too_new = Database::open(&db_path, &backup_dir);
        let err = too_new
            .reset_keeping_settings(&dir.path().join("safety"))
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::SchemaTooNew);
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        assert_eq!(migrate::user_version(&conn).unwrap(), 999);
    }
}
