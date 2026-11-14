//! 一括の再取得のための、取得元への問い合わせの工夫（0027 §4.2〜§4.4）のテスト。
//!
//! - Open Library の検索を 10 冊ずつにまとめる / Google の Dynamic Links を 10 冊ずつにまとめる。
//!   結果（統合した値・取得元の状態・キャッシュの本文）は、1 冊ずつ取ったときと同じ（AT-49）。
//! - 取得元が 401・403・429 を返したら、その呼び出しの残りは問い合わせず、同じエラーにする。
//!
//! HTTP の層は偽物。応答は `tests/fixtures/` のもの（実際の応答。Google の volume だけは合成）を使う。

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bibcontainer_metadata::error::FetchError;
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::provider::{MemoryRawCache, Provider, RawResponse};
use bibcontainer_metadata::providers::google::DYNAMIC_LINKS_NOTE;
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenLibrary, default_providers};
use bibcontainer_metadata::service::{FetchRequest, MetadataService, ProviderOutcome};
use bibcontainer_metadata::types::ProviderId;
use bibcontainer_metadata::{MetaFetchResult, RetryPolicy, Transport};
use common::fixture;
use serde_json::Value;

const CLEAN_CODE: &str = "9780132350884";
const PRIDE: &str = "9780141439518";
const RADIATION: &str = "9781493934362";
const KOKORO: &str = "9784003101117";
const ONIGIRI: &str = "9784780802047";
const CLEAN_CODE_ID: &str = "hjEFCAAAQBAJ";
const KEY: &str = "SECRET-KEY-123";

type Reply = Result<HttpResponse, TransportError>;
type Handler = Box<dyn Fn(&str) -> Reply + Send + Sync>;

fn reply(status: u16, content_type: &str, body: impl Into<String>) -> Reply {
    Ok(HttpResponse {
        status,
        content_type: Some(content_type.to_string()),
        retry_after: None,
        body: body.into().into_bytes(),
    })
}

fn json(body: impl Into<String>) -> Reply {
    reply(200, "application/json; charset=UTF-8", body)
}

fn jsonp(body: impl Into<String>) -> Reply {
    reply(200, "application/javascript; charset=ISO-8859-1", body)
}

fn status(code: u16) -> Reply {
    reply(code, "application/json", "{}")
}

struct Fake {
    handler: Handler,
    log: Mutex<Vec<String>>,
}

impl Fake {
    fn new(handler: Handler) -> Arc<Self> {
        Arc::new(Fake {
            handler,
            log: Mutex::new(Vec::new()),
        })
    }

    fn urls(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn count(&self, needle: &str) -> usize {
        self.urls().iter().filter(|u| u.contains(needle)).count()
    }
}

#[async_trait]
impl HttpClient for Fake {
    async fn get(&self, req: HttpRequest) -> Reply {
        self.log.lock().unwrap().push(req.url.clone());
        (self.handler)(&req.url)
    }
}

fn no_retry() -> RetryPolicy {
    RetryPolicy {
        max_retries: 0,
        base_delay: Duration::ZERO,
        max_delay: Duration::ZERO,
    }
}

/// リトライなし（429 を 1 回で受け取り、通信の回数を数えやすくする）。待ち時間は `start_paused` で飛ばす。
fn transport(http: &Arc<Fake>) -> Transport {
    Transport::new(http.clone()).with_retry(no_retry())
}

/// 先頭 12 桁から、チェックデジットを付けた ISBN-13 を作る。
fn isbn13(first12: &str) -> String {
    assert_eq!(first12.len(), 12);
    let sum: u32 = first12
        .bytes()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    format!("{first12}{}", (10 - sum % 10) % 10)
}

/// どの取得元にも収録のない ISBN（洋書 `978-0`・和書 `978-4`）を `n` 冊。
fn unknown_isbns(prefix: &str, n: usize) -> Vec<String> {
    (0..n)
        .map(|i| isbn13(&format!("{prefix}{:08}", 70000 + i)))
        .collect()
}

fn url_after<'a>(url: &'a str, marker: &str) -> Option<&'a str> {
    url.split(marker).nth(1)
}

