//! 取得元ごとの解析のテスト。ネットワークは使わず、`tests/fixtures/` の実際の応答だけで動く。

mod common;

use bibcontainer_metadata::provider::{Provider, RawResponse};
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd, OpenLibrary};
use bibcontainer_metadata::types::{Candidate, CandidateAuthor, ProviderId};
use common::fixture;

fn raw(provider: ProviderId, isbn: &str, body: String) -> RawResponse {
    RawResponse {
        provider,
        isbn13: isbn.to_string(),
        body,
    }
}

fn openbd(isbn: &str) -> Candidate {
    let r = raw(
        ProviderId::Openbd,
        isbn,
        fixture(&format!("openbd_{isbn}.json")),
    );
    OpenBd::new()
        .parse(&r)
        .unwrap()
        .expect("openBD に収録がある")
}

fn ndl(isbn: &str) -> Option<Candidate> {
    let r = raw(ProviderId::Ndl, isbn, fixture(&format!("ndl_{isbn}.xml")));
    Ndl::new().parse(&r).unwrap()
}

fn author(name: &str, kana: Option<&str>, role: Option<&str>) -> CandidateAuthor {
    CandidateAuthor {
        name: name.into(),
        kana: kana.map(Into::into),
        role: role.map(Into::into),
    }
}

// ---------------------------------------------------------------------------
// openBD
// ---------------------------------------------------------------------------

#[test]
fn openbd_publisher_data_at10() {
    // AT-10: 書名「おにぎりレシピ101」、出版社「ポット出版」、Cコード 0077、書影あり
    let c = openbd("9784780802047");
    assert_eq!(c.title.as_deref(), Some("おにぎりレシピ101"));
    assert_eq!(
        c.subtitle.as_deref(),
        Some("EVERYDAY ONIGIRI 101 Healthy, Easy Japanese Riceball Recipes")
    );
    assert_eq!(c.publisher.as_deref(), Some("ポット出版"));
    assert_eq!(c.c_code.as_deref(), Some("0077"));
    assert_eq!(c.list_price, Some(1300));
    assert_eq!(c.ref_price_incl, None);
    assert_eq!(c.published_on.as_deref(), Some("2014-04-08"));
    assert_eq!(c.pages, Some(126));
    assert_eq!(c.language.as_deref(), Some("ja"));
    assert_eq!(c.covers.len(), 1);
    assert_eq!(c.covers[0].url, "https://cover.openbd.jp/9784780802047.jpg");
    assert_eq!(c.covers[0].provider, ProviderId::Openbd);
    // 「山田玲子／著 水野菜生／英訳」は役割を分け、ヨミは ONIX の Contributor から付く
    assert_eq!(
        c.authors,
        vec![
            author("山田玲子", Some("ヤマダ レイコ"), Some("著")),
            author("水野菜生", Some("ミズノ ナオ"), Some("英訳"))
        ]
    );
    // 内容紹介は TextType 03（短い説明の 02 ではない）
    let d = c.description.unwrap();
    assert!(
        d.starts_with("101人いれば、101通りの好みのおにぎりがあります。"),
        "{d}"
    );
    assert!(d.contains("Onigiri"));
}

#[test]
fn openbd_ndl_derived_record_has_gaps_but_parses() {
    // 版元データでなく NDL 由来の簡易変換。Cコード・書影・ページ数はなく、価格は「(税込)」付き
    let c = openbd("9784003101117");
    assert_eq!(c.title.as_deref(), Some("こころ"));
    assert_eq!(c.publisher.as_deref(), Some("岩波書店"));
    assert_eq!(c.series.as_deref(), Some("岩波文庫"));
    assert_eq!(c.published_on.as_deref(), Some("1989-05"));
    assert_eq!(c.c_code, None);
    assert!(c.covers.is_empty(), "cover が空の文字列なら書影なし");
    assert_eq!(c.list_price, None, "(税込) の価格は本体価格にしない");
    assert_eq!(c.ref_price_incl, Some(310));
    // 「夏目,漱石,1867-1916」→ 夏目漱石（生没年は捨てる）
    assert_eq!(
        c.authors,
        vec![author("夏目漱石", Some("ナツメ ソウセキ"), None)]
    );

    let c = openbd("9784101001012");
    assert_eq!(c.list_price, Some(362));
    assert_eq!(c.published_on.as_deref(), Some("2006-05"));
}

