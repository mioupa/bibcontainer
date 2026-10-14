//! 端末ごとの設定（`local_settings`）。型付きのセクションごとに get/set する。
//!
//! `set_*` は範囲に収めた値を保存して、保存した値を返す。`get_*` は、保存された値に欠けた項目があれば
//! 既定値で補って返す。`providers`（書誌の取得元）のセクションは、書誌取得側のモジュールが持つ。

use std::sync::Arc;

use tauri::{AppHandle, Runtime, State};

use crate::backups::default_backup_dir;
use crate::database::Database;
use crate::error::AppError;
use crate::settings::{
    self, BackupSettings, DisplaySettings, ListSettings, PriceSettings, ScanSettings,
};

#[tauri::command]
pub async fn get_scan_settings(db: State<'_, Arc<Database>>) -> Result<ScanSettings, AppError> {
    db.run(|conn, _| settings::load(conn)).await
}

#[tauri::command]
pub async fn set_scan_settings(
    db: State<'_, Arc<Database>>,
    settings: ScanSettings,
) -> Result<ScanSettings, AppError> {
    db.run(move |conn, _| settings::save(conn, settings)).await
}

#[tauri::command]
pub async fn get_display_settings(
    db: State<'_, Arc<Database>>,
) -> Result<DisplaySettings, AppError> {
    db.run(|conn, _| settings::load(conn)).await
}

#[tauri::command]
pub async fn set_display_settings(
    db: State<'_, Arc<Database>>,
    settings: DisplaySettings,
) -> Result<DisplaySettings, AppError> {
    db.run(move |conn, _| settings::save(conn, settings)).await
}

#[tauri::command]
pub async fn get_list_settings(db: State<'_, Arc<Database>>) -> Result<ListSettings, AppError> {
    db.run(|conn, _| settings::load(conn)).await
}

#[tauri::command]
pub async fn set_list_settings(
    db: State<'_, Arc<Database>>,
    settings: ListSettings,
) -> Result<ListSettings, AppError> {
    db.run(move |conn, _| settings::save(conn, settings)).await
}

#[tauri::command]
pub async fn get_price_settings(db: State<'_, Arc<Database>>) -> Result<PriceSettings, AppError> {
    db.run(|conn, _| settings::load(conn)).await
}

#[tauri::command]
pub async fn set_price_settings(
    db: State<'_, Arc<Database>>,
    settings: PriceSettings,
) -> Result<PriceSettings, AppError> {
    db.run(move |conn, _| settings::save(conn, settings)).await
}

/// バックアップの設定。保存先が未設定なら、実際の既定のパス（`app_local_data_dir/backups`）が入る。
#[tauri::command]
pub async fn get_backup_settings<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
) -> Result<BackupSettings, AppError> {
    let default_dir = default_backup_dir(&app)?;
    db.run(move |conn, _| {
        let loaded: BackupSettings = settings::load(conn)?;
        Ok(loaded.for_display(&default_dir))
    })
    .await
}

/// バックアップの設定を保存する。保存先は空（既定の場所）か絶対パス。
#[tauri::command]
pub async fn set_backup_settings<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    settings: BackupSettings,
) -> Result<BackupSettings, AppError> {
    let default_dir = default_backup_dir(&app)?;
    db.run(move |conn, _| {
        let saved = settings::save(conn, settings.for_storage(&default_dir)?)?;
        Ok(saved.for_display(&default_dir))
    })
    .await
}
