//! エラー型。

use std::path::PathBuf;

/// このクレートの `Result`。
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// コアライブラリのエラー。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SQLite エラー: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON エラー: {0}")]
    Json(#[from] serde_json::Error),

    /// DB の版がアプリの知る最新版より新しい。
    /// 呼び出し側は、読み取り専用で起動して更新を促す、などの扱いに分岐できる。
    #[error("DB のスキーマ版 v{found} は、このアプリが対応する最新版 v{supported} より新しい")]
    SchemaTooNew { found: u32, supported: u32 },

    #[error("マイグレーション v{version}（{name}）に失敗: {source}")]
    MigrationFailed {
        version: u32,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },

    #[error("マイグレーション一覧が不正: {0}")]
    InvalidMigrations(String),

    #[error("UTF-8 でないパスは扱えない: {0}")]
    InvalidPath(PathBuf),

    #[error("HLC が不正: {0}")]
    InvalidHlc(String),

    #[error("Op が不正: {0}")]
    InvalidOp(String),

    /// ローカルの変更の入力が不正（項目の値、列挙値の範囲、書式など）。`field` は項目名（なければ空）。
    #[error("入力が不正です（{field}）: {message}")]
    Validation { field: String, message: String },

    /// 対象のエンティティが見つからない。
    #[error("{what}が見つかりません: {id}")]
    NotFound { what: &'static str, id: String },

    /// 今の状態ではできない操作（保管場所に本が残っている、循環する移動など）。
    #[error("{0}")]
    Conflict(String),

    /// バックアップとして使えないファイル。
    #[error("バックアップとして使えません: {0}")]
    InvalidBackup(String),

    #[error("local_meta に {0} がない")]
    MissingMeta(&'static str),

    #[error("local_meta の {key} が壊れている: {value:?}")]
    CorruptMeta { key: &'static str, value: String },

    /// 新しく採番した HLC が、対象項目の既存の時計より新しくない。
    /// 他端末の HLC を取り込んだのに `observe_hlc` を呼んでいない場合に起こる。
    #[error(
        "採番した HLC {hlc} が {entity}/{id} の既存の項目時計より新しくない（他端末の HLC を観測していない）"
    )]
    ClockBehind {
        hlc: String,
        entity: &'static str,
        id: String,
    },
}

impl Error {
    /// 項目の値が不正なときのエラー。
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Validation {
            field: field.into(),
            message: message.into(),
        }
    }
}