#[test]
fn openbd_multiple_authors_without_roles() {
    let c = openbd("9784003261354");
    assert_eq!(c.title.as_deref(), Some("罪と罰 上"));
    assert_eq!(
        c.authors,
        vec![
            author("Dostoyevsky, Fyodor", None, None),
            author("江川卓", Some("エガワ タク"), None)
        ]
    );
    assert_eq!(c.list_price, Some(760));

    // 全角英数・「米川／正夫」のゆれ
    let c = openbd("9784003261460");
    assert_eq!(
        c.authors[1],
        author("米川正夫", Some("ヨネカワ マサオ"), None)
    );
    assert_eq!(c.authors[0].name, "Dostoevskii, FyodorMikhailovich");
    assert_eq!(c.published_on.as_deref(), Some("1976-01"), "「1976-01」");
}

#[test]
fn openbd_missing_is_not_found_and_bad_json_is_error() {
    let p = OpenBd::new();
    assert_eq!(
        p.parse(&raw(
            ProviderId::Openbd,
            "9784123456784",
            fixture("openbd_null.json")
        ))
        .unwrap(),
        None
    );
    assert!(
        p.parse(&raw(ProviderId::Openbd, "9784123456784", "<html>".into()))
            .is_err()
    );
    assert!(
        p.parse(&raw(ProviderId::Openbd, "9784123456784", "[1,2]".into()))
            .is_err()
    );
}

#[test]
fn openbd_tolerates_shape_variations() {
    // ONIX の配列と単独のオブジェクトが入れ替わっても、数値の価格でも読める
    let body = r#"{
      "summary": {"isbn": "9784000000000", "title": "", "series": "", "volume": "", "publisher": "", "pubdate": "", "cover": "", "author": ""},
      "onix": {
        "DescriptiveDetail": {
          "TitleDetail": [{"TitleType": "01", "TitleElement": [{"TitleElementLevel": "01", "PartNumber": "3", "TitleText": {"content": "テスト書名"}, "Subtitle": {"content": "副題"}}]}],
          "Contributor": {"PersonName": {"content": "山田 太郎", "collationkey": "ヤマダ タロウ"}, "ContributorRole": ["B06"]},
          "Subject": {"SubjectSchemeIdentifier": "78", "SubjectCode": "0195"},
          "Extent": {"ExtentType": "11", "ExtentUnit": "03", "ExtentValue": "320"},
          "Language": {"LanguageRole": "01", "LanguageCode": "eng"}
        },
        "PublishingDetail": {"Publisher": {"PublisherName": "テスト社"}, "PublishingDate": {"PublishingDateRole": "01", "Date": "20200229"}},
        "ProductSupply": {"SupplyDetail": {"Price": {"PriceType": "01", "PriceAmount": 1200, "CurrencyCode": "JPY"}}},
        "CollateralDetail": {"SupportingResource": {"ResourceContentType": "01", "ResourceVersion": {"ResourceLink": "http://cover.example/x.jpg"}}}
      }
    }"#;
    let c = OpenBd::new()
        .parse(&raw(ProviderId::Openbd, "9784000000000", body.into()))
        .unwrap()
        .unwrap();
    assert_eq!(c.title.as_deref(), Some("テスト書名"));
    assert_eq!(c.subtitle.as_deref(), Some("副題"));
    assert_eq!(c.volume.as_deref(), Some("3"));
    // 著者の文字列が空のときは Contributor から（ONIX の B06 = 訳）
    assert_eq!(
        c.authors,
        vec![author("山田 太郎", Some("ヤマダ タロウ"), Some("訳"))]
    );
    assert_eq!(c.c_code.as_deref(), Some("0195"));
    assert_eq!(c.pages, Some(320));
    assert_eq!(c.language.as_deref(), Some("en"));
    assert_eq!(c.publisher.as_deref(), Some("テスト社"));
    assert_eq!(c.published_on.as_deref(), Some("2020-02-29"));
    assert_eq!(c.list_price, Some(1200));
    assert_eq!(
        c.covers[0].url, "https://cover.example/x.jpg",
        "http は https にする"
    );
}

