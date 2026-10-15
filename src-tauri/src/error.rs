//! IPC コマンドのエラー型。
//!
//! フロントには `{ kind, message }` の形で届く。`kind` で画面の出し分けを、`message` で
//! 原因の表示をする。型は ts-rs で `src/bindings/AppError.ts` に出力する。

use serde::{Serialize, Serializer};
use ts_rs::TS;

#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
    /// 起動時に DB を開けなかった（DB に触るコマンドはすべてこのエラーを返す）。
    #[error("データベースを開けませんでした: {reason}")]
    DbUnavailable { reason: String },

    /// DB が、このアプリより新しい版で作られている。DB には何も書いていない。
    #[error(
        "データベースのスキーマ版 v{found} は、このアプリが対応する最新版 v{supported} より新しいため開けません"
    )]
    SchemaTooNew { found: u32, supported: u32 },

    /// DB の操作に失敗した。
    #[error("データベースの操作に失敗しました: {0}")]
    Database(String),

    /// 上のどれでもない、アプリ内部の不具合。
    #[error("内部エラー: {0}")]
    Internal(String),

    /// 入力が不正（項目の値、書式、範囲など）。メッセージに項目名を含む。
    #[error("{0}")]
    Validation(String),

    /// 対象が見つからない（書籍、保管場所、タグなど）。
    #[error("{0}")]
    NotFound(String),

    /// 今の状態ではできない操作（本が残る保管場所の削除、循環する移動など）。
    #[error("{0}")]
    Conflict(String),

    /// バックアップとして使えないファイル。
    #[error("{0}")]
    InvalidBackup(String),

    /// ネットワークの操作に失敗した（書誌の取得など）。
    #[error("ネットワークの操作に失敗しました: {0}")]
    Network(String),

    /// 書誌の取得元（API）が、エラーや想定外の応答を返した。
    #[error("書誌の取得元でエラーが起きました: {0}")]
    #[allow(
        dead_code,
        reason = "種類（provider）は IPC の取り決めに含む。今の書誌取得は、取得元のエラーを取得結果（ProviderReport）で返し、失敗にしない"
    )]
    Provider(String),

    /// ファイルの読み書きに失敗した。
    #[error("ファイルの操作に失敗しました: {0}")]
    Io(String),

    /// OS のキーチェーンの操作に失敗した。
    #[error("キーチェーンの操作に失敗しました: {0}")]
    Keychain(String),
}

/// エラーの種類。フロントはこれで表示を切り替える。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorKind {
    DbUnavailable,
    SchemaTooNew,
    Database,
    Internal,
    Validation,
    NotFound,
    Conflict,
    InvalidBackup,
    Network,
    Provider,
    Io,
    Keychain,
}

/// コマンドが失敗したときにフロントへ届くエラー（Rust の `AppError` を直列化した形。TypeScript では `AppError`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "AppError")]
pub struct AppErrorPayload {
    pub kind: ErrorKind,
    pub message: String,
}

impl AppError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::DbUnavailable { .. } => ErrorKind::DbUnavailable,
            Self::SchemaTooNew { .. } => ErrorKind::SchemaTooNew,
            Self::Database(_) => ErrorKind::Database,
            Self::Internal(_) => ErrorKind::Internal,
            Self::Validation(_) => ErrorKind::Validation,
            Self::NotFound(_) => ErrorKind::NotFound,
            Self::Conflict(_) => ErrorKind::Conflict,
            Self::InvalidBackup(_) => ErrorKind::InvalidBackup,
            Self::Network(_) => ErrorKind::Network,
            Self::Provider(_) => ErrorKind::Provider,
            Self::Io(_) => ErrorKind::Io,
            Self::Keychain(_) => ErrorKind::Keychain,
        }
    }

    /// 起動時に DB を開けなかったときのエラーに変換する。
    pub fn from_open(err: bibcontainer_core::Error) -> Self {
        match err {
            bibcontainer_core::Error::SchemaTooNew { found, supported } => {
                Self::SchemaTooNew { found, supported }
            }
            other => Self::DbUnavailable {
                reason: other.to_string(),
            },
        }
    }
}

