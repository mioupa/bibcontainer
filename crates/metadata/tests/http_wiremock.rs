//! 実際の HTTP 層（reqwest）のテスト。ローカルのモックサーバ（wiremock）に対して通信する。
//! User-Agent、タイムアウト、リトライ、応答の大きさの上限、API キーを漏らさないこと、各取得元の URL の形を確かめる。

mod common;

use std::sync::Arc;
use std::time::Duration;

use bibcontainer_metadata::cover::{CoverDownload, download_cover, import_cover_url};
use bibcontainer_metadata::error::{CoverError, FetchError};
use bibcontainer_metadata::http::{
    DEFAULT_TIMEOUT, HttpClient, HttpRequest, TransportError, user_agent,
};
use bibcontainer_metadata::provider::Provider;
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd, OpenLibrary};
use bibcontainer_metadata::service::{FetchRequest, MetadataService, ProviderOutcome};
use bibcontainer_metadata::transport::ProviderRateLimit;
use bibcontainer_metadata::types::*;
use bibcontainer_metadata::{ReqwestClient, RetryPolicy, Transport};
use common::{fixture, fixture_bytes};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const UA: &str = "bibcontainer/0.1.0 (+https://github.com/mioupa/bibcontainer)";
const ONIGIRI: &str = "9784780802047";

fn fast_retry(max_retries: u32) -> RetryPolicy {
    RetryPolicy {
        max_retries,
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(50),
    }
}

fn transport(retries: u32) -> Transport {
    Transport::new(Arc::new(ReqwestClient::new(UA).unwrap())).with_retry(fast_retry(retries))
}

fn limit() -> ProviderRateLimit {
    ProviderRateLimit::new("test", 0, 4)
}

fn json_body(file: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(fixture(file), "application/json; charset=UTF-8")
}

#[test]
fn defaults_match_the_requirements() {
    assert_eq!(
        DEFAULT_TIMEOUT,
        Duration::from_secs(10),
        "FR-META-11: タイムアウト 10 秒"
    );
    let r = RetryPolicy::default();
    assert_eq!(r.max_retries, 3, "FR-META-11: 最大 3 回");
    assert_eq!(r.base_delay, Duration::from_secs(1));
    assert_eq!(
        user_agent("0.1.0", "https://github.com/mioupa/bibcontainer"),
        UA
    );
}

#[tokio::test]
async fn sends_the_configured_user_agent_and_no_referer() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let t = transport(0);
    let resp = t
        .get_ok(&limit(), HttpRequest::get(format!("{}/x", server.uri())))
        .await
        .unwrap();
    assert_eq!(resp.text(), "ok");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].headers.get("user-agent").unwrap(), UA);
    assert!(
        requests[0].headers.get("referer").is_none(),
        "Referer は付けない"
    );
}

#[tokio::test]
async fn times_out_and_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(600)))
        .mount(&server)
        .await;
    let client = ReqwestClient::with_timeout(UA, Duration::from_millis(150)).unwrap();
    let t = Transport::new(Arc::new(client)).with_retry(fast_retry(2));
    let err = t
        .get_ok(&limit(), HttpRequest::get(format!("{}/slow", server.uri())))
        .await
        .unwrap_err();
    match err {
        FetchError::Network(m) => assert!(m.contains("タイムアウト"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        3,
        "最初の 1 回 + リトライ 2 回"
    );
}

#[tokio::test]
async fn retries_503_then_succeeds_and_does_not_retry_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(200).set_body_string("recovered"))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/gone"))
        .respond_with(ResponseTemplate::new(404).set_body_string("nope"))
        .mount(&server)
        .await;
    let t = transport(3);
    let r = t
        .get_ok(
            &limit(),
            HttpRequest::get(format!("{}/flaky", server.uri())),
        )
        .await
        .unwrap();
    assert_eq!(r.text(), "recovered");
    let err = t
        .get_ok(&limit(), HttpRequest::get(format!("{}/gone", server.uri())))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        FetchError::Http {
            status: 404,
            message: "nope".into()
        }
    );
    let gone = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/gone")
        .count();
    assert_eq!(gone, 1);
}

#[tokio::test]
async fn rejects_responses_over_the_size_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/big"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 100_000]))
        .mount(&server)
        .await;
    let t = transport(3);
    let err = t
        .get(
            &limit(),
            HttpRequest::get(format!("{}/big", server.uri())).with_max_bytes(10_000),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, FetchError::Network(m) if m.contains("大きすぎます")),
        "{err:?}"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "大きすぎるのはリトライしない"
    );
}

