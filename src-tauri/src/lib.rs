//! bibcontainer の Tauri アプリ本体（殻）。
//!
//! 業務ロジックは Tauri に依存しない `bibcontainer-core` に置き、ここでは
//! ウィンドウ・IPC コマンド・ログの初期化など、アプリとしての配線だけを行う。

mod commands;
mod database;
mod error;
mod logging;

use std::sync::Arc;

use tauri::{Manager, RunEvent};

use database::{BACKUP_DIR_NAME, DB_FILE_NAME, Database};
use error::AppError;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // ログが出せなくてもアプリは起動する。
            match logging::init(app.handle()) {
                Ok(guard) => {
                    app.manage(guard);
                }
                Err(err) => eprintln!("ログの初期化に失敗しました: {err}"),
            }
            tracing::info!(version = %app.package_info().version, "bibcontainer を起動しました");

            // DB を開けなくても落とさない。理由は state に残し、フロントがエラー画面を出す。
            let db = match app.path().app_local_data_dir() {
                Ok(dir) => Database::open(&dir.join(DB_FILE_NAME), &dir.join(BACKUP_DIR_NAME)),
                Err(err) => {
                    tracing::error!(error = %err, "アプリのデータフォルダを決められません");
                    Database::failed(AppError::DbUnavailable {
                        reason: format!("アプリのデータフォルダを決められません: {err}"),
                    })
                }
            };
            app.manage(Arc::new(db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .build(tauri::generate_context!())
        .expect("Tauri アプリの構築に失敗しました")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // プロセス終了時に、WAL を DB 本体へ書き戻し、バッファに残っているログを書き出す。
                if let Some(db) = app.try_state::<Arc<Database>>() {
                    db.checkpoint();
                }
                if let Some(guard) = app.try_state::<logging::LogGuard>() {
                    guard.flush();
                }
            }
        });
}
