//! Google Books の ISBN 検索が 0 件を返し続ける障害（2026-10-01 ごろから）への回避策のテスト。
//! 検索が「該当なし」のとき、Dynamic Links（キーなし）で volume ID を得て、`volumes/<ID>` を取る。
//! HTTP の層は偽物に差し替え、応答は `tests/fixtures/` のもの（Dynamic Links は実際の応答、volume は合成）を返す。
//! ネットワークは使わない。仕様は `providers/google.rs` の先頭を参照。

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bibcontainer_metadata::error::FetchError;
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::provider::{
    CacheEntry, MemoryRawCache, Provider, RawCache, RawResponse,
};
use bibcontainer_metadata::providers::GoogleBooks;
use bibcontainer_metadata::providers::google::{
    DYNAMIC_LINKS_NOTE, GoogleFailure, ZERO_QUOTA_NOTE, classify_failure,
};
use bibcontainer_metadata::service::{FetchRequest, MetadataService, ProviderOutcome};
use bibcontainer_metadata::types::ProviderId;
use bibcontainer_metadata::{RetryPolicy, Transport};
use common::fixture;
use tokio::time::Instant;

const CLEAN_CODE: &str = "9780132350884";
const CLEAN_CODE_ID: &str = "hjEFCAAAQBAJ";
const KOKORO: &str = "9784003101117";
const KEY: &str = "SECRET-KEY-123";

const SEARCH_URL_START: &str = "https://www.googleapis.com/books/v1/volumes?q=isbn:";

type Reply = Result<HttpResponse, TransportError>;

fn reply(status: u16, content_type: &str, body: impl Into<String>) -> Reply {
    Ok(HttpResponse {
        status,
        content_type: Some(content_type.to_string()),
        retry_after: None,
        body: body.into().into_bytes(),
    })
}

fn json(status: u16, body: impl Into<String>) -> Reply {
    reply(status, "application/json; charset=UTF-8", body)
}

fn jsonp(body: impl Into<String>) -> Reply {
    reply(200, "application/javascript; charset=ISO-8859-1", body)
}

/// 3 種類の通信（検索・Dynamic Links・volume）に、決めた応答を返す偽の HTTP クライアント。呼ばれた時刻と URL を記録する。
struct Fake {
    search: Reply,
    dynamic_links: Reply,
    volume: Reply,
    log: Mutex<Vec<(Instant, String)>>,
}

impl Fake {
    fn new(search: Reply, dynamic_links: Reply, volume: Reply) -> Arc<Self> {
        Arc::new(Fake {
            search,
            dynamic_links,
            volume,
            log: Mutex::new(Vec::new()),
        })
    }

    /// 検索が 0 件（障害のあいだの応答）。
    fn empty_search(dynamic_links: Reply, volume: Reply) -> Arc<Self> {
        Fake::new(
            json(200, fixture("google_synthetic_notfound.json")),
            dynamic_links,
            volume,
        )
    }

    fn urls(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .map(|(_, u)| u.clone())
            .collect()
    }

    fn times(&self) -> Vec<Instant> {
        self.log.lock().unwrap().iter().map(|(t, _)| *t).collect()
    }
}

#[async_trait]
impl HttpClient for Fake {
    async fn get(&self, req: HttpRequest) -> Reply {
        self.log
            .lock()
            .unwrap()
            .push((Instant::now(), req.url.clone()));
        if req.url.contains("books?bibkeys=") {
            self.dynamic_links.clone()
        } else if req.url.contains("/books/v1/volumes?q=") {
            self.search.clone()
        } else if req.url.contains("/books/v1/volumes/") {
            self.volume.clone()
        } else {
            panic!("想定していない通信: {}", req.url)
        }
    }
}

fn no_retry() -> RetryPolicy {
    RetryPolicy {
        max_retries: 0,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
    }
}

fn transport(http: &Arc<Fake>) -> Transport {
    Transport::new(http.clone()).with_retry(no_retry())
}

async fn fetch(http: &Arc<Fake>, isbn: &str) -> Result<RawResponse, FetchError> {
    GoogleBooks::new(Some(KEY.to_string()))
        .fetch_raw(&transport(http), isbn)
        .await
}