/// 本物の接続エラー（閉じているポートへの接続拒否）は、`Connect` になり、リトライせずにすぐ返る。
/// 既定のリトライ（1 + 2 + 4 秒）が効いていれば 7 秒かかるので、1 秒未満で終わることで確かめる。
#[tokio::test]
async fn refused_connections_fail_at_once_without_retries() {
    // 空いているポートを借りてすぐ閉じる（そのポートへの接続は拒否される）
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let url = format!("http://127.0.0.1:{port}/x");

    let client = ReqwestClient::new(UA).unwrap();
    let err = client.get(HttpRequest::get(url.clone())).await.unwrap_err();
    assert!(matches!(err, TransportError::Connect(_)), "{err:?}");
    assert!(!err.is_retryable());

    let t = Transport::new(Arc::new(client)); // 既定のリトライ（最大 3 回、1 秒から）
    let start = std::time::Instant::now();
    let err = t.get(&limit(), HttpRequest::get(url)).await.unwrap_err();
    assert!(matches!(err, FetchError::Network(_)), "{err:?}");
    // Windows の TCP は、拒否されても数回やり直してから諦めるので、1 回の接続に約 2 秒かかる。
    // リトライが効いていれば（接続 4 回 + 待ち 7 秒で）10 秒を超えるので、5 秒未満なら見分けられる。
    let limit = if cfg!(windows) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(1)
    };
    assert!(
        start.elapsed() < limit,
        "リトライを待たずに返る: {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn connection_errors_do_not_leak_the_api_key() {
    // 閉じているポート。reqwest のエラー文には URL が入るので、キーが漏れないことを確かめる
    let g = GoogleBooks::with_base_url("http://127.0.0.1:1", Some("TOP-SECRET-KEY".into()));
    let t = transport(0);
    let err = g.fetch_raw(&t, ONIGIRI).await.unwrap_err();
    let text = format!("{err} {err:?}");
    assert!(matches!(err, FetchError::Network(_)), "{text}");
    assert!(!text.contains("TOP-SECRET-KEY"), "{text}");
}

// ---------------------------------------------------------------------------
// 取得元ごとの URL の形（モックサーバにフィクスチャを返させ、本物の HTTP 層で AT-10 を通す）
// ---------------------------------------------------------------------------

async fn mock_all(server: &MockServer, google_key: Option<&str>) {
    Mock::given(method("GET"))
        .and(path("/v1/get"))
        .and(query_param("isbn", ONIGIRI))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!("[{}]", fixture("openbd_9784780802047.json")),
            "application/json; charset=UTF-8",
        ))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/sru"))
        .and(query_param("operation", "searchRetrieve"))
        .and(query_param("recordSchema", "dcndl"))
        .and(query_param("recordPacking", "xml"))
        .and(query_param("onlyBib", "true"))
        .and(query_param("maximumRecords", "10"))
        .and(query_param("query", format!("isbn={ONIGIRI}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture("ndl_9784780802047.xml"), "text/xml;charset=UTF-8"),
        )
        .mount(server)
        .await;
    let google = Mock::given(method("GET"))
        .and(path("/books/v1/volumes"))
        .and(query_param("q", format!("isbn:{ONIGIRI}")));
    match google_key {
        Some(key) => google.and(query_param("key", key)),
        None => google.and(query_param_is_missing("key")),
    }
    .respond_with(json_body("google_synthetic_9784780802047.json"))
    .mount(server)
    .await;
    // Open Library: 日本の本は検索だけで「該当なし」
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .and(query_param("isbn", ONIGIRI))
        .respond_with(json_body("openlibrary_search_9784003101117.json"))
        .mount(server)
        .await;
}

fn providers_for(server: &MockServer, google_key: Option<&str>) -> Vec<Arc<dyn Provider>> {
    vec![
        Arc::new(OpenBd::with_base_url(&format!("{}/v1", server.uri()))),
        Arc::new(Ndl::with_base_url(&server.uri())),
        Arc::new(GoogleBooks::with_base_url(
            &server.uri(),
            google_key.map(str::to_string),
        )),
        Arc::new(OpenLibrary::with_base_url(&server.uri())),
    ]
}

#[tokio::test]
async fn at10_over_a_real_http_stack() {
    let server = MockServer::start().await;
    mock_all(&server, None).await;
    let service = MetadataService::new(providers_for(&server, None), Arc::new(transport(0)));
    let result = service.fetch(&FetchRequest::new(ONIGIRI)).await;
    for p in ProviderId::ALL {
        let report = result.reports.iter().find(|r| r.provider == p).unwrap();
        // 日本の本は Open Library にない
        let want = if p == ProviderId::OpenLibrary {
            ProviderOutcome::NotFound
        } else {
            ProviderOutcome::Found
        };
        assert_eq!(report.outcome, want, "{p:?}: {:?}", result.reports);
    }
    assert_eq!(
        result.merged.text(MetaField::Title),
        Some("おにぎりレシピ101")
    );
    assert_eq!(result.merged.text(MetaField::Publisher), Some("ポット出版"));
    assert_eq!(result.merged.text(MetaField::CCode), Some("0077"));
    assert_eq!(
        result.merged.text(MetaField::TitleKana),
        Some("オニギリ レシピ ヒャクイチ")
    );
    // どの通信にも、設定した User-Agent が付く
    for r in server.received_requests().await.unwrap() {
        assert_eq!(r.headers.get("user-agent").unwrap(), UA);
    }
}

#[tokio::test]
async fn google_key_goes_in_the_query_when_given() {
    let server = MockServer::start().await;
    mock_all(&server, Some("MY-KEY")).await;
    let service = MetadataService::new(
        providers_for(&server, Some("MY-KEY")),
        Arc::new(transport(0)),
    );
    let result = service
        .fetch(&FetchRequest::new(ONIGIRI).with_enabled(vec![ProviderId::Google]))
        .await;
    assert_eq!(
        result.reports[0].outcome,
        ProviderOutcome::Found,
        "{:?}",
        result.reports
    );
}

#[tokio::test]
async fn openbd_batch_over_http_splits_by_100() {
    let server = MockServer::start().await;
    // 100 件ずつに分けて問い合わせる（250 件 → 3 回）。応答はすべて「該当なし」
    Mock::given(method("GET"))
        .and(path("/v1/get"))
        .respond_with(|req: &wiremock::Request| {
            let isbns = req
                .url
                .query_pairs()
                .find(|(k, _)| k == "isbn")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            let n = isbns.split(',').count();
            let body = format!("[{}]", vec!["null"; n].join(","));
            ResponseTemplate::new(200).set_body_raw(body, "application/json")
        })
        .mount(&server)
        .await;
    let openbd = OpenBd::with_base_url(&format!("{}/v1", server.uri()));
    let isbns: Vec<String> = (0..250).map(|i| format!("97840000{i:05}")).collect();
    let t = transport(0);
    let results = openbd.fetch_raw_batch(&t, &isbns).await;
    assert_eq!(results.len(), 250);
    assert!(
        results
            .iter()
            .all(|(_, r)| r.as_ref().unwrap().body == "null")
    );
    assert_eq!(results[0].0, isbns[0]);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn openbd_wrong_shapes_are_parse_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/get"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"error":"x"}"#, "application/json"),
        )
        .mount(&server)
        .await;
    let openbd = OpenBd::with_base_url(&format!("{}/v1", server.uri()));
    let err = openbd.fetch_raw(&transport(0), ONIGIRI).await.unwrap_err();
    assert!(matches!(err, FetchError::Parse(_)), "{err:?}");
}

