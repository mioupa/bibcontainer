//! 取得の流れ（並行取得・キャッシュ・オフライン判定・レート制限）のテスト。
//! HTTP の層を偽物に差し替え、応答は `tests/fixtures/` の実際の応答を返す。ネットワークは使わない。

mod common;

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::provider::{CacheEntry, CacheError, MemoryRawCache, Provider, RawCache};
use bibcontainer_metadata::providers::google::{GoogleFailure, ZERO_QUOTA_NOTE, classify_failure};
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd, default_providers};
use bibcontainer_metadata::service::{
    DEFAULT_CACHE_TTL, DEFAULT_NOT_FOUND_TTL, FetchRequest, MetadataService, ProviderOutcome,
};
use bibcontainer_metadata::types::*;
use bibcontainer_metadata::{ProviderReport, Transport};
use common::fixture;
use tokio::time::Instant;

const ONIGIRI: &str = "9784780802047";
const KOKORO: &str = "9784003101117";
const YUKIGUNI: &str = "9784101001012";
const MISSING: &str = "9784123456784";

type Handler = Box<dyn Fn(&str) -> Result<HttpResponse, TransportError> + Send + Sync>;

struct FakeHttp {
    handler: Mutex<Handler>,
    log: Mutex<Vec<(Instant, String)>>,
    latency: Duration,
    ndl_in_flight: AtomicUsize,
    ndl_max_in_flight: AtomicUsize,
}

impl FakeHttp {
    fn new(handler: Handler) -> Arc<Self> {
        Arc::new(FakeHttp {
            handler: Mutex::new(handler),
            log: Mutex::new(Vec::new()),
            latency: Duration::ZERO,
            ndl_in_flight: AtomicUsize::new(0),
            ndl_max_in_flight: AtomicUsize::new(0),
        })
    }

    fn with_latency(handler: Handler, latency: Duration) -> Arc<Self> {
        let mut f = Arc::try_unwrap(FakeHttp::new(handler)).ok().unwrap();
        f.latency = latency;
        Arc::new(f)
    }

    fn urls(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .map(|(_, u)| u.clone())
            .collect()
    }

    fn urls_containing(&self, needle: &str) -> Vec<(Instant, String)> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, u)| u.contains(needle))
            .cloned()
            .collect()
    }

    fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
}

#[async_trait]
impl HttpClient for FakeHttp {
    async fn get(&self, req: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.log
            .lock()
            .unwrap()
            .push((Instant::now(), req.url.clone()));
        let is_ndl = req.url.contains("ndlsearch.ndl.go.jp");
        if is_ndl {
            let now = self.ndl_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.ndl_max_in_flight.fetch_max(now, Ordering::SeqCst);
        }
        if !self.latency.is_zero() {
            tokio::time::sleep(self.latency).await;
        }
        let result = (self.handler.lock().unwrap())(&req.url);
        if is_ndl {
            self.ndl_in_flight.fetch_sub(1, Ordering::SeqCst);
        }
        result
    }
}

fn response(status: u16, content_type: &str, body: String) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status,
        content_type: Some(content_type.to_string()),
        retry_after: None,
        body: body.into_bytes(),
    })
}