fn dl_url(isbn: &str) -> String {
    format!("https://books.google.com/books?bibkeys=ISBN:{isbn}&jscmd=viewapi&callback=cb")
}

fn volume_url(id: &str) -> String {
    format!("https://www.googleapis.com/books/v1/volumes/{id}?key={KEY}")
}

fn clean_code_volume() -> String {
    fixture("google_synthetic_volume_hjEFCAAAQBAJ.json")
}

fn clean_code_dl() -> String {
    fixture("google_dynamiclinks_9780132350884.jsonp")
}

// ---------------------------------------------------------------------------
// 検索で見つかるとき: 回避策は動かない
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_search_that_finds_the_book_makes_exactly_one_request() {
    let search = fixture("google_synthetic_9784003101117.json");
    // 検索で見つかるなら、Dynamic Links も volume も呼ばない（呼ばれたら panic するのではなく、件数で確かめる）
    let http = Fake::new(json(200, search.clone()), jsonp("cb({});"), json(500, "{}"));
    let raw = fetch(&http, KOKORO).await.unwrap();
    assert_eq!(
        raw.body, search,
        "検索の応答をそのまま返す（これまでどおり）"
    );
    assert_eq!(http.urls().len(), 1);
    assert!(http.urls()[0].starts_with(SEARCH_URL_START));
    let c = GoogleBooks::new(None).parse(&raw).unwrap().unwrap();
    assert_eq!(c.title.as_deref(), Some("こころ"));
}

#[tokio::test]
async fn search_http_errors_and_unreadable_bodies_do_not_start_the_fallback() {
    // HTTP エラー（これまでどおりの意味づけ）
    let quota = fixture("google_quota_exceeded.json");
    let http = Fake::new(json(429, quota), jsonp("cb({});"), json(500, "{}"));
    let err = fetch(&http, CLEAN_CODE).await.unwrap_err();
    assert!(
        matches!(err, FetchError::Http { status: 429, .. }),
        "{err:?}"
    );
    assert_eq!(http.urls().len(), 1);

    // 解析できない応答は、そのまま返す（`parse` が解析エラーにする）。回避策には進まない
    let http = Fake::new(
        json(200, "<html>x</html>"),
        jsonp("cb({});"),
        json(500, "{}"),
    );
    let raw = fetch(&http, CLEAN_CODE).await.unwrap();
    assert!(GoogleBooks::new(None).parse(&raw).is_err());
    assert_eq!(http.urls().len(), 1);

    // 検索自体の通信エラーも、そのまま
    let http = Fake::new(
        Err(TransportError::Connect("offline".into())),
        jsonp("cb({});"),
        json(500, "{}"),
    );
    assert!(matches!(
        fetch(&http, CLEAN_CODE).await.unwrap_err(),
        FetchError::Network(_)
    ));
    assert_eq!(http.urls().len(), 1);
}

// ---------------------------------------------------------------------------
// 検索が 0 件: Dynamic Links → volume
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn empty_search_and_empty_dynamic_links_is_not_found_after_two_requests() {
    let http = Fake::empty_search(
        jsonp(fixture("google_dynamiclinks_notfound.jsonp")),
        json(500, "{}"),
    );
    let raw = fetch(&http, "9784123456784").await.unwrap();
    assert_eq!(
        raw.body,
        fixture("google_synthetic_notfound.json"),
        "該当なしは、検索の応答を返す"
    );
    assert_eq!(GoogleBooks::new(None).parse(&raw).unwrap(), None);
    let urls = http.urls();
    assert_eq!(urls.len(), 2, "{urls:?}");
    assert!(urls[0].starts_with(SEARCH_URL_START));
    assert_eq!(urls[1], dl_url("9784123456784"));
}