fn google_search_url_isbn(url: &str) -> Option<&str> {
    url_after(url, "volumes?q=isbn:").and_then(|q| q.split('&').next())
}

/// Dynamic Links の URL から、問い合わせた ISBN の一覧。
fn dl_isbns(url: &str) -> Option<Vec<String>> {
    let list = url_after(url, "books?bibkeys=")?.split('&').next()?;
    Some(
        list.split(',')
            .map(|k| k.strip_prefix("ISBN:").unwrap_or(k).to_string())
            .collect(),
    )
}

fn dl_entry(isbn: &str, id: &str) -> String {
    format!(
        r#""ISBN:{isbn}":{{"bib_key":"ISBN:{isbn}","info_url":"https://books.google.com/books?id={id}&source=gbs_ViewAPI"}}"#
    )
}

fn ol_edition_response(url: &str) -> Option<Reply> {
    let olid = url_after(url, "openlibrary.org/books/")?.strip_suffix(".json")?;
    Some(
        match std::fs::read_to_string(common::fixture_path(&format!(
            "openlibrary_edition_{olid}.json"
        ))) {
            Ok(body) => reply(200, "application/json", body),
            Err(_) => reply(404, "application/json", "{}"),
        },
    )
}

/// Open Library の 1 冊ずつの検索（フィクスチャがなければ該当なし）。
fn ol_single_response(url: &str) -> Option<Reply> {
    let isbn = url_after(url, "openlibrary.org/search.json?isbn=")?
        .split('&')
        .next()?;
    Some(reply(
        200,
        "application/json",
        common::open_library_batch_body(&[isbn.to_string()]),
    ))
}

/// AT-49 の世界。Open Library は 1 冊ずつのフィクスチャ（まとめた検索は、その docs をつなげる）。
/// Google は、検索で `KOKORO`・`ONIGIRI` が見つかり、`CLEAN_CODE` は検索では該当なし（障害）で Dynamic Links から取れる。
fn world(url: &str) -> Reply {
    if let Some(isbns) = common::open_library_batch_isbns(url) {
        return reply(
            200,
            "application/json",
            common::open_library_batch_body(&isbns),
        );
    }
    if let Some(r) = ol_single_response(url).or_else(|| ol_edition_response(url)) {
        return r;
    }
    if let Some(isbn) = google_search_url_isbn(url) {
        let file = match isbn {
            KOKORO => "google_synthetic_9784003101117.json",
            ONIGIRI => "google_synthetic_9784780802047.json",
            _ => "google_synthetic_notfound.json",
        };
        return json(fixture(file));
    }
    if let Some(isbns) = dl_isbns(url) {
        let entries: Vec<String> = isbns
            .iter()
            .filter(|i| i.as_str() == CLEAN_CODE)
            .map(|i| dl_entry(i, CLEAN_CODE_ID))
            .collect();
        return jsonp(format!("cb({{{}}});", entries.join(",")));
    }
    if url.contains(&format!("/books/v1/volumes/{CLEAN_CODE_ID}")) {
        return json(fixture("google_synthetic_volume_hjEFCAAAQBAJ.json"));
    }
    reply(404, "text/plain", "not found")
}

struct Rig {
    http: Arc<Fake>,
    cache: Arc<MemoryRawCache>,
    service: MetadataService,
}

fn rig(handler: Handler) -> Rig {
    let http = Fake::new(handler);
    let cache = Arc::new(MemoryRawCache::new());
    let service = MetadataService::new(
        default_providers(Some(KEY.to_string())),
        Arc::new(transport(&http)),
    )
    .with_cache(cache.clone())
    .with_clock(Arc::new(|| 1_800_000_000));
    Rig {
        http,
        cache,
        service,
    }
}

fn requests(isbns: &[String], enabled: &[ProviderId]) -> Vec<FetchRequest> {
    isbns
        .iter()
        .map(|i| FetchRequest::new(i).with_enabled(enabled.to_vec()))
        .collect()
}