/// 取得元ごとに、フィクスチャから応答を作る。
fn fixtures_handler() -> Handler {
    Box::new(|url: &str| {
        if let Some(list) = url.split("api.openbd.jp/v1/get?isbn=").nth(1) {
            let elements: Vec<String> = list
                .split(',')
                .map(|isbn| {
                    let path = common::fixture_path(&format!("openbd_{isbn}.json"));
                    std::fs::read_to_string(path).unwrap_or_else(|_| "null\n".to_string())
                })
                .collect();
            return response(
                200,
                "application/json; charset=UTF-8",
                format!("[{}]", elements.join(",")),
            );
        }
        if let Some(q) = url.split("query=isbn%3D").nth(1) {
            let isbn = q.split('&').next().unwrap();
            let path = common::fixture_path(&format!("ndl_{isbn}.xml"));
            let body =
                std::fs::read_to_string(path).unwrap_or_else(|_| fixture("ndl_notfound.xml"));
            return response(200, "text/xml;charset=UTF-8", body);
        }
        // Google の Dynamic Links（検索が該当なしのときの回避策）。この偽物では、どの ISBN も知らない
        if url.contains("books?bibkeys=ISBN:") {
            return response(
                200,
                "application/javascript; charset=ISO-8859-1",
                "cb({});".to_string(),
            );
        }
        if let Some(q) = url.split("volumes?q=isbn:").nth(1) {
            let isbn = q.split('&').next().unwrap();
            let file = match isbn {
                KOKORO => "google_synthetic_9784003101117.json",
                ONIGIRI => "google_synthetic_9784780802047.json",
                _ => "google_synthetic_notfound.json",
            };
            return response(200, "application/json; charset=UTF-8", fixture(file));
        }
        // Open Library: 検索（フィクスチャがなければ「該当なし」）と、版の JSON
        if let Some(q) = url.split("openlibrary.org/search.json?isbn=").nth(1) {
            let isbn = q.split('&').next().unwrap();
            let path = common::fixture_path(&format!("openlibrary_search_{isbn}.json"));
            let body = std::fs::read_to_string(path)
                .unwrap_or_else(|_| fixture("openlibrary_search_9784003101117.json"));
            return response(200, "application/json", body);
        }
        if let Some(olid) = url
            .split("openlibrary.org/books/")
            .nth(1)
            .and_then(|r| r.strip_suffix(".json"))
        {
            return match std::fs::read_to_string(common::fixture_path(&format!(
                "openlibrary_edition_{olid}.json"
            ))) {
                Ok(body) => response(200, "application/json", body),
                Err(_) => response(404, "application/json", "{}".to_string()),
            };
        }
        response(404, "text/plain", "not found".to_string())
    })
}

struct Rig {
    http: Arc<FakeHttp>,
    cache: Arc<MemoryRawCache>,
    clock: Arc<AtomicI64>,
    service: MetadataService,
}

fn rig_with(http: Arc<FakeHttp>, google_key: Option<&str>) -> Rig {
    let cache = Arc::new(MemoryRawCache::new());
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let transport = Arc::new(Transport::new(http.clone()));
    let c = clock.clone();
    let service =
        MetadataService::new(default_providers(google_key.map(str::to_string)), transport)
            .with_cache(cache.clone())
            .with_clock(Arc::new(move || c.load(Ordering::SeqCst)));
    Rig {
        http,
        cache,
        clock,
        service,
    }
}

fn rig() -> Rig {
    rig_with(FakeHttp::new(fixtures_handler()), None)
}

fn outcome(result: &bibcontainer_metadata::MetaFetchResult, p: ProviderId) -> ProviderOutcome {
    result
        .reports
        .iter()
        .find(|r| r.provider == p)
        .unwrap_or_else(|| panic!("{p:?} の状態がない: {:?}", result.reports))
        .outcome
        .clone()
}

fn report(result: &bibcontainer_metadata::MetaFetchResult, p: ProviderId) -> ProviderReport {
    result
        .reports
        .iter()
        .find(|r| r.provider == p)
        .unwrap()
        .clone()
}

// ---------------------------------------------------------------------------
// 基本の流れ
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at10_fetches_all_providers_and_merges() {
    let r = rig();
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert_eq!(result.isbn13, ONIGIRI);
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
    assert_eq!(result.merged.text(MetaField::Publisher), Some("ポット出版"));
    assert_eq!(result.merged.text(MetaField::CCode), Some("0077"));
    assert_eq!(
        result.merged.source(MetaField::CCode),
        Some(MetaSource::Openbd)
    );
    assert_eq!(
        result.merged.covers_in_order()[0].url,
        "https://cover.openbd.jp/9784780802047.jpg"
    );
    for p in ProviderId::ALL {
        // 日本の本は Open Library にない（検索だけで「該当なし」）
        let want = if p == ProviderId::OpenLibrary {
            ProviderOutcome::NotFound
        } else {
            ProviderOutcome::Found
        };
        assert_eq!(outcome(&result, p), want, "{p:?}");
        assert!(!report(&result, p).from_cache);
    }
    assert!(!result.offline);
    assert!(!result.has_error());
    assert_eq!(result.found_count(), 3);
    assert_eq!(result.candidates.len(), 3);
    // 取得元ごとに 1 回ずつ（Open Library は検索だけ）
    let urls = r.http.urls();
    assert_eq!(urls.len(), 4, "{urls:?}");
    assert!(urls.iter().any(|u| {
        u.starts_with("https://openlibrary.org/search.json?isbn=9784780802047&fields=")
            && u.contains("editions.isbn")
    }));
    assert!(
        urls.iter()
            .any(|u| u == "https://api.openbd.jp/v1/get?isbn=9784780802047")
    );
    assert!(
        urls.iter()
            .any(|u| u.starts_with("https://ndlsearch.ndl.go.jp/api/sru?")
                && u.contains("recordSchema=dcndl")
                && u.contains("onlyBib=true")
                && u.ends_with("query=isbn%3D9784780802047"))
    );
    assert!(urls.iter().any(|u| {
        u.starts_with("https://www.googleapis.com/books/v1/volumes?q=isbn:9784780802047")
            && !u.contains("key=")
    }));
}

