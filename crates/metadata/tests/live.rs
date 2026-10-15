//! 実際のネットワークを使うテスト。通常は実行しない（`#[ignore]`）。
//!
//! 実行: `cargo test -p bibcontainer-metadata --test live -- --ignored --test-threads=1`
//!
//! 取得元の仕様が変わっていないか、ダミー画像の定数が今も正しいかを確かめるためのもの。
//! Google Books の確認には API キーが要る。環境変数 `GOOGLE_BOOKS_API_KEY` に入れておくと使う（なければその部分は飛ばす）。
//! どの取得元にも負荷をかけないよう、通信は最小限にしてある。

use std::sync::Arc;

use bibcontainer_metadata::cover::{
    CoverDownload, download_cover, is_known_placeholder, process_cover,
};
use bibcontainer_metadata::http::user_agent;
use bibcontainer_metadata::provider::Provider;
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd, OpenLibrary, default_providers};
use bibcontainer_metadata::service::{FetchRequest, MetadataService, ProviderOutcome};
use bibcontainer_metadata::types::*;
use bibcontainer_metadata::{FetchError, ReqwestClient, Transport};

const ONIGIRI: &str = "9784780802047";

fn transport() -> Transport {
    let ua = user_agent(
        env!("CARGO_PKG_VERSION"),
        "https://github.com/mioupa/bibcontainer",
    );
    Transport::new(Arc::new(ReqwestClient::new(&ua).unwrap()))
}

fn google_key() -> Option<String> {
    std::env::var("GOOGLE_BOOKS_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_openbd_at10() {
    let t = transport();
    let p = OpenBd::new();
    let raw = p.fetch_raw(&t, ONIGIRI).await.unwrap();
    let c = p.parse(&raw).unwrap().expect("openBD に収録がある");
    assert_eq!(c.title.as_deref(), Some("おにぎりレシピ101"));
    assert_eq!(c.publisher.as_deref(), Some("ポット出版"));
    assert_eq!(c.c_code.as_deref(), Some("0077"));
    assert!(!c.covers.is_empty());
    // 未収録
    let raw = p.fetch_raw(&t, "9784123456784").await.unwrap();
    assert_eq!(p.parse(&raw).unwrap(), None);
    // まとめて
    let batch = p
        .fetch_raw_batch(&t, &[ONIGIRI.to_string(), "9784003101117".to_string()])
        .await;
    assert_eq!(batch.len(), 2);
    assert!(batch.iter().all(|(_, r)| r.is_ok()));
}

#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_ndl_sru() {
    let t = transport();
    let p = Ndl::new();
    let raw = p.fetch_raw(&t, ONIGIRI).await.unwrap();
    let c = p.parse(&raw).unwrap().expect("NDL に収録がある");
    assert_eq!(c.title.as_deref(), Some("おにぎりレシピ101"));
    assert_eq!(c.publisher.as_deref(), Some("ポット出版"));
    assert_eq!(c.ndc.as_deref(), Some("596.3"));
    assert!(c.pages.is_some());
    // 誤って登録された ISBN（ErrorISBN）では採用しない
    let raw = p.fetch_raw(&t, "9784000000000").await.unwrap();
    assert_eq!(p.parse(&raw).unwrap(), None);
    let raw = p.fetch_raw(&t, "9784123456784").await.unwrap();
    assert_eq!(p.parse(&raw).unwrap(), None);
}

/// Open Library（API キーなし）。書誌は 2 回の通信（検索、版の JSON）、書影は 1 回。
/// Open Library は大量のアクセスを禁じているので、通信は最小限にしてある（取得元のレート制限は 1 秒に 1 件）。
#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_open_library_clean_code_and_cover() {
    let t = transport();
    let p = OpenLibrary::new();
    let raw = p.fetch_raw(&t, "9780132350884").await.unwrap();
    let c = p.parse(&raw).unwrap().expect("Open Library に収録がある");
    println!("{c:#?}");
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    assert_eq!(c.authors[0].name, "Robert C. Martin");
    assert_eq!(c.language.as_deref(), Some("en"));
    assert!(c.pages.is_some_and(|n| n > 0));
    assert!(c.published_on.is_some());
    assert!(c.description.is_some());
    // 書影の ID を指定した URL（ISBN を指定した URL にあるレート制限がない）から画像が取れる
    let CoverDownload::Image(bytes) = download_cover(&t, &c.covers[0]).await.unwrap() else {
        panic!("Open Library の書影が取れない");
    };
    assert!(process_cover(&bytes).is_ok());
}

#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_open_library_not_found_is_a_single_request() {
    let t = transport();
    let p = OpenLibrary::new();
    // 岩波文庫『こころ』は Open Library にない（2026-10-15）。見つかるようになったら、この期待を直す
    let raw = p.fetch_raw(&t, "9784003101117").await.unwrap();
    assert_eq!(p.parse(&raw).unwrap(), None);
    // 存在しない書影の ID は、`default=false` で 404（空白の画像ではない）
    let cover = CandidateCover {
        provider: ProviderId::OpenLibrary,
        url: "https://covers.openlibrary.org/b/id/999999999-L.jpg?default=false".into(),
    };
    let err = download_cover(&t, &cover).await.unwrap_err();
    assert!(
        matches!(err, FetchError::Http { status: 404, .. }),
        "{err:?}"
    );
}

