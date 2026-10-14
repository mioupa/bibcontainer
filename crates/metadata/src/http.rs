//! HTTP の層。
//!
//! 取得元の実装は `HttpClient` トレイトだけを通して通信する。本番は `ReqwestClient`、
//! テストでは応答を差し替えた偽物を渡せる（リトライ・レート制限・キャッシュの検証用）。
//! リトライとレート制限は `transport::Transport` が受け持つ。

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use thiserror::Error;

/// 応答本体の既定の上限（テキスト系）。
pub const DEFAULT_MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
/// 画像の上限。
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
/// タイムアウトの既定値（FR-META-11）。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// User-Agent を作る（FR-META-09）。例: `bibcontainer/0.1.0 (+https://github.com/mioupa/bibcontainer)`
pub fn user_agent(app_version: &str, contact_url: &str) -> String {
    format!("bibcontainer/{app_version} (+{contact_url})")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// 応答本体の上限。超えたら `TransportError::TooLarge`。
    pub max_bytes: usize,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        HttpRequest {
            url: url.into(),
            headers: Vec::new(),
            max_bytes: DEFAULT_MAX_BODY_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    /// `Content-Type` ヘッダ（パラメータ付きのまま）
    pub content_type: Option<String>,
    /// `Retry-After` ヘッダが秒数のとき、その値
    pub retry_after: Option<Duration>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// 本文を UTF-8 の文字列として読む（壊れた部分は置き換える）。
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// `Content-Type` の MIME 型（小文字、パラメータなし）。
    pub fn mime(&self) -> Option<String> {
        self.content_type.as_ref().map(|c| {
            c.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
    }
}

/// 通信そのものの失敗（応答が得られなかった）。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("タイムアウトしました")]
    Timeout,
    /// 接続そのものが確立できなかった（DNS の解決失敗・接続拒否・到達不能など。reqwest の `is_connect()`）。
    /// オフラインのときに起きる。待っても回復しないので、リトライしない（`is_retryable`）。
    #[error("接続できません: {0}")]
    Connect(String),
    #[error("応答の受信に失敗しました: {0}")]
    Body(String),
    #[error("応答が大きすぎます（上限 {0} バイト）")]
    TooLarge(usize),
    #[error("{0}")]
    Other(String),
}

impl TransportError {
    /// リトライする価値があるか。
    ///
    /// 接続が確立できないエラー（`Connect`）は、オフラインのときに起き、すぐに分かる。やり直しても
    /// 7 秒（1 + 2 + 4 秒）待つだけなので、リトライせず、すぐ通信エラーとして返す（AT-11、FR-SCAN-16）。
    /// タイムアウト・応答の受信の失敗は、通信路が一時的に不安定なだけのことがあるので、リトライする。
    pub fn is_retryable(&self) -> bool {
        !matches!(
            self,
            TransportError::TooLarge(_) | TransportError::Connect(_)
        )
    }
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    /// GET する。ステータスコードが 4xx・5xx でも `Ok` で返す（判断は呼び出し側）。
    async fn get(&self, req: HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// reqwest（rustls）による実装。
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    /// `user_agent` は `user_agent()` で作ったもの。タイムアウトは 10 秒。
    pub fn new(user_agent: &str) -> Result<Self, TransportError> {
        Self::with_timeout(user_agent, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(user_agent: &str, timeout: Duration) -> Result<Self, TransportError> {
        install_crypto_provider();
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(timeout)
            .connect_timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| TransportError::Other(describe_error(e)))?;
        Ok(ReqwestClient { client })
    }
}

/// TLS の暗号プロバイダ（ring）をプロセスの既定として入れる。
///
/// reqwest の `rustls-no-provider` は、既定のプロバイダが入っていないとクライアントを作れない。
/// aws-lc-rs（reqwest の `rustls` 機能の既定）は Windows のビルドに NASM や CMake が要ることがあるので、
/// C の依存が少ない ring を使う。すでに別のプロバイダが入っていれば、それを使う。
pub fn install_crypto_provider() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        // 入れ済み（別のクレートが先に入れた）なら Err が返るが、それでよい
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// reqwest のエラーの文章。URL（API キーを含み得る）は除く。
fn describe_error(e: reqwest::Error) -> String {
    let e = e.without_url();
    let mut msg = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    mask_secrets(&msg)
}

fn classify(e: reqwest::Error, in_body: bool) -> TransportError {
    // 接続のタイムアウトは `is_connect()` でもあるが、先に見る `is_timeout()` で `Timeout`（リトライする）にする
    if e.is_timeout() {
        TransportError::Timeout
    } else if e.is_connect() {
        TransportError::Connect(describe_error(e))
    } else if in_body {
        TransportError::Body(describe_error(e))
    } else {
        TransportError::Other(describe_error(e))
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn get(&self, req: HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut builder = self.client.get(&req.url);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        let mut resp = builder.send().await.map_err(|e| classify(e, false))?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| classify(e, true))? {
            if body.len() + chunk.len() > req.max_bytes {
                return Err(TransportError::TooLarge(req.max_bytes));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            content_type,
            retry_after,
            body,
        })
    }
}

static SECRET_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)((?:[?&]|\b)(?:api[_-]?key|key|access[_-]?key|access[_-]?token|token|application[_-]?id|app[_-]?id|secret|signature|password)=)[^&#\s'\)]*")
        .expect("正規表現")
});

/// 文字列に含まれる秘密情報（`key=…` などのクエリ値）を `***` に置き換える。
pub fn mask_secrets(s: &str) -> String {
    SECRET_PARAM.replace_all(s, "${1}***").into_owned()
}

/// ログに出す URL。API キーなどのクエリ値を伏せる（10.3）。
pub fn mask_url(url: &str) -> String {
    mask_secrets(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_format() {
        assert_eq!(
            user_agent("0.1.0", "https://github.com/mioupa/bibcontainer"),
            "bibcontainer/0.1.0 (+https://github.com/mioupa/bibcontainer)"
        );
    }

    #[test]
    fn masks_api_keys() {
        assert_eq!(
            mask_url(
                "https://www.googleapis.com/books/v1/volumes?q=isbn:9784003101117&key=AIzaSyABC-def_123"
            ),
            "https://www.googleapis.com/books/v1/volumes?q=isbn:9784003101117&key=***"
        );
        assert_eq!(
            mask_url("https://x.example/a?key=SECRET&q=1"),
            "https://x.example/a?key=***&q=1"
        );
        assert_eq!(
            mask_url("https://x.example/a?API_KEY=S3&b=2"),
            "https://x.example/a?API_KEY=***&b=2"
        );
        assert_eq!(
            mask_url("https://x.example/a?token=abc#frag"),
            "https://x.example/a?token=***#frag"
        );
        // 秘密でないクエリは残す。`monkey=` のように語の途中に `key=` が現れる場合も壊さない
        assert_eq!(
            mask_url("https://x.example/a?monkey=1&q=isbn:1"),
            "https://x.example/a?monkey=1&q=isbn:1"
        );
        assert_eq!(
            mask_secrets("error sending request for url (https://x/?q=1&key=SECRET)"),
            "error sending request for url (https://x/?q=1&key=***)"
        );
    }

    #[test]
    fn only_connection_failures_and_oversized_bodies_are_not_retried() {
        assert!(!TransportError::Connect("dns error".into()).is_retryable());
        assert!(!TransportError::TooLarge(10).is_retryable());
        assert!(TransportError::Timeout.is_retryable());
        assert!(TransportError::Body("reset".into()).is_retryable());
        assert!(TransportError::Other("x".into()).is_retryable());
    }

    #[test]
    fn response_helpers() {
        let r = HttpResponse {
            status: 200,
            content_type: Some("Image/JPEG; charset=binary".into()),
            retry_after: None,
            body: "あ".as_bytes().to_vec(),
        };
        assert!(r.is_success());
        assert_eq!(r.mime().as_deref(), Some("image/jpeg"));
        assert_eq!(r.text(), "あ");
    }
}
