//! キーワード検索（書名・著者。フェーズ 3）のテスト。ネットワークは使わない。
//!
//! - 解析: `tests/fixtures/` の実際の応答（NDL は 2026-10-17、Open Library は 2026-10-17）と、実際の応答にない形を作った応答。
//!   Google Books の検索の成功例は、API キーがなく取れていないので、公式の `volumes.list` の形に合わせた合成
//!   （`google_synthetic_search_clean_code.json`。id が `SYNTHETIC…`）。
//! - 通信: HTTP の層を偽物に差し替えて、送った URL・回数・間隔を見る（`Fake`）。NDL の問い合わせは、wiremock で
//!   本物の HTTP の層を通して、サーバーが受け取る `query` が CQL のままであることも確かめる。

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bibcontainer_metadata::error::{FetchError, SearchError};
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::provider::{MAX_SEARCH_TERM_CHARS, Provider, SearchQuery};
use bibcontainer_metadata::providers::google::{GoogleFailure, classify_failure};
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd, OpenLibrary, default_providers};
use bibcontainer_metadata::service::MetadataService;
use bibcontainer_metadata::types::{ProviderId, SearchResults};
use bibcontainer_metadata::{ReqwestClient, RetryPolicy, Transport};
use common::fixture;
use tokio::time::Instant;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn xml(body: impl Into<String>) -> Reply {
    reply(200, "text/xml;charset=UTF-8", body)
}

fn json(body: impl Into<String>) -> Reply {
    reply(200, "application/json; charset=UTF-8", body)
}

/// 呼ばれた URL と時刻（`tokio::time::Instant`）を記録して、決めた応答を返す偽の HTTP クライアント。
struct Fake {
    handler: Handler,
    log: Mutex<Vec<(Instant, String)>>,
}

impl Fake {
    fn new(handler: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Fake {
            handler: Box::new(handler),
            log: Mutex::new(Vec::new()),
        })
    }

    /// どの URL にも同じ応答を返す。
    fn always(make: impl Fn() -> Reply + Send + Sync + 'static) -> Arc<Self> {
        Fake::new(move |_| make())
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
        (self.handler)(&req.url)
    }
}

fn transport(fake: &Arc<Fake>) -> Transport {
    Transport::new(fake.clone())
}

fn query(title: Option<&str>, author: Option<&str>) -> SearchQuery {
    SearchQuery {
        title: title.map(str::to_string),
        author: author.map(str::to_string),
    }
}

/// `query=` の値（パーセントエンコードを戻したもの）。
fn query_param_of(url: &str, name: &str) -> String {
    let query = url.split_once('?').expect("クエリがある").1;
    let value = query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("{name} がない: {url}"));
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(&value[i + 1..i + 3], 16).expect("16 進数"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("UTF-8")
}

// ---------------------------------------------------------------------------
// 条件の整え方（取得元によらない）
// ---------------------------------------------------------------------------

#[test]
fn query_is_normalized_and_checked() {
    // NFKC（全角英数・全角空白 → 半角）、前後の空白、連続する空白
    let q = query(Some("  Ｃｌｅａｎ　　Ｃｏｄｅ \n"), Some("\tMartin ")).validated();
    assert_eq!(q, Ok(query(Some("Clean Code"), Some("Martin"))));
    // 空は None。片方だけでよい
    assert_eq!(
        query(Some("こころ"), Some("  　 ")).validated(),
        Ok(query(Some("こころ"), None))
    );
    assert_eq!(
        query(None, Some("夏目漱石")).validated(),
        Ok(query(None, Some("夏目漱石")))
    );
    // 何度通しても同じ
    let once = query(Some("ＡＢＣ　ＤＥＦ"), None).validated().unwrap();
    assert_eq!(once.validated(), Ok(once.clone()));
}

#[test]
fn query_without_a_term_is_invalid() {
    for q in [
        query(None, None),
        query(Some(""), None),
        query(Some("   "), Some("\u{3000}")),
    ] {
        match q.validated() {
            Err(SearchError::InvalidQuery(m)) => assert_eq!(m, "書名か著者を入れてください"),
            other => panic!("{q:?}: {other:?}"),
        }
    }
}

#[test]
fn query_terms_are_limited_in_characters_not_bytes() {
    let ok = "あ".repeat(MAX_SEARCH_TERM_CHARS);
    assert_eq!(MAX_SEARCH_TERM_CHARS, 200);
    assert!(query(Some(&ok), Some(&ok)).validated().is_ok());
    let long = "あ".repeat(MAX_SEARCH_TERM_CHARS + 1);
    match query(Some(&long), None).validated() {
        Err(SearchError::InvalidQuery(m)) => assert!(m.contains("書名"), "{m}"),
        other => panic!("{other:?}"),
    }
    match query(Some("x"), Some(&long)).validated() {
        Err(SearchError::InvalidQuery(m)) => assert!(m.contains("著者"), "{m}"),
        other => panic!("{other:?}"),
    }
    // 空白を整えたあとの長さで数える（前後の空白は数えない）
    let padded = format!("  {ok}  ");
    assert!(query(Some(&padded), None).validated().is_ok());
}

// ---------------------------------------------------------------------------
// NDL: 問い合わせ
// ---------------------------------------------------------------------------

const NDL_TAIL: &str = "mediatype = \"books\" AND dpid = \"iss-ndl-opac\" AND sortBy = \"issued_date/sort.descending\"";

/// NDL に問い合わせて、送った URL を返す（応答は「該当なし」）。
async fn ndl_url(q: SearchQuery) -> Result<String, SearchError> {
    let fake = Fake::always(|| xml(fixture("ndl_notfound.xml")));
    Ndl::new().search(&transport(&fake), &q).await?;
    let urls = fake.urls();
    assert_eq!(urls.len(), 1);
    Ok(urls[0].clone())
}

async fn ndl_cql(title: Option<&str>, author: Option<&str>) -> String {
    let url = ndl_url(query(title, author)).await.unwrap();
    query_param_of(&url, "query")
}

