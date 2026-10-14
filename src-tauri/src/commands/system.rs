//! ログ・バックアップのフォルダを OS のファイラで開く。

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::backups::{configured, default_backup_dir};
use crate::database::Database;
use crate::error::AppError;

/// フォルダがなければ作ってから、OS で開く。
fn open_dir<R: Runtime>(app: &AppHandle<R>, dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|err| AppError::Io(format!("フォルダを開けませんでした: {err}")))
}

/// ログのフォルダを開く。
#[tauri::command]
pub async fn open_logs_dir<R: Runtime>(app: AppHandle<R>) -> Result<(), AppError> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|err| AppError::Io(format!("ログのフォルダを決められません: {err}")))?;
    open_dir(&app, &dir)
}

/// バックアップの保存先のフォルダを開く（設定の保存先。未設定なら既定の場所）。
#[tauri::command]
pub async fn open_backup_dir<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
) -> Result<(), AppError> {
    let default_dir = default_backup_dir(&app)?;
    let dir = {
        let for_settings = default_dir.clone();
        db.run(move |conn, _| Ok(configured(conn, &for_settings)?.0))
            .await
            .unwrap_or(default_dir)
    };
    open_dir(&app, &dir)
}
