//! Open Library の取得元のテスト。
//!
//! - 解析: `tests/fixtures/openlibrary_*.json`（2026-10-15 に取った実際の応答）と、実際の応答にない形を作った JSON
//!   （`title_prefix`、`covers` の `-1`、文字列の `description`、統合された版 など）。
//! - 取得の流れ: 偽の HTTP（回数と順番、エラーの扱い、キャッシュ、レート制限）と、本物の HTTP 層（wiremock）。

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bibcontainer_metadata::cover::{CoverDownload, download_cover};
use bibcontainer_metadata::error::FetchError;
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::provider::{MemoryRawCache, Provider, RawResponse};
use bibcontainer_metadata::providers::{OpenLibrary, default_rate_limit};
use bibcontainer_metadata::service::{FetchRequest, MetadataService, ProviderOutcome};
use bibcontainer_metadata::types::*;
use bibcontainer_metadata::{ReqwestClient, RetryPolicy, Transport};
use common::fixture;
use serde_json::{Value, json};
use tokio::time::Instant;

const CLEAN_CODE: &str = "9780132350884";
const PRIDE: &str = "9780141439518";
const RADIATION: &str = "9781493934362";
const KOKORO: &str = "9784003101117";

const CLEAN_CODE_EDITION: &str = "OL26222911M";
const PRIDE_EDITION: &str = "OL3700132M";
const RADIATION_EDITION: &str = "OL34386079M";

const UA: &str = "bibcontainer/0.1.0 (+https://github.com/mioupa/bibcontainer)";