fn outcome(result: &MetaFetchResult, p: ProviderId) -> ProviderOutcome {
    result
        .reports
        .iter()
        .find(|r| r.provider == p)
        .unwrap_or_else(|| panic!("{p:?} の状態がない"))
        .outcome
        .clone()
}

/// キャッシュの本文を、比べられる形にする。Open Library の包みは、検索の `docs` と `edition` だけを見る
/// （まとめた検索から作る検索の応答には、`documentation_url` などの付随の項目がないため）。
fn comparable_body(provider: ProviderId, body: &str) -> Value {
    let v: Value = serde_json::from_str(body).unwrap();
    if provider == ProviderId::OpenLibrary {
        serde_json::json!({ "docs": v["search"]["docs"], "edition": v["edition"] })
    } else {
        v
    }
}

// ---------------------------------------------------------------------------
// AT-49: 20 冊をまとめて取っても、結果は 1 冊ずつと同じ
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn at49_twenty_isbns_are_batched_and_give_the_same_results_as_one_by_one() {
    let mut isbns: Vec<String> = vec![
        CLEAN_CODE.into(),
        KOKORO.into(),
        PRIDE.into(),
        ONIGIRI.into(),
        RADIATION.into(),
    ];
    isbns.extend(unknown_isbns("9780", 8));
    isbns.extend(unknown_isbns("9784", 7));
    assert_eq!(isbns.len(), 20);
    let enabled = [ProviderId::Google, ProviderId::OpenLibrary];

    // まとめて
    let batched = rig(Box::new(world));
    let results = batched
        .service
        .fetch_many(&requests(&isbns, &enabled))
        .await;
    let ol_searches = batched.http.count("openlibrary.org/search.json");
    let dl = batched.http.count("books?bibkeys=");
    assert_eq!(ol_searches, 2, "Open Library の検索は 10 冊ずつ 2 回");
    assert!(
        dl <= 2,
        "Dynamic Links は 10 冊ずつ（検索で該当なしの 18 冊 → 2 回）: {dl}"
    );
    assert_eq!(dl, 2);
    assert_eq!(
        batched.http.count("openlibrary.org/search.json?q="),
        2,
        "1 冊ずつの検索は使わない"
    );
    // 版の JSON は見つかった 3 冊だけ
    assert_eq!(batched.http.count("openlibrary.org/books/"), 3);

    // 1 冊ずつ（同じ世界）
    let single = rig(Box::new(world));
    let mut singles = Vec::new();
    for isbn in &isbns {
        singles.push(
            single
                .service
                .fetch(&requests(std::slice::from_ref(isbn), &enabled)[0])
                .await,
        );
    }
    assert_eq!(single.http.count("openlibrary.org/search.json?isbn="), 20);

    for (isbn, (b, s)) in isbns.iter().zip(results.iter().zip(&singles)) {
        assert_eq!(b, s, "{isbn}: 統合した値・取得元の状態・書影の URL が同じ");
        for p in enabled {
            let (cb, cs) = (batched.cache.entry(p, isbn), single.cache.entry(p, isbn));
            assert_eq!(cb.is_some(), cs.is_some(), "{isbn} {p:?}");
            if let (Some(cb), Some(cs)) = (cb, cs) {
                assert_eq!(cb.found, cs.found, "{isbn} {p:?}");
                assert_eq!(
                    comparable_body(p, &cb.body),
                    comparable_body(p, &cs.body),
                    "{isbn} {p:?}: キャッシュの本文"
                );
            }
        }
    }
    // 取れたものの確認
    assert_eq!(
        outcome(&results[0], ProviderId::Google),
        ProviderOutcome::Found,
        "Dynamic Links 経由で取れる"
    );
    assert_eq!(
        outcome(&results[0], ProviderId::OpenLibrary),
        ProviderOutcome::Found
    );
    assert_eq!(
        outcome(&results[2], ProviderId::OpenLibrary),
        ProviderOutcome::Found
    );
    assert_eq!(
        outcome(&results[5], ProviderId::OpenLibrary),
        ProviderOutcome::NotFound
    );
    assert_eq!(
        outcome(&results[1], ProviderId::Google),
        ProviderOutcome::Found,
        "検索で見つかる本は Dynamic Links に行かない"
    );
}