// ---------------------------------------------------------------------------
// 書影
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cover_403_is_an_http_error_and_no_referer_is_sent() {
    // アクセスを拒否された（403）ときは HTTP エラー。アプリは Referer を付けない・偽装しない（docs/decisions/0007）。
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/cover/{ONIGIRI}.jpg")))
        .respond_with(
            ResponseTemplate::new(403).set_body_raw("<h1>Request blocked</h1>", "text/html"),
        )
        .mount(&server)
        .await;
    let cover = CandidateCover {
        provider: ProviderId::Openbd,
        url: format!("{}/cover/{ONIGIRI}.jpg", server.uri()),
    };
    let err = download_cover(&transport(0), &cover).await.unwrap_err();
    assert!(
        matches!(&err, FetchError::Http { status: 403, message } if message.contains("Request blocked")),
        "{err:?}"
    );
    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("referer").is_none());
}

#[tokio::test]
async fn cover_response_that_is_xml_is_not_an_image() {
    // 画像がないときに XML のエラーを 200 で返す取得元がある。Content-Type で見分ける。
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/cover/{ONIGIRI}.jpg")))
        .respond_with(ResponseTemplate::new(200).set_body_raw("<error/>", "text/xml;charset=UTF-8"))
        .mount(&server)
        .await;
    let cover = CandidateCover {
        provider: ProviderId::Openbd,
        url: format!("{}/cover/{ONIGIRI}.jpg", server.uri()),
    };
    let r = download_cover(&transport(0), &cover).await.unwrap();
    assert_eq!(
        r,
        CoverDownload::NotAnImage {
            content_type: "text/xml".into()
        }
    );
}

#[tokio::test]
async fn google_placeholder_is_detected_over_http() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/books/content"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture_bytes("google_noimage_575x750.png"), "image/png"),
        )
        .mount(&server)
        .await;
    let cover = CandidateCover {
        provider: ProviderId::Google,
        url: format!("{}/books/content?id=X&zoom=0", server.uri()),
    };
    assert_eq!(
        download_cover(&transport(0), &cover).await.unwrap(),
        CoverDownload::Placeholder
    );
}

#[tokio::test]
async fn import_from_a_user_supplied_url_over_http() {
    let server = MockServer::start().await;
    let img = image::RgbImage::from_fn(900, 1200, |x, y| {
        image::Rgb([(x / 4) as u8, (y / 5) as u8, 90])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/c.png"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.into_inner(), "image/png"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("<html>login</html>", "text/html"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let t = transport(0);
    let stored = import_cover_url(&t, dir.path(), &format!("{}/c.png", server.uri()))
        .await
        .unwrap();
    assert_eq!((stored.width, stored.height), (600, 800));
    assert!(stored.path.exists());
    let err = import_cover_url(&t, dir.path(), &format!("{}/page", server.uri()))
        .await
        .unwrap_err();
    assert!(matches!(err, CoverError::Decode(_)), "{err:?}");
}