fn json_fixture(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

fn search_fixture(isbn: &str) -> Value {
    json_fixture(&format!("openlibrary_search_{isbn}.json"))
}

fn edition_fixture(olid: &str) -> Value {
    json_fixture(&format!("openlibrary_edition_{olid}.json"))
}

/// 取得元の包み（生の応答）。
fn raw(isbn: &str, search: &Value, edition: Option<&Value>) -> RawResponse {
    RawResponse {
        provider: ProviderId::OpenLibrary,
        isbn13: isbn.to_string(),
        body: json!({ "search": search, "edition": edition }).to_string(),
    }
}

fn parse(isbn: &str, search: &Value, edition: Option<&Value>) -> Option<Candidate> {
    OpenLibrary::new()
        .parse(&raw(isbn, search, edition))
        .unwrap()
}

fn author(name: &str) -> CandidateAuthor {
    CandidateAuthor {
        name: name.into(),
        kana: None,
        role: None,
    }
}

/// 検索の応答を作る（作品 1 件、版の項目 `edition_docs`）。
fn search_with(work: Value, edition_docs: Vec<Value>) -> Value {
    let mut work = work;
    work["editions"] = json!({ "numFound": edition_docs.len(), "start": 0, "docs": edition_docs });
    json!({ "numFound": 1, "start": 0, "docs": [work] })
}

// ---------------------------------------------------------------------------
// 解析（実際の応答）
// ---------------------------------------------------------------------------

#[test]
fn clean_code_maps_every_field() {
    let c = parse(
        CLEAN_CODE,
        &search_fixture(CLEAN_CODE),
        Some(&edition_fixture(CLEAN_CODE_EDITION)),
    )
    .unwrap();
    assert_eq!(c.provider, ProviderId::OpenLibrary);
    assert_eq!(c.isbn13, CLEAN_CODE);
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    assert_eq!(
        c.subtitle.as_deref(),
        Some("A Handbook of Agile Software Craftsmanship")
    );
    assert_eq!(c.authors, vec![author("Robert C. Martin")]);
    assert_eq!(c.publisher.as_deref(), Some("Prentice Hall"));
    assert_eq!(c.published_on.as_deref(), Some("2008-07"), "`July 2008`");
    assert_eq!(c.pages, Some(431));
    assert_eq!(c.language.as_deref(), Some("en"), "`/languages/eng`");
    // 内容紹介は、`{ "type": "/type/text", "value": … }` の value。改行は `\n` にそろえる
    let d = c.description.unwrap();
    assert!(d.starts_with("Even bad code can function."), "{d}");
    assert!(!d.contains('\r'));
    assert!(
        d.contains("\n\nNoted software expert Robert C. Martin"),
        "{d}"
    );
    assert!(d.ends_with("producing better code."), "{d}");
    // 書影: 版の JSON の covers の先頭
    assert_eq!(
        c.covers,
        vec![CandidateCover {
            provider: ProviderId::OpenLibrary,
            url: "https://covers.openlibrary.org/b/id/15126503-L.jpg?default=false".into()
        }]
    );
    // 持っていない項目
    assert_eq!(c.title_kana, None);
    assert_eq!(c.ndc, None);
    assert_eq!(c.c_code, None);
    assert_eq!(c.list_price, None);
    assert_eq!(c.ref_price_incl, None);
    assert_eq!(c.size_text, None);
    assert_eq!(c.series, None);
    assert_eq!(c.volume, None);
}

#[test]
fn pride_and_prejudice_has_no_subtitle_and_a_bare_year() {
    let c = parse(
        PRIDE,
        &search_fixture(PRIDE),
        Some(&edition_fixture(PRIDE_EDITION)),
    )
    .unwrap();
    assert_eq!(c.title.as_deref(), Some("Pride and Prejudice"));
    assert_eq!(c.subtitle, None);
    assert_eq!(c.authors, vec![author("Jane Austen")]);
    assert_eq!(c.publisher.as_deref(), Some("Penguin Books"));
    assert_eq!(c.published_on.as_deref(), Some("2003"));
    assert_eq!(c.pages, Some(435));
    assert_eq!(c.language.as_deref(), Some("en"));
    let d = c.description.unwrap();
    assert!(d.starts_with("When Elizabeth Bennet first meets"), "{d}");
    assert!(
        d.ends_with("\n--back cover"),
        "`\\r\\n` は `\\n` になる: {d:?}"
    );
    assert_eq!(
        c.covers[0].url,
        "https://covers.openlibrary.org/b/id/15261362-L.jpg?default=false"
    );
}

#[test]
fn radiation_materials_science_has_no_description() {
    let c = parse(
        RADIATION,
        &search_fixture(RADIATION),
        Some(&edition_fixture(RADIATION_EDITION)),
    )
    .unwrap();
    assert_eq!(
        c.title.as_deref(),
        Some("Fundamentals of Radiation Materials Science")
    );
    assert_eq!(c.subtitle.as_deref(), Some("Metals and Alloys"));
    assert_eq!(c.authors, vec![author("Gary S. Was")]);
    assert_eq!(c.publisher.as_deref(), Some("Springer"));
    assert_eq!(c.published_on.as_deref(), Some("2016"));
    assert_eq!(c.pages, Some(1002));
    assert_eq!(c.description, None, "版の JSON に description がない");
    assert_eq!(
        c.covers[0].url,
        "https://covers.openlibrary.org/b/id/13817654-L.jpg?default=false"
    );
}

#[test]
fn not_found_search_is_none() {
    assert_eq!(parse(KOKORO, &search_fixture(KOKORO), None), None);
    // docs が空、または docs がなくて numFound が 0 なら、該当なし
    assert_eq!(
        parse(KOKORO, &json!({"numFound": 0, "docs": []}), None),
        None
    );
    assert_eq!(parse(KOKORO, &json!({"numFound": 0}), None), None);
}

// ---------------------------------------------------------------------------
// 解析（実際の応答にない形）
// ---------------------------------------------------------------------------

#[test]
fn search_only_when_the_edition_json_is_missing() {
    // 版の JSON が取れなかった（404）ときは、検索の結果だけ。ページ数と内容紹介はない
    let c = parse(CLEAN_CODE, &search_fixture(CLEAN_CODE), None).unwrap();
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    assert_eq!(
        c.subtitle.as_deref(),
        Some("A Handbook of Agile Software Craftsmanship")
    );
    assert_eq!(c.authors, vec![author("Robert C. Martin")]);
    assert_eq!(c.publisher.as_deref(), Some("Prentice Hall"));
    assert_eq!(c.published_on.as_deref(), Some("2008-07"));
    assert_eq!(c.language.as_deref(), Some("en"));
    assert_eq!(c.pages, None);
    assert_eq!(c.description, None);
    assert_eq!(
        c.covers[0].url, "https://covers.openlibrary.org/b/id/15126503-L.jpg?default=false",
        "検索の cover_i"
    );
}

#[test]
fn redirect_type_edition_is_ignored_like_a_missing_one() {
    // 統合されて別の版に移った記録（実際の応答の形: type.key が /type/redirect）。本文の情報はない
    let redirect = json!({
        "type": {"key": "/type/redirect"},
        "location": "/books/OL1M",
        "key": "/books/OL26222911M",
        "latest_revision": 2,
        "revision": 2
    });
    let with = parse(CLEAN_CODE, &search_fixture(CLEAN_CODE), Some(&redirect));
    let without = parse(CLEAN_CODE, &search_fixture(CLEAN_CODE), None);
    assert_eq!(with, without);
    assert!(with.is_some());
    // 取得元の包みで edition が null でも同じ
    assert_eq!(
        OpenLibrary::new()
            .parse(&RawResponse {
                provider: ProviderId::OpenLibrary,
                isbn13: CLEAN_CODE.into(),
                body: json!({"search": search_fixture(CLEAN_CODE)}).to_string(),
            })
            .unwrap(),
        without,
        "edition がなくても読める"
    );
}

#[test]
fn title_prefix_is_prepended_and_title_and_subtitle_come_from_the_same_record() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "Hobbit (work)"}),
        vec![json!({
            "key": "/books/OL1M",
            "title": "Hobbit (search)",
            "subtitle": "Subtitle from search",
            "isbn": [PRIDE]
        })],
    );
    // 版の JSON の書名に title_prefix（`The ` — 末尾の空白まで値）を付ける。副書名も版の JSON から（なければ空）
    let edition =
        json!({"title_prefix": "The ", "title": "Hobbit", "type": {"key": "/type/edition"}});
    let c = parse(PRIDE, &search, Some(&edition)).unwrap();
    assert_eq!(c.title.as_deref(), Some("The Hobbit"));
    assert_eq!(
        c.subtitle, None,
        "書名が版の JSON から来たなら、検索の副書名は混ぜない"
    );
    let edition =
        json!({"title_prefix": "The ", "title": "Hobbit", "subtitle": "There and Back Again"});
    let c = parse(PRIDE, &search, Some(&edition)).unwrap();
    assert_eq!(c.title.as_deref(), Some("The Hobbit"));
    assert_eq!(c.subtitle.as_deref(), Some("There and Back Again"));
    // 版の JSON に書名がなければ、検索の版の書名と副書名
    let c = parse(PRIDE, &search, Some(&json!({"number_of_pages": 10}))).unwrap();
    assert_eq!(c.title.as_deref(), Some("Hobbit (search)"));
    assert_eq!(c.subtitle.as_deref(), Some("Subtitle from search"));
    // 検索の版にも書名がなければ、作品の書名（副書名はなし）
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "Hobbit (work)"}),
        vec![json!({"key": "/books/OL1M", "subtitle": "orphan", "isbn": [PRIDE]})],
    );
    let c = parse(PRIDE, &search, None).unwrap();
    assert_eq!(c.title.as_deref(), Some("Hobbit (work)"));
    assert_eq!(c.subtitle, None);
}