/// 実行中のコアの操作で起きたエラー。
///
/// - 入力の誤りと「今はできない操作」は、画面が出し分けられるよう専用の種類にする
///   （`Validation` のメッセージには、コアが付ける項目名が入る）。
/// - 壊れた Op・HLC など、アプリ側の不具合が疑われるものは `Internal`。
impl From<bibcontainer_core::Error> for AppError {
    fn from(err: bibcontainer_core::Error) -> Self {
        use bibcontainer_core::Error as E;
        let message = err.to_string();
        match err {
            E::SchemaTooNew { found, supported } => Self::SchemaTooNew { found, supported },
            E::Validation { .. } => Self::Validation(message),
            E::NotFound { .. } => Self::NotFound(message),
            E::Conflict(_) => Self::Conflict(message),
            E::InvalidBackup(_) => Self::InvalidBackup(message),
            E::Io(_) | E::InvalidPath(_) => Self::Io(message),
            E::Json(_) | E::InvalidHlc(_) | E::InvalidOp(_) | E::ClockBehind { .. } => {
                Self::Internal(message)
            }
            E::Sqlite(_)
            | E::MigrationFailed { .. }
            | E::InvalidMigrations(_)
            | E::MissingMeta(_)
            | E::CorruptMeta { .. } => Self::Database(message),
        }
    }
}

/// 同期エンジンのエラー。メッセージは、エンジンが作った日本語をそのまま使う。
///
/// - 同期フォルダの読み書きの失敗は `io`。
/// - 今の状態ではできないこと（設定されていない、別のライブラリ、手元にデータがある、ライブラリがない・すでにある、
///   端末 ID の重複、新しいアプリが必要、`library.json` が読めない）は `conflict`。
/// - DB の失敗は、コアのエラーと同じ分け方。
impl From<bibcontainer_sync::SyncError> for AppError {
    fn from(err: bibcontainer_sync::SyncError) -> Self {
        use bibcontainer_sync::SyncError as E;
        let message = err.to_string();
        match err {
            E::Transport(_) => Self::Io(message),
            E::Db(core) => Self::from(core),
            E::Json(_) => Self::Internal(message),
            E::Internal(detail) => Self::Internal(detail),
            E::NotConfigured
            | E::LibraryMismatch { .. }
            | E::AlreadyInLibrary { .. }
            | E::UpdateRequired { .. }
            | E::DuplicateDevice { .. }
            | E::LibraryMissing
            | E::LibraryExists
            | E::LocalHasData
            | E::BadLibraryFile(_) => Self::Conflict(message),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AppErrorPayload {
            kind: self.kind(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_kind_and_message() {
        let err = AppError::SchemaTooNew {
            found: 9,
            supported: 1,
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "schema_too_new");
        assert_eq!(json["message"], err.to_string());
        assert_eq!(json.as_object().unwrap().len(), 2);
    }

    #[test]
    fn core_schema_too_new_keeps_its_kind() {
        let core = bibcontainer_core::Error::SchemaTooNew {
            found: 9,
            supported: 1,
        };
        assert_eq!(AppError::from_open(core).kind(), ErrorKind::SchemaTooNew);

        let core = bibcontainer_core::Error::MissingMeta("device_id");
        assert_eq!(AppError::from_open(core).kind(), ErrorKind::DbUnavailable);

        let core = bibcontainer_core::Error::MissingMeta("device_id");
        assert_eq!(AppError::from(core).kind(), ErrorKind::Database);
    }

    #[test]
    fn maps_core_errors_to_their_kinds() {
        use bibcontainer_core::Error as E;
        let kind = |e: E| AppError::from(e).kind();
        assert_eq!(
            kind(E::validation("title", "空です")),
            ErrorKind::Validation
        );
        assert_eq!(
            kind(E::NotFound {
                what: "書籍",
                id: "x".into()
            }),
            ErrorKind::NotFound
        );
        assert_eq!(kind(E::Conflict("できない".into())), ErrorKind::Conflict);
        assert_eq!(
            kind(E::InvalidBackup("壊れている".into())),
            ErrorKind::InvalidBackup
        );
        assert_eq!(
            kind(E::Io(std::io::Error::other("読めない"))),
            ErrorKind::Io
        );
        assert_eq!(kind(E::InvalidOp("x".into())), ErrorKind::Internal);
    }

    #[test]
    fn validation_message_names_the_field() {
        let err = AppError::from(bibcontainer_core::Error::validation("rating", "1〜5 です"));
        assert!(err.to_string().contains("rating"), "{err}");
        assert!(err.to_string().contains("1〜5 です"), "{err}");
    }

    #[test]
    fn every_kind_serializes_in_snake_case() {
        let pairs = [
            (AppError::Validation("a".into()), "validation"),
            (AppError::NotFound("a".into()), "not_found"),
            (AppError::Conflict("a".into()), "conflict"),
            (AppError::InvalidBackup("a".into()), "invalid_backup"),
            (AppError::Network("a".into()), "network"),
            (AppError::Provider("a".into()), "provider"),
            (AppError::Io("a".into()), "io"),
            (AppError::Keychain("a".into()), "keychain"),
        ];
        for (err, kind) in pairs {
            assert_eq!(serde_json::to_value(&err).unwrap()["kind"], kind);
        }
    }
}
