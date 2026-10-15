//! 同期（`crates/sync`）をアプリにつなぐ層。設計の経緯は `docs/decisions/0016-sync-integration.md`
//! （エンジンの使い方は 0014 §11、同期の設計は 0013、画面は 0015）。
//!
//! | ファイル | 内容 |
//! |---|---|
//! | `settings` | 同期の端末ごとの設定（`local_settings` の `sync`。同期フォルダ、端末名） |
//! | `db` | エンジンの `SyncDb` を `Database` の Mutex につなぐ |
//! | `schedule` | いつ動かすかの判定（書き出しのまとめ 5 秒、監視の 2 秒、60 秒ごと、手入れの間隔）。純粋 |
//! | `status` | `SyncStatus` の組み立てと、直近の結果（`Live`）。純粋 |
//! | `watch` | 同期フォルダのファイル監視（`notify`） |
//! | `controller` | managed state。スケジューラ、周期の実行、設定・解除・置き換え・端末名・書影の整理、終了時の書き出し |
//! | `commands` | IPC コマンド |
//!
//! 組み込み（`lib.rs`）:
//! 1. `.setup` の中で、`Arc<Database>` を managed state に入れたあとに `setup(app.handle())` を呼ぶ。
//! 2. `generate_handler!` に `commands::*` の 7 コマンドを足してある。
//! 3. ウィンドウが前面に来たら `trigger_import(app)`、終了要求のときは `on_exit_requested(...)` を呼ぶ。
//! 4. データを変えるコマンドが呼ぶ `events::emit_library_changed` が、書き出しの合図（`notify_local_change`）も送る。
//! 5. `restore_backup` は、同期の `exclusive()` を取ってから DB を置き換え、あとで `after_restore()` を呼ぶ。

pub mod commands;
pub mod controller;
pub mod db;
pub mod schedule;
pub mod settings;
pub mod status;
pub mod watch;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use bibcontainer_core::SystemClock;
use tauri::{AppHandle, ExitRequestApi, Manager, Runtime};

pub use controller::{EXIT_FLUSH_TIMEOUT, Hooks, SharedClock, Signal, SyncConfig, SyncController};

use crate::database::Database;
use crate::error::AppError;
use crate::events;
use crate::metadata::COVERS_DIR_NAME;

/// `sync-status` イベントの名前。ペイロードは `SyncStatus`。
pub const SYNC_STATUS: &str = "sync-status";

/// イベントを出すコントローラを作る（`app` のイベントで、状態と蔵書の変更を知らせる）。
pub fn build_controller<R: Runtime>(
    app: &AppHandle<R>,
    db: Arc<Database>,
    config: SyncConfig,
    clock: SharedClock,
) -> Arc<SyncController> {
    let status_app = app.clone();
    let library_app = app.clone();
    SyncController::new(
        db,
        config,
        clock,
        Hooks {
            status: Box::new(move |status| {
                use tauri::Emitter;
                if let Err(err) = status_app.emit(SYNC_STATUS, status) {
                    tracing::warn!(error = %err, "sync-status を送れませんでした");
                }
            }),
            library_changed: Box::new(move |books, others| {
                events::emit_remote_library_changed(&library_app, books, others);
            }),
        },
    )
}

/// 同期の状態を作って managed state に入れ、スケジューラを始める。
///
/// `Arc<Database>` を managed state に入れたあとで呼ぶ。同期が設定されていれば、起動の少しあとに
/// 取り込み・書き出しをする。同期フォルダが使えなくても、ローカルの操作は止めない。
pub fn setup(app: &AppHandle) -> Result<(), AppError> {
    let local = app
        .path()
        .app_local_data_dir()
        .map_err(|e| AppError::Io(format!("アプリのデータフォルダを決められません: {e}")))?;
    let db = app
        .try_state::<Arc<Database>>()
        .map(|s| Arc::clone(s.inner()))
        .ok_or_else(|| {
            AppError::Internal("DB の状態が先に登録されている必要があります".to_owned())
        })?;
    let config = SyncConfig {
        covers_dir: local.join(COVERS_DIR_NAME),
        default_backup_dir: local.join(crate::database::BACKUP_DIR_NAME),
        os: std::env::consts::OS.to_owned(),
        app_version: app.package_info().version.to_string(),
        default_device_name: settings::default_device_name(),
    };
    let controller = build_controller(app, db, config, Arc::new(SystemClock));
    controller.start();
    app.manage(controller);
    Ok(())
}

/// 自分の変更があった。少し待って、同期フォルダへ書き出す（`events::emit_library_changed` から呼ぶ）。
pub fn notify_local_change<R: Runtime>(app: &AppHandle<R>) {
    if let Some(sync) = app.try_state::<Arc<SyncController>>() {
        sync.signal(Signal::LocalChange);
    }
}

/// ウィンドウが前面に来た。すぐ取り込む（前回から 3 秒はあける）。
pub fn trigger_import<R: Runtime>(app: &AppHandle<R>) {
    if let Some(sync) = app.try_state::<Arc<SyncController>>() {
        sync.signal(Signal::Focus);
    }
}

/// 終了の要求（`RunEvent::ExitRequested`）。同期が設定されていれば、いったん終了を止めて、未書き出しの変更を
/// 最大 5 秒かけて書き出してから、あらためて終了する（FR-SYNC-07）。設定されていなければ、そのまま終了する。
pub fn on_exit_requested<R: Runtime>(app: &AppHandle<R>, api: &ExitRequestApi, code: Option<i32>) {
    let Some(sync) = app
        .try_state::<Arc<SyncController>>()
        .map(|s| Arc::clone(&s))
    else {
        return;
    };
    // 2 回目（書き出しを終えたあとの `exit`）は止めない
    if !sync.begin_exit() || !sync.is_configured() {
        return;
    }
    api.prevent_exit();
    let app = app.clone();
    std::thread::spawn(move || {
        tauri::async_runtime::block_on(sync.flush_on_exit(EXIT_FLUSH_TIMEOUT));
        app.exit(code.unwrap_or(0));
    });
}
