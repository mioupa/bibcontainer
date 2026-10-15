//! 同期の IPC コマンド（0013 §9、0016）。
//!
//! すべて async。中身は `SyncController`（`controller.rs`）にあり、ここは薄い口。状態の変化は、コマンドの戻り値と
//! `sync-status` イベントの両方で伝わる。蔵書が他の端末の変更で変わったときは `library-changed` が出る。
//!
//! | コマンド | 戻り値 |
//! |---|---|
//! | `get_sync_status` | `SyncStatus` |
//! | `inspect_sync_folder(path)` | `FolderInspection`（何も残さない） |
//! | `setup_sync(path, mode)` | 設定後の `SyncStatus`（最初の取り込みが済むまで待つ） |
//! | `leave_sync` | 解除後の `SyncStatus` |
//! | `sync_now` | 同期後の `SyncStatus`（失敗したらエラー。状態にも記録される） |
//! | `set_device_name(name)` | 変更後の `SyncStatus`（空・65 文字以上は `validation`） |
//! | `gc_covers` | 消した書影の数（`number`） |

use std::sync::Arc;

use bibcontainer_sync::{FolderInspection, SetupMode, SyncStatus};
use tauri::State;

use super::SyncController;
use crate::error::AppError;

#[tauri::command]
pub async fn get_sync_status(sync: State<'_, Arc<SyncController>>) -> Result<SyncStatus, AppError> {
    sync.status().await
}

#[tauri::command]
pub async fn inspect_sync_folder(
    sync: State<'_, Arc<SyncController>>,
    path: String,
) -> Result<FolderInspection, AppError> {
    sync.inspect(path).await
}

#[tauri::command]
pub async fn setup_sync(
    sync: State<'_, Arc<SyncController>>,
    path: String,
    mode: SetupMode,
) -> Result<SyncStatus, AppError> {
    sync.setup(path, mode).await
}

#[tauri::command]
pub async fn leave_sync(sync: State<'_, Arc<SyncController>>) -> Result<SyncStatus, AppError> {
    sync.leave().await
}

#[tauri::command]
pub async fn sync_now(sync: State<'_, Arc<SyncController>>) -> Result<SyncStatus, AppError> {
    sync.sync_now().await
}

#[tauri::command]
pub async fn set_device_name(
    sync: State<'_, Arc<SyncController>>,
    name: String,
) -> Result<SyncStatus, AppError> {
    sync.set_device_name(name).await
}

#[tauri::command]
pub async fn gc_covers(sync: State<'_, Arc<SyncController>>) -> Result<u32, AppError> {
    sync.gc_covers().await
}