#[test]
fn title_whitespace_and_width_are_normalized() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "x"}),
        vec![json!({"key": "/books/OL1M", "isbn": [PRIDE]})],
    );
    let edition = json!({"title": "  Ｃｌｅａｎ   Code \n", "subtitle": " A　Handbook "});
    let c = parse(PRIDE, &search, Some(&edition)).unwrap();
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    assert_eq!(c.subtitle.as_deref(), Some("A Handbook"));
}

#[test]
fn covers_skip_minus_one_and_fall_back_to_the_search_cover() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "T"}),
        vec![json!({"key": "/books/OL1M", "cover_i": 777, "isbn": [PRIDE]})],
    );
    let url = |id: i64| format!("https://covers.openlibrary.org/b/id/{id}-L.jpg?default=false");
    let cover_of = |edition: Value| parse(PRIDE, &search, Some(&edition)).unwrap().covers;
    // -1 は飛ばして、最初の正の ID
    let covers = cover_of(json!({"covers": [-1, 123, 456]}));
    assert_eq!(covers.len(), 1);
    assert_eq!(covers[0].url, url(123));
    // 正の ID がなければ、検索の cover_i
    assert_eq!(cover_of(json!({"covers": [-1]}))[0].url, url(777));
    assert_eq!(cover_of(json!({"covers": []}))[0].url, url(777));
    assert_eq!(cover_of(json!({}))[0].url, url(777));
    // どちらもなければ書影なし
    let bare = search_with(
        json!({"key": "/works/OL1W", "title": "T"}),
        vec![json!({"key": "/books/OL1M", "isbn": [PRIDE]})],
    );
    let c = parse(PRIDE, &bare, Some(&json!({"covers": [-1]}))).unwrap();
    assert!(c.covers.is_empty());
    // 0 や文字列は ID として使わない
    let c = parse(PRIDE, &bare, Some(&json!({"covers": [0, "5", null]}))).unwrap();
    assert!(c.covers.is_empty());
}

#[test]
fn description_may_be_a_string_or_an_object_and_simple_html_is_removed() {
    let search = search_fixture(PRIDE);
    let desc = |d: Value| {
        parse(PRIDE, &search, Some(&json!({ "description": d })))
            .unwrap()
            .description
    };
    assert_eq!(
        desc(json!("A plain string.")).as_deref(),
        Some("A plain string.")
    );
    assert_eq!(
        desc(json!({"type": "/type/text", "value": "From an object."})).as_deref(),
        Some("From an object.")
    );
    assert_eq!(
        desc(json!(
            "  Line one.\r\nLine two.<br><b>Bold</b> &amp; more.  "
        ))
        .as_deref(),
        Some("Line one.\nLine two.\nBold & more.")
    );
    // 空・形が違うものは、内容紹介なし
    assert_eq!(desc(json!("   ")), None);
    assert_eq!(desc(json!({"type": "/type/text"})), None);
    assert_eq!(desc(json!(42)), None);
    assert_eq!(desc(json!(null)), None);
}