// ---------------------------------------------------------------------------
// Open Library
// ---------------------------------------------------------------------------

fn ol_raw_batch_isbns() -> Vec<String> {
    vec![
        CLEAN_CODE.into(),
        PRIDE.into(),
        KOKORO.into(),
        RADIATION.into(),
    ]
}

#[tokio::test(start_paused = true)]
async fn ol_real_batch_response_is_split_per_isbn_in_response_order() {
    // 実際の応答（2026-11-14）。9780132350884 は、Clean Code のあとに、同じ ISBN を持つ別の作品（スパム）が並ぶ
    let body = fixture("openlibrary_search_batch_4.json");
    let http = Fake::new(Box::new(move |url: &str| {
        if url.contains("openlibrary.org/search.json?q=") {
            return reply(200, "application/json", body.clone());
        }
        ol_edition_response(url).unwrap_or_else(|| reply(404, "text/plain", "x"))
    }));
    let isbns = ol_raw_batch_isbns();
    let ol = OpenLibrary::new();
    let results = ol.fetch_raw_batch(&transport(&http), &isbns).await;
    assert_eq!(results.len(), 4);
    assert_eq!(
        results.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>(),
        isbns.iter().map(String::as_str).collect::<Vec<_>>(),
        "順序は入力のまま"
    );
    let url = &http.urls()[0];
    assert!(url.contains("limit=100"), "{url}");
    assert!(
        url.contains("isbn%3A%289780132350884%20OR%209780141439518%20OR"),
        "{url}"
    );
    assert!(url.contains("fields=key,title,author_name,editions,editions.key"));
    // 版の JSON は見つかった 3 冊だけ
    assert_eq!(http.count("openlibrary.org/books/"), 3);

    let clean = results[0].1.as_ref().unwrap();
    let body: Value = serde_json::from_str(&clean.body).unwrap();
    assert_eq!(body["search"]["numFound"], 2);
    assert_eq!(body["search"]["docs"][0]["title"], "Clean Code");
    assert_eq!(
        body["search"]["docs"][1]["title"],
        "The Earn2Trade Discount Code"
    );
    let c = ol.parse(clean).unwrap().unwrap();
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    assert_eq!(c.pages, Some(431), "版の JSON も取れている");

    let pride = ol.parse(results[1].1.as_ref().unwrap()).unwrap().unwrap();
    assert_eq!(pride.title.as_deref(), Some("Pride and Prejudice"));
    // 収録のない本は該当なし
    assert_eq!(ol.parse(results[2].1.as_ref().unwrap()).unwrap(), None);
    let radiation = ol.parse(results[3].1.as_ref().unwrap()).unwrap().unwrap();
    assert!(radiation.title.as_deref().unwrap().contains("Radiation"));
}

#[tokio::test(start_paused = true)]
async fn ol_truncated_response_falls_back_to_single_searches_for_unmatched_isbns() {
    // numFound は 3 だが、docs は 1 件（limit で切れた）。Clean Code だけが見つかり、残りの 2 冊は 1 冊ずつ取り直す
    let http = Fake::new(Box::new(|url: &str| {
        if common::open_library_batch_isbns(url).is_some() {
            let mut v: Value =
                serde_json::from_str(&common::open_library_batch_body(&[CLEAN_CODE.to_string()]))
                    .unwrap();
            v["numFound"] = 3.into();
            return reply(200, "application/json", v.to_string());
        }
        ol_single_response(url)
            .or_else(|| ol_edition_response(url))
            .unwrap_or_else(|| reply(404, "text/plain", "x"))
    }));
    let isbns: Vec<String> = vec![CLEAN_CODE.into(), PRIDE.into(), KOKORO.into()];
    let ol = OpenLibrary::new();
    let results = ol.fetch_raw_batch(&transport(&http), &isbns).await;
    assert_eq!(http.count("search.json?q="), 1);
    assert_eq!(
        http.count("search.json?isbn="),
        2,
        "見つからなかった 2 冊は 1 冊ずつ: {:?}",
        http.urls()
    );
    let titles: Vec<Option<String>> = results
        .iter()
        .map(|(_, r)| ol.parse(r.as_ref().unwrap()).unwrap().and_then(|c| c.title))
        .collect();
    assert_eq!(titles[0].as_deref(), Some("Clean Code"));
    assert_eq!(
        titles[1].as_deref(),
        Some("Pride and Prejudice"),
        "取り直して見つかる"
    );
    assert_eq!(titles[2], None, "取り直しても無ければ該当なし");
}