#[tokio::test(start_paused = true)]
async fn empty_search_with_a_dynamic_links_id_and_a_matching_volume_is_found_after_three_requests()
{
    let volume = clean_code_volume();
    let http = Fake::empty_search(jsonp(clean_code_dl()), json(200, volume.clone()));
    let raw = fetch(&http, CLEAN_CODE).await.unwrap();

    // 3 回の通信: 検索、Dynamic Links、volume。1 秒ずつあける（同じレート制限のグループ）
    let urls = http.urls();
    assert_eq!(urls.len(), 3, "{urls:?}");
    assert!(urls[0].starts_with(SEARCH_URL_START));
    assert_eq!(urls[1], dl_url(CLEAN_CODE));
    assert_eq!(urls[2], volume_url(CLEAN_CODE_ID));
    let times = http.times();
    assert!(times[1] - times[0] >= Duration::from_secs(1));
    assert!(times[2] - times[1] >= Duration::from_secs(1));

    // 生の応答は、volume の JSON を受け取ったまま
    assert_eq!(raw.provider, ProviderId::Google);
    assert_eq!(raw.isbn13, CLEAN_CODE);
    assert_eq!(raw.body, volume);

    // 検索の応答と同じように、候補になる。書影は zoom=0 → zoom=1、内容紹介は HTML を除いたもの
    let c = GoogleBooks::new(None).parse(&raw).unwrap().unwrap();
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    let covers: Vec<&str> = c.covers.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        covers,
        vec![
            "https://books.google.com/books/content?id=hjEFCAAAQBAJ&printsec=frontcover&img=1&zoom=0",
            "https://books.google.com/books/content?id=hjEFCAAAQBAJ&printsec=frontcover&img=1&zoom=1",
        ]
    );
    let d = c.description.unwrap();
    assert!(
        !d.contains('<') && !d.contains("&amp;") && !d.contains("&#") && !d.contains("&nbsp;"),
        "{d}"
    );
    assert!(d.contains("\n\nWhat you will learn:\n\n"), "{d}");
}