#[tokio::test]
async fn input_notation_is_normalized_and_invalid_isbn_makes_no_request() {
    let r = rig();
    let result = r.service.fetch(&FetchRequest::new("4-7808-0204-0")).await;
    assert_eq!(result.isbn13, ONIGIRI);
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );

    r.http.clear();
    let result = r.service.fetch(&FetchRequest::new("9784003101118")).await;
    assert!(result.reports.is_empty());
    assert!(result.merged.is_empty());
    assert!(!result.offline);
    assert!(r.http.urls().is_empty());
}

#[tokio::test]
async fn google_key_is_sent_only_to_google_and_second_tier_is_merged() {
    let r = rig_with(FakeHttp::new(fixtures_handler()), Some("SECRET-KEY"));
    let tier = MetaSecondTier {
        c_code: Some("0193".into()),
        list_price: Some(700),
    };
    let result = r
        .service
        .fetch(&FetchRequest::new(KOKORO).with_second_tier(tier))
        .await;
    for (url, has_key) in r
        .http
        .urls()
        .iter()
        .map(|u| (u, u.contains("key=SECRET-KEY")))
    {
        assert_eq!(has_key, url.contains("googleapis.com"), "{url}");
    }
    assert_eq!(result.merged.text(MetaField::CCode), Some("0193"));
    assert_eq!(result.merged.number(MetaField::ListPrice), Some(700));
}

#[tokio::test]
async fn disabled_providers_are_not_called() {
    let r = rig();
    let result = r
        .service
        .fetch(&FetchRequest::new(ONIGIRI).with_enabled(vec![ProviderId::Openbd]))
        .await;
    assert_eq!(result.reports.len(), 1);
    assert_eq!(result.reports[0].provider, ProviderId::Openbd);
    assert_eq!(r.http.urls().len(), 1);
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
    // NDL を外すと、書名のヨミは取れない
    assert!(result.merged.text(MetaField::TitleKana).is_none());
}

#[tokio::test]
async fn not_found_everywhere() {
    let r = rig();
    let result = r.service.fetch(&FetchRequest::new(MISSING)).await;
    assert_eq!(
        outcome(&result, ProviderId::Openbd),
        ProviderOutcome::NotFound
    );
    assert_eq!(outcome(&result, ProviderId::Ndl), ProviderOutcome::NotFound);
    assert_eq!(
        outcome(&result, ProviderId::Google),
        ProviderOutcome::NotFound
    );
    assert_eq!(
        outcome(&result, ProviderId::OpenLibrary),
        ProviderOutcome::NotFound
    );
    assert!(result.merged.is_empty());
    assert!(!result.offline);
    assert!(!result.has_error());
    assert_eq!(result.found_count(), 0);
    assert!(result.candidates.is_empty());
}

#[tokio::test]
async fn providers_info_lists_credits() {
    let r = rig();
    let infos = r.service.providers_info();
    let ids: Vec<ProviderId> = infos.iter().map(|i| i.id).collect();
    assert_eq!(
        ids,
        vec![
            ProviderId::Openbd,
            ProviderId::Ndl,
            ProviderId::Google,
            ProviderId::OpenLibrary
        ]
    );
    assert!(infos.iter().all(|i| !i.credit.is_empty()));
}

// ---------------------------------------------------------------------------
// 失敗と継続（FR-META-11）、オフライン（AT-11）
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn all_network_providers_failing_means_offline() {
    let http = FakeHttp::new(Box::new(|_| Err(TransportError::Connect("offline".into()))));
    let r = rig_with(http, None);
    let tier = MetaSecondTier {
        c_code: Some("0193".into()),
        list_price: Some(700),
    };
    let result = r
        .service
        .fetch(&FetchRequest::new(ONIGIRI).with_second_tier(tier))
        .await;
    assert!(result.offline, "{:?}", result.reports);
    assert!(result.has_error());
    for p in ProviderId::ALL {
        assert!(
            matches!(outcome(&result, p), ProviderOutcome::NetworkError { .. }),
            "{p:?}"
        );
    }
    // 2段目の値は残る（保存して、あとで補完する）
    assert_eq!(result.merged.text(MetaField::CCode), Some("0193"));
    assert_eq!(result.merged.number(MetaField::ListPrice), Some(700));
    assert!(result.merged.chosen(MetaField::Title).is_none());
    // 何も保存されない
    assert!(r.cache.is_empty());
    // 接続できないエラー（DNS の解決失敗・接続拒否など）は、リトライしない。各取得元に 1 回だけ
    assert_eq!(r.http.urls_containing("api.openbd.jp").len(), 1);
    assert_eq!(r.http.urls_containing("ndlsearch.ndl.go.jp").len(), 1);
    assert_eq!(r.http.urls_containing("googleapis.com").len(), 1);
    assert_eq!(r.http.urls_containing("openlibrary.org").len(), 1);
}