#[test]
fn openbd_splits_isbd_subtitle_when_no_explicit_subtitle() {
    let body = r#"{"summary":{"isbn":"9784000000000","title":"人はなんで生きるか : 他四篇","author":""},"onix":{"DescriptiveDetail":{"TitleDetail":{"TitleType":"01","TitleElement":{"TitleElementLevel":"01","TitleText":{"content":"人はなんで生きるか : 他四篇"}}}}}}"#;
    let c = OpenBd::new()
        .parse(&raw(ProviderId::Openbd, "9784000000000", body.into()))
        .unwrap()
        .unwrap();
    assert_eq!(c.title.as_deref(), Some("人はなんで生きるか"));
    assert_eq!(c.subtitle.as_deref(), Some("他四篇"));
}

// ---------------------------------------------------------------------------
// NDL
// ---------------------------------------------------------------------------

#[test]
fn ndl_at10_book() {
    let c = ndl("9784780802047").expect("NDL に収録がある");
    // `おにぎりレシピ101 : EVERYDAY ONIGIRI` を書名と副書名に分ける
    assert_eq!(c.title.as_deref(), Some("おにぎりレシピ101"));
    assert_eq!(c.subtitle.as_deref(), Some("EVERYDAY ONIGIRI"));
    assert_eq!(c.title_kana.as_deref(), Some("オニギリ レシピ ヒャクイチ"));
    assert_eq!(c.publisher.as_deref(), Some("ポット出版"));
    assert_eq!(c.published_on.as_deref(), Some("2014-04"));
    assert_eq!(c.list_price, Some(1300));
    // `123p ; 21cm` はページ数と大きさに分ける
    assert_eq!(c.pages, Some(123));
    assert_eq!(c.size_text.as_deref(), Some("21cm"));
    assert_eq!(c.ndc.as_deref(), Some("596.3"));
    assert_eq!(c.language.as_deref(), Some("ja"), "先頭の言語（jpn）");
    assert_eq!(
        c.authors,
        vec![author("山田玲子", Some("ヤマダ レイコ"), Some("著"))]
    );
    assert_eq!(c.c_code, None, "NDL に Cコードはない");
    assert!(c.covers.is_empty());
}

#[test]
fn ndl_prefers_the_ndl_own_record_among_several_for_the_same_isbn() {
    // 雪国: 1 件目は CiNii（NII）のレコードで価格・ページ数がない。NDL 自身のレコードを選ぶ。
    let c = ndl("9784101001012").unwrap();
    assert_eq!(
        c.published_on.as_deref(),
        Some("2006-05"),
        "NII の 1987 ではなく NDL の 2006.5"
    );
    assert_eq!(c.list_price, Some(362));
    assert_eq!(c.pages, Some(208));
    assert_eq!(c.size_text.as_deref(), Some("16cm"));
    assert_eq!(c.series.as_deref(), Some("新潮文庫"));
    assert_eq!(c.title_kana.as_deref(), Some("ユキグニ"));
    assert_eq!(c.ndc.as_deref(), Some("913.6"));
    assert_eq!(
        c.authors,
        vec![author("川端康成", Some("カワバタ ヤスナリ"), Some("著"))]
    );
}

#[test]
fn ndl_tax_included_price_and_old_ndc_notation() {
    let c = ndl("9784003101117").unwrap();
    assert_eq!(c.list_price, None);
    assert_eq!(c.ref_price_incl, Some(310), "`310円 (税込)`");
    assert_eq!(c.ndc.as_deref(), Some("913.6"), "NDC8 の `dc:subject`");
    assert_eq!(
        c.authors,
        vec![author("夏目漱石", Some("ナツメ ソウセキ"), Some("作"))]
    );
    assert_eq!(c.pages, Some(300));
    assert_eq!(c.size_text.as_deref(), Some("15cm"));
}

#[test]
fn ndl_skips_series_wide_aggregate_record_and_reads_volume() {
    // ONE PIECE 1 巻: 4 件のレコードのうち、シリーズ全体を 1 件にまとめたもの（ISBN が数十個）は使わない。
    let c = ndl("9784088725093").unwrap();
    assert_eq!(c.title.as_deref(), Some("One piece"));
    assert_eq!(c.volume.as_deref(), Some("巻1"));
    assert_eq!(c.series.as_deref(), Some("ジャンプ・コミックス"));
    assert_eq!(c.list_price, Some(390));
    assert_eq!(c.pages, Some(207));
    assert_eq!(c.ndc.as_deref(), Some("726.1"));
    assert_eq!(c.published_on.as_deref(), Some("1997-12"));
    assert_eq!(
        c.title_kana, None,
        "ヨミが原文の綴り（One piece）だけなら入れない"
    );
    assert_eq!(
        c.authors,
        vec![author("尾田栄一郎", Some("オダ エイイチロウ"), Some("著"))]
    );
}