#[test]
fn pages_come_from_number_of_pages_then_pagination() {
    let search = search_fixture(PRIDE);
    let pages = |edition: Value| parse(PRIDE, &search, Some(&edition)).unwrap().pages;
    assert_eq!(pages(json!({"number_of_pages": 320})), Some(320));
    // number_of_pages が優先
    assert_eq!(
        pages(json!({"number_of_pages": 320, "pagination": "xii, 999 p."})),
        Some(320)
    );
    // number_of_pages がなければ pagination（実際の応答の形: `xlii, 435 p. ;`）
    let mut e = edition_fixture(PRIDE_EDITION);
    e.as_object_mut().unwrap().remove("number_of_pages");
    assert_eq!(pages(e), Some(435));
    assert_eq!(pages(json!({"pagination": "448 pages"})), Some(448));
    // 正の整数でなければ使わない
    assert_eq!(pages(json!({"number_of_pages": 0})), None);
    assert_eq!(pages(json!({"number_of_pages": -5})), None);
    assert_eq!(pages(json!({"number_of_pages": "abc"})), None);
    assert_eq!(pages(json!({"pagination": "1 v."})), None);
    assert_eq!(pages(json!({})), None);
}

#[test]
fn language_comes_from_the_edition_json_then_the_search_doc() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "T"}),
        vec![json!({"key": "/books/OL1M", "language": ["fre", "eng"], "isbn": [PRIDE]})],
    );
    let lang = |edition: Option<Value>| {
        parse(PRIDE, &search, edition.as_ref())
            .unwrap()
            .language
            .unwrap()
    };
    assert_eq!(
        lang(Some(json!({"languages": [{"key": "/languages/ger"}]}))),
        "de"
    );
    assert_eq!(
        lang(Some(json!({"languages": [{"key": "/languages/jpn"}]}))),
        "ja"
    );
    assert_eq!(lang(Some(json!({"languages": []}))), "fr", "検索の先頭");
    assert_eq!(lang(None), "fr");
    // 変換表にない言語は、コードのまま
    assert_eq!(
        lang(Some(json!({"languages": [{"key": "/languages/gre"}]}))),
        "gre"
    );
}

#[test]
fn publisher_and_date_come_from_the_edition_json_then_the_search_doc() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "T"}),
        vec![json!({
            "key": "/books/OL1M",
            "publisher": ["Search Press", "Other"],
            "publish_date": ["Mar 05, 2016", "2017"],
            "isbn": [PRIDE]
        })],
    );
    let c = parse(PRIDE, &search, None).unwrap();
    assert_eq!(c.publisher.as_deref(), Some("Search Press"));
    assert_eq!(c.published_on.as_deref(), Some("2016-03-05"));
    let edition =
        json!({"publishers": ["Edition House", "Second"], "publish_date": "5 March 2015"});
    let c = parse(PRIDE, &search, Some(&edition)).unwrap();
    assert_eq!(c.publisher.as_deref(), Some("Edition House"));
    assert_eq!(c.published_on.as_deref(), Some("2015-03-05"));
    // 日付が読めなければ空（推測しない）
    let c = parse(
        PRIDE,
        &search_with(
            json!({"key": "/works/OL1W", "title": "T"}),
            vec![json!({"key": "/books/OL1M", "publish_date": ["Spring"], "isbn": [PRIDE]})],
        ),
        Some(&json!({"publish_date": "sometime"})),
    )
    .unwrap();
    assert_eq!(c.published_on, None);
}

#[test]
fn authors_fall_back_from_the_edition_doc_to_the_work() {
    let work = |a: Value| json!({"key": "/works/OL1W", "title": "T", "author_name": a});
    let ed = |extra: Value| {
        let mut d = json!({"key": "/books/OL1M", "isbn": [PRIDE]});
        for (k, v) in extra.as_object().unwrap() {
            d[k] = v.clone();
        }
        d
    };
    // 版の著者があれば、作品の著者は使わない。名前だけ（役割・ヨミなし）。重複と空は除く
    let search = search_with(
        work(json!(["Work Author"])),
        vec![ed(
            json!({"author_name": ["Jane  Austen", "Jane Austen", "", "Vivien Jones"]}),
        )],
    );
    let c = parse(PRIDE, &search, None).unwrap();
    assert_eq!(
        c.authors,
        vec![author("Jane Austen"), author("Vivien Jones")]
    );
    // 版に著者がなければ作品の著者
    let search = search_with(work(json!(["Work Author"])), vec![ed(json!({}))]);
    assert_eq!(
        parse(PRIDE, &search, None).unwrap().authors,
        vec![author("Work Author")]
    );
    let search = search_with(
        work(json!(["Work Author"])),
        vec![ed(json!({"author_name": []}))],
    );
    assert_eq!(
        parse(PRIDE, &search, None).unwrap().authors,
        vec![author("Work Author")]
    );
    // 「姓, 名」や役割らしい語も、日本語の役割の分離は掛けず、そのまま名前にする
    let search = search_with(work(json!(["Austen, Jane"])), vec![ed(json!({}))]);
    assert_eq!(
        parse(PRIDE, &search, None).unwrap().authors,
        vec![author("Austen, Jane")]
    );
}

