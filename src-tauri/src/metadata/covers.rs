//! 書影の取り込み（FR-COVER-05）。ファイル・バイト列（ドロップ・貼り付け）・URL から、変換して保存し、ハッシュを返す。
//!
//! 本への設定はここでは行わない。フロントが返ってきたハッシュを `update_book` で `cover_hash`（`Manual`）として書く。

use std::path::PathBuf;

use bibcontainer_metadata::http::MAX_IMAGE_BYTES;
use bibcontainer_metadata::{CoverError, FetchError, cover};

use super::state::MetadataState;
use crate::error::AppError;

/// 書影の処理のエラーを、画面に出せる種類に直す。
pub fn cover_error(err: CoverError) -> AppError {
    match err {
        CoverError::Decode(m) => AppError::Validation(format!("画像として読み込めません: {m}")),
        CoverError::TooLarge(m) => AppError::Validation(format!("画像が大きすぎます（{m}）")),
        CoverError::TooSmall { width, height } => {
            AppError::Validation(format!("画像が小さすぎます（{width}x{height}）"))
        }
        CoverError::InvalidUrl(m) => AppError::Validation(format!("URL が正しくありません: {m}")),
        CoverError::Encode(m) => AppError::Internal(format!("画像の変換に失敗しました: {m}")),
        CoverError::Io(m) => AppError::Io(m),
        CoverError::Fetch(FetchError::Network(m)) => AppError::Network(m),
        CoverError::Fetch(FetchError::Http { status, message }) => AppError::Network(format!(
            "画像の取得に失敗しました（HTTP {status}）: {message}"
        )),
        CoverError::Fetch(FetchError::Parse(m)) => AppError::Network(m),
    }
}

impl MetadataState {
    /// 画像ファイルから取り込む。保存した書影のハッシュを返す。
    pub async fn import_cover_file(&self, path: &str) -> Result<String, AppError> {
        let path = PathBuf::from(path.trim());
        if path.as_os_str().is_empty() {
            return Err(AppError::Validation("ファイルのパスが空です".to_owned()));
        }
        let covers = self.dirs.covers.clone();
        let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
            let meta = std::fs::metadata(&path)?;
            if !meta.is_file() {
                return Err(AppError::Validation(format!(
                    "ファイルではありません: {}",
                    path.display()
                )));
            }
            if meta.len() > MAX_IMAGE_BYTES as u64 {
                return Err(AppError::Validation(format!(
                    "画像が大きすぎます（{} バイト。上限 {MAX_IMAGE_BYTES} バイト）",
                    meta.len()
                )));
            }
            Ok(std::fs::read(&path)?)
        })
        .await
        .map_err(|e| AppError::Internal(format!("ファイルの読み込みが中断されました: {e}")))??;
        store(covers, bytes).await
    }

    /// 画像のバイト列（ファイルのドロップ・クリップボード）から取り込む。
    pub async fn import_cover_bytes(&self, bytes: Vec<u8>) -> Result<String, AppError> {
        if bytes.is_empty() {
            return Err(AppError::Validation("画像のデータが空です".to_owned()));
        }
        store(self.dirs.covers.clone(), bytes).await
    }

    /// 利用者が入力した画像の URL（http・https）から取り込む。画像でない応答は拒否する。
    pub async fn import_cover_url(&self, url: &str) -> Result<String, AppError> {
        let active = self.active().await?;
        let stored = active
            .service
            .import_cover_url(url, &self.dirs.covers)
            .await
            .map_err(cover_error)?;
        Ok(stored.hash)
    }
}

async fn store(covers: PathBuf, bytes: Vec<u8>) -> Result<String, AppError> {
    let stored = cover::import_cover_bytes_async(covers, bytes)
        .await
        .map_err(cover_error)?;
    Ok(stored.hash)
}