#[test]
fn ndl_ignores_records_matched_only_by_an_error_isbn() {
    // 4-000000-00-0 は別の本（4-938853-04-3）に誤って付けられた ISBN。SRU はそれでも当ててくるので、型が ErrorISBN の識別子では採用しない。
    assert!(ndl("9784000000000").is_none());
}

#[test]
fn ndl_record_does_not_exist_is_not_found() {
    let p = Ndl::new();
    let r = raw(
        ProviderId::Ndl,
        "9784123456784",
        fixture("ndl_notfound.xml"),
    );
    assert_eq!(p.parse(&r).unwrap(), None);
}

#[test]
fn ndl_errors() {
    let p = Ndl::new();
    // XML でない
    assert!(
        p.parse(&raw(ProviderId::Ndl, "9784003101117", "<html>".into()))
            .is_err()
    );
    // SRU の形でない
    assert!(
        p.parse(&raw(ProviderId::Ndl, "9784003101117", "<a/>".into()))
            .is_err()
    );
    // 「該当なし」以外のエラー
    let xml = r#"<searchRetrieveResponse xmlns="http://www.loc.gov/zing/srw/"><diagnostics><diagnostic xmlns="http://www.loc.gov/zing/srw/diagnostic/"><uri>info:srw/diagnostic/1/1</uri><message>illegal query syntax</message></diagnostic></diagnostics></searchRetrieveResponse>"#;
    let err = p
        .parse(&raw(ProviderId::Ndl, "9784003101117", xml.into()))
        .unwrap_err();
    assert!(err.to_string().contains("illegal query syntax"), "{err}");
    // レコードも診断もない
    let xml = r#"<searchRetrieveResponse xmlns="http://www.loc.gov/zing/srw/"><numberOfRecords>0</numberOfRecords></searchRetrieveResponse>"#;
    assert_eq!(
        p.parse(&raw(ProviderId::Ndl, "9784003101117", xml.into()))
            .unwrap(),
        None
    );
}

#[test]
fn ndl_matches_isbn10_and_hyphenated_identifiers() {
    // フィクスチャの ISBN 識別子は `4-00-310111-1`（ISBN-10 のハイフン付き）。ISBN-13 を探しても当たる。
    assert!(ndl("9784003101117").is_some());
    // 別の ISBN を探したら、同じ応答でも該当なし
    let r = raw(
        ProviderId::Ndl,
        "9784780802047",
        fixture("ndl_9784003101117.xml"),
    );
    assert_eq!(Ndl::new().parse(&r).unwrap(), None);
}

// ---------------------------------------------------------------------------
// Google Books
// ---------------------------------------------------------------------------

fn google(isbn: &str, file: &str) -> Option<Candidate> {
    GoogleBooks::new(None)
        .parse(&raw(ProviderId::Google, isbn, fixture(file)))
        .unwrap()
}

#[test]
fn google_picks_the_item_whose_identifier_matches() {
    // 合成の応答（実際の応答は取れなかった。docs/decisions/0007 参照）。2 件目は別の ISBN の本。
    let c = google("9784003101117", "google_synthetic_9784003101117.json").unwrap();
    assert_eq!(c.title.as_deref(), Some("こころ"));
    assert_eq!(c.publisher.as_deref(), Some("岩波書店"));
    assert_eq!(c.published_on.as_deref(), Some("1989-05-16"));
    assert_eq!(c.pages, Some(300));
    assert_eq!(c.language.as_deref(), Some("ja"));
    assert_eq!(c.authors, vec![author("夏目 漱石", None, None)]);
    // HTML タグと実体参照を除く
    assert_eq!(
        c.description.as_deref(),
        Some("先生と私の物語。\n\n改版。\"K\"の死と&遺書。")
    );
    // 書影: http → https、edge=curl を外し、大きい方（zoom=0）を先にする
    let urls: Vec<&str> = c.covers.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://books.google.com/books/content?id=SYNTHETIC0001&printsec=frontcover&img=1&zoom=0&source=gbs_api",
            "https://books.google.com/books/content?id=SYNTHETIC0001&printsec=frontcover&img=1&zoom=1&source=gbs_api",
        ]
    );
}

