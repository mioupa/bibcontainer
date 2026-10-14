//! bibcontainer の Tauri アプリ本体（殻）。
//!
//! 業務ロジックは Tauri に依存しない `bibcontainer-core` に置き、ここでは
//! ウィンドウ・IPC コマンド・ログの初期化など、アプリとしての配線だけを行う。

mod backups;
mod commands;
mod database;
mod error;
mod events;
mod logging;
mod metadata;
mod settings;

use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};

use database::{BACKUP_DIR_NAME, DB_FILE_NAME, Database};
use error::AppError;

pub fn run() {
    // `cover` URI スキーム（書影。0005 §7）は、Builder に登録する
    metadata::register_cover_protocol(tauri::Builder::default())
        // ファイルの選択・保存先の指定（JS から使う）と、フォルダを OS で開く（open_logs_dir など）
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
            let db = Arc::new(db);
            app.manage(Arc::clone(&db));

            // 書誌取得の状態（取得元の設定・キー・書影）を作り、取得待ちの自動再取得を始める
            // （起動の 5 秒後から 5 分ごと）。失敗しても起動は続ける（書誌の取得だけが使えない）。
            if let Err(err) = metadata::setup(app.handle()) {
                tracing::error!(error = %err, "書誌取得の準備に失敗しました");
            }

            // 日次バックアップは、起動の数秒後と 1 時間ごとに「時期か」を確かめて取る。
            backups::start_scheduler(app.handle().clone(), db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            // 一覧・検索・参照
            commands::library::list_books,
            commands::library::get_rows,
            commands::library::get_book_detail,
            commands::library::search_books,
            commands::library::find_by_isbn,
            commands::library::purchase_store_suggestions,
            commands::library::pending_book_ids,
            // 書籍の変更
            commands::books::create_book,
            commands::books::create_books,
            commands::books::update_book,
            commands::books::update_books,
            commands::books::modify_tags,
            commands::books::trash_books,
            commands::books::restore_books,
            commands::books::purge_books,
            // 保管場所・タグ・シリーズ
            commands::organize::list_locations,
            commands::organize::create_location,
            commands::organize::rename_location,
            commands::organize::move_location,
            commands::organize::reorder_locations,
            commands::organize::delete_location,
            commands::organize::list_tags,
            commands::organize::rename_tag,
            commands::organize::set_tag_color,
            commands::organize::list_series,
            commands::organize::find_or_create_series,
            commands::organize::rename_series,
            // バーコード
            commands::barcode::classify_code,
            commands::barcode::c_code_table,
            commands::barcode::describe_c_code,
            // 設定
            commands::settings::get_scan_settings,
            commands::settings::set_scan_settings,
            commands::settings::get_display_settings,
            commands::settings::set_display_settings,
            commands::settings::get_list_settings,
            commands::settings::set_list_settings,
            commands::settings::get_price_settings,
            commands::settings::set_price_settings,
            commands::settings::get_backup_settings,
            commands::settings::set_backup_settings,
            // 書き出し・取り込み
            commands::io::export_csv,
            commands::io::export_json,
            commands::io::parse_isbn_list,
            // バックアップ
            commands::backup::list_backups,
            commands::backup::manual_backup,
            commands::backup::restore_backup,
            // フォルダを開く
            commands::system::open_logs_dir,
            commands::system::open_backup_dir,
            // 書誌取得（0010）
            metadata::commands::get_provider_settings,
            metadata::commands::set_provider_settings,
            metadata::commands::list_providers,
            metadata::commands::test_provider,
            metadata::commands::set_api_key,
            metadata::commands::delete_api_key,
            metadata::commands::has_api_key,
            metadata::commands::fetch_metadata,
            metadata::commands::create_books_from_drafts,
            metadata::commands::refetch_books,
            metadata::commands::trigger_pending_refetch,
            metadata::commands::import_cover_file,
            metadata::commands::import_cover_bytes,
            metadata::commands::import_cover_url,
        ])
        .build(tauri::generate_context!())
        .expect("Tauri アプリの構築に失敗しました")
        .run(|app, event| {
            // ウィンドウが前面に来たら、取得待ちの本を試す（30 秒以内の再実行は metadata 側で抑える）
            if let RunEvent::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } = &event
            {
                metadata::trigger_pending_refetch(app);
            }
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