#[tokio::test(start_paused = true)]
async fn the_api_key_is_never_sent_to_dynamic_links() {
    let http = Fake::empty_search(jsonp(clean_code_dl()), json(200, clean_code_volume()));
    fetch(&http, CLEAN_CODE).await.unwrap();
    let urls = http.urls();
    assert_eq!(urls.len(), 3);
    // 検索と volume には付く
    assert!(urls[0].contains(&format!("key={KEY}")), "{}", urls[0]);
    assert!(urls[2].ends_with(&format!("?key={KEY}")), "{}", urls[2]);
    // books.google.com の Dynamic Links には、キーも、キーの値も付かない
    assert!(urls[1].starts_with("https://books.google.com/books?"));
    assert!(!urls[1].contains("key="), "{}", urls[1]);
    assert!(!urls[1].contains(KEY), "{}", urls[1]);
    // books.google.com に出ていく通信は、Dynamic Links だけ（ほかには出ない）
    assert_eq!(
        urls.iter()
            .filter(|u| u.contains("books.google.com"))
            .count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn a_volume_for_another_isbn_or_a_404_is_not_found() {
    // Dynamic Links が、別の本の ID を返した（volume の ISBN が合わない）
    let http = Fake::empty_search(
        jsonp(clean_code_dl()),
        json(200, fixture("google_synthetic_volume_fz5TPAAACAAJ.json")),
    );
    let raw = fetch(&http, CLEAN_CODE).await.unwrap();
    assert_eq!(raw.body, fixture("google_synthetic_notfound.json"));
    assert_eq!(GoogleBooks::new(None).parse(&raw).unwrap(), None);
    assert_eq!(http.urls().len(), 3);

    // volume が 404（ID が存在しない）
    let not_found = r#"{"error":{"code":404,"message":"The volume ID could not be found.","status":"NOT_FOUND"}}"#;
    let http = Fake::empty_search(jsonp(clean_code_dl()), json(404, not_found));
    let raw = fetch(&http, CLEAN_CODE).await.unwrap();
    assert_eq!(GoogleBooks::new(None).parse(&raw).unwrap(), None);
    assert_eq!(http.urls().len(), 3);

    // 識別子がない volume は、検索の応答と同じく採用する
    let no_ids =
        r#"{"kind":"books#volume","id":"hjEFCAAAQBAJ","volumeInfo":{"title":"識別子なし"}}"#;
    let http = Fake::empty_search(jsonp(clean_code_dl()), json(200, no_ids));
    let raw = fetch(&http, CLEAN_CODE).await.unwrap();
    assert_eq!(raw.body, no_ids);
    // 200 でも volumeInfo のない応答（想定外の形）は、該当なしではなく解析エラー
    for body in [
        "{}",
        r#"{"kind":"books#volume","id":"hjEFCAAAQBAJ"}"#,
        "[]",
        "x",
    ] {
        let http = Fake::empty_search(jsonp(clean_code_dl()), json(200, body));
        assert!(
            matches!(fetch(&http, CLEAN_CODE).await, Err(FetchError::Parse(_))),
            "{body}"
        );
    }
}

// ---------------------------------------------------------------------------
// 失敗: キャッシュされず、意味づけは検索と同じ
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn dynamic_links_failures_fail_the_whole_fetch_and_are_not_cached() {
    let service_for = |http: &Arc<Fake>| {
        let cache = Arc::new(MemoryRawCache::new());
        let service = MetadataService::new(
            vec![Arc::new(GoogleBooks::new(Some(KEY.to_string()))) as Arc<dyn Provider>],
            Arc::new(transport(http)),
        )
        .with_cache(cache.clone());
        (service, cache)
    };
    let req = FetchRequest::new(CLEAN_CODE).with_enabled(vec![ProviderId::Google]);

    // HTTP エラー（captcha の 429、5xx など）: Google の取得全体の失敗。保存しない
    for status in [429, 500, 403] {
        let http = Fake::empty_search(
            reply(status, "text/html", "<html><b>unusual traffic</b></html>"),
            json(200, clean_code_volume()),
        );
        let (service, cache) = service_for(&http);
        let result = service.fetch(&req).await;
        match &result.reports[0].outcome {
            ProviderOutcome::HttpError {
                status: got,
                message,
            } => {
                assert_eq!(*got, status);
                assert!(message.contains("unusual traffic"), "{message}");
                // キーのエラーとは読まない（403 でも）
                assert!(message.contains(DYNAMIC_LINKS_NOTE), "{message}");
                assert_eq!(classify_failure(*got, message), GoogleFailure::Other);
            }
            other => panic!("{status}: {other:?}"),
        }
        assert!(cache.is_empty(), "{status}: 失敗は保存しない");
        assert_eq!(http.urls().len(), 2, "{status}: volume は取りにいかない");
        assert!(result.has_error());
        assert!(!result.offline, "{status}");
    }

    // JSONP でない・読めない応答: 解析エラー。保存しない
    for body in [
        "<html>Our systems have detected unusual traffic</html>",
        "cb({not json});",
        "cb([1,2]);",
        "",
    ] {
        let http = Fake::empty_search(jsonp(body), json(200, clean_code_volume()));
        let (service, cache) = service_for(&http);
        let result = service.fetch(&req).await;
        assert!(
            matches!(
                result.reports[0].outcome,
                ProviderOutcome::ParseError { .. }
            ),
            "{body:?}: {:?}",
            result.reports[0].outcome
        );
        assert!(cache.is_empty(), "{body:?}");
        assert_eq!(http.urls().len(), 2, "{body:?}");
    }

    // 通信エラー（Dynamic Links だけつながらない）: 通信エラー。保存しない
    let http = Fake::empty_search(Err(TransportError::Timeout), json(200, clean_code_volume()));
    let (service, cache) = service_for(&http);
    let result = service.fetch(&req).await;
    assert!(matches!(
        result.reports[0].outcome,
        ProviderOutcome::NetworkError { .. }
    ));
    assert!(cache.is_empty());
}

/// 既定のリトライ（1・2・4 秒）は Dynamic Links にも効く。それでも 5xx なら、その HTTP エラーを返す。
#[tokio::test(start_paused = true)]
async fn dynamic_links_5xx_is_retried_by_the_transport_before_failing() {
    let http = Fake::empty_search(
        reply(503, "text/html", "<html>busy</html>"),
        json(200, clean_code_volume()),
    );
    let t = Transport::new(http.clone()); // 既定のリトライ（最大 3 回）
    let err = GoogleBooks::new(Some(KEY.to_string()))
        .fetch_raw(&t, CLEAN_CODE)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, FetchError::Http { status: 503, message } if message.contains(DYNAMIC_LINKS_NOTE)),
        "{err:?}"
    );
    // 検索 1 回 + Dynamic Links 4 回（最初の 1 回 + リトライ 3 回）
    assert_eq!(http.urls().len(), 5);
}