#[test]
fn google_without_publisher_images_or_pages() {
    let c = google("9784780802047", "google_synthetic_9784780802047.json").unwrap();
    assert_eq!(c.title.as_deref(), Some("おにぎりレシピ101"));
    assert_eq!(c.subtitle.as_deref(), Some("Everyday Onigiri"));
    assert_eq!(c.publisher, None);
    assert_eq!(c.pages, None, "pageCount が 0 なら不明");
    assert_eq!(c.published_on.as_deref(), Some("2014"));
    assert!(c.covers.is_empty());
    assert_eq!(c.authors.len(), 2);
}

#[test]
fn google_not_found_and_errors() {
    assert_eq!(
        google("9784000000001", "google_synthetic_notfound.json"),
        None
    );
    // 識別子のある別の本しか返らなければ該当なし
    let r = raw(
        ProviderId::Google,
        "9784780802047",
        fixture("google_synthetic_9784003101117.json"),
    );
    assert_eq!(GoogleBooks::new(None).parse(&r).unwrap(), None);
    // 割り当て切れの応答（実際の応答）は解析エラー（通常は HTTP エラーとして届く）
    let r = raw(
        ProviderId::Google,
        "9784780802047",
        fixture("google_quota_exceeded.json"),
    );
    assert!(GoogleBooks::new(None).parse(&r).is_err());
    assert!(
        GoogleBooks::new(None)
            .parse(&raw(ProviderId::Google, "9784780802047", "x".into()))
            .is_err()
    );
}

#[test]
fn google_item_without_identifiers_is_used_as_is() {
    let body = r#"{"items":[{"volumeInfo":{"title":"識別子なし","authors":["甲"]}}]}"#;
    let c = GoogleBooks::new(None)
        .parse(&raw(ProviderId::Google, "9784003101117", body.into()))
        .unwrap()
        .unwrap();
    assert_eq!(c.title.as_deref(), Some("識別子なし"));
}

/// volume 1 件の応答（`GET /books/v1/volumes/<ID>`）。検索が使えないあいだの回避策（Dynamic Links で ID を得て取る）で届く。
/// フィクスチャは合成（`google_synthetic_volume_*.json`。キーを使えない環境で作った）。実際の応答の形に合わせてある。
fn google_volume(isbn: &str, body: &str) -> Option<Candidate> {
    GoogleBooks::new(None)
        .parse(&raw(ProviderId::Google, isbn, body.to_string()))
        .unwrap()
}