#[tokio::test(start_paused = true)]
async fn ndl_search_url_is_the_one_checked_against_the_real_server() {
    // 2026-10-17 に実際のサーバーへ送って、結果が返ることを確かめた URL（件数だけ 5 → 50）
    let url = ndl_url(query(Some("Clean Code"), Some("martin")))
        .await
        .unwrap();
    assert_eq!(
        url,
        "https://ndlsearch.ndl.go.jp/api/sru?operation=searchRetrieve&version=1.2&recordSchema=dcndl&recordPacking=xml&onlyBib=true&maximumRecords=50&query=title%20%3D%20%22Clean%20Code%22%20AND%20creator%20%3D%20%22martin%22%20AND%20mediatype%20%3D%20%22books%22%20AND%20dpid%20%3D%20%22iss-ndl-opac%22%20AND%20sortBy%20%3D%20%22issued_date%2Fsort.descending%22"
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_cql_has_the_given_clauses_in_a_fixed_order() {
    assert_eq!(
        ndl_cql(Some("こころ"), Some("夏目漱石")).await,
        format!("title = \"こころ\" AND creator = \"夏目漱石\" AND {NDL_TAIL}")
    );
    assert_eq!(
        ndl_cql(Some("スローループ"), None).await,
        format!("title = \"スローループ\" AND {NDL_TAIL}")
    );
    assert_eq!(
        ndl_cql(None, Some("夏目漱石")).await,
        format!("creator = \"夏目漱石\" AND {NDL_TAIL}")
    );
    // 複数の語は 1 つの引用符の中に（空白は 1 つに整える）。NFKC もかかる
    assert_eq!(
        ndl_cql(Some("ハリー・ポッター　　賢者の石"), None).await,
        format!("title = \"ハリー・ポッター 賢者の石\" AND {NDL_TAIL}")
    );
    assert_eq!(
        ndl_cql(Some("Ｃｌｅａｎ Ｃｏｄｅ"), None).await,
        format!("title = \"Clean Code\" AND {NDL_TAIL}")
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_cql_strips_quotes_and_backslashes_from_the_terms() {
    // `"` と `\` は CQL の文字列を壊すので空白にして、空白を整える
    assert_eq!(
        ndl_cql(Some("\"Clean\" Code\\"), Some("Mar\\\"tin")).await,
        format!("title = \"Clean Code\" AND creator = \"Mar tin\" AND {NDL_TAIL}")
    );
    // 空になった条件は落とす
    assert_eq!(
        ndl_cql(Some("\"\" \\"), Some("martin")).await,
        format!("creator = \"martin\" AND {NDL_TAIL}")
    );
    assert_eq!(
        ndl_cql(Some("こころ"), Some("\"")).await,
        format!("title = \"こころ\" AND {NDL_TAIL}")
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_cql_keeps_and_or_words_inside_quotes() {
    // 語の途中に AND・OR が入る語（organic、Andy）も、語として入る語（war and peace）も、引用符に入れて
    // `=` の前後に空白を付ける形なら、2026-10-17 に実際のサーバーでエラーにならなかった
    for (word, author) in [
        ("organic", None),
        ("Andy Weir", Some("ORACLE")),
        ("war and peace", None),
    ] {
        let cql = ndl_cql(Some(word), author).await;
        assert!(cql.starts_with(&format!("title = \"{word}\"")), "{cql}");
        if let Some(a) = author {
            assert!(
                cql.contains(&format!(" AND creator = \"{a}\" AND ")),
                "{cql}"
            );
        }
        assert!(cql.ends_with(NDL_TAIL), "{cql}");
    }
}

#[tokio::test(start_paused = true)]
async fn ndl_cql_lowercases_words_spelled_like_operators() {
    // 演算子と同じ綴りの語だけ（語の一部は変えない）。大文字小文字の混ざった `And` も、小文字にそろえる
    assert_eq!(
        ndl_cql(
            Some("Pride AND Prejudice OR Not And ORACLE android"),
            Some("NOT")
        )
        .await,
        format!(
            "title = \"Pride and Prejudice or not and ORACLE android\" AND creator = \"not\" AND {NDL_TAIL}"
        )
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_invalid_queries_send_nothing() {
    let long = "あ".repeat(201);
    for q in [
        query(None, None),
        query(Some("   "), Some("")),
        // 引用符とバックスラッシュだけ（取り除くと何も残らない）
        query(Some("\"\"\\"), Some("\\")),
        query(Some(&long), None),
        query(None, Some(&long)),
    ] {
        let fake = Fake::always(|| xml(fixture("ndl_notfound.xml")));
        let err = Ndl::new().search(&transport(&fake), &q).await.unwrap_err();
        assert!(
            matches!(err, SearchError::InvalidQuery(_)),
            "{q:?}: {err:?}"
        );
        assert!(fake.urls().is_empty(), "{q:?}");
    }
}

#[tokio::test]
async fn ndl_search_reaches_a_real_http_server_with_the_cql_intact() {
    let server = MockServer::start().await;
    let cql = format!("title = \"Clean Code\" AND creator = \"夏目 漱石\" AND {NDL_TAIL}");
    Mock::given(method("GET"))
        .and(path("/api/sru"))
        .and(query_param("operation", "searchRetrieve"))
        .and(query_param("version", "1.2"))
        .and(query_param("recordSchema", "dcndl"))
        .and(query_param("onlyBib", "true"))
        .and(query_param("maximumRecords", "50"))
        .and(query_param("query", cql.as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/xml;charset=UTF-8")
                .set_body_string(fixture("ndl_search_slowloop.xml")),
        )
        .mount(&server)
        .await;
    let client = ReqwestClient::new("bibcontainer/test").unwrap();
    let t = Transport::new(Arc::new(client));
    let results = Ndl::with_base_url(&server.uri())
        .search(&t, &query(Some("Clean Code"), Some("夏目 漱石")))
        .await
        .unwrap();
    assert_eq!(results.hits.len(), 11);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// NDL: 解析（実際の応答）
// ---------------------------------------------------------------------------

async fn ndl_search_with(body: String) -> Result<SearchResults, SearchError> {
    let fake = Fake::always(move || xml(body.clone()));
    Ndl::new()
        .search(&transport(&fake), &query(Some("x"), None))
        .await
}

async fn ndl_hits(fixture_name: &str) -> SearchResults {
    ndl_search_with(fixture(fixture_name)).await.unwrap()
}

#[tokio::test(start_paused = true)]
async fn ndl_slowloop_has_every_volume_newest_first() {
    let results = ndl_hits("ndl_search_slowloop.xml").await;
    assert_eq!(results.total, Some(11));
    assert_eq!(results.hits.len(), 11);
    // サーバーの順は 10、11、1〜9。刊行年の新しい順（11、10、9、…、1）に並べ直す
    let volumes: Vec<&str> = results
        .hits
        .iter()
        .map(|h| h.candidate.volume.as_deref().unwrap())
        .collect();
    assert_eq!(
        volumes,
        ["11", "10", "9", "8", "7", "6", "5", "4", "3", "2", "1"]
    );
    let dates: Vec<&str> = results
        .hits
        .iter()
        .map(|h| h.candidate.published_on.as_deref().unwrap())
        .collect();
    assert_eq!(
        dates,
        [
            "2026-07", "2025-03", "2024-05", "2023-08", "2022-11", "2022-03", "2021-12", "2021-01",
            "2020-06", "2019-10", "2019-03"
        ]
    );

    let newest = &results.hits[0];
    assert_eq!(newest.provider, ProviderId::Ndl);
    assert_eq!(newest.isbn13.as_deref(), Some("9784832297333"));
    assert_eq!(newest.candidate.isbn13, "9784832297333");
    assert_eq!(newest.candidate.provider, ProviderId::Ndl);
    assert_eq!(newest.candidate.title.as_deref(), Some("スローループ"));
    assert_eq!(newest.candidate.publisher.as_deref(), Some("芳文社"));
    assert_eq!(newest.candidate.authors[0].name, "うちのまいこ");
    assert_eq!(newest.candidate.authors[0].role.as_deref(), Some("著"));
    // ページの URL は、書誌の `rdf:about` の `#material` を除いたもの
    assert_eq!(
        newest.link.as_deref(),
        Some("https://ndlsearch.ndl.go.jp/books/R100000002-I034807010")
    );
    let oldest = results.hits.last().unwrap();
    assert_eq!(oldest.isbn13.as_deref(), Some("9784832270763"));
    assert_eq!(
        oldest.link.as_deref(),
        Some("https://ndlsearch.ndl.go.jp/books/R100000002-I029554538")
    );
    // どの ISBN も重ならず、候補の ISBN は結果の ISBN と同じ
    let mut isbns: Vec<&str> = results
        .hits
        .iter()
        .map(|h| h.isbn13.as_deref().expect("全巻に ISBN がある"))
        .collect();
    assert!(
        results
            .hits
            .iter()
            .all(|h| h.candidate.isbn13 == h.isbn13.clone().unwrap())
    );
    isbns.sort();
    isbns.dedup();
    assert_eq!(isbns.len(), 11);
}

#[tokio::test(start_paused = true)]
async fn ndl_kokoro_converts_isbn10_and_leaves_old_books_without_isbn() {
    let results = ndl_hits("ndl_search_kokoro_soseki.xml").await;
    assert_eq!(
        results.total,
        Some(101),
        "該当の総件数は、返った 20 件より多い"
    );
    assert_eq!(results.hits.len(), 20);
    // 刊行年の新しい順（日付のない本はない）。和暦だけの古い本は、年（`dcterms:issued`）が使われる
    let dates: Vec<&str> = results
        .hits
        .iter()
        .map(|h| h.candidate.published_on.as_deref().expect("日付がある"))
        .collect();
    let mut sorted = dates.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(dates, sorted);
    assert_eq!(
        &dates[..7],
        [
            "2020-01", "2016-12", "2014-07", "2010-05", "2006-12", "2005-07", "1997-04"
        ]
    );
    assert_eq!(&dates[18..], ["1917", "1914"]);

    // ISBN があるのは 7 件。ハイフン付き・ISBN-10 は ISBN-13 に直す
    let isbns: Vec<Option<&str>> = results.hits.iter().map(|h| h.isbn13.as_deref()).collect();
    assert_eq!(
        &isbns[..7],
        [
            Some("9787538761344"), // 中国語版（ISBN-13）
            Some("9784803009736"), // 978-4-8030-0973-6
            Some("9784796405775"), // 978-4-7964-0577-5
            Some("9784834284157"), // 978-4-8342-8415-7
            Some("9784003550045"), // 4-00-355004-8（ISBN-10）。同じレコードの SetISBN は使わない
            Some("9784091848161"), // 4-09-184816-8（ISBN-10）
            Some("9784010660423"), // 4-01-066042-2（ISBN-10）
        ]
    );
    assert!(
        isbns[7..].iter().all(Option::is_none),
        "古い本は ISBN がない"
    );
    // ISBN がない本は、候補の ISBN も空
    for h in &results.hits {
        assert_eq!(h.candidate.isbn13, h.isbn13.clone().unwrap_or_default());
        assert!(
            h.link
                .as_deref()
                .is_some_and(|l| l.starts_with("https://ndlsearch.ndl.go.jp/books/R100000002-I"))
        );
    }
    // 著者の役割は、ISBN の取得と同じ読み方
    assert_eq!(results.hits[6].candidate.authors[0].name, "夏目漱石");
    assert_eq!(
        results.hits[6].candidate.series.as_deref(),
        Some("愛と青春の名作集")
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_server_side_sorted_response_keeps_its_order_for_equal_dates() {
    // `sortBy` を付けた実際の応答（5 件とも刊行年 2026 だけ。並べ直しても、応答の順のまま）
    let results = ndl_hits("ndl_search_war_and_peace_sorted.xml").await;
    assert_eq!(results.total, Some(1791));
    let titles: Vec<&str> = results
        .hits
        .iter()
        .map(|h| h.candidate.title.as_deref().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Why are we in Okinawa?",
            "The Russo-Ukrainian War",
            "The 'pacifist principle' in the Italian and Japanese constitutions in the wake of the war in Ukraine",
            "Medievalism in Japanese manga",
            "Beyond the San Francisco system",
        ]
    );
    assert!(
        results
            .hits
            .iter()
            .all(|h| h.candidate.published_on.as_deref() == Some("2026"))
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_not_found_is_an_empty_result_and_other_diagnostics_are_errors() {
    for body in [
        fixture("ndl_notfound.xml"),
        // `numberOfRecords` が 0 で、レコードも診断もない
        "<searchRetrieveResponse xmlns=\"http://www.loc.gov/zing/srw/\"><version>1.2</version><numberOfRecords>0</numberOfRecords></searchRetrieveResponse>".to_string(),
    ] {
        let results = ndl_search_with(body).await.unwrap();
        assert_eq!(
            results,
            SearchResults {
                total: Some(0),
                hits: Vec::new()
            }
        );
    }
    // `sortBy` を末尾に付けて値に巻き込まれたときの、実際のエラーの応答
    match ndl_search_with(fixture("ndl_search_error_diagnostic.xml")).await {
        Err(SearchError::Fetch(FetchError::Parse(m))) => {
            assert!(m.contains("illegal mediaType value"), "{m}")
        }
        other => panic!("{other:?}"),
    }
    // XML でない・SRU の形でない
    for body in ["<html>busy</html>", "not xml", ""] {
        assert!(
            matches!(
                ndl_search_with(body.to_string()).await,
                Err(SearchError::Fetch(FetchError::Parse(_)))
            ),
            "{body:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn ndl_http_errors_are_fetch_errors() {
    let fake = Fake::always(|| reply(400, "text/plain", "bad request"));
    let err = Ndl::new()
        .search(&transport(&fake), &query(Some("x"), None))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        SearchError::Fetch(FetchError::Http {
            status: 400,
            message: "bad request".into()
        })
    );
    // つながらない
    let fake = Fake::always(|| Err(TransportError::Connect("dns".into())));
    let err = Ndl::new()
        .search(&transport(&fake), &query(Some("x"), None))
        .await
        .unwrap_err();
    assert!(
        matches!(err, SearchError::Fetch(FetchError::Network(_))),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------
// NDL: 実際の応答にない形（合成した SRU）
// ---------------------------------------------------------------------------

const ISBN_TYPE: &str = "http://ndl.go.jp/dcndl/terms/ISBN";

struct Rec<'a> {
    about: &'a str,
    title: &'a str,
    /// （識別子の型の URI、値）
    ids: Vec<(&'a str, &'a str)>,
    date: Option<&'a str>,
}

impl<'a> Rec<'a> {
    fn new(title: &'a str) -> Self {
        Rec {
            about: "https://ndlsearch.ndl.go.jp/books/R100000002-I000000000001#material",
            title,
            ids: Vec::new(),
            date: None,
        }
    }

    fn isbn(mut self, isbn: &'a str) -> Self {
        self.ids.push((ISBN_TYPE, isbn));
        self
    }

    fn date(mut self, date: &'a str) -> Self {
        self.date = Some(date);
        self
    }

    fn about(mut self, about: &'a str) -> Self {
        self.about = about;
        self
    }

    fn xml(&self) -> String {
        let ids: String = self
            .ids
            .iter()
            .map(|(t, v)| {
                format!("<dcterms:identifier rdf:datatype=\"{t}\">{v}</dcterms:identifier>")
            })
            .collect();
        let date = self
            .date
            .map(|d| format!("<dcterms:date>{d}</dcterms:date>"))
            .unwrap_or_default();
        format!(
            "<record><recordSchema>info:srw/schema/1/dc-v1.1</recordSchema><recordPacking>xml</recordPacking><recordData>\
             <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcndl=\"http://ndl.go.jp/dcndl/terms/\">\
             <dcndl:BibResource rdf:about=\"{}\">{ids}<dcterms:title>{}</dcterms:title>{date}</dcndl:BibResource>\
             </rdf:RDF></recordData></record>",
            self.about, self.title
        )
    }
}

fn sru(total: u64, records: &[Rec<'_>]) -> String {
    let records: String = records.iter().map(Rec::xml).collect();
    format!(
        "<searchRetrieveResponse xmlns=\"http://www.loc.gov/zing/srw/\"><version>1.2</version><numberOfRecords>{total}</numberOfRecords><records>{records}</records></searchRetrieveResponse>"
    )
}

fn titles(results: &SearchResults) -> Vec<&str> {
    results
        .hits
        .iter()
        .map(|h| h.candidate.title.as_deref().unwrap())
        .collect()
}

#[tokio::test(start_paused = true)]
async fn ndl_duplicate_isbns_keep_only_the_first_record() {
    let results = ndl_search_with(sru(
        3,
        &[
            Rec::new("先に出たもの")
                .isbn("978-4-00-310111-7")
                .date("2020"),
            // 同じ ISBN（10 桁の表記でも）の 2 件目は捨てる
            Rec::new("同じ ISBN").isbn("4003101111").date("2021"),
            Rec::new("別の本").isbn("978-4-7808-0204-7").date("2019"),
        ],
    ))
    .await
    .unwrap();
    assert_eq!(titles(&results), ["先に出たもの", "別の本"]);
    // 件数は応答のまま（重なりを除く前）
    assert_eq!(results.total, Some(3));
}

#[tokio::test(start_paused = true)]
async fn ndl_isbn_is_the_first_valid_isbn_typed_identifier() {
    let mut rec = Rec::new("本");
    rec.ids = vec![
        ("http://ndl.go.jp/dcndl/terms/ErrorISBN", "9784003101118"),
        ("http://ndl.go.jp/dcndl/terms/SetISBN", "9784780802047"),
        (ISBN_TYPE, "978-4-00-310111-8"), // チェックデジットが違う
        (ISBN_TYPE, "ISBN 未登録"),
        (ISBN_TYPE, "４-７８０８-０２０４-０"), // 全角・ISBN-10
        (ISBN_TYPE, "978-4-00-310111-7"),
    ];
    let results = ndl_search_with(sru(1, &[rec])).await.unwrap();
    assert_eq!(results.hits[0].isbn13.as_deref(), Some("9784780802047"));

    // 使える ISBN が 1 つもなければ None
    let mut rec = Rec::new("本");
    rec.ids = vec![("http://ndl.go.jp/dcndl/terms/ErrorISBN", "9784003101117")];
    let results = ndl_search_with(sru(1, &[rec])).await.unwrap();
    assert_eq!(results.hits[0].isbn13, None);
    assert_eq!(results.hits[0].candidate.isbn13, "");
}

#[tokio::test(start_paused = true)]
async fn ndl_records_with_too_many_isbns_are_series_wide_and_get_none() {
    // ISBN が 4 つまでは 1 冊のレコード（先頭を使う）。5 つ以上はシリーズ全体のレコード
    let four = [
        "9784003101117",
        "9784780802047",
        "9784101001012",
        "9784088725093",
    ];
    let mut rec = Rec::new("4 つ");
    rec.ids = four.iter().map(|i| (ISBN_TYPE, *i)).collect();
    let mut series = Rec::new("全巻");
    series.ids = four
        .iter()
        .chain(["9784048930598"].iter())
        .map(|i| (ISBN_TYPE, *i))
        .collect();
    let results = ndl_search_with(sru(2, &[rec, series])).await.unwrap();
    assert_eq!(results.hits[0].isbn13.as_deref(), Some("9784003101117"));
    assert_eq!(results.hits[1].isbn13, None);
    assert_eq!(titles(&results), ["4 つ", "全巻"]);
}

#[tokio::test(start_paused = true)]
async fn ndl_records_without_a_date_go_last_and_the_rest_keep_a_stable_newest_first_order() {
    let results = ndl_search_with(sru(
        6,
        &[
            Rec::new("日付なし 1"),
            Rec::new("2019-03 a").date("2019.3"),
            Rec::new("2020 a").date("2020"),
            Rec::new("2019-03 b").date("2019.3"),
            Rec::new("日付なし 2"),
            Rec::new("2020-01").date("2020.1"),
        ],
    ))
    .await
    .unwrap();
    assert_eq!(
        titles(&results),
        [
            "2020-01",
            "2020 a",
            "2019-03 a",
            "2019-03 b",
            "日付なし 1",
            "日付なし 2"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn ndl_links_are_only_the_ndl_site_over_https() {
    let cases = [
        (
            "https://ndlsearch.ndl.go.jp/books/R100000002-I1#material",
            Some("https://ndlsearch.ndl.go.jp/books/R100000002-I1"),
        ),
        (
            "https://ndlsearch.ndl.go.jp/books/R100000002-I1",
            Some("https://ndlsearch.ndl.go.jp/books/R100000002-I1"),
        ),
        ("http://ndlsearch.ndl.go.jp/books/R100000002-I1", None),
        ("https://ndlsearch.ndl.go.jp.evil.example/books/x", None),
        ("https://evil.example/https://ndlsearch.ndl.go.jp/", None),
        ("javascript:alert(1)", None),
        ("", None),
    ];
    for (about, expected) in cases {
        let results = ndl_search_with(sru(1, &[Rec::new("本").about(about)]))
            .await
            .unwrap();
        assert_eq!(results.hits[0].link.as_deref(), expected, "{about:?}");
    }
}

// ---------------------------------------------------------------------------
// Google Books
// ---------------------------------------------------------------------------

const GOOGLE_SEARCH: &str = "https://www.googleapis.com/books/v1/volumes?q=";

async fn google_url(key: Option<&str>, q: SearchQuery) -> String {
    let fake = Fake::always(|| json(r#"{"kind":"books#volumes","totalItems":0}"#));
    GoogleBooks::new(key.map(str::to_string))
        .search(&transport(&fake), &q)
        .await
        .unwrap();
    let urls = fake.urls();
    assert_eq!(urls.len(), 1);
    urls[0].clone()
}

#[tokio::test(start_paused = true)]
async fn google_search_url_joins_the_terms_and_adds_the_key_only_when_given() {
    assert_eq!(
        google_url(Some("KEY"), query(Some("Clean Code"), Some("martin"))).await,
        format!("{GOOGLE_SEARCH}Clean%20Code%20martin&maxResults=20&printType=books&key=KEY")
    );
    assert_eq!(
        google_url(None, query(Some("こころ"), None)).await,
        format!("{GOOGLE_SEARCH}%E3%81%93%E3%81%93%E3%82%8D&maxResults=20&printType=books")
    );
    assert_eq!(
        google_url(None, query(None, Some("夏目 漱石"))).await,
        format!(
            "{GOOGLE_SEARCH}%E5%A4%8F%E7%9B%AE%20%E6%BC%B1%E7%9F%B3&maxResults=20&printType=books"
        )
    );
    // NFKC にそろえてから送る。キーは URL に使える形にエンコードする
    assert_eq!(
        google_url(Some("A B"), query(Some("Ｃｌｅａｎ　Ｃｏｄｅ"), None)).await,
        format!("{GOOGLE_SEARCH}Clean%20Code&maxResults=20&printType=books&key=A%20B")
    );
}

#[tokio::test(start_paused = true)]
async fn google_invalid_queries_send_nothing() {
    let fake = Fake::always(|| json("{}"));
    let err = GoogleBooks::new(None)
        .search(&transport(&fake), &query(Some(" "), None))
        .await
        .unwrap_err();
    assert!(matches!(err, SearchError::InvalidQuery(_)));
    assert!(fake.urls().is_empty());
}

async fn google_search_with(body: String) -> Result<SearchResults, SearchError> {
    let fake = Fake::always(move || json(body.clone()));
    GoogleBooks::new(Some("KEY".into()))
        .search(
            &transport(&fake),
            &query(Some("clean code"), Some("martin")),
        )
        .await
}

#[tokio::test(start_paused = true)]
async fn google_synthetic_search_keeps_google_order_and_every_item() {
    let results = google_search_with(fixture("google_synthetic_search_clean_code.json"))
        .await
        .unwrap();
    assert_eq!(results.total, Some(1234));
    // 7 件のうち、`volumeInfo` がない最後の 1 件だけ見せるものがない。残りは Google の順のまま
    let titles: Vec<&str> = results
        .hits
        .iter()
        .map(|h| h.candidate.title.as_deref().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Clean Code",
            "The Clean Coder",
            "クリーンコード",
            "Clean Code Collection",
            "Clean Code (OCLC only)",
            "Clean Code (checksum is wrong, id is too short)",
        ]
    );
    assert!(
        results
            .hits
            .iter()
            .all(|h| h.provider == ProviderId::Google)
    );
    assert!(
        results
            .hits
            .iter()
            .all(|h| h.candidate.provider == ProviderId::Google)
    );
    // 出版年の古い新しいで並べ替えない（2008 の次に 2011、2009）
    let dates: Vec<Option<&str>> = results
        .hits
        .iter()
        .map(|h| h.candidate.published_on.as_deref())
        .collect();
    assert_eq!(
        &dates[..3],
        [Some("2008-08-01"), Some("2011"), Some("2009-05")]
    );

    let first = &results.hits[0];
    assert_eq!(first.isbn13.as_deref(), Some("9780132350884"));
    assert_eq!(first.candidate.isbn13, "9780132350884");
    assert_eq!(
        first.candidate.subtitle.as_deref(),
        Some("A Handbook of Agile Software Craftsmanship")
    );
    assert_eq!(first.candidate.authors[0].name, "Robert C. Martin");
    assert_eq!(
        first.candidate.publisher.as_deref(),
        Some("Pearson Education")
    );
    assert_eq!(first.candidate.pages, Some(464));
    assert_eq!(first.candidate.language.as_deref(), Some("en"));
    assert_eq!(
        first.link.as_deref(),
        Some("https://books.google.com/books?id=SYNTHETICSRCH01")
    );
    // 書影は、大きい方（zoom=0）が先。https、`edge=curl` なし
    let covers: Vec<&str> = first
        .candidate
        .covers
        .iter()
        .map(|c| c.url.as_str())
        .collect();
    assert_eq!(
        covers,
        [
            "https://books.google.com/books/content?id=SYNTHETICSRCH01&printsec=frontcover&img=1&zoom=0&source=gbs_api",
            "https://books.google.com/books/content?id=SYNTHETICSRCH01&printsec=frontcover&img=1&zoom=1&source=gbs_api",
        ]
    );
    // 内容紹介の HTML は除く（ISBN の取得と同じ）
    assert_eq!(
        first.candidate.description.as_deref(),
        Some(
            "Even bad code can function.\nBut if code isn't clean, it can bring a development organization to its knees."
        )
    );

    // ISBN-10 だけなら、13 桁に直す
    assert_eq!(results.hits[1].isbn13.as_deref(), Some("9780137081073"));
    assert_eq!(results.hits[1].candidate.isbn13, "9780137081073");
    // 日本語の本
    assert_eq!(results.hits[2].isbn13.as_deref(), Some("9784048676885"));
    assert_eq!(results.hits[2].candidate.authors.len(), 2);
    // 識別子がない・ISBN でない・チェックデジットが違う本も、残る（ISBN は None、候補の ISBN は空）
    for i in [3, 4, 5] {
        assert_eq!(results.hits[i].isbn13, None, "{i}");
        assert_eq!(results.hits[i].candidate.isbn13, "", "{i}");
    }
    // `id` が URL に使える形（4〜32 文字の英数字・`_`・`-`）でなければ、リンクは付けない
    assert!(results.hits[..5].iter().all(|h| h.link.is_some()));
    assert_eq!(results.hits[5].link, None);
}

#[tokio::test(start_paused = true)]
async fn google_search_item_maps_like_the_isbn_fetch() {
    // 同じ項目から、ISBN の取得（`parse`）で作った候補と、検索の候補が一致する（写像を共有している）
    use bibcontainer_metadata::provider::RawResponse;
    let body = fixture("google_synthetic_search_clean_code.json");
    let results = google_search_with(body.clone()).await.unwrap();
    let p = GoogleBooks::new(None);
    for hit in results.hits.iter().take(3) {
        let isbn = hit.isbn13.as_deref().unwrap();
        let fetched = p
            .parse(&RawResponse {
                provider: ProviderId::Google,
                isbn13: isbn.to_string(),
                body: body.clone(),
            })
            .unwrap()
            .expect("ISBN が合う項目がある");
        assert_eq!(fetched, hit.candidate, "{isbn}");
    }
}

#[tokio::test(start_paused = true)]
async fn google_empty_and_malformed_responses() {
    // 該当なし
    let r = google_search_with(r#"{"kind":"books#volumes","totalItems":0}"#.into())
        .await
        .unwrap();
    assert_eq!(
        r,
        SearchResults {
            total: Some(0),
            hits: Vec::new()
        }
    );
    // `items` が空の配列でも同じ
    let r = google_search_with(r#"{"totalItems":0,"items":[]}"#.into())
        .await
        .unwrap();
    assert!(r.hits.is_empty());
    for body in ["not json", r#"{"error":{"code":400}}"#, ""] {
        assert!(
            matches!(
                google_search_with(body.into()).await,
                Err(SearchError::Fetch(FetchError::Parse(_)))
            ),
            "{body:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn google_http_errors_are_mapped_like_the_isbn_fetch() {
    let search = |status: u16, body: String, key: Option<&str>| {
        let fake =
            Fake::always(move || reply(status, "application/json; charset=UTF-8", body.clone()));
        let key = key.map(str::to_string);
        async move {
            let err = GoogleBooks::new(key)
                .search(&transport(&fake), &query(Some("x"), None))
                .await
                .unwrap_err();
            (err, fake.urls().len())
        }
    };
    // 実際の応答（キーなしの 429。割り当て 0）。1 日の上限は、やり直さない
    let (err, calls) = search(429, fixture("google_quota_exceeded.json"), None).await;
    assert_eq!(calls, 1);
    match err {
        SearchError::Fetch(FetchError::Http { status, message }) => {
            assert_eq!(status, 429);
            assert!(message.contains("Queries per day"), "{message}");
            assert!(message.contains("API キー"), "{message}");
            assert_eq!(classify_failure(status, &message), GoogleFailure::ZeroQuota);
        }
        other => panic!("{other:?}"),
    }
    // キーが無効（400）。キーをメッセージに出さない
    let invalid_key = r#"{"error":{"code":400,"message":"API key not valid. key=AIzaSECRET123 Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#;
    let (err, calls) = search(400, invalid_key.into(), Some("AIzaSECRET123")).await;
    assert_eq!(calls, 1, "400 はやり直さない");
    match err {
        SearchError::Fetch(FetchError::Http { status, message }) => {
            assert_eq!(status, 400);
            assert!(!message.contains("AIzaSECRET123"), "{message}");
            assert_eq!(
                classify_failure(status, &message),
                GoogleFailure::InvalidKey
            );
        }
        other => panic!("{other:?}"),
    }
    // 一時的な障害（503）は、やり直したあと HTTP エラー
    let (err, calls) = search(
        503,
        r#"{"error":{"code":503,"message":"The service is currently unavailable."}}"#.into(),
        Some("K"),
    )
    .await;
    assert_eq!(calls, 4, "最初の 1 回 + リトライ 3 回");
    assert!(
        matches!(
            err,
            SearchError::Fetch(FetchError::Http { status: 503, .. })
        ),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------
// Open Library
// ---------------------------------------------------------------------------

const OL_FIELDS: &str = "key,title,subtitle,author_name,cover_i,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.isbn,editions.publisher,editions.publish_date,editions.language,editions.cover_i";

async fn ol_url(q: SearchQuery) -> String {
    let fake = Fake::always(|| json(r#"{"numFound":0,"start":0,"docs":[]}"#));
    OpenLibrary::new()
        .search(&transport(&fake), &q)
        .await
        .unwrap();
    let urls = fake.urls();
    assert_eq!(urls.len(), 1);
    urls[0].clone()
}

#[tokio::test(start_paused = true)]
async fn open_library_search_url_has_only_the_given_terms() {
    assert_eq!(
        ol_url(query(Some("Clean Code"), Some("martin"))).await,
        format!(
            "https://openlibrary.org/search.json?title=Clean%20Code&author=martin&fields={OL_FIELDS}&limit=20"
        )
    );
    assert_eq!(
        ol_url(query(Some("こころ"), None)).await,
        format!(
            "https://openlibrary.org/search.json?title=%E3%81%93%E3%81%93%E3%82%8D&fields={OL_FIELDS}&limit=20"
        )
    );
    assert_eq!(
        ol_url(query(None, Some("Ｍａｒｔｉｎ"))).await,
        format!("https://openlibrary.org/search.json?author=Martin&fields={OL_FIELDS}&limit=20")
    );
    // 作品全体の `isbn` は頼まない（版の `editions.isbn` だけ）
    assert!(
        OL_FIELDS.split(',').all(|f| f != "isbn"),
        "作品全体の isbn を頼まない"
    );
    // 値に `&`・`#` があっても、ほかのパラメータにならない
    let url = ol_url(query(Some("a&b#c=d"), None)).await;
    assert!(url.contains("title=a%26b%23c%3Dd&fields="), "{url}");
}

#[tokio::test(start_paused = true)]
async fn open_library_invalid_queries_send_nothing() {
    let fake = Fake::always(|| json("{}"));
    let err = OpenLibrary::new()
        .search(&transport(&fake), &query(None, Some("")))
        .await
        .unwrap_err();
    assert!(matches!(err, SearchError::InvalidQuery(_)));
    assert!(fake.urls().is_empty());
}

async fn ol_search_with(body: String) -> Result<SearchResults, SearchError> {
    let fake = Fake::always(move || json(body.clone()));
    OpenLibrary::new()
        .search(&transport(&fake), &query(Some("x"), None))
        .await
}

#[tokio::test(start_paused = true)]
async fn open_library_clean_code_uses_the_matching_edition() {
    let results = ol_search_with(fixture("openlibrary_searchkw_clean_code_martin.json"))
        .await
        .unwrap();
    assert_eq!(results.total, Some(3));
    assert_eq!(results.hits.len(), 3, "作品 1 件が 1 件");

    let first = &results.hits[0];
    assert_eq!(first.provider, ProviderId::OpenLibrary);
    assert_eq!(first.candidate.provider, ProviderId::OpenLibrary);
    // ISBN は版の `isbn` から（作品全体の `isbn` には、ほかの版の ISBN が 24 件混ざっている）
    assert_eq!(first.isbn13.as_deref(), Some("9780132350884"));
    assert_eq!(first.candidate.isbn13, "9780132350884");
    assert_eq!(first.candidate.title.as_deref(), Some("Clean Code"));
    assert_eq!(
        first.candidate.subtitle.as_deref(),
        Some("A Handbook of Agile Software Craftsmanship")
    );
    assert_eq!(first.candidate.authors[0].name, "Robert C. Martin");
    assert_eq!(first.candidate.publisher.as_deref(), Some("Prentice Hall"));
    assert_eq!(first.candidate.published_on.as_deref(), Some("2008-07"));
    assert_eq!(first.candidate.language.as_deref(), Some("en"));
    // 書影は、版の `cover_i`（作品の 8065615 ではない）
    assert_eq!(
        first
            .candidate
            .covers
            .iter()
            .map(|c| c.url.as_str())
            .collect::<Vec<_>>(),
        ["https://covers.openlibrary.org/b/id/15126503-L.jpg?default=false"]
    );
    assert_eq!(
        first.link.as_deref(),
        Some("https://openlibrary.org/books/OL26222911M")
    );

    // 2 件目: 13 桁の ISBN。書影なし
    let second = &results.hits[1];
    assert_eq!(second.isbn13.as_deref(), Some("9780132911221"));
    assert!(second.candidate.covers.is_empty());
    assert_eq!(
        second.link.as_deref(),
        Some("https://openlibrary.org/books/OL35572351M")
    );
    // 3 件目: 作品の著者名のつづりは `Robert Martin`、発行日は年だけ
    let third = &results.hits[2];
    assert_eq!(third.candidate.authors[0].name, "Robert Martin");
    assert_eq!(third.candidate.published_on.as_deref(), Some("2025"));
    assert_eq!(third.isbn13.as_deref(), Some("9780135398579"));
}

#[tokio::test(start_paused = true)]
async fn open_library_kokoro_has_books_without_isbn_and_prefers_the_edition_cover() {
    let results = ol_search_with(fixture("openlibrary_searchkw_kokoro.json"))
        .await
        .unwrap();
    assert_eq!(results.total, Some(14), "返った 10 件より多い");
    assert_eq!(results.hits.len(), 10);
    let first = &results.hits[0];
    assert_eq!(first.isbn13, None);
    assert_eq!(first.candidate.isbn13, "");
    assert_eq!(first.candidate.authors[0].name, "夏目漱石");
    assert_eq!(first.candidate.publisher.as_deref(), Some("Iwanami Shoten"));
    assert_eq!(first.candidate.published_on.as_deref(), Some("1914"));
    assert_eq!(first.candidate.language.as_deref(), Some("ja"));
    assert_eq!(
        first.candidate.covers[0].url,
        "https://covers.openlibrary.org/b/id/9440872-L.jpg?default=false",
        "版の cover_i（作品の 6910184 より優先）"
    );
    // 作品も版も著者がない本
    assert!(results.hits[1].candidate.authors.is_empty());
    // 13 桁が先に並ぶ版は 13 桁、ISBN のある本
    assert_eq!(results.hits[2].isbn13.as_deref(), Some("9784022510945"));
    // 副書名は、版の記録から（作品にはない）
    assert_eq!(
        results.hits[3].candidate.subtitle.as_deref(),
        Some("Michikusa")
    );
    // 日本語の ISBN-13 は 978-4 で始まる
    assert!(
        results
            .hits
            .iter()
            .filter_map(|h| h.isbn13.as_deref())
            .all(|i| i.starts_with("9784"))
    );
}

#[tokio::test(start_paused = true)]
async fn open_library_slowloop_edition_without_most_fields() {
    let results = ol_search_with(fixture("openlibrary_searchkw_slowloop.json"))
        .await
        .unwrap();
    assert_eq!(results.hits.len(), 1);
    let hit = &results.hits[0];
    assert_eq!(hit.candidate.title.as_deref(), Some("スローループ"));
    assert_eq!(hit.isbn13.as_deref(), Some("9784832270763"));
    assert!(hit.candidate.authors.is_empty());
    assert_eq!(hit.candidate.publisher, None);
    assert_eq!(hit.candidate.published_on, None);
    assert_eq!(
        hit.link.as_deref(),
        Some("https://openlibrary.org/books/OL38220971M")
    );
}

#[tokio::test(start_paused = true)]
async fn open_library_shapes_missing_from_the_real_responses() {
    let body = serde_json::json!({
        "numFound": 5, "start": 0,
        "docs": [
            // ISBN-10 だけ → 13 桁に直す。13 桁があれば、10 桁より先に使う
            {"key": "/works/OL1W", "title": "A", "editions": {"docs": [
                {"key": "/books/OL1M", "title": "A", "isbn": ["4003101111"]}]}},
            {"key": "/works/OL2W", "title": "B", "editions": {"docs": [
                {"key": "/books/OL2M", "title": "B", "isbn": ["4780802040", "9784003101117"]}]}},
            // 版に ISBN がなく、作品全体の `isbn` だけにある → 使わない
            {"key": "/works/OL3W", "title": "C", "isbn": ["9784003101117"], "editions": {"docs": [
                {"key": "/books/OL3M", "title": "C", "isbn": ["9784003101118", "4003101112"]}]}},
            // 版がない → 作品の項目だけ。リンクは作品の key
            {"key": "/works/OL4W", "title": "D", "subtitle": "d", "author_name": ["Author D"], "cover_i": 77},
            // key が想定の形でない版・作品 → リンクなし。版の key だけ悪ければ、作品の key
            {"key": "/works/OL5W/../x", "title": "E", "editions": {"docs": [
                {"key": "/books/evil", "title": "E"}]}},
            {"key": "/works/OL6W", "title": "F", "editions": {"docs": [
                {"key": "https://evil.example/books/OL6M", "title": "F"}]}},
        ]
    });
    let results = ol_search_with(body.to_string()).await.unwrap();
    assert_eq!(results.hits.len(), 6);
    assert_eq!(results.total, Some(5));
    let isbns: Vec<Option<&str>> = results.hits.iter().map(|h| h.isbn13.as_deref()).collect();
    assert_eq!(
        isbns,
        [
            Some("9784003101117"),
            Some("9784003101117"),
            None,
            None,
            None,
            None
        ]
    );
    let links: Vec<Option<&str>> = results.hits.iter().map(|h| h.link.as_deref()).collect();
    assert_eq!(
        links,
        [
            Some("https://openlibrary.org/books/OL1M"),
            Some("https://openlibrary.org/books/OL2M"),
            Some("https://openlibrary.org/books/OL3M"),
            Some("https://openlibrary.org/works/OL4W"),
            None,
            Some("https://openlibrary.org/works/OL6W"),
        ]
    );
    let d = &results.hits[3].candidate;
    assert_eq!(d.title.as_deref(), Some("D"));
    assert_eq!(d.subtitle.as_deref(), Some("d"));
    assert_eq!(d.authors[0].name, "Author D");
    assert_eq!(
        d.covers[0].url,
        "https://covers.openlibrary.org/b/id/77-L.jpg?default=false"
    );
}

#[tokio::test(start_paused = true)]
async fn open_library_links_stay_on_the_real_site_when_the_base_url_is_replaced() {
    let body = fixture("openlibrary_searchkw_clean_code_martin.json");
    let fake = Fake::always(move || json(body.clone()));
    let results = OpenLibrary::with_base_url("http://127.0.0.1:9")
        .search(&transport(&fake), &query(Some("x"), None))
        .await
        .unwrap();
    assert!(fake.urls()[0].starts_with("http://127.0.0.1:9/search.json?"));
    // 書影は接続先に合わせる（書誌と同じ）が、リンクは利用者のブラウザで開く本物のサイト
    assert!(
        results.hits[0].candidate.covers[0]
            .url
            .starts_with("http://127.0.0.1:9/b/id/")
    );
    assert!(
        results.hits[0]
            .link
            .as_deref()
            .unwrap()
            .starts_with("https://openlibrary.org/")
    );
}

#[tokio::test(start_paused = true)]
async fn open_library_empty_and_malformed_responses() {
    let r = ol_search_with(r#"{"numFound":0,"start":0,"docs":[]}"#.into())
        .await
        .unwrap();
    assert_eq!(
        r,
        SearchResults {
            total: Some(0),
            hits: Vec::new()
        }
    );
    // `docs` がなくても、`numFound` が 0 なら該当なし
    let r = ol_search_with(r#"{"numFound":0}"#.into()).await.unwrap();
    assert!(r.hits.is_empty());
    for body in ["not json", r#"{"numFound":3}"#, "[]"] {
        assert!(
            matches!(
                ol_search_with(body.into()).await,
                Err(SearchError::Fetch(FetchError::Parse(_)))
            ),
            "{body:?}"
        );
    }
    // HTTP エラー
    let fake = Fake::always(|| reply(500, "text/plain", "oops"));
    let err = OpenLibrary::new()
        .search(&transport(&fake), &query(Some("x"), None))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            SearchError::Fetch(FetchError::Http { status: 500, .. })
        ),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------
// MetadataService::search と、取得元の対応状況
// ---------------------------------------------------------------------------

fn service(fake: &Arc<Fake>) -> MetadataService {
    MetadataService::new(
        default_providers(Some("KEY".into())),
        Arc::new(transport(fake)),
    )
}

fn routed_fake() -> Arc<Fake> {
    Fake::new(|url| {
        if url.contains("ndlsearch.ndl.go.jp") {
            xml(fixture("ndl_search_slowloop.xml"))
        } else if url.contains("googleapis.com") {
            json(fixture("google_synthetic_search_clean_code.json"))
        } else if url.contains("openlibrary.org") {
            json(fixture("openlibrary_searchkw_clean_code_martin.json"))
        } else {
            reply(404, "text/plain", "not found")
        }
    })
}

#[test]
fn only_ndl_google_and_open_library_support_search() {
    let infos: Vec<(ProviderId, bool)> = default_providers(None)
        .iter()
        .map(|p| (p.id(), p.info().supports_search))
        .collect();
    assert_eq!(
        infos,
        [
            (ProviderId::Openbd, false),
            (ProviderId::Ndl, true),
            (ProviderId::Google, true),
            (ProviderId::OpenLibrary, true),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn service_routes_a_search_to_the_given_provider() {
    let fake = routed_fake();
    let svc = service(&fake);
    let q = query(Some("clean code"), Some("martin"));
    let ndl = svc.search(ProviderId::Ndl, &q).await.unwrap();
    assert_eq!(ndl.hits.len(), 11);
    let google = svc.search(ProviderId::Google, &q).await.unwrap();
    assert_eq!(google.total, Some(1234));
    let ol = svc.search(ProviderId::OpenLibrary, &q).await.unwrap();
    assert_eq!(ol.total, Some(3));
    let urls = fake.urls();
    assert_eq!(urls.len(), 3);
    assert!(urls[0].starts_with("https://ndlsearch.ndl.go.jp/api/sru?"));
    assert!(urls[1].starts_with("https://www.googleapis.com/books/v1/volumes?q="));
    assert!(urls[1].ends_with("&key=KEY"));
    assert!(urls[2].starts_with("https://openlibrary.org/search.json?"));
}

#[tokio::test(start_paused = true)]
async fn service_rejects_unsupported_providers_and_invalid_queries_without_a_request() {
    let fake = routed_fake();
    let svc = service(&fake);
    let q = query(Some("x"), None);
    // openBD は検索に対応していない
    assert_eq!(
        svc.search(ProviderId::Openbd, &q).await.unwrap_err(),
        SearchError::Unsupported
    );
    // 登録されていない取得元
    let only_ndl = MetadataService::new(
        vec![Arc::new(Ndl::new()) as Arc<dyn Provider>],
        Arc::new(transport(&fake)),
    );
    assert_eq!(
        only_ndl.search(ProviderId::Google, &q).await.unwrap_err(),
        SearchError::Unsupported
    );
    // 条件が正しくない
    for provider in ProviderId::ALL {
        let err = svc
            .search(provider, &query(None, Some("  ")))
            .await
            .unwrap_err();
        assert!(
            matches!(err, SearchError::InvalidQuery(_)),
            "{provider:?}: {err:?}"
        );
    }
    assert!(fake.urls().is_empty());
}

#[tokio::test(start_paused = true)]
async fn openbd_search_stays_unsupported_by_default() {
    let fake = routed_fake();
    let err = OpenBd::new()
        .search(&transport(&fake), &query(Some("x"), None))
        .await
        .unwrap_err();
    assert_eq!(err, SearchError::Unsupported);
    assert!(fake.urls().is_empty());
}

#[tokio::test(start_paused = true)]
async fn searches_share_the_rate_limit_group_with_isbn_fetches() {
    // 検索と ISBN の取得は、同じ取得元なら同じレート制限（1 秒に 1 件）に入る
    let fake = routed_fake();
    let t = transport(&fake);
    let q = query(Some("x"), None);

    let ndl = Ndl::new();
    ndl.search(&t, &q).await.unwrap();
    ndl.fetch_raw(&t, "9784003101117").await.unwrap();
    ndl.search(&t, &q).await.unwrap();

    let google = GoogleBooks::new(Some("KEY".into()));
    google.search(&t, &q).await.unwrap();
    google.fetch_raw(&t, "9780132350884").await.ok();

    let ol = OpenLibrary::new();
    ol.search(&t, &q).await.unwrap();
    ol.fetch_raw(&t, "9780132350884").await.ok();

    let times = fake.times();
    let urls = fake.urls();
    for group in ["ndlsearch.ndl.go.jp", "googleapis.com", "openlibrary.org"] {
        let at: Vec<Instant> = urls
            .iter()
            .zip(&times)
            .filter(|(u, _)| u.contains(group))
            .map(|(_, t)| *t)
            .collect();
        assert!(at.len() >= 2, "{group}: {at:?}");
        for w in at.windows(2) {
            assert!(w[1] - w[0] >= Duration::from_secs(1), "{group}");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn retry_policy_applies_to_searches_too() {
    let fake = Fake::always(|| reply(503, "text/plain", "busy"));
    let t = Transport::new(fake.clone()).with_retry(RetryPolicy {
        max_retries: 1,
        base_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(1),
    });
    let err = Ndl::new()
        .search(&t, &query(Some("x"), None))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        SearchError::Fetch(FetchError::Http { status: 503, .. })
    ));
    assert_eq!(fake.urls().len(), 2);
}

// ---------------------------------------------------------------------------
// ISBN の取得は変わらない（検索の追加で、取得の写像を共有した）
// ---------------------------------------------------------------------------

#[test]
fn isbn_fetch_of_the_synthetic_search_response_still_picks_the_matching_item() {
    use bibcontainer_metadata::provider::RawResponse;
    let p = GoogleBooks::new(None);
    let raw = |isbn: &str| RawResponse {
        provider: ProviderId::Google,
        isbn13: isbn.to_string(),
        body: fixture("google_synthetic_search_clean_code.json"),
    };
    // ISBN が合う項目を選ぶ（取得の規則）。ISBN が合わず、識別子のない項目（4 件目）があれば、それを使う規則も変わらない
    let c = p.parse(&raw("9784048676885")).unwrap().unwrap();
    assert_eq!(c.title.as_deref(), Some("クリーンコード"));
    assert_eq!(c.isbn13, "9784048676885");
    let c = p.parse(&raw("9780000000002")).unwrap().unwrap();
    assert_eq!(c.title.as_deref(), Some("Clean Code Collection"));
}