#[test]
fn the_edition_with_the_isbn_is_chosen_else_the_first() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "Work"}),
        vec![
            json!({"key": "/books/OL1M", "title": "Other edition", "isbn": ["9780000000002"]}),
            json!({"key": "/books/OL2M", "title": "Wanted edition", "isbn": ["0141439513"]}),
        ],
    );
    // ISBN-13 を探しても、ISBN-10 の表記（0141439513）を持つ版が選ばれる
    let c = parse(PRIDE, &search, None).unwrap();
    assert_eq!(c.title.as_deref(), Some("Wanted edition"));
    // どの版も ISBN が合わなければ、先頭の版
    let c = parse(CLEAN_CODE, &search, None).unwrap();
    assert_eq!(c.title.as_deref(), Some("Other edition"));
    // 版がない（editions.docs が空、editions がない）なら該当なし
    let none = search_with(json!({"key": "/works/OL1W", "title": "Work"}), vec![]);
    assert_eq!(parse(PRIDE, &none, None), None);
    let none = json!({"numFound": 1, "docs": [{"key": "/works/OL1W", "title": "Work"}]});
    assert_eq!(parse(PRIDE, &none, None), None);
}

#[test]
fn unreadable_responses_are_parse_errors() {
    let p = OpenLibrary::new();
    let bad = |body: &str| {
        p.parse(&RawResponse {
            provider: ProviderId::OpenLibrary,
            isbn13: PRIDE.into(),
            body: body.into(),
        })
        .is_err()
    };
    assert!(bad("<html>"));
    assert!(bad("[]"));
    assert!(bad(r#"{"edition": null}"#), "search がない");
    assert!(bad(r#"{"search": {"numFound": 1}}"#), "docs がない");
    // docs がなくても numFound が 0 なら、該当なし
    assert!(!bad(r#"{"search": {"numFound": 0}}"#));
    assert!(bad(r#"{"search": {"docs": "x"}}"#));
    assert!(!bad(r#"{"search": {"docs": []}, "edition": null}"#));
}

// ---------------------------------------------------------------------------
// 取得の流れ（偽の HTTP）
// ---------------------------------------------------------------------------

type Handler = Box<dyn Fn(&str) -> Result<HttpResponse, TransportError> + Send + Sync>;

struct FakeHttp {
    handler: Handler,
    log: Mutex<Vec<(Instant, String)>>,
}

impl FakeHttp {
    fn new(handler: Handler) -> Arc<Self> {
        Arc::new(FakeHttp {
            handler,
            log: Mutex::new(Vec::new()),
        })
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
impl HttpClient for FakeHttp {
    async fn get(&self, req: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.log
            .lock()
            .unwrap()
            .push((Instant::now(), req.url.clone()));
        (self.handler)(&req.url)
    }
}

fn respond(status: u16, body: impl ToString) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status,
        content_type: Some("application/json".into()),
        retry_after: None,
        body: body.to_string().into_bytes(),
    })
}

/// 本物の Open Library と同じ形で、フィクスチャを返す偽のサーバ。`edition` で版の JSON の応答を差し替えられる。
fn handler_with(
    edition: impl Fn(&str) -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
) -> Handler {
    Box::new(move |url| {
        if let Some(q) = url.strip_prefix("https://openlibrary.org/search.json?isbn=") {
            let isbn = q.split('&').next().unwrap();
            let path = common::fixture_path(&format!("openlibrary_search_{isbn}.json"));
            return match std::fs::read_to_string(path) {
                Ok(body) => respond(200, body),
                Err(_) => respond(200, fixture(&format!("openlibrary_search_{KOKORO}.json"))),
            };
        }
        if url.starts_with("https://openlibrary.org/books/") {
            return edition(url);
        }
        respond(404, "unexpected")
    })
}

fn edition_from_fixtures(url: &str) -> Result<HttpResponse, TransportError> {
    let olid = url
        .strip_prefix("https://openlibrary.org/books/")
        .and_then(|r| r.strip_suffix(".json"))
        .unwrap();
    respond(200, fixture(&format!("openlibrary_edition_{olid}.json")))
}

struct Rig {
    http: Arc<FakeHttp>,
    cache: Arc<MemoryRawCache>,
    service: MetadataService,
}

fn rig(handler: Handler) -> Rig {
    let http = FakeHttp::new(handler);
    let cache = Arc::new(MemoryRawCache::new());
    let service = MetadataService::new(
        vec![Arc::new(OpenLibrary::new()) as Arc<dyn Provider>],
        Arc::new(Transport::new(http.clone())),
    )
    .with_cache(cache.clone())
    .with_clock(Arc::new(|| 1_800_000_000));
    Rig {
        http,
        cache,
        service,
    }
}

fn request(isbn: &str) -> FetchRequest {
    FetchRequest::new(isbn).with_enabled(vec![ProviderId::OpenLibrary])
}

fn only_outcome(result: &bibcontainer_metadata::MetaFetchResult) -> ProviderOutcome {
    assert_eq!(result.reports.len(), 1);
    result.reports[0].outcome.clone()
}

#[tokio::test(start_paused = true)]
async fn not_found_costs_exactly_one_request_and_is_cached_as_not_found() {
    let r = rig(handler_with(edition_from_fixtures));
    let result = r.service.fetch(&request(KOKORO)).await;
    assert_eq!(only_outcome(&result), ProviderOutcome::NotFound);
    assert!(result.merged.is_empty());
    assert!(!result.offline);
    let urls = r.http.urls();
    assert_eq!(urls.len(), 1, "{urls:?}");
    assert!(urls[0].starts_with("https://openlibrary.org/search.json?isbn=9784003101117&fields="));
    // 「該当なし」として保存される（有効期限は 1 日。サービスが扱う）
    let entry = r.cache.entry(ProviderId::OpenLibrary, KOKORO).unwrap();
    assert!(!entry.found);
    // 2 回目はキャッシュ
    let again = r.service.fetch(&request(KOKORO)).await;
    assert!(again.reports[0].from_cache);
    assert_eq!(r.http.urls().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn found_costs_two_requests_search_first_then_the_edition_json() {
    let r = rig(handler_with(edition_from_fixtures));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert_eq!(only_outcome(&result), ProviderOutcome::Found);
    assert_eq!(result.merged.text(MetaField::Title), Some("Clean Code"));
    assert_eq!(result.merged.number(MetaField::Pages), Some(431));
    assert_eq!(
        result.merged.source(MetaField::Description),
        Some(MetaSource::OpenLibrary)
    );
    let urls = r.http.urls();
    assert_eq!(urls.len(), 2, "{urls:?}");
    assert!(urls[0].starts_with("https://openlibrary.org/search.json?isbn=9780132350884&fields="));
    assert!(urls[0].contains("fields=key,title,author_name,editions,editions.key,"));
    assert_eq!(urls[1], "https://openlibrary.org/books/OL26222911M.json");

    // 保存するのは、検索と版の包み
    let entry = r.cache.entry(ProviderId::OpenLibrary, CLEAN_CODE).unwrap();
    assert!(entry.found);
    let body: Value = serde_json::from_str(&entry.body).unwrap();
    assert_eq!(body["search"]["docs"][0]["key"], "/works/OL17618370W");
    assert_eq!(body["edition"]["number_of_pages"], 431);

    // 2 回目はキャッシュの包みから（通信なしで、同じ結果）
    let again = r.service.fetch(&request(CLEAN_CODE)).await;
    assert!(again.reports[0].from_cache);
    assert_eq!(again.merged, result.merged);
    assert_eq!(r.http.urls().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn each_real_fixture_is_fetched_with_its_own_edition() {
    let r = rig(handler_with(edition_from_fixtures));
    for (isbn, olid, pages) in [
        (PRIDE, PRIDE_EDITION, 435),
        (RADIATION, RADIATION_EDITION, 1002),
    ] {
        let result = r.service.fetch(&request(isbn)).await;
        assert_eq!(only_outcome(&result), ProviderOutcome::Found);
        assert_eq!(result.merged.number(MetaField::Pages), Some(pages));
        assert!(
            r.http
                .urls()
                .contains(&format!("https://openlibrary.org/books/{olid}.json"))
        );
    }
}

#[tokio::test(start_paused = true)]
async fn edition_404_gives_the_search_data_only() {
    let r = rig(handler_with(|_| respond(404, r#"{"error":"notfound"}"#)));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert_eq!(only_outcome(&result), ProviderOutcome::Found);
    assert_eq!(result.merged.text(MetaField::Title), Some("Clean Code"));
    assert_eq!(
        result.merged.text(MetaField::Publisher),
        Some("Prentice Hall")
    );
    assert!(result.merged.chosen(MetaField::Pages).is_none());
    assert!(result.merged.chosen(MetaField::Description).is_none());
    assert_eq!(r.http.urls().len(), 2, "404 はリトライしない");
    // 取得元の包みの edition は null。保存される
    let entry = r.cache.entry(ProviderId::OpenLibrary, CLEAN_CODE).unwrap();
    assert!(entry.found);
    let body: Value = serde_json::from_str(&entry.body).unwrap();
    assert!(body["edition"].is_null());
}

#[tokio::test(start_paused = true)]
async fn redirect_edition_is_not_followed_and_gives_the_search_data_only() {
    let r = rig(handler_with(|_| {
        respond(
            200,
            r#"{"type":{"key":"/type/redirect"},"location":"/books/OL99M","key":"/books/OL26222911M"}"#,
        )
    }));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert_eq!(only_outcome(&result), ProviderOutcome::Found);
    assert_eq!(result.merged.text(MetaField::Title), Some("Clean Code"));
    assert!(result.merged.chosen(MetaField::Pages).is_none());
    let urls = r.http.urls();
    assert_eq!(
        urls.len(),
        2,
        "リダイレクト先（OL99M）は取りにいかない: {urls:?}"
    );
    let entry = r.cache.entry(ProviderId::OpenLibrary, CLEAN_CODE).unwrap();
    let body: Value = serde_json::from_str(&entry.body).unwrap();
    assert!(body["edition"].is_null());
}

#[tokio::test(start_paused = true)]
async fn edition_server_error_fails_the_whole_fetch_and_nothing_is_cached() {
    let r = rig(handler_with(|_| respond(500, "internal error")));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    match only_outcome(&result) {
        ProviderOutcome::HttpError { status, .. } => assert_eq!(status, 500),
        other => panic!("{other:?}"),
    }
    assert!(result.merged.is_empty());
    assert!(!result.offline, "サーバにはつながっている");
    assert!(
        r.cache.is_empty(),
        "取得全体の失敗は保存しない（あとで取り直す）"
    );
    // 検索 1 回 + 版の JSON は、トランスポートがリトライした 4 回
    let urls = r.http.urls();
    assert_eq!(urls.len(), 5, "{urls:?}");
    assert_eq!(urls.iter().filter(|u| u.contains("/books/")).count(), 4);
}

#[tokio::test(start_paused = true)]
async fn edition_429_after_retries_and_other_http_errors_also_fail_the_fetch() {
    for status in [429u16, 403, 502] {
        let r = rig(handler_with(move |_| respond(status, "no")));
        let result = r.service.fetch(&request(CLEAN_CODE)).await;
        match only_outcome(&result) {
            ProviderOutcome::HttpError { status: got, .. } => assert_eq!(got, status),
            other => panic!("{status}: {other:?}"),
        }
        assert!(r.cache.is_empty(), "{status}");
    }
}

#[tokio::test(start_paused = true)]
async fn edition_network_error_fails_the_whole_fetch() {
    let r = rig(handler_with(|_| Err(TransportError::Connect("dns".into()))));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert!(matches!(
        only_outcome(&result),
        ProviderOutcome::NetworkError { .. }
    ));
    assert!(result.offline);
    assert!(r.cache.is_empty());
    assert_eq!(r.http.urls().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn edition_that_is_not_json_fails_the_whole_fetch_as_a_parse_error() {
    let r = rig(handler_with(|_| respond(200, "<html>maintenance</html>")));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert!(matches!(
        only_outcome(&result),
        ProviderOutcome::ParseError { .. }
    ));
    assert!(r.cache.is_empty());
    assert_eq!(r.http.urls().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn search_network_error_is_a_network_error_and_makes_no_second_request() {
    let r = rig(Box::new(|_| Err(TransportError::Connect("offline".into()))));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert!(matches!(
        only_outcome(&result),
        ProviderOutcome::NetworkError { .. }
    ));
    assert!(result.offline);
    assert!(r.cache.is_empty());
    assert_eq!(r.http.urls().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn search_http_error_and_broken_bodies_fail_without_a_second_request() {
    let r = rig(Box::new(|_| respond(503, "maintenance")));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert!(matches!(
        only_outcome(&result),
        ProviderOutcome::HttpError { status: 503, .. }
    ));
    assert!(r.cache.is_empty());
    assert_eq!(
        r.http.urls().len(),
        4,
        "リトライだけ。版の JSON は取りにいかない"
    );

    for body in ["<html>maintenance</html>", r#"{"error":"x"}"#, "[]"] {
        let r = rig(Box::new(move |_| respond(200, body)));
        let result = r.service.fetch(&request(CLEAN_CODE)).await;
        assert!(
            matches!(only_outcome(&result), ProviderOutcome::ParseError { .. }),
            "{body}"
        );
        assert!(r.cache.is_empty());
        assert_eq!(r.http.urls().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn an_edition_key_in_an_unexpected_shape_is_never_requested() {
    let search = search_with(
        json!({"key": "/works/OL1W", "title": "T"}),
        vec![json!({"key": "https://evil.example/x", "isbn": [CLEAN_CODE]})],
    );
    let body = search.to_string();
    let r = rig(Box::new(move |_| respond(200, body.clone())));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    assert!(matches!(
        only_outcome(&result),
        ProviderOutcome::ParseError { .. }
    ));
    assert_eq!(r.http.urls().len(), 1, "想定外の key の版は取りにいかない");
}

#[tokio::test(start_paused = true)]
async fn requests_and_cover_downloads_share_one_rate_limit_group() {
    let limit = default_rate_limit(ProviderId::OpenLibrary);
    assert_eq!(limit.group, "openlibrary");
    assert_eq!(limit.min_interval_ms, 1000);
    assert_eq!(limit.max_concurrent, 1);

    // 書誌の 2 回の通信と、書影のダウンロードが、同じグループで 1 秒以上あく
    let r = rig(Box::new(|url| {
        if url.starts_with("https://covers.openlibrary.org/") {
            return Ok(HttpResponse {
                status: 404,
                content_type: None,
                retry_after: None,
                body: Vec::new(),
            });
        }
        handler_with(edition_from_fixtures)(url)
    }));
    let result = r.service.fetch(&request(CLEAN_CODE)).await;
    let covers = result.merged.covers_in_order();
    assert_eq!(covers.len(), 1);
    let e = download_cover(r.service.transport(), &covers[0])
        .await
        .unwrap_err();
    // `default=false` の書影がなければ 404（空白の画像ではない）
    assert!(matches!(e, FetchError::Http { status: 404, .. }), "{e:?}");
    let urls = r.http.urls();
    assert_eq!(urls.len(), 3, "{urls:?}");
    assert_eq!(
        urls[2],
        "https://covers.openlibrary.org/b/id/15126503-L.jpg?default=false"
    );
    let times = r.http.times();
    for w in times.windows(2) {
        assert!(w[1] - w[0] >= Duration::from_secs(1), "{:?}", w[1] - w[0]);
    }
}

// ---------------------------------------------------------------------------
// 本物の HTTP 層（wiremock）
// ---------------------------------------------------------------------------

mod over_http {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn transport() -> Transport {
        Transport::new(Arc::new(ReqwestClient::new(UA).unwrap())).with_retry(RetryPolicy {
            max_retries: 1,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
        })
    }

    fn json_response(body: String) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(body, "application/json")
    }

    async fn mock_clean_code(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/search.json"))
            .and(query_param("isbn", CLEAN_CODE))
            .and(query_param(
                "fields",
                "key,title,author_name,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.publisher,editions.publish_date,editions.language,editions.cover_i,editions.isbn",
            ))
            .respond_with(json_response(fixture(&format!(
                "openlibrary_search_{CLEAN_CODE}.json"
            ))))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/books/{CLEAN_CODE_EDITION}.json")))
            .respond_with(json_response(fixture(&format!(
                "openlibrary_edition_{CLEAN_CODE_EDITION}.json"
            ))))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn fetch_over_a_real_http_stack_sends_the_user_agent_and_the_search_parameters() {
        let server = MockServer::start().await;
        mock_clean_code(&server).await;
        let p = OpenLibrary::with_base_url(&server.uri());
        let raw = p.fetch_raw(&transport(), CLEAN_CODE).await.unwrap();
        let c = p.parse(&raw).unwrap().unwrap();
        assert_eq!(c.title.as_deref(), Some("Clean Code"));
        assert_eq!(c.pages, Some(431));
        assert_eq!(
            c.covers[0].url,
            format!("{}/b/id/15126503-L.jpg?default=false", server.uri())
        );
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url.path(), "/search.json");
        assert_eq!(requests[1].url.path(), "/books/OL26222911M.json");
        for r in &requests {
            assert_eq!(r.headers.get("user-agent").unwrap(), UA);
            assert!(r.headers.get("referer").is_none());
        }
    }

    #[tokio::test]
    async fn not_found_over_http_is_a_single_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search.json"))
            .respond_with(json_response(fixture(&format!(
                "openlibrary_search_{KOKORO}.json"
            ))))
            .mount(&server)
            .await;
        let p = OpenLibrary::with_base_url(&server.uri());
        let raw = p.fetch_raw(&transport(), KOKORO).await.unwrap();
        assert_eq!(p.parse(&raw).unwrap(), None);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn edition_errors_over_http() {
        let search = fixture(&format!("openlibrary_search_{CLEAN_CODE}.json"));
        // 404: 検索の結果だけで続ける
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search.json"))
            .respond_with(json_response(search.clone()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/books/{CLEAN_CODE_EDITION}.json")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let p = OpenLibrary::with_base_url(&server.uri());
        let raw = p.fetch_raw(&transport(), CLEAN_CODE).await.unwrap();
        assert!(p.parse(&raw).unwrap().unwrap().pages.is_none());
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "404 はリトライしない"
        );
        // 500: 取得全体の失敗
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search.json"))
            .respond_with(json_response(search))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/books/{CLEAN_CODE_EDITION}.json")))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let p = OpenLibrary::with_base_url(&server.uri());
        let err = p.fetch_raw(&transport(), CLEAN_CODE).await.unwrap_err();
        assert!(
            matches!(&err, FetchError::Http { status: 500, message } if message == "boom"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn cover_by_id_is_downloaded_through_the_cover_url_and_a_missing_cover_is_404() {
        let server = MockServer::start().await;
        let img = image::RgbImage::from_fn(300, 400, |x, y| {
            image::Rgb([(x / 2) as u8, (y / 2) as u8, 90])
        });
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        Mock::given(method("GET"))
            .and(path("/b/id/15126503-L.jpg"))
            .and(query_param("default", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.into_inner(), "image/jpeg"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/b/id/999-L.jpg"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let p = OpenLibrary::with_base_url(&server.uri());
        let search = search_fixture(CLEAN_CODE);
        let c = p
            .parse(&raw(
                CLEAN_CODE,
                &search,
                Some(&edition_fixture(CLEAN_CODE_EDITION)),
            ))
            .unwrap()
            .unwrap();
        let t = transport();
        let got = download_cover(&t, &c.covers[0]).await.unwrap();
        assert!(matches!(got, CoverDownload::Image(_)), "{got:?}");
        let missing = CandidateCover {
            provider: ProviderId::OpenLibrary,
            url: format!("{}/b/id/999-L.jpg?default=false", server.uri()),
        };
        let err = download_cover(&t, &missing).await.unwrap_err();
        assert!(
            matches!(err, FetchError::Http { status: 404, .. }),
            "{err:?}"
        );
    }
}