#[test]
fn google_volume_by_id_is_read_like_a_search_item() {
    let c = google_volume(
        "9780132350884",
        &fixture("google_synthetic_volume_hjEFCAAAQBAJ.json"),
    )
    .unwrap();
    assert_eq!(c.title.as_deref(), Some("Clean Code"));
    assert_eq!(
        c.subtitle.as_deref(),
        Some("A Handbook of Agile Software Craftsmanship")
    );
    assert_eq!(c.authors, vec![author("Robert C. Martin", None, None)]);
    assert_eq!(c.publisher.as_deref(), Some("Pearson Education"));
    assert_eq!(c.published_on.as_deref(), Some("2008-08-01"));
    assert_eq!(c.pages, Some(464));
    assert_eq!(c.language.as_deref(), Some("en"));
    // description の HTML: 段落・`<br>` は改行、リストは 1 項目 1 行、インラインのタグは除く、実体参照・数値参照は戻す
    assert_eq!(
        c.description.as_deref(),
        Some(
            "Even bad code can function. But if code isn't clean, it can bring a development organization to its knees.\n\n\
             Noted software expert Robert C. Martin presents a revolutionary paradigm with Clean Code: A Handbook of Agile Software Craftsmanship.\n\n\
             What you will learn:\n\n\
             How to tell the difference between good and bad code\n\
             How to write good code & how to transform bad code into good code\n\
             How to create good names, good functions, good objects, and good classes\n\n\
             Readers will come away from this book understanding \u{2013} \u{201C}how to code well\u{201D}."
        )
    );
    // 書影: 最上位の id から、大きい方（zoom=0）→ zoom=1。`imageLinks` の `publisher/content…imgtk` の URL は使わない
    let urls: Vec<&str> = c.covers.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://books.google.com/books/content?id=hjEFCAAAQBAJ&printsec=frontcover&img=1&zoom=0",
            "https://books.google.com/books/content?id=hjEFCAAAQBAJ&printsec=frontcover&img=1&zoom=1",
        ]
    );
    assert!(c.covers.iter().all(|c| c.provider == ProviderId::Google));

    let c = google_volume(
        "9784003101117",
        &fixture("google_synthetic_volume_fz5TPAAACAAJ.json"),
    )
    .unwrap();
    assert_eq!(c.title.as_deref(), Some("こころ"));
    assert_eq!(c.authors, vec![author("夏目漱石", None, None)]);
    assert_eq!(c.published_on.as_deref(), Some("1989"));
    assert_eq!(
        c.description.as_deref(),
        Some("先生と私の物語。\n\n改版。\"K\"の死と&遺書。")
    );
    let urls: Vec<&str> = c.covers.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://books.google.com/books/content?id=fz5TPAAACAAJ&printsec=frontcover&img=1&zoom=0",
            "https://books.google.com/books/content?id=fz5TPAAACAAJ&printsec=frontcover&img=1&zoom=1",
        ]
    );
}

#[test]
fn google_volume_with_another_isbn_is_not_found() {
    // こころの volume に、Clean Code の ISBN で問い合わせた（Dynamic Links が別の本の ID を返した）
    assert_eq!(
        google_volume(
            "9780132350884",
            &fixture("google_synthetic_volume_fz5TPAAACAAJ.json")
        ),
        None
    );
    // ISBN-10 だけの識別子も、13 桁に直して照合する
    let body = r#"{"kind":"books#volume","id":"abcd1234","volumeInfo":{"title":"T","industryIdentifiers":[{"type":"ISBN_10","identifier":"4003101111"}]}}"#;
    assert!(google_volume("9784003101117", body).is_some());
    assert_eq!(google_volume("9784780802047", body), None);
}

#[test]
fn google_volume_without_identifiers_is_used_as_is_and_without_image_links_has_no_cover() {
    let body = r#"{"kind":"books#volume","id":"abcd1234","volumeInfo":{"title":"識別子なし","authors":["甲"]}}"#;
    let c = google_volume("9784003101117", body).unwrap();
    assert_eq!(c.title.as_deref(), Some("識別子なし"));
    assert!(c.covers.is_empty(), "imageLinks がなければ書影なし");

    // `kind` がなくても、最上位に volumeInfo があれば volume 1 件として読む
    let body = r#"{"id":"abcd1234","volumeInfo":{"title":"kind なし","imageLinks":{"thumbnail":"http://books.google.com/books/content?id=abcd1234&zoom=1"}}}"#;
    let c = google_volume("9784003101117", body).unwrap();
    assert_eq!(c.title.as_deref(), Some("kind なし"));
    assert_eq!(c.covers.len(), 2);

    // imageLinks があっても、最上位の id がない・形が違うときは、書影を作らない（URL に入れない）
    for id in [r#""id":"a/../b","#, r#""id":"x","#, ""] {
        let body = format!(
            r#"{{"kind":"books#volume",{id}"volumeInfo":{{"title":"T","imageLinks":{{"thumbnail":"http://books.google.com/books/content?id=zzzz9999&zoom=1"}}}}}}"#
        );
        let c = google_volume("9784003101117", &body).unwrap();
        assert!(c.covers.is_empty(), "{id}");
    }
    // 空の imageLinks は書影なし
    let body =
        r#"{"kind":"books#volume","id":"abcd1234","volumeInfo":{"title":"T","imageLinks":{}}}"#;
    assert!(
        google_volume("9784003101117", body)
            .unwrap()
            .covers
            .is_empty()
    );
}

