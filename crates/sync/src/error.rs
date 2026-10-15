//! 同期エンジンのエラー。

use crate::status::SyncState;
use crate::transport::TransportError;

/// このクレートの `Result`。
pub type Result<T, E = SyncError> = std::result::Result<T, E>;

/// 同期エンジンのエラー。メッセージは、そのまま画面に出せる日本語。
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// 同期フォルダの読み書きに失敗した。
    #[error("同期フォルダを使えません。{0}")]
    Transport(#[from] TransportError),

    /// ローカル DB の操作に失敗した。
    #[error("{0}")]
    Db(#[from] bibcontainer_core::Error),

    /// JSON を作れなかった。
    #[error("JSON エラー: {0}")]
    Json(#[from] serde_json::Error),

    /// 同期フォルダが設定されていない（ライブラリ ID がない）。
    #[error("同期が設定されていません")]
    NotConfigured,

    /// 同期フォルダのライブラリが、この端末のライブラリと違う（FR-SYNC-10）。同期しない。
    #[error(
        "このフォルダは別のライブラリのものです（フォルダ: {folder}、この端末: {local}）。同期しません"
    )]
    LibraryMismatch { folder: String, local: String },

    /// この端末は既に別のライブラリに参加している。先に同期を解除する。
    #[error("この端末は既に別のライブラリ（{local}）に参加しています。先に同期を解除してください")]
    AlreadyInLibrary { local: String },

    /// ライブラリを読むのに、もっと新しいアプリが必要。
    #[error(
        "このライブラリを使うには、アプリのバージョン {required} 以上が必要です（この端末: {current}）。アプリを更新してください"
    )]
    UpdateRequired { required: String, current: String },

    /// 同じ端末 ID を別の端末が使っている疑い。同期を止める。
    #[error(
        "同じ端末 ID（{device_id}）を別の端末が使っている可能性があります（{detail}）。アプリのデータフォルダを別の PC に丸ごとコピーしていないか確認してください。同期を止めました"
    )]
    DuplicateDevice { device_id: String, detail: String },

    /// 同期フォルダにライブラリがない（`library.json` がない）。
    #[error(
        "同期フォルダにライブラリ（library.json）がありません。同期が終わっていないか、別のフォルダかもしれません"
    )]
    LibraryMissing,

    /// 新しく作ろうとしたフォルダに、既にライブラリがある。
    #[error("このフォルダには既にライブラリがあります。参加してください")]
    LibraryExists,

    /// 手元にデータがあるので、そのまま参加できない。
    #[error("手元にデータがあります。ライブラリに統合するか、手元のデータを置き換えてください")]
    LocalHasData,

    /// `library.json` が読めない・形式が違う。
    #[error("ライブラリの情報（library.json）を読めません: {0}")]
    BadLibraryFile(String),

    /// 起こらないはずの状態（ロックの破損など）。
    #[error("内部エラー: {0}")]
    Internal(String),
}

impl SyncError {
    /// この失敗を、画面の同期状態（`SyncState`）にすると何か。
    pub fn state(&self) -> SyncState {
        match self {
            SyncError::LibraryMismatch { .. } => SyncState::Mismatch,
            SyncError::UpdateRequired { .. } => SyncState::UpdateRequired,
            _ => SyncState::Error,
        }
    }
}
