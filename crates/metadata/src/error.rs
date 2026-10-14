//! エラー型。

use thiserror::Error;

/// 取得元への問い合わせの失敗。取得元ごとの状態（成功／該当なし／通信エラー／HTTP エラー／解析エラー）の元になる。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FetchError {
    /// つながらない・タイムアウト・途中で切れた、など（リトライしても駄目だったもの）。
    #[error("通信エラー: {0}")]
    Network(String),
    /// サーバが 2xx 以外を返した（リトライしても駄目だったもの）。
    #[error("HTTP エラー {status}: {message}")]
    Http { status: u16, message: String },
    /// 応答は届いたが、期待した形ではなかった。
    #[error("解析エラー: {0}")]
    Parse(String),
}

/// 生の応答を候補に直せなかった。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct ParseError(pub String);

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        ParseError(message.into())
    }
}

/// キーワード検索（フェーズ 3）のエラー。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SearchError {
    #[error("この取得元はキーワード検索に対応していません")]
    Unsupported,
    #[error(transparent)]
    Fetch(#[from] FetchError),
}

/// 書影の処理のエラー。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoverError {
    #[error("画像として読み込めません: {0}")]
    Decode(String),
    #[error("画像が大きすぎます（{0}）")]
    TooLarge(String),
    #[error("画像が小さすぎます（{width}x{height}）")]
    TooSmall { width: u32, height: u32 },
    #[error("画像の変換に失敗しました: {0}")]
    Encode(String),
    #[error("ファイルの読み書きに失敗しました: {0}")]
    Io(String),
    #[error("URL が正しくありません: {0}")]
    InvalidUrl(String),
    #[error(transparent)]
    Fetch(#[from] FetchError),
}