#[test]
fn google_search_items_keep_their_own_cover_handling() {
    // volume の JSON を検索の応答の `items` に入れた形は、検索の規則のまま
    // （`imageLinks` の URL から、`edge=curl` を外し、zoom=0 を先にする）。単独の volume の規則（id から組み立てる）にはならない
    let volume: serde_json::Value =
        serde_json::from_str(&fixture("google_synthetic_volume_hjEFCAAAQBAJ.json")).unwrap();
    let search = serde_json::json!({"kind": "books#volumes", "totalItems": 1, "items": [volume]});
    let c = google_volume("9780132350884", &search.to_string()).unwrap();
    let urls: Vec<&str> = c.covers.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://books.google.com/books/publisher/content?id=hjEFCAAAQBAJ&printsec=frontcover&img=1&zoom=0&imgtk=SYNTHETIC&source=gbs_api",
            "https://books.google.com/books/publisher/content?id=hjEFCAAAQBAJ&printsec=frontcover&img=1&zoom=1&imgtk=SYNTHETIC&source=gbs_api",
        ]
    );
    // 検索が 0 件の応答（Google の障害のあいだの応答。`totalItems: 0`）は、これまでどおり該当なし
    assert_eq!(
        google_volume("9780132350884", &fixture("google_synthetic_notfound.json")),
        None
    );
}

// ---------------------------------------------------------------------------
// 能力情報
// ---------------------------------------------------------------------------

#[test]
fn provider_infos_describe_auth_rate_limit_and_credits() {
    use bibcontainer_metadata::provider::ProviderAuth;
    use bibcontainer_metadata::types::MetaField;

    let openbd = OpenBd::new().info();
    assert_eq!(openbd.id, ProviderId::Openbd);
    assert_eq!(openbd.auth, ProviderAuth::None);
    assert!(openbd.supports_batch);
    assert!(openbd.fields.contains(&MetaField::CCode));
    assert_eq!(openbd.rate_limit.min_interval_ms, 1000);

    let ndl = Ndl::new().info();
    assert_eq!(ndl.rate_limit.max_concurrent, 1, "NDL は並列にしない");
    assert_eq!(ndl.rate_limit.min_interval_ms, 1000);
    assert!(ndl.fields.contains(&MetaField::Ndc));
    assert!(!ndl.fields.contains(&MetaField::CCode));

    let google = GoogleBooks::new(None).info();
    assert!(
        matches!(google.auth, ProviderAuth::Required { .. }),
        "キーなしは 429 で必ず失敗するので、キーは必須"
    );
    assert_eq!(google.rate_limit.min_interval_ms, 1000);

    let open_library = OpenLibrary::new().info();
    assert_eq!(open_library.id, ProviderId::OpenLibrary);
    assert_eq!(open_library.display_name, "Open Library");
    assert_eq!(open_library.auth, ProviderAuth::None, "API キーは要らない");
    assert!(open_library.needs_network);
    assert!(!open_library.supports_batch);
    assert_eq!(open_library.rate_limit.group, "openlibrary");
    assert_eq!(open_library.rate_limit.min_interval_ms, 1000);
    assert_eq!(open_library.rate_limit.max_concurrent, 1);
    assert_eq!(
        open_library.fields,
        vec![
            MetaField::Title,
            MetaField::Subtitle,
            MetaField::Authors,
            MetaField::Publisher,
            MetaField::PublishedOn,
            MetaField::Pages,
            MetaField::Language,
            MetaField::Description,
            MetaField::Cover,
        ]
    );
    // Open Library はアプリからのリンクを求めている。名前と URL をクレジットに入れる
    assert!(open_library.credit.contains("Open Library"));
    assert!(open_library.credit.contains("https://openlibrary.org/"));

    for info in [openbd, ndl, google, open_library] {
        assert!(!info.credit.is_empty());
        assert!(!info.display_name.is_empty());
        assert!(!info.supports_search, "キーワード検索はフェーズ 3");
    }
}

#[tokio::test]
async fn search_is_unsupported_by_default() {
    use bibcontainer_metadata::error::SearchError;
    use bibcontainer_metadata::provider::SearchQuery;
    use bibcontainer_metadata::{ReqwestClient, Transport};
    use std::sync::Arc;

    let t = Transport::new(Arc::new(ReqwestClient::new("bibcontainer/test").unwrap()));
    let err = OpenBd::new()
        .search(
            &t,
            &SearchQuery {
                title: Some("x".into()),
                author: None,
                limit: 5,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err, SearchError::Unsupported);
}