#[tokio::test(start_paused = true)]
async fn volume_http_errors_are_classified_exactly_like_search_errors() {
    let zero = fixture("google_quota_exceeded.json");
    let daily = zero.replace(
        "\"quota_limit_value\": \"0\"",
        "\"quota_limit_value\": \"1000\"",
    );
    let invalid_key = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID"}]}}"#;
    let expired = r#"{"error":{"code":400,"message":"API key expired. Please renew the API key.","status":"INVALID_ARGUMENT"}}"#;
    let disabled = r#"{"error":{"code":403,"message":"Books API has not been used in project 123 before or it is disabled.","status":"PERMISSION_DENIED"}}"#;
    let unauthenticated = r#"{"error":{"code":401,"message":"Request had invalid authentication credentials.","status":"UNAUTHENTICATED"}}"#;
    let per_minute = r#"{"error":{"code":429,"message":"Quota exceeded for limit 'Queries per minute'","status":"RESOURCE_EXHAUSTED","details":[{"metadata":{"quota_limit_value":"60"}}]}}"#;
    let server_error = r#"{"error":{"code":503,"message":"The service is currently unavailable.","status":"UNAVAILABLE"}}"#;

    let cases: Vec<(u16, String, GoogleFailure)> = vec![
        (400, invalid_key.to_owned(), GoogleFailure::InvalidKey),
        (400, expired.to_owned(), GoogleFailure::InvalidKey),
        (403, disabled.to_owned(), GoogleFailure::InvalidKey),
        (401, unauthenticated.to_owned(), GoogleFailure::InvalidKey),
        (429, zero.clone(), GoogleFailure::ZeroQuota),
        (429, daily, GoogleFailure::DailyQuota),
        (429, per_minute.to_owned(), GoogleFailure::Other),
        (503, server_error.to_owned(), GoogleFailure::Other),
    ];
    for (status, body, expected) in cases {
        // 検索が同じ応答を返したときの失敗
        let http = Fake::new(
            json(status, body.clone()),
            jsonp("cb({});"),
            json(500, "{}"),
        );
        let from_search = fetch(&http, CLEAN_CODE).await.unwrap_err();
        // 検索は 0 件で、volume が同じ応答を返したときの失敗
        let http = Fake::empty_search(jsonp(clean_code_dl()), json(status, body.clone()));
        let from_volume = fetch(&http, CLEAN_CODE).await.unwrap_err();
        assert_eq!(http.urls().len(), 3, "{status}");
        assert_eq!(from_volume, from_search, "{status}: 検索と同じエラー");
        match from_volume {
            FetchError::Http {
                status: got,
                message,
            } => {
                assert_eq!(got, status);
                assert_eq!(
                    classify_failure(got, &message),
                    expected,
                    "{status}: {message}"
                );
                assert!(!message.contains(KEY));
                assert!(
                    !message.contains(DYNAMIC_LINKS_NOTE),
                    "volume のエラーは、キーの意味づけを受ける: {message}"
                );
                if expected == GoogleFailure::ZeroQuota {
                    assert!(message.contains(ZERO_QUOTA_NOTE));
                }
            }
            other => panic!("{status}: {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn an_invalid_volume_id_is_a_parse_error_and_no_volume_request_is_made() {
    for url in [
        "https://books.google.com/books?id=../../etc/passwd&source=gbs_ViewAPI",
        "https://books.google.com/books?id=ab%2Fcd1234",
        "https://books.google.com/books?id=abc",
        "https://books.google.com/books?id=abcdefghijklmnopqrstuvwxyz0123456",
        "https://books.google.com/books?id=ab?cd1234",
    ] {
        let dl = format!(
            r#"cb({{"ISBN:{CLEAN_CODE}":{{"bib_key":"ISBN:{CLEAN_CODE}","info_url":"{url}"}}}});"#
        );
        let http = Fake::empty_search(jsonp(dl), json(200, clean_code_volume()));
        let err = fetch(&http, CLEAN_CODE).await.unwrap_err();
        assert!(matches!(err, FetchError::Parse(_)), "{url}: {err:?}");
        let urls = http.urls();
        assert_eq!(urls.len(), 2, "{url}: volume を取りにいかない: {urls:?}");
        assert!(
            urls.iter().all(|u| !u.contains("/books/v1/volumes/")),
            "{urls:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// サービスを通した流れ（キャッシュ・取得元の状態）
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn through_the_service_found_and_not_found_are_cached_as_before() {
    let cache = Arc::new(MemoryRawCache::new());
    let http = Fake::empty_search(jsonp(clean_code_dl()), json(200, clean_code_volume()));
    let service = MetadataService::new(
        vec![Arc::new(GoogleBooks::new(Some(KEY.to_string()))) as Arc<dyn Provider>],
        Arc::new(transport(&http)),
    )
    .with_cache(cache.clone());
    let req = FetchRequest::new(CLEAN_CODE).with_enabled(vec![ProviderId::Google]);
    let result = service.fetch(&req).await;
    assert_eq!(result.reports[0].outcome, ProviderOutcome::Found);
    assert_eq!(result.candidates[0].title.as_deref(), Some("Clean Code"));
    // 保存するのは volume の JSON。2 回目はキャッシュで、通信しない
    let entry = cache.entry(ProviderId::Google, CLEAN_CODE).unwrap();
    assert!(entry.found);
    assert_eq!(entry.body, clean_code_volume());
    let again = service.fetch(&req).await;
    assert!(again.reports[0].from_cache);
    assert_eq!(http.urls().len(), 3);

    // 該当なし（Dynamic Links も `{}`）: 該当なしとして保存する（既定で 1 日）
    let cache = Arc::new(MemoryRawCache::new());
    let http = Fake::empty_search(jsonp("cb({});"), json(500, "{}"));
    let service = MetadataService::new(
        vec![Arc::new(GoogleBooks::new(Some(KEY.to_string()))) as Arc<dyn Provider>],
        Arc::new(transport(&http)),
    )
    .with_cache(cache.clone());
    let result = service.fetch(&req).await;
    assert_eq!(result.reports[0].outcome, ProviderOutcome::NotFound);
    let entry = cache.entry(ProviderId::Google, CLEAN_CODE).unwrap();
    assert!(!entry.found);
    assert_eq!(entry.body, fixture("google_synthetic_notfound.json"));
}

#[tokio::test(start_paused = true)]
async fn old_cached_search_bodies_still_parse_and_old_cache_entries_need_no_network() {
    // 回避策を入れる前にキャッシュした検索の応答（合成フィクスチャ）は、通信なしで同じ結果になる
    let cache = Arc::new(MemoryRawCache::new());
    cache
        .put(
            ProviderId::Google,
            KOKORO,
            CacheEntry {
                body: fixture("google_synthetic_9784003101117.json"),
                fetched_at: 1_800_000_000,
                found: true,
            },
        )
        .await
        .unwrap();
    let http = Fake::new(json(500, "{}"), jsonp("cb({});"), json(500, "{}"));
    let service = MetadataService::new(
        vec![Arc::new(GoogleBooks::new(Some(KEY.to_string()))) as Arc<dyn Provider>],
        Arc::new(transport(&http)),
    )
    .with_cache(cache)
    .with_clock(Arc::new(|| 1_800_000_000));
    let result = service
        .fetch(&FetchRequest::new(KOKORO).with_enabled(vec![ProviderId::Google]))
        .await;
    assert!(result.reports[0].from_cache);
    assert_eq!(result.reports[0].outcome, ProviderOutcome::Found);
    assert_eq!(result.candidates[0].title.as_deref(), Some("こころ"));
    assert_eq!(
        result.candidates[0].covers[0].url,
        "https://books.google.com/books/content?id=SYNTHETIC0001&printsec=frontcover&img=1&zoom=0&source=gbs_api"
    );
    assert!(http.urls().is_empty());
}
