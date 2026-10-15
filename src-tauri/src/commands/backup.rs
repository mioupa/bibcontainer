//! バックアップの一覧・作成・復元（FR-IO-05）。保存先と世代数は設定の `backup` セクション。

use std::path::PathBuf;
use std::sync::Arc;

use bibcontainer_core::SystemClock;
use bibcontainer_core::backup::{self, BackupInfo, RestoreReport};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::backups::{configured, default_backup_dir};
use crate::database::Database;
use crate::error::AppError;
use crate::events::emit_library_changed;
use crate::sync::SyncController;

/// バックアップの一覧（新しい順）。保存先のフォルダがまだなければ空。
#[tauri::command]
pub async fn list_backups<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
) -> Result<Vec<BackupInfo>, AppError> {
    let default_dir = default_backup_dir(&app)?;
    db.run(move |conn, _| {
        let (dir, _) = configured(conn, &default_dir)?;
        Ok(backup::list_backups(&dir)?)
    })
    .await
}

/// 手動でバックアップを取る（世代の整理の対象外）。アプリの接続とは別の読み取り専用の接続で取るので、
/// 取っている間も、ほかの操作は待たされない。
#[tauri::command]
pub async fn manual_backup<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
) -> Result<BackupInfo, AppError> {
    let default_dir = default_backup_dir(&app)?;
    db.run_backup(move |conn| {
        let (dir, _) = configured(conn, &default_dir)?;
        Ok(backup::manual_backup(conn, &dir, &SystemClock)?)
    })
    .await
}

/// バックアップから復元する。接続を閉じて DB を置き換え、開き直す（復元前の DB は保存先に
/// `pre-restore-…` として残る）。復元した DB には新しい端末 ID が振られる。
///
/// 開き直したあと `library-changed`（`others: true`）を出す。一覧の全行が入れ替わるので、呼び出し側は
/// 全データを取り直すこと。
#[tauri::command]
pub async fn restore_backup<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    path: String,
) -> Result<RestoreReport, AppError> {
    let default_dir = default_backup_dir(&app)?;
    // 今の DB を残す場所。DB を開けていなくて設定が読めないときは、既定の保存先にする。
    let safety_dir = {
        let for_settings = default_dir.clone();
        db.run(move |conn, _| Ok(configured(conn, &for_settings)?.0))
            .await
            .unwrap_or(default_dir)
    };
    // 同期が動いている間は、DB を置き換えない（取り込み・書き出しの途中の DB を入れ替えないため）。
    let sync = app
        .try_state::<Arc<SyncController>>()
        .map(|s| Arc::clone(&s));
    let guard = match &sync {
        Some(sync) => Some(sync.exclusive().await),
        None => None,
    };
    let report = db.restore(PathBuf::from(path), safety_dir).await;
    drop(guard);
    let report = report?;
    emit_library_changed(&app, Vec::new(), true);
    // 復元した DB は、新しい端末 ID と「スナップショットが必要」の印を持っている。同期しているなら、すぐ同期し直す
    // （古い端末 ID の変更と他の端末の変更を取り込み直す）。ネットワークを待たせないよう、別のタスクで行う。
    if let Some(sync) = sync {
        tauri::async_runtime::spawn(async move { sync.after_restore().await });
    }
    Ok(report)
}