/// オフラインのとき（接続が確立できない）は、バックオフ（1 + 2 + 4 秒）を待たずに、すぐ `offline` を返す。
/// 1 冊ごとに 7 秒かかると、連続登録で何冊も読む間、待たされ続ける（AT-11、FR-SCAN-16）。
#[tokio::test(start_paused = true)]
async fn offline_is_reported_at_once_without_waiting_for_retries() {
    let http = FakeHttp::new(Box::new(|_| {
        Err(TransportError::Connect(
            "dns error: failed to lookup address information".into(),
        ))
    }));
    let r = rig_with(http, None);

    // 時間を止めたテストなので、待った時間はそのまま経過時間に出る。1 冊目は待ちなし
    let start = Instant::now();
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert!(result.offline, "{:?}", result.reports);
    assert_eq!(Instant::now() - start, Duration::ZERO);

    // 続けて読んでも、待つのは NDL の 1 秒間隔（レート制限）だけ。1 冊ごとの 7 秒のリトライは待たない
    let start = Instant::now();
    for isbn in [KOKORO, YUKIGUNI] {
        let result = r.service.fetch(&FetchRequest::new(isbn)).await;
        assert!(result.offline, "{isbn}: {:?}", result.reports);
    }
    assert!(Instant::now() - start <= Duration::from_secs(2));
}

/// タイムアウトは、通信路が一時的に不安定なだけのことがあるので、今までどおりリトライする。
/// 3 回やり直しても応答がなければ、`offline`。
#[tokio::test(start_paused = true)]
async fn timeouts_are_still_retried_before_reporting_offline() {
    let http = FakeHttp::new(Box::new(|_| Err(TransportError::Timeout)));
    let r = rig_with(http, None);
    let start = Instant::now();
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert!(result.offline, "{:?}", result.reports);
    assert_eq!(r.http.urls_containing("api.openbd.jp").len(), 4);
    assert!(
        Instant::now() - start >= Duration::from_secs(7),
        "1 + 2 + 4 秒"
    );
}

#[tokio::test(start_paused = true)]
async fn one_provider_down_does_not_stop_the_others() {
    // Google が 503 を返し続ける（リトライしても回復しない）
    let base = fixtures_handler();
    let http = FakeHttp::new(Box::new(move |url| {
        if url.contains("googleapis.com") {
            response(
                503,
                "application/json",
                r#"{"error":{"message":"overloaded"}}"#.into(),
            )
        } else {
            base(url)
        }
    }));
    let r = rig_with(http, None);
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert!(!result.offline, "つながっているのでオフラインではない");
    assert!(result.has_error());
    assert!(matches!(
        outcome(&result, ProviderId::Google),
        ProviderOutcome::HttpError { status: 503, .. }
    ));
    assert_eq!(outcome(&result, ProviderId::Openbd), ProviderOutcome::Found);
    assert_eq!(outcome(&result, ProviderId::Ndl), ProviderOutcome::Found);
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
    assert_eq!(r.http.urls_containing("googleapis.com").len(), 4);
}