#[tokio::test(start_paused = true)]
async fn ol_untruncated_response_without_a_match_is_not_found_without_extra_requests() {
    let http = Fake::new(Box::new(world));
    let isbns = unknown_isbns("9780", 3);
    let ol = OpenLibrary::new();
    let results = ol.fetch_raw_batch(&transport(&http), &isbns).await;
    assert_eq!(http.urls().len(), 1);
    for (_, r) in results {
        assert_eq!(ol.parse(&r.unwrap()).unwrap(), None);
    }
}

#[tokio::test(start_paused = true)]
async fn ol_group_error_spreads_to_that_group_only() {
    let first = Mutex::new(true);
    let http = Fake::new(Box::new(move |url: &str| {
        if let Some(isbns) = common::open_library_batch_isbns(url) {
            if std::mem::take(&mut *first.lock().unwrap()) {
                return reply(500, "text/plain", "boom");
            }
            return reply(
                200,
                "application/json",
                common::open_library_batch_body(&isbns),
            );
        }
        ol_edition_response(url).unwrap_or_else(|| reply(404, "text/plain", "x"))
    }));
    let mut isbns: Vec<String> = unknown_isbns("9780", 9);
    isbns.push(PRIDE.into());
    isbns.push(CLEAN_CODE.into());
    let ol = OpenLibrary::new();
    let results = ol.fetch_raw_batch(&transport(&http), &isbns).await;
    assert_eq!(
        http.count("search.json?q="),
        2,
        "2 つ目の群も問い合わせる（500 は制限ではない）"
    );
    for (isbn, r) in &results[..10] {
        assert!(
            matches!(r, Err(FetchError::Http { status: 500, .. })),
            "{isbn}: {r:?}"
        );
    }
    let clean = results[10].1.as_ref().unwrap();
    assert_eq!(
        ol.parse(clean).unwrap().unwrap().title.as_deref(),
        Some("Clean Code")
    );
}

#[tokio::test(start_paused = true)]
async fn a_single_isbn_still_uses_the_single_search() {
    let r = rig(Box::new(world));
    let enabled = [ProviderId::OpenLibrary];
    let result = r
        .service
        .fetch(&requests(&[CLEAN_CODE.to_string()], &enabled)[0])
        .await;
    assert_eq!(r.http.count("search.json?isbn="), 1);
    assert_eq!(r.http.count("search.json?q="), 0);
    assert_eq!(
        outcome(&result, ProviderId::OpenLibrary),
        ProviderOutcome::Found
    );
}

