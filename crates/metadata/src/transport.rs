//! 取得元への通信の作法（FR-META-08／11）。
//!
//! - レート制限: 取得元（レート制限のグループ）ごとに、リクエストの開始間隔と同時接続数を制限する。
//!   NDL は同時接続 1、1 秒に 1 件。同じグループ（`ndl`）を使う取得元は一緒に数える。
//! - リトライ: タイムアウト・接続エラー・429・500・502・503・504 を、指数バックオフ（1 秒・2 秒・4 秒）で最大 3 回やり直す。
//!   やり直しても 5xx・429 のままなら、その応答を返す（HTTP エラー）。応答が得られなければ通信エラー。
//!   429 でも「1 日あたりの上限」（Google Books の割り当て切れ）はやり直しても無駄なので、すぐ返す。
//! - ログに出す URL は API キーを伏せる。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio::time::{Instant, sleep, sleep_until};
use ts_rs::TS;

use crate::error::FetchError;
use crate::http::{HttpClient, HttpRequest, HttpResponse, mask_secrets, mask_url};

/// 取得元ごとのレート制限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderRateLimit {
    /// 同じ名前のものは、制限を共有する（同じ取得元の書誌と書影など）。
    pub group: String,
    /// リクエストの開始から次の開始までの最短間隔（ミリ秒）。0 なら制限なし。
    pub min_interval_ms: u64,
    /// 同時に送ってよいリクエスト数。
    pub max_concurrent: u32,
}

impl ProviderRateLimit {
    pub fn new(group: &str, min_interval_ms: u64, max_concurrent: u32) -> Self {
        ProviderRateLimit {
            group: group.to_string(),
            min_interval_ms,
            max_concurrent: max_concurrent.max(1),
        }
    }

    /// 制限なし（ユーザーが入力した画像 URL など）。
    pub fn unlimited(group: &str) -> Self {
        ProviderRateLimit::new(group, 0, 8)
    }
}

/// リトライの方針。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// 最初の 1 回に加えてやり直す回数。
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// `retry_index`（0 から数える）回目のやり直しの前に待つ時間。
    pub fn delay(&self, retry_index: u32, retry_after: Option<Duration>) -> Duration {
        let factor = 2u32.saturating_pow(retry_index);
        let backoff = self.base_delay.saturating_mul(factor).min(self.max_delay);
        match retry_after {
            Some(r) => backoff.max(r.min(Duration::from_secs(60))),
            None => backoff,
        }
    }
}

struct Gate {
    permits: Semaphore,
    last_start: tokio::sync::Mutex<Option<Instant>>,
    min_interval: Duration,
}

impl Gate {
    fn new(limit: &ProviderRateLimit) -> Self {
        Gate {
            permits: Semaphore::new(limit.max_concurrent.max(1) as usize),
            last_start: tokio::sync::Mutex::new(None),
            min_interval: Duration::from_millis(limit.min_interval_ms),
        }
    }

    /// 順番が来るまで待つ。戻り値を持っている間が「通信中」。
    async fn acquire(&self) -> tokio::sync::SemaphorePermit<'_> {
        let permit = self.permits.acquire().await.expect("セマフォは閉じない");
        // ロックを持ったまま待つので、待っている呼び出しは到着順に並ぶ
        let mut last = self.last_start.lock().await;
        if let Some(t) = *last {
            sleep_until(t + self.min_interval).await;
        }
        *last = Some(Instant::now());
        permit
    }
}

fn is_retryable_status(resp: &HttpResponse) -> bool {
    match resp.status {
        500 | 502 | 503 | 504 => true,
        429 => !is_daily_quota(resp),
        _ => false,
    }
}

/// 429 のうち「1 日あたりの上限」（待っても回復しない）か。
fn is_daily_quota(resp: &HttpResponse) -> bool {
    let head = &resp.body[..resp.body.len().min(4096)];
    String::from_utf8_lossy(head)
        .to_ascii_lowercase()
        .contains("per day")
}