#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_openbd_cover_download_and_google_placeholders() {
    let t = transport();
    let cover = CandidateCover {
        provider: ProviderId::Openbd,
        url: format!("https://cover.openbd.jp/{ONIGIRI}.jpg"),
    };
    let CoverDownload::Image(bytes) = download_cover(&t, &cover).await.unwrap() else {
        panic!("openBD の書影が取れない");
    };
    let processed = process_cover(&bytes).unwrap();
    assert!(processed.width.max(processed.height) <= 800);

    // 存在しない ID の書影が、既知のダミー画像と一致する（zoom の違いで 3 種類）
    for zoom in [0, 1, 5] {
        let url = format!(
            "https://books.google.com/books/content?id=AAAAAAAAAAAA&printsec=frontcover&img=1&zoom={zoom}&source=gbs_api"
        );
        let cover = CandidateCover {
            provider: ProviderId::Google,
            url,
        };
        let r = download_cover(&t, &cover).await.unwrap();
        assert_eq!(r, CoverDownload::Placeholder, "zoom={zoom}");
    }
    let _ = is_known_placeholder(&[]);
}

#[tokio::test]
#[ignore = "実際のネットワークと API キーを使う"]
async fn live_google_books_with_key() {
    let Some(key) = google_key() else {
        println!("GOOGLE_BOOKS_API_KEY がないので飛ばす");
        return;
    };
    let t = transport();
    let p = GoogleBooks::new(Some(key));
    let raw = p.fetch_raw(&t, "9784003101117").await.unwrap();
    let c = p.parse(&raw).unwrap();
    println!("{c:#?}");
    assert!(c.is_some());
}

/// Google の ISBN 検索が 0 件を返す障害（2026-10-01 ごろから）への回避策（Dynamic Links → volume）。
/// 1 冊で 3 回の通信（検索・Dynamic Links・volume。1 秒ずつあける）。Google が直って、最初の検索で見つかるようになっても、
/// この期待（書名・書影・内容紹介の HTML が除かれていること）は変わらない。
#[tokio::test]
#[ignore = "実際のネットワークと API キーを使う"]
async fn live_google_books_finds_a_book_even_while_isbn_search_is_broken() {
    let Some(key) = google_key() else {
        println!("GOOGLE_BOOKS_API_KEY がないので飛ばす");
        return;
    };
    let t = transport();
    let p = GoogleBooks::new(Some(key));
    let raw = p.fetch_raw(&t, "9780132350884").await.unwrap();
    let c = p.parse(&raw).unwrap().expect("Google Books に収録がある");
    println!("{c:#?}");
    // 検索が直っていれば検索の応答、障害のあいだは volume 1 件の応答
    println!(
        "volume 1 件の応答（回避策）か: {}",
        raw.body.contains("\"books#volume\"")
    );
    assert!(
        c.title.as_deref().is_some_and(|t| t.contains("Clean Code")),
        "{:?}",
        c.title
    );
    assert!(c.authors.iter().any(|a| a.name.contains("Martin")));
    assert_eq!(c.language.as_deref(), Some("en"));
    assert!(!c.covers.is_empty());
    if let Some(d) = &c.description {
        assert!(
            !d.contains("<br") && !d.contains("<p>") && !d.contains("<i>"),
            "{d}"
        );
    }
}

#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_google_books_without_key_is_rejected() {
    if google_key().is_some() {
        return;
    }
    // 2026-10-14 の時点では、キーなしは割り当て 0 で 429 になる。解消されたら、この期待を直す。
    let t = transport();
    let p = GoogleBooks::new(None);
    match p.fetch_raw(&t, ONIGIRI).await {
        Err(FetchError::Http {
            status: 429,
            message,
        }) => println!("429: {message}"),
        Ok(raw) => println!("キーなしで取れるようになった: {} バイト", raw.body.len()),
        Err(e) => panic!("{e}"),
    }
}

#[tokio::test]
#[ignore = "実際のネットワークを使う"]
async fn live_full_flow_at10() {
    let service = MetadataService::new(default_providers(google_key()), Arc::new(transport()));
    let result = service.fetch(&FetchRequest::new(ONIGIRI)).await;
    println!("{:#?}", result.reports);
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
    assert!(!result.merged.covers_in_order().is_empty());
    assert!(!result.offline);
    for p in [ProviderId::Openbd, ProviderId::Ndl] {
        let r = result.reports.iter().find(|r| r.provider == p).unwrap();
        assert_eq!(r.outcome, ProviderOutcome::Found, "{p:?}");
    }
}
