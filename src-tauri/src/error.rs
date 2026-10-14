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

/// 実行中の DB 操作で起きたエラー。
impl From<bibcontainer_core::Error> for AppError {
    fn from(err: bibcontainer_core::Error) -> Self {
        match err {
            bibcontainer_core::Error::SchemaTooNew { found, supported } => {
                Self::SchemaTooNew { found, supported }
            }
            other => Self::Database(other.to_string()),
        }
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
}