/// レート制限とリトライを付けて GET する。
pub struct Transport {
    client: Arc<dyn HttpClient>,
    retry: RetryPolicy,
    gates: Mutex<HashMap<String, Arc<Gate>>>,
}

impl Transport {
    pub fn new(client: Arc<dyn HttpClient>) -> Self {
        Transport {
            client,
            retry: RetryPolicy::default(),
            gates: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    fn gate(&self, limit: &ProviderRateLimit) -> Arc<Gate> {
        let mut gates = self.gates.lock().expect("ゲートの表");
        gates
            .entry(limit.group.clone())
            .or_insert_with(|| Arc::new(Gate::new(limit)))
            .clone()
    }

    /// GET する。HTTP のステータスが 2xx 以外でも、応答が得られれば `Ok`（呼び出し側が見る）。
    /// リトライしても通信できなければ `FetchError::Network`。
    pub async fn get(
        &self,
        limit: &ProviderRateLimit,
        req: HttpRequest,
    ) -> Result<HttpResponse, FetchError> {
        let gate = self.gate(limit);
        let shown = mask_url(&req.url);
        let mut retry_index = 0u32;
        loop {
            let result = {
                let _permit = gate.acquire().await;
                tracing::debug!(url = %shown, attempt = retry_index + 1, "GET");
                self.client.get(req.clone()).await
            };
            // `last` は、リトライしても 5xx・429 のままだったときに、その応答をそのまま返すために持っておく
            let (reason, retry_after, last) = match result {
                Ok(resp) if !is_retryable_status(&resp) => return Ok(resp),
                Ok(resp) => (
                    format!("HTTP {}", resp.status),
                    resp.retry_after,
                    Some(resp),
                ),
                Err(e) if !e.is_retryable() => {
                    return Err(FetchError::Network(mask_secrets(&e.to_string())));
                }
                Err(e) => (mask_secrets(&e.to_string()), None, None),
            };
            if retry_index >= self.retry.max_retries {
                tracing::warn!(url = %shown, %reason, "リトライの上限に達しました");
                return match last {
                    // サーバは応答している（HTTP エラー）。呼び出し側がステータスで判断する
                    Some(resp) => Ok(resp),
                    None if self.retry.max_retries == 0 => Err(FetchError::Network(reason)),
                    None => Err(FetchError::Network(format!(
                        "{reason}（{} 回やり直しても回復しませんでした）",
                        self.retry.max_retries
                    ))),
                };
            }
            let delay = self.retry.delay(retry_index, retry_after);
            tracing::warn!(url = %shown, %reason, ?delay, "やり直します");
            sleep(delay).await;
            retry_index += 1;
        }
    }

    /// `get` のあと、2xx 以外を `FetchError::Http` にする。
    pub async fn get_ok(
        &self,
        limit: &ProviderRateLimit,
        req: HttpRequest,
    ) -> Result<HttpResponse, FetchError> {
        let resp = self.get(limit, req).await?;
        if resp.is_success() {
            Ok(resp)
        } else {
            Err(http_error(&resp))
        }
    }
}

static HTML_TAG: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)<!--.*?-->|<[^>]*>").expect("正規表現"));

/// 2xx 以外の応答を `FetchError::Http` にする。メッセージは本文の先頭 200 文字（HTML のタグは除き、秘密は伏せる）。
pub fn http_error(resp: &HttpResponse) -> FetchError {
    let text = resp.text();
    let plain = if text.contains('<') {
        HTML_TAG.replace_all(&text, " ").into_owned()
    } else {
        text
    };
    let excerpt: String = plain
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect();
    FetchError::Http {
        status: resp.status,
        message: mask_secrets(&excerpt),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::http::TransportError;

    /// 決めた順に応答を返す偽の HTTP クライアント。呼ばれた時刻（`tokio::time::Instant`）を記録する。
    struct Script {
        responses: Mutex<Vec<Result<HttpResponse, TransportError>>>,
        calls: Mutex<Vec<Instant>>,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        latency: Duration,
    }

    impl Script {
        fn new(responses: Vec<Result<HttpResponse, TransportError>>) -> Arc<Self> {
            Arc::new(Script {
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                latency: Duration::ZERO,
            })
        }
    }

    #[async_trait]
    impl HttpClient for Script {
        async fn get(&self, _req: HttpRequest) -> Result<HttpResponse, TransportError> {
            self.calls.lock().unwrap().push(Instant::now());
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            if !self.latency.is_zero() {
                sleep(self.latency).await;
            }
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            let mut r = self.responses.lock().unwrap();
            if r.len() > 1 {
                r.remove(0)
            } else {
                r[0].clone()
            }
        }
    }

    fn resp(status: u16, body: &str) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status,
            content_type: None,
            retry_after: None,
            body: body.as_bytes().to_vec(),
        })
    }

    fn limit() -> ProviderRateLimit {
        ProviderRateLimit::new("test", 0, 4)
    }

    #[test]
    fn http_error_message_is_plain_text_without_secrets() {
        let html = "<!DOCTYPE HTML>\n<HTML><HEAD><TITLE>ERROR</TITLE></HEAD><BODY><H1>403 ERROR</H1>\n<H2>The request could not be satisfied.</H2><!-- c --></BODY></HTML>";
        let FetchError::Http { status, message } = http_error(&HttpResponse {
            status: 403,
            content_type: None,
            retry_after: None,
            body: html.as_bytes().to_vec(),
        }) else {
            panic!()
        };
        assert_eq!(status, 403);
        assert_eq!(
            message,
            "ERROR 403 ERROR The request could not be satisfied."
        );
        let FetchError::Http { message, .. } = http_error(&HttpResponse {
            status: 400,
            content_type: None,
            retry_after: None,
            body: "bad request: https://x/?key=SECRET".as_bytes().to_vec(),
        }) else {
            panic!()
        };
        assert_eq!(message, "bad request: https://x/?key=***");
    }

    #[test]
    fn backoff_is_exponential() {
        let p = RetryPolicy::default();
        assert_eq!(p.delay(0, None), Duration::from_secs(1));
        assert_eq!(p.delay(1, None), Duration::from_secs(2));
        assert_eq!(p.delay(2, None), Duration::from_secs(4));
        assert_eq!(p.delay(10, None), Duration::from_secs(30), "上限で止まる");
        assert_eq!(
            p.delay(0, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
        assert_eq!(
            p.delay(0, Some(Duration::from_secs(600))),
            Duration::from_secs(60)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn retries_with_backoff_then_succeeds() {
        let client = Script::new(vec![
            resp(503, "busy"),
            Err(TransportError::Timeout),
            resp(200, "ok"),
        ]);
        let t = Transport::new(client.clone());
        let start = Instant::now();
        let r = t
            .get(&limit(), HttpRequest::get("https://x.example/"))
            .await
            .unwrap();
        assert_eq!(r.text(), "ok");
        let calls = client.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[1] - start, Duration::from_secs(1));
        assert_eq!(calls[2] - start, Duration::from_secs(3), "1 秒 + 2 秒");
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_three_retries() {
        let client = Script::new(vec![Err(TransportError::Connect("refused".into()))]);
        let t = Transport::new(client.clone());
        let start = Instant::now();
        let err = t
            .get(&limit(), HttpRequest::get("https://x.example/?key=SECRET"))
            .await
            .unwrap_err();
        assert!(matches!(&err, FetchError::Network(m) if !m.contains("SECRET")));
        let calls = client.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 4, "最初の 1 回 + リトライ 3 回");
        assert_eq!(calls[3] - start, Duration::from_secs(7), "1 + 2 + 4 秒");
    }

    #[tokio::test(start_paused = true)]
    async fn http_errors_are_returned_after_retries_for_5xx() {
        let client = Script::new(vec![resp(500, "boom")]);
        let t = Transport::new(client.clone());
        let err = t
            .get_ok(&limit(), HttpRequest::get("https://x.example/"))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            FetchError::Http {
                status: 500,
                message: "boom".into()
            },
            "5xx が続いたら、最後の応答を HTTP エラーとして返す"
        );
        assert_eq!(client.calls.lock().unwrap().len(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn client_errors_are_not_retried() {
        let client = Script::new(vec![resp(404, "nope"), resp(200, "ok")]);
        let t = Transport::new(client.clone());
        let err = t
            .get_ok(&limit(), HttpRequest::get("https://x.example/"))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            FetchError::Http {
                status: 404,
                message: "nope".into()
            }
        );
        assert_eq!(client.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn daily_quota_429_is_not_retried_but_other_429_is() {
        let quota = r#"{"error":{"code":429,"message":"Quota exceeded for quota metric 'Queries' and limit 'Queries per day'"}}"#;
        let client = Script::new(vec![resp(429, quota)]);
        let t = Transport::new(client.clone());
        let err = t
            .get_ok(&limit(), HttpRequest::get("https://x.example/"))
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::Http { status: 429, .. }));
        assert_eq!(client.calls.lock().unwrap().len(), 1);

        let client = Script::new(vec![resp(429, "slow down"), resp(200, "ok")]);
        let t = Transport::new(client.clone());
        assert!(
            t.get_ok(&limit(), HttpRequest::get("https://x.example/"))
                .await
                .is_ok()
        );
        assert_eq!(client.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn too_large_is_not_retried() {
        let client = Script::new(vec![Err(TransportError::TooLarge(10))]);
        let t = Transport::new(client.clone());
        assert!(
            t.get(&limit(), HttpRequest::get("https://x.example/"))
                .await
                .is_err()
        );
        assert_eq!(client.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_spaces_requests_one_second_apart() {
        let client = Script::new(vec![resp(200, "ok")]);
        let t = Arc::new(Transport::new(client.clone()));
        let lim = ProviderRateLimit::new("ndl", 1000, 1);
        let start = Instant::now();
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let t = t.clone();
            let lim = lim.clone();
            tasks.push(tokio::spawn(async move {
                t.get(&lim, HttpRequest::get("https://x.example/"))
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        let calls = client.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0] - start, Duration::ZERO, "最初は待たない");
        assert_eq!(calls[1] - start, Duration::from_secs(1));
        assert_eq!(calls[2] - start, Duration::from_secs(2));
    }

    #[tokio::test(start_paused = true)]
    async fn same_group_shares_the_limit_and_never_runs_in_parallel() {
        let client = Arc::new(Script {
            responses: Mutex::new(vec![resp(200, "ok")]),
            calls: Mutex::new(Vec::new()),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
            latency: Duration::from_millis(300),
        });
        let t = Arc::new(Transport::new(client.clone()));
        // 同じグループの取得元は制限を共有する。同時接続は 1。
        let a = ProviderRateLimit::new("ndl", 1000, 1);
        let b = ProviderRateLimit::new("ndl", 1000, 1);
        let (t1, t2) = (t.clone(), t.clone());
        let h1 =
            tokio::spawn(async move { t1.get(&a, HttpRequest::get("https://x.example/1")).await });
        let h2 =
            tokio::spawn(async move { t2.get(&b, HttpRequest::get("https://x.example/2")).await });
        h1.await.unwrap().unwrap();
        h2.await.unwrap().unwrap();
        assert_eq!(client.max_in_flight.load(Ordering::SeqCst), 1);
        let calls = client.calls.lock().unwrap().clone();
        assert!(calls[1] - calls[0] >= Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn different_groups_do_not_wait_for_each_other() {
        let client = Script::new(vec![resp(200, "ok")]);
        let t = Transport::new(client.clone());
        let start = Instant::now();
        t.get(
            &ProviderRateLimit::new("openbd", 1000, 1),
            HttpRequest::get("https://x.example/1"),
        )
        .await
        .unwrap();
        t.get(
            &ProviderRateLimit::new("google", 1000, 1),
            HttpRequest::get("https://x.example/2"),
        )
        .await
        .unwrap();
        assert_eq!(Instant::now() - start, Duration::ZERO);
    }
}