#[tokio::test(start_paused = true)]
async fn google_daily_quota_is_reported_at_once_with_a_hint_about_the_api_key() {
    // 2026-10-14 に実際に返ってきた応答（キーなし）
    let base = fixtures_handler();
    let http = FakeHttp::new(Box::new(move |url| {
        if url.contains("googleapis.com") {
            response(
                429,
                "application/json; charset=UTF-8",
                fixture("google_quota_exceeded.json"),
            )
        } else {
            base(url)
        }
    }));
    let r = rig_with(http, None);
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    match outcome(&result, ProviderId::Google) {
        ProviderOutcome::HttpError { status, message } => {
            assert_eq!(status, 429);
            assert!(message.contains("Queries per day"), "{message}");
            assert!(message.contains("API キー"), "{message}");
            assert!(
                message.contains(ZERO_QUOTA_NOTE),
                "上限が 0（quota_limit_value）の印が付く: {message}"
            );
            assert_eq!(classify_failure(status, &message), GoogleFailure::ZeroQuota);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        r.http.urls_containing("googleapis.com").len(),
        1,
        "1 日の上限はリトライしない"
    );
    // キーを持っているときは、案内を付けない
    let base = fixtures_handler();
    let http = FakeHttp::new(Box::new(move |url| {
        if url.contains("googleapis.com") {
            response(
                429,
                "application/json; charset=UTF-8",
                fixture("google_quota_exceeded.json"),
            )
        } else {
            base(url)
        }
    }));
    let r = rig_with(http, Some("KEY"));
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    match outcome(&result, ProviderId::Google) {
        ProviderOutcome::HttpError { message, .. } => {
            assert!(!message.contains("API キーを設定"), "{message}")
        }
        other => panic!("{other:?}"),
    }
}

/// Google がエラーを返したとき、`classify_failure` が、利用者に伝えるべき意味（キーが無効か、1 日の上限か、
/// 割り当て 0 か）を取り出せる。400・403 の本文は、Google の API の標準のエラーの形（`error.status`・
/// `error.details[].reason`）に合わせた合成（実際のキーで確かめたものではない）。
#[tokio::test(start_paused = true)]
async fn google_http_errors_are_classified_by_what_the_user_needs_to_know() {
    let zero = fixture("google_quota_exceeded.json");
    // 1 日の上限に達したときは、上限が 0 ではない（実際の応答の `quota_limit_value` を差し替えた合成）
    let daily = zero.replace(
        "\"quota_limit_value\": \"0\"",
        "\"quota_limit_value\": \"1000\"",
    );
    assert_ne!(daily, zero);
    let invalid_key = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","errors":[{"message":"API key not valid. Please pass a valid API key.","domain":"global","reason":"badRequest"}],"status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID","domain":"googleapis.com","metadata":{"service":"books.googleapis.com"}}]}}"#;
    let expired = r#"{"error":{"code":400,"message":"API key expired. Please renew the API key.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID"}]}}"#;
    let disabled = r#"{"error":{"code":403,"message":"Books API has not been used in project 123 before or it is disabled. Enable it by visiting https://console.developers.google.com/apis/api/books.googleapis.com/overview?project=123 then retry.","status":"PERMISSION_DENIED","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"SERVICE_DISABLED","domain":"googleapis.com","metadata":{"service":"books.googleapis.com"}}]}}"#;
    let restricted = r#"{"error":{"code":403,"message":"Requests from referer <empty> are blocked.","status":"PERMISSION_DENIED"}}"#;
    let unauthenticated = r#"{"error":{"code":401,"message":"Request had invalid authentication credentials.","status":"UNAUTHENTICATED"}}"#;
    let per_minute = r#"{"error":{"code":429,"message":"Quota exceeded for quota metric 'Queries' and limit 'Queries per minute' of service 'books.googleapis.com'","status":"RESOURCE_EXHAUSTED","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"RATE_LIMIT_EXCEEDED","metadata":{"quota_limit_value":"60"}}]}}"#;
    let server_error = r#"{"error":{"code":503,"message":"The service is currently unavailable.","status":"UNAVAILABLE"}}"#;

    let cases: Vec<(u16, String, GoogleFailure)> = vec![
        (429, zero, GoogleFailure::ZeroQuota),
        (429, daily, GoogleFailure::DailyQuota),
        (400, invalid_key.to_owned(), GoogleFailure::InvalidKey),
        (400, expired.to_owned(), GoogleFailure::InvalidKey),
        (403, disabled.to_owned(), GoogleFailure::InvalidKey),
        (403, restricted.to_owned(), GoogleFailure::InvalidKey),
        (401, unauthenticated.to_owned(), GoogleFailure::InvalidKey),
        (429, per_minute.to_owned(), GoogleFailure::Other),
        (503, server_error.to_owned(), GoogleFailure::Other),
    ];
    for (status, body, expected) in cases {
        let base = fixtures_handler();
        let http = FakeHttp::new(Box::new(move |url| {
            if url.contains("googleapis.com") {
                response(status, "application/json; charset=UTF-8", body.clone())
            } else {
                base(url)
            }
        }));
        let r = rig_with(http, Some("KEY"));
        let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
        match outcome(&result, ProviderId::Google) {
            ProviderOutcome::HttpError {
                status: got,
                message,
            } => {
                assert_eq!(got, status);
                assert_eq!(
                    classify_failure(got, &message),
                    expected,
                    "{status}: {message}"
                );
            }
            other => panic!("{status}: {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn transient_failure_is_retried_and_succeeds() {
    let base = fixtures_handler();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let http = FakeHttp::new(Box::new(move |url| {
        if url.contains("api.openbd.jp") && c.fetch_add(1, Ordering::SeqCst) < 2 {
            return Err(TransportError::Timeout);
        }
        base(url)
    }));
    let r = rig_with(http, None);
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert_eq!(outcome(&result, ProviderId::Openbd), ProviderOutcome::Found);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn parse_errors_are_reported_and_not_cached() {
    let base = fixtures_handler();
    let http = FakeHttp::new(Box::new(move |url| {
        if url.contains("ndlsearch.ndl.go.jp") {
            response(200, "text/html", "<html>メンテナンス中</html>".into())
        } else {
            base(url)
        }
    }));
    let r = rig_with(http, None);
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert!(matches!(
        outcome(&result, ProviderId::Ndl),
        ProviderOutcome::ParseError { .. }
    ));
    assert!(r.cache.entry(ProviderId::Ndl, ONIGIRI).is_none());
    assert!(r.cache.entry(ProviderId::Openbd, ONIGIRI).is_some());
    assert!(!result.offline);
    assert!(result.has_error());
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
}

// ---------------------------------------------------------------------------
// キャッシュ（FR-META-10）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cache_avoids_the_network_until_it_expires() {
    let r = rig();
    let first = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert_eq!(
        r.cache.len(),
        4,
        "openBD・NDL・Google・Open Library（該当なし）"
    );
    assert!(
        !r.cache
            .entry(ProviderId::OpenLibrary, ONIGIRI)
            .unwrap()
            .found
    );
    let e = r.cache.entry(ProviderId::Openbd, ONIGIRI).unwrap();
    assert!(e.found);
    assert_eq!(e.fetched_at, 1_800_000_000);
    assert!(e.body.contains("おにぎりレシピ101"));
    r.http.clear();

    // 29 日後: 書誌が取れた取得元はまだ有効。通信しない。結果は同じ。
    // 「該当なし」の Open Library だけは、1 日で期限切れなので取り直す（見つからないまま）
    r.clock.fetch_add(29 * 86_400, Ordering::SeqCst);
    let second = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    let urls = r.http.urls();
    assert_eq!(urls.len(), 1, "{urls:?}");
    assert!(urls[0].starts_with("https://openlibrary.org/search.json?"));
    assert_eq!(second.merged, first.merged);
    assert!(report(&second, ProviderId::Openbd).from_cache);
    assert!(report(&second, ProviderId::Ndl).from_cache);
    assert!(report(&second, ProviderId::Google).from_cache);
    assert!(!report(&second, ProviderId::OpenLibrary).from_cache);
    r.http.clear();

    // 31 日後: 期限切れ。取り直して、保存し直す
    r.clock.fetch_add(2 * 86_400, Ordering::SeqCst);
    let third = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert_eq!(r.http.urls().len(), 4);
    assert!(!report(&third, ProviderId::Openbd).from_cache);
    assert_eq!(
        r.cache
            .entry(ProviderId::Openbd, ONIGIRI)
            .unwrap()
            .fetched_at,
        1_800_000_000 + 31 * 86_400
    );
    assert_eq!(DEFAULT_CACHE_TTL, Duration::from_secs(30 * 86_400));
}

#[tokio::test]
async fn not_found_is_cached_for_a_shorter_time() {
    let r = rig();
    r.service.fetch(&FetchRequest::new(MISSING)).await;
    let e = r.cache.entry(ProviderId::Openbd, MISSING).unwrap();
    assert!(!e.found);
    r.http.clear();
    r.clock.fetch_add(23 * 3600, Ordering::SeqCst);
    r.service.fetch(&FetchRequest::new(MISSING)).await;
    assert!(r.http.urls().is_empty());
    r.clock.fetch_add(2 * 3600, Ordering::SeqCst);
    r.service.fetch(&FetchRequest::new(MISSING)).await;
    assert_eq!(
        r.http.urls().len(),
        5,
        "25 時間後は取り直す（Google は、検索が該当なしなので Dynamic Links も 1 回）"
    );
    assert_eq!(DEFAULT_NOT_FOUND_TTL, Duration::from_secs(86_400));
}

#[tokio::test]
async fn force_refresh_ignores_the_cache_and_updates_it() {
    let r = rig();
    r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    r.http.clear();
    r.clock.fetch_add(100, Ordering::SeqCst);
    let result = r
        .service
        .fetch(&FetchRequest::new(ONIGIRI).force_refresh())
        .await;
    assert_eq!(r.http.urls().len(), 4);
    assert!(!report(&result, ProviderId::Openbd).from_cache);
    assert_eq!(
        r.cache
            .entry(ProviderId::Openbd, ONIGIRI)
            .unwrap()
            .fetched_at,
        1_800_000_100
    );
}

#[tokio::test]
async fn reparse_from_cache_needs_no_network_when_the_mapping_changes() {
    // 生の応答を保存してあれば、`parse` だけをやり直せる（FR-META-10 の目的）。
    // ここでは、キャッシュに入れた応答から、通信なしで同じ結果が得られることを確かめる。
    let cache = Arc::new(MemoryRawCache::new());
    cache
        .put(
            ProviderId::Openbd,
            ONIGIRI,
            CacheEntry {
                body: fixture("openbd_9784780802047.json"),
                fetched_at: 1_800_000_000,
                found: true,
            },
        )
        .await
        .unwrap();
    let http = FakeHttp::new(Box::new(|_| panic!("通信してはいけない")));
    let service = MetadataService::new(
        vec![Arc::new(OpenBd::new()) as Arc<dyn Provider>],
        Arc::new(Transport::new(http)),
    )
    .with_cache(cache)
    .with_clock(Arc::new(|| 1_800_000_100));
    let result = service
        .fetch(&FetchRequest::new(ONIGIRI).with_enabled(vec![ProviderId::Openbd]))
        .await;
    assert_eq!(result.merged.text(MetaField::CCode), Some("0077"));
    assert!(result.reports[0].from_cache);
}

#[tokio::test]
async fn unparsable_cache_entry_is_refetched() {
    let r = rig();
    r.cache
        .put(
            ProviderId::Openbd,
            ONIGIRI,
            CacheEntry {
                body: "壊れた".into(),
                fetched_at: 1_800_000_000,
                found: true,
            },
        )
        .await
        .unwrap();
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert_eq!(outcome(&result, ProviderId::Openbd), ProviderOutcome::Found);
    assert!(!report(&result, ProviderId::Openbd).from_cache);
    assert!(
        r.cache
            .entry(ProviderId::Openbd, ONIGIRI)
            .unwrap()
            .body
            .contains("おにぎりレシピ101")
    );
}

#[tokio::test]
async fn network_errors_are_not_cached_and_a_broken_cache_does_not_stop_fetching() {
    struct BrokenCache;
    #[async_trait]
    impl RawCache for BrokenCache {
        async fn get(&self, _: ProviderId, _: &str) -> Result<Option<CacheEntry>, CacheError> {
            Err(CacheError("disk full".into()))
        }
        async fn put(&self, _: ProviderId, _: &str, _: CacheEntry) -> Result<(), CacheError> {
            Err(CacheError("disk full".into()))
        }
    }
    let service = MetadataService::new(
        default_providers(None),
        Arc::new(Transport::new(FakeHttp::new(fixtures_handler()))),
    )
    .with_cache(Arc::new(BrokenCache));
    let result = service.fetch(&FetchRequest::new(ONIGIRI)).await;
    assert_eq!(result.found_count(), 3);
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
}

// ---------------------------------------------------------------------------
// レート制限・並列・一括
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn fetch_many_batches_openbd_and_paces_the_others() {
    let r = rig();
    let isbns = [ONIGIRI, KOKORO, YUKIGUNI];
    let reqs: Vec<FetchRequest> = isbns.iter().map(|i| FetchRequest::new(i)).collect();
    let results = r.service.fetch_many(&reqs).await;
    assert_eq!(results.len(), 3);
    assert_eq!(
        results[0].merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
    assert_eq!(results[1].merged.text(MetaField::Title), Some("こころ"));
    assert_eq!(results[2].merged.text(MetaField::Title), Some("雪国"));
    assert_eq!(results[2].merged.number(MetaField::Pages), Some(208));

    // openBD は 1 回の通信（カンマ区切り）
    let openbd = r.http.urls_containing("api.openbd.jp");
    assert_eq!(openbd.len(), 1);
    assert!(
        openbd[0]
            .1
            .ends_with(&format!("isbn={ONIGIRI},{KOKORO},{YUKIGUNI}")),
        "{}",
        openbd[0].1
    );
    // NDL・Google・Open Library は 1 件ずつ、1 秒以上あけて
    for host in ["ndlsearch.ndl.go.jp", "googleapis.com", "openlibrary.org"] {
        let calls = r.http.urls_containing(host);
        assert_eq!(calls.len(), 3, "{host}");
        for w in calls.windows(2) {
            assert!(
                w[1].0 - w[0].0 >= Duration::from_secs(1),
                "{host}: {:?}",
                w[1].0 - w[0].0
            );
        }
    }
    // 同じ ISBN を重複して頼んでも 1 回しか取らない
    r.http.clear();
    r.cache_clear_for_test();
    let reqs = vec![
        FetchRequest::new(ONIGIRI),
        FetchRequest::new("9784780802047"),
    ];
    let results = r.service.fetch_many(&reqs).await;
    assert_eq!(results[0].merged, results[1].merged);
    assert_eq!(r.http.urls().len(), 4);
}

impl Rig {
    /// キャッシュを空にするかわりに、期限切れにする。
    fn cache_clear_for_test(&self) {
        self.clock.fetch_add(40 * 86_400, Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn ndl_is_never_called_in_parallel_even_for_concurrent_fetches() {
    let http = FakeHttp::with_latency(fixtures_handler(), Duration::from_millis(300));
    let r = rig_with(http, None);
    let svc = &r.service;
    let reqs: Vec<FetchRequest> = [ONIGIRI, KOKORO, YUKIGUNI]
        .iter()
        .map(|i| FetchRequest::new(i))
        .collect();
    let results = futures_util::future::join_all(reqs.iter().map(|req| svc.fetch(req))).await;
    // 雪国は Google の合成フィクスチャにない（該当なし）ので 2 件
    assert!(results.iter().all(|r| r.found_count() >= 2));
    assert_eq!(r.http.ndl_max_in_flight.load(Ordering::SeqCst), 1);
    let calls = r.http.urls_containing("ndlsearch.ndl.go.jp");
    assert_eq!(calls.len(), 3);
    for w in calls.windows(2) {
        assert!(w[1].0 - w[0].0 >= Duration::from_secs(1));
    }
}

#[tokio::test]
async fn provider_set_can_be_customized() {
    let service = MetadataService::new(
        vec![
            Arc::new(Ndl::new()) as Arc<dyn Provider>,
            Arc::new(GoogleBooks::new(None)),
        ],
        Arc::new(Transport::new(FakeHttp::new(fixtures_handler()))),
    );
    let result = service.fetch(&FetchRequest::new(ONIGIRI)).await;
    // openBD は登録されていないので、有効にしても呼ばれない
    assert!(
        result
            .reports
            .iter()
            .all(|r| r.provider != ProviderId::Openbd)
    );
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
    assert_eq!(
        result.merged.source(MetaField::Title),
        Some(MetaSource::Ndl)
    );
}

#[tokio::test]
async fn acquire_cover_uses_the_merged_cover_order_and_the_shared_transport() {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};

    let img = RgbImage::from_fn(700, 1000, |x, y| Rgb([(x / 3) as u8, (y / 4) as u8, 50]));
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 90)
        .write_image(img.as_raw(), 700, 1000, ExtendedColorType::Rgb8)
        .unwrap();
    let base = fixtures_handler();
    let http = FakeHttp::new(Box::new(move |url| {
        if url.starts_with("https://cover.openbd.jp/") {
            return Ok(HttpResponse {
                status: 200,
                content_type: Some("image/jpeg".into()),
                retry_after: None,
                body: jpeg.clone(),
            });
        }
        base(url)
    }));
    let r = rig_with(http, None);
    let result = r.service.fetch(&FetchRequest::new(ONIGIRI)).await;
    let dir = tempfile::tempdir().unwrap();
    let cover = r.service.acquire_cover(&result.merged, dir.path()).await;
    let saved = cover.saved.expect("openBD の書影を保存できた");
    assert_eq!(saved.provider, ProviderId::Openbd);
    assert_eq!((saved.width, saved.height), (560, 800));
    assert!(bibcontainer_metadata::cover::cover_path(dir.path(), &saved.hash).exists());
    // 2 回目は同じハッシュ。書き直さない
    let again = r.service.acquire_cover(&result.merged, dir.path()).await;
    assert_eq!(again.saved.unwrap().hash, saved.hash);
}