#[tokio::test(start_paused = true)]
async fn ol_429_on_a_group_stops_the_later_groups() {
    let http = Fake::new(Box::new(|url: &str| {
        if url.contains("search.json?q=") {
            return status(429);
        }
        panic!("想定していない通信: {url}")
    }));
    let isbns = unknown_isbns("9780", 25);
    let ol = OpenLibrary::new();
    let results = ol.fetch_raw_batch(&transport(&http), &isbns).await;
    assert_eq!(
        http.urls().len(),
        1,
        "最初の群で 429 → 残りの群は問い合わせない"
    );
    assert_eq!(results.len(), 25);
    for (isbn, r) in results {
        assert!(
            matches!(r, Err(FetchError::Http { status: 429, .. })),
            "{isbn}: {r:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn ol_429_on_an_edition_request_stops_further_requests() {
    // 1 つ目の群は成功。Clean Code の版の取得で 429 → 以降の版・群には問い合わせない
    let http = Fake::new(Box::new(|url: &str| {
        if let Some(isbns) = common::open_library_batch_isbns(url) {
            return reply(
                200,
                "application/json",
                common::open_library_batch_body(&isbns),
            );
        }
        if url.contains("openlibrary.org/books/") {
            return status(429);
        }
        panic!("想定していない通信: {url}")
    }));
    let mut isbns: Vec<String> = vec![CLEAN_CODE.into(), PRIDE.into()];
    isbns.extend(unknown_isbns("9780", 10));
    let ol = OpenLibrary::new();
    let results = ol.fetch_raw_batch(&transport(&http), &isbns).await;
    assert_eq!(http.count("search.json?q="), 1);
    assert_eq!(http.count("openlibrary.org/books/"), 1, "版は 1 回だけ");
    assert_eq!(results.len(), 12);
    for (isbn, r) in results {
        assert!(
            matches!(r, Err(FetchError::Http { status: 429, .. })),
            "{isbn}: {r:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Google（Dynamic Links のまとめ）
// ---------------------------------------------------------------------------

/// 検索はすべて該当なし。Dynamic Links と volume は `dl`・`volume` で決める。
fn google_world(
    dl: impl Fn(&[String]) -> Reply + Send + Sync + 'static,
    volume: impl Fn(&str) -> Reply + Send + Sync + 'static,
) -> Handler {
    Box::new(move |url: &str| {
        if google_search_url_isbn(url).is_some() {
            return json(fixture("google_synthetic_notfound.json"));
        }
        if let Some(isbns) = dl_isbns(url) {
            return dl(&isbns);
        }
        if let Some(id) = url_after(url, "/books/v1/volumes/") {
            return volume(id.split('?').next().unwrap());
        }
        panic!("想定していない通信: {url}")
    })
}

fn clean_code_volume(_: &str) -> Reply {
    json(fixture("google_synthetic_volume_hjEFCAAAQBAJ.json"))
}

async fn google_batch(
    http: &Arc<Fake>,
    isbns: &[String],
) -> Vec<(String, Result<RawResponse, FetchError>)> {
    GoogleBooks::new(Some(KEY.to_string()))
        .fetch_raw_batch(&transport(http), isbns)
        .await
}

#[tokio::test(start_paused = true)]
async fn google_dynamic_links_batch_looks_up_each_isbn_by_its_key() {
    // 実際の応答（2026-11-14）。3 冊を問い合わせ、2 冊の項目がある（3 冊目はない）
    let jsonp_body = fixture("google_dynamiclinks_batch_3.jsonp");
    let http = Fake::new(google_world(
        move |_| jsonp(jsonp_body.clone()),
        |id| match id {
            CLEAN_CODE_ID => clean_code_volume(id),
            // こころ（`fz5TPAAACAAJ`）
            _ => json(fixture("google_synthetic_volume_fz5TPAAACAAJ.json")),
        },
    ));
    let missing = "9784123456784".to_string();
    let isbns: Vec<String> = vec![CLEAN_CODE.into(), KOKORO.into(), missing.clone()];
    let results = google_batch(&http, &isbns).await;

    assert_eq!(http.count("books?bibkeys="), 1, "Dynamic Links は 1 回");
    assert_eq!(
        http.urls()
            .iter()
            .find(|u| u.contains("books?bibkeys="))
            .unwrap(),
        &format!(
            "https://books.google.com/books?bibkeys=ISBN:{CLEAN_CODE},ISBN:{KOKORO},ISBN:{missing}&jscmd=viewapi&callback=cb"
        )
    );
    assert!(
        !http
            .urls()
            .iter()
            .any(|u| u.contains("books.google.com") && u.contains("key=")),
        "Dynamic Links にキーを送らない"
    );
    assert_eq!(
        http.count("/books/v1/volumes/"),
        2,
        "volume は項目のあった 2 冊だけ"
    );
    let g = GoogleBooks::new(None);
    let clean = results[0].1.as_ref().unwrap();
    assert_eq!(
        clean.body,
        fixture("google_synthetic_volume_hjEFCAAAQBAJ.json")
    );
    assert!(g.parse(clean).unwrap().is_some());
    let kokoro = results[1].1.as_ref().unwrap();
    assert_eq!(
        kokoro.body,
        fixture("google_synthetic_volume_fz5TPAAACAAJ.json")
    );
    assert_eq!(
        g.parse(kokoro).unwrap().unwrap().title.as_deref(),
        Some("こころ")
    );
    // 項目がなければ該当なし。本文は検索の応答
    let none = results[2].1.as_ref().unwrap();
    assert_eq!(none.body, fixture("google_synthetic_notfound.json"));
    assert_eq!(g.parse(none).unwrap(), None);
}

#[tokio::test(start_paused = true)]
async fn the_only_entry_fallback_is_not_used_when_several_isbns_are_queried() {
    // 項目が 1 つだけ（キーは問い合わせた ISBN のどれでもない）。1 冊のときは使い、2 冊以上のときは使わない
    let other = dl_entry("9780000000002", CLEAN_CODE_ID);
    let make = |other: String| {
        Fake::new(google_world(
            move |_| jsonp(format!("cb({{{other}}});")),
            clean_code_volume,
        ))
    };
    let http = make(other.clone());
    let results = google_batch(&http, &[CLEAN_CODE.to_string()]).await;
    assert!(
        GoogleBooks::new(None)
            .parse(results[0].1.as_ref().unwrap())
            .unwrap()
            .is_some(),
        "1 冊だけの問い合わせでは、唯一の項目を使う（これまでどおり）"
    );

    let http = make(other);
    let results = google_batch(&http, &[CLEAN_CODE.to_string(), PRIDE.to_string()]).await;
    assert_eq!(http.count("/books/v1/volumes/"), 0);
    for (_, r) in results {
        assert_eq!(GoogleBooks::new(None).parse(&r.unwrap()).unwrap(), None);
    }
}

#[tokio::test(start_paused = true)]
async fn a_dynamic_links_error_spreads_to_its_group_only() {
    let first = Mutex::new(true);
    let http = Fake::new(google_world(
        move |isbns| {
            if std::mem::take(&mut *first.lock().unwrap()) {
                return reply(500, "text/html", "<html>oops</html>");
            }
            let entries: Vec<String> = isbns
                .iter()
                .filter(|i| i.as_str() == CLEAN_CODE)
                .map(|i| dl_entry(i, CLEAN_CODE_ID))
                .collect();
            jsonp(format!("cb({{{}}});", entries.join(",")))
        },
        clean_code_volume,
    ));
    let mut isbns = unknown_isbns("9780", 10);
    isbns.push(CLEAN_CODE.into());
    let results = google_batch(&http, &isbns).await;
    assert_eq!(http.count("books?bibkeys="), 2);
    for (isbn, r) in &results[..10] {
        match r {
            Err(FetchError::Http {
                status: 500,
                message,
            }) => {
                assert!(message.contains(DYNAMIC_LINKS_NOTE), "{message}")
            }
            other => panic!("{isbn}: {other:?}"),
        }
    }
    assert!(
        GoogleBooks::new(None)
            .parse(results[10].1.as_ref().unwrap())
            .unwrap()
            .is_some(),
        "別の群は取れる"
    );

    // JSONP でない応答も、その群のエラー
    let http = Fake::new(google_world(
        |_| jsonp("<html>captcha</html>"),
        clean_code_volume,
    ));
    let results = google_batch(&http, &isbns[..3]).await;
    for (_, r) in results {
        assert!(matches!(r, Err(FetchError::Parse(_))), "{r:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn google_dynamic_links_429_stops_dynamic_links_but_searches_continue() {
    let http = Fake::new(google_world(|_| status(429), clean_code_volume));
    let isbns = unknown_isbns("9780", 25);
    let results = google_batch(&http, &isbns).await;
    assert_eq!(
        http.count("volumes?q=isbn:"),
        25,
        "検索は 1 冊ずつ、全部に行く"
    );
    assert_eq!(
        http.count("books?bibkeys="),
        1,
        "Dynamic Links は 1 回で止まる"
    );
    assert_eq!(results.len(), 25);
    for (isbn, r) in results {
        match r {
            Err(FetchError::Http {
                status: 429,
                message,
            }) => {
                assert!(message.contains(DYNAMIC_LINKS_NOTE), "{isbn}: {message}")
            }
            other => panic!("{isbn}: {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn google_search_403_stops_the_remaining_requests() {
    let n = Mutex::new(0);
    let http = Fake::new(Box::new(move |url: &str| {
        if google_search_url_isbn(url).is_some() {
            let mut n = n.lock().unwrap();
            *n += 1;
            return if *n == 2 {
                reply(
                    403,
                    "application/json",
                    r#"{"error":{"code":403,"message":"Daily Limit Exceeded per day"}}"#,
                )
            } else {
                json(fixture("google_synthetic_notfound.json"))
            };
        }
        panic!("想定していない通信: {url}")
    }));
    let isbns = unknown_isbns("9780", 4);
    let results = google_batch(&http, &isbns).await;
    assert_eq!(
        http.count("volumes?q=isbn:"),
        2,
        "2 冊目の 403 のあと、検索はしない"
    );
    // 1 冊目は検索で該当なしだが、Google の API が拒否したので、volume には進めない（Dynamic Links にも聞かない）
    assert_eq!(http.count("books?bibkeys="), 0);
    for (isbn, r) in &results {
        match r {
            Err(FetchError::Http {
                status: 403,
                message,
            }) => {
                assert!(
                    message.contains("per day"),
                    "{isbn}: 元のメッセージを残す: {message}"
                )
            }
            other => panic!("{isbn}: {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn google_volume_429_stops_volume_and_dynamic_links_requests() {
    let http = Fake::new(google_world(
        |isbns| {
            let entries: Vec<String> = isbns.iter().map(|i| dl_entry(i, CLEAN_CODE_ID)).collect();
            jsonp(format!("cb({{{}}});", entries.join(",")))
        },
        |_| status(429),
    ));
    let isbns = unknown_isbns("9780", 12);
    let results = google_batch(&http, &isbns).await;
    assert_eq!(
        http.count("/books/v1/volumes/"),
        1,
        "volume は 1 回で止まる"
    );
    assert_eq!(
        http.count("books?bibkeys="),
        1,
        "次の群の Dynamic Links も問い合わせない"
    );
    for (isbn, r) in results {
        assert!(
            matches!(r, Err(FetchError::Http { status: 429, .. })),
            "{isbn}: {r:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 1 冊ずつの流れ（まとめない取得元）の打ち切り
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn a_non_batch_provider_that_refuses_is_not_asked_again() {
    let n = Mutex::new(0);
    let http = Fake::new(Box::new(move |url: &str| {
        if url.contains("ndlsearch.ndl.go.jp") {
            let mut n = n.lock().unwrap();
            *n += 1;
            return if *n == 2 {
                reply(429, "text/plain", "Too Many Requests")
            } else {
                reply(200, "text/xml;charset=UTF-8", fixture("ndl_notfound.xml"))
            };
        }
        panic!("想定していない通信: {url}")
    }));
    let service = MetadataService::new(
        vec![Arc::new(Ndl::new()) as Arc<dyn Provider>],
        Arc::new(transport(&http)),
    );
    let isbns = unknown_isbns("9784", 3);
    let results = service
        .fetch_many(&requests(&isbns, &[ProviderId::Ndl]))
        .await;
    assert_eq!(http.urls().len(), 2, "3 冊目は問い合わせない");
    assert_eq!(
        outcome(&results[0], ProviderId::Ndl),
        ProviderOutcome::NotFound
    );
    for r in &results[1..] {
        assert_eq!(
            outcome(r, ProviderId::Ndl),
            ProviderOutcome::HttpError {
                status: 429,
                message: "Too Many Requests".into()
            }
        );
    }
}
