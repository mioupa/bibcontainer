//! 書誌取得（`crates/metadata`）をアプリにつなぐ層。設計の経緯は `docs/decisions/0010-metadata-integration.md`。
//!
//! | ファイル | 内容 |
//! |---|---|
//! | `settings` | 取得元の設定（`local_settings` の `providers`） |
//! | `keychain` | API キーの保存（OS のキーチェーン） |
//! | `cache` | 生の応答のキャッシュ（`provider_cache`） |
//! | `state` | `MetadataService` を持つ managed state。設定やキーが変わると作り直す |
//! | `fetch` | ISBN 1 件の取得（`fetch_metadata`）と、登録フォームの下書き |
//! | `draft` | `BookDraft` への写像、取得状態（`meta_status`）の決め方 |
//! | `register` | 下書きからの書籍の作成（`create_books_from_drafts`） |
//! | `refetch` | 再取得と、取得待ちの自動再取得 |
//! | `providers` | 取得元の一覧・接続テスト・API キーの操作 |
//! | `covers` | 書影の取り込み（ファイル・バイト列・URL） |
//! | `cover_protocol` | `cover` URI スキーム |
//! | `commands` | IPC コマンド |
//!
//! 組み込み（`lib.rs`。0009 §11）:
//! 1. `tauri::Builder` を `register_cover_protocol(...)` に通す。
//! 2. `.setup` の中で、`Arc<Database>` を managed state に入れたあとに `setup(app.handle())` を呼ぶ。
//! 3. `generate_handler!` に `commands::*` のコマンドを足してある。
//! 4. ウィンドウが前面に来たら `trigger_pending_refetch(app)` を呼ぶ。

pub mod cache;
pub mod commands;
pub mod cover_protocol;
pub mod covers;
pub mod draft;
pub mod fetch;
pub mod keychain;
pub mod providers;
pub mod refetch;
pub mod register;
pub mod settings;
pub mod state;

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use bibcontainer_metadata::{ReqwestClient, Transport};
use tauri::{AppHandle, Manager};

pub use cover_protocol::register_cover_protocol;
pub use state::MetadataState;

use crate::database::Database;
use crate::error::AppError;
use keychain::OsKeyStore;
use state::{CoverDirs, Notifier, TransportFactory};

/// 書影の本体を置くフォルダの名前（`app_local_data_dir()` の直下）。
pub const COVERS_DIR_NAME: &str = "covers";
/// サムネイルのキャッシュを置くフォルダの名前（`app_cache_dir()` の直下）。
pub const THUMBS_DIR_NAME: &str = "thumbs";

/// 本番の通信の層（reqwest + rustls（ring）、リトライ・レート制限つき）を作る関数。
pub fn production_transport_factory() -> TransportFactory {
    Arc::new(|user_agent| {
        let client = ReqwestClient::new(user_agent)
            .map_err(|e| AppError::Network(format!("通信の準備に失敗しました: {e}")))?;
        Ok(Arc::new(Transport::new(Arc::new(client))))
    })
}

/// 書誌取得の状態を作って managed state に入れ、取得待ちの自動再取得を始める。
///
/// `Arc<Database>` を managed state に入れたあとで呼ぶ。キーチェーンには、最初の取得のときまで触らない。
pub fn setup(app: &AppHandle) -> Result<(), AppError> {
    let local = app
        .path()
        .app_local_data_dir()
        .map_err(|e| AppError::Io(format!("アプリのデータフォルダを決められません: {e}")))?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Io(format!("アプリのキャッシュフォルダを決められません: {e}")))?;
    let db = app
        .try_state::<Arc<Database>>()
        .map(|s| Arc::clone(s.inner()))
        .ok_or_else(|| {
            AppError::Internal("DB の状態が先に登録されている必要があります".to_owned())
        })?;
    let handle = app.clone();
    let notifier: Notifier = Arc::new(move |books, others| {
        crate::events::emit_library_changed(&handle, books, others);
    });
    let state = MetadataState::new(
        db,
        CoverDirs {
            covers: local.join(COVERS_DIR_NAME),
            thumbs: cache.join(THUMBS_DIR_NAME),
        },
        Arc::new(OsKeyStore),
        app.package_info().version.to_string(),
        production_transport_factory(),
        notifier,
    );
    state.start_background();
    app.manage(state);
    Ok(())
}

/// 取得待ちの本を、今すぐ試す（ウィンドウが前面に来たときに呼ぶ）。すぐ戻る。
/// 前回の試行から 30 秒たっていなければ、何もしない。
pub fn trigger_pending_refetch(app: &AppHandle) {
    if let Some(state) = app.try_state::<Arc<MetadataState>>() {
        state.inner().trigger();
    }
}
