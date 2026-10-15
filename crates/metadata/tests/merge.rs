//! 統合（FR-META-04／05）のテスト。取得元の候補は `tests/fixtures/` の実際の応答から作る。

mod common;

use PriorityProfile::{Foreign, Japanese};
use bibcontainer_metadata::MetaMerged;
use bibcontainer_metadata::merge::{
    MetaPick, MetaPriorities, MetaPrioritySets, PriorityProfile, merge,
};
use bibcontainer_metadata::provider::{Provider, RawResponse};
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd, OpenLibrary};
use bibcontainer_metadata::types::*;
use common::fixture;

const ISBN_ONIGIRI: &str = "9784780802047";

fn openbd(isbn: &str) -> Candidate {
    let raw = RawResponse {
        provider: ProviderId::Openbd,
        isbn13: isbn.into(),
        body: fixture(&format!("openbd_{isbn}.json")),
    };
    OpenBd::new().parse(&raw).unwrap().unwrap()
}

fn ndl(isbn: &str) -> Candidate {
    let raw = RawResponse {
        provider: ProviderId::Ndl,
        isbn13: isbn.into(),
        body: fixture(&format!("ndl_{isbn}.xml")),
    };
    Ndl::new().parse(&raw).unwrap().unwrap()
}

fn google_onigiri() -> Candidate {
    let raw = RawResponse {
        provider: ProviderId::Google,
        isbn13: ISBN_ONIGIRI.into(),
        body: fixture("google_synthetic_9784780802047.json"),
    };
    GoogleBooks::new(None).parse(&raw).unwrap().unwrap()
}

/// Open Library（実際の応答のフィクスチャ。検索と版の JSON を、取得元の包みにして解析する）。
fn open_library(isbn: &str, search: &str, edition: Option<&str>) -> Candidate {
    let search: serde_json::Value = serde_json::from_str(&fixture(search)).unwrap();
    let edition: Option<serde_json::Value> =
        edition.map(|f| serde_json::from_str(&fixture(f)).unwrap());
    let raw = RawResponse {
        provider: ProviderId::OpenLibrary,
        isbn13: isbn.into(),
        body: serde_json::json!({ "search": search, "edition": edition }).to_string(),
    };
    OpenLibrary::new().parse(&raw).unwrap().unwrap()
}

fn clean_code_open_library() -> Candidate {
    open_library(
        "9780132350884",
        "openlibrary_search_9780132350884.json",
        Some("openlibrary_edition_OL26222911M.json"),
    )
}

/// 書影の URL だけを持つ候補（書誌の値はない）。
fn cover_only(provider: ProviderId, isbn: &str, url: &str) -> Candidate {
    let mut c = Candidate::empty(provider, isbn);
    c.covers = vec![CandidateCover {
        provider,
        url: url.into(),
    }];
    c
}

fn onigiri_all() -> Vec<Candidate> {
    vec![openbd(ISBN_ONIGIRI), ndl(ISBN_ONIGIRI), google_onigiri()]
}

#[test]
fn at10_title_publisher_ccode_and_cover_come_from_openbd() {
    let m = merge(
        ISBN_ONIGIRI,
        &onigiri_all(),
        None,
        &MetaPriorities::default(),
    );
    assert_eq!(m.text(MetaField::Title), Some("おにぎりレシピ101"));
    assert_eq!(m.text(MetaField::Publisher), Some("ポット出版"));
    assert_eq!(m.text(MetaField::CCode), Some("0077"));
    assert_eq!(m.source(MetaField::CCode), Some(MetaSource::Openbd));
    let covers = m.covers_in_order();
    assert_eq!(
        covers[0].url, "https://cover.openbd.jp/9784780802047.jpg",
        "書影は openBD が先"
    );
    assert_eq!(covers.len(), 1, "NDL にも Google（合成）にも書影はない");
    assert!(!m.is_empty());
}

#[test]
fn default_priorities_follow_the_requirement_table() {
    let m = merge(
        ISBN_ONIGIRI,
        &onigiri_all(),
        None,
        &MetaPriorities::default(),
    );
    // 書名・副書名・著者・出版社・発行日: openBD
    assert_eq!(m.source(MetaField::Title), Some(MetaSource::Openbd));
    assert_eq!(m.source(MetaField::Subtitle), Some(MetaSource::Openbd));
    assert_eq!(m.source(MetaField::Authors), Some(MetaSource::Openbd));
    assert_eq!(m.source(MetaField::Publisher), Some(MetaSource::Openbd));
    assert_eq!(m.text(MetaField::PublishedOn), Some("2014-04-08"));
    assert_eq!(m.source(MetaField::PublishedOn), Some(MetaSource::Openbd));
    // 書名のヨミ・NDC: NDL（openBD にない）
    assert_eq!(
        m.text(MetaField::TitleKana),
        Some("オニギリ レシピ ヒャクイチ")
    );
    assert_eq!(m.source(MetaField::TitleKana), Some(MetaSource::Ndl));
    assert_eq!(m.text(MetaField::Ndc), Some("596.3"));
    // ページ数・大きさ: NDL が openBD（126 ページ）に優先する
    assert_eq!(m.number(MetaField::Pages), Some(123));
    assert_eq!(m.source(MetaField::Pages), Some(MetaSource::Ndl));
    assert_eq!(m.text(MetaField::SizeText), Some("21cm"));
    // 本体価格: 2段目がなければ openBD
    assert_eq!(m.number(MetaField::ListPrice), Some(1300));
    assert_eq!(m.source(MetaField::ListPrice), Some(MetaSource::Openbd));
    // 内容紹介: openBD（Google にはない）
    assert_eq!(m.source(MetaField::Description), Some(MetaSource::Openbd));
}

#[test]
fn others_are_kept_as_alternatives_without_duplicates() {
    let m = merge(
        ISBN_ONIGIRI,
        &onigiri_all(),
        None,
        &MetaPriorities::default(),
    );
    // 書名: openBD が採用。NDL・Google も同じ「おにぎりレシピ101」なので、候補には残らない
    let title = m.choice(MetaField::Title).unwrap();
    assert!(title.others.is_empty(), "{:?}", title.others);
    // 発行日: openBD の 2014-04-08 に対し、NDL の 2014-04 と Google の 2014 が候補になる
    let date = m.choice(MetaField::PublishedOn).unwrap();
    let others: Vec<_> = date
        .others
        .iter()
        .map(|p| (p.source, p.value.clone()))
        .collect();
    assert_eq!(
        others,
        vec![
            (MetaSource::Ndl, MetaValue::Text("2014-04".into())),
            (MetaSource::Google, MetaValue::Text("2014".into())),
        ]
    );
    // ページ数: NDL の 123 が採用、openBD の 126 が候補
    let pages = m.choice(MetaField::Pages).unwrap();
    assert_eq!(
        pages.others,
        vec![MetaPick {
            value: MetaValue::Number(126),
            source: MetaSource::Openbd
        }]
    );
    // 値のない項目も、結果には必ず入る
    assert_eq!(m.fields.len(), MetaField::ALL.len());
    assert!(m.choice(MetaField::Volume).unwrap().chosen.is_none());
}

#[test]
fn second_tier_wins_for_c_code_and_list_price_and_openbd_values_become_alternatives() {
    // 2段目（バーコード）は、本体価格と Cコードで最優先。値が違えば openBD の値は候補に回る。
    let tier = MetaSecondTier {
        c_code: Some("0193".into()),
        list_price: Some(700),
    };
    let m = merge(
        ISBN_ONIGIRI,
        &onigiri_all(),
        Some(&tier),
        &MetaPriorities::default(),
    );
    assert_eq!(m.text(MetaField::CCode), Some("0193"));
    assert_eq!(m.source(MetaField::CCode), Some(MetaSource::Barcode));
    assert_eq!(m.number(MetaField::ListPrice), Some(700));
    assert_eq!(m.source(MetaField::ListPrice), Some(MetaSource::Barcode));
    let c = m.choice(MetaField::CCode).unwrap();
    assert_eq!(c.others.len(), 1);
    assert_eq!(c.others[0].source, MetaSource::Openbd);
    assert_eq!(c.others[0].value, MetaValue::Text("0077".into()));
    let p = m.choice(MetaField::ListPrice).unwrap();
    // openBD の 1300 と NDL の 1300 は同じ値なので、候補は 1 つ
    assert_eq!(p.others.len(), 1);
    assert_eq!(p.others[0].value, MetaValue::Number(1300));

    // 2段目の価格が「不明」（None）なら、openBD の本体価格を使う
    let tier = MetaSecondTier {
        c_code: Some("0077".into()),
        list_price: None,
    };
    let m = merge(
        ISBN_ONIGIRI,
        &onigiri_all(),
        Some(&tier),
        &MetaPriorities::default(),
    );
    assert_eq!(m.number(MetaField::ListPrice), Some(1300));
    assert_eq!(m.source(MetaField::ListPrice), Some(MetaSource::Openbd));
    assert_eq!(m.source(MetaField::CCode), Some(MetaSource::Barcode));
    assert!(
        m.choice(MetaField::CCode).unwrap().others.is_empty(),
        "同じ 0077 は候補にならない"
    );
}

#[test]
fn second_tier_alone_still_gives_c_code_and_price_when_all_providers_fail() {
    let tier = MetaSecondTier {
        c_code: Some("0193".into()),
        list_price: Some(700),
    };
    let m = merge(ISBN_ONIGIRI, &[], Some(&tier), &MetaPriorities::default());
    assert_eq!(m.text(MetaField::CCode), Some("0193"));
    assert_eq!(m.number(MetaField::ListPrice), Some(700));
    assert!(m.chosen(MetaField::Title).is_none());
    assert!(!m.is_empty());
}

#[test]
fn priorities_can_be_reordered_per_group() {
    let mut p = MetaPriorities::default();
    p.set(
        MetaGroup::Title,
        vec![MetaSource::Ndl, MetaSource::Openbd, MetaSource::Google],
    );
    p.set(MetaGroup::Cover, vec![MetaSource::Google]);
    let m = merge(ISBN_ONIGIRI, &onigiri_all(), None, &p);
    assert_eq!(m.source(MetaField::Title), Some(MetaSource::Ndl));
    // 副書名は書名と同じ取得元（NDL の「EVERYDAY ONIGIRI」）。openBD の長い副書名は候補に回る
    assert_eq!(m.text(MetaField::Subtitle), Some("EVERYDAY ONIGIRI"));
    assert_eq!(m.source(MetaField::Subtitle), Some(MetaSource::Ndl));
    let sub = m.choice(MetaField::Subtitle).unwrap();
    assert!(sub.others.iter().any(|o| o.source == MetaSource::Openbd
        && matches!(&o.value, MetaValue::Text(t) if t.starts_with("EVERYDAY ONIGIRI 101"))));
    // 設定にない取得元は既定の順で末尾に足される（Cover に Google だけを書くと、openBD、Open Library が続く）
    assert_eq!(
        p.order(MetaGroup::Cover),
        vec![
            MetaSource::Google,
            MetaSource::Openbd,
            MetaSource::OpenLibrary
        ]
    );
    // ほかの行は変わらない
    assert_eq!(m.source(MetaField::Publisher), Some(MetaSource::Openbd));
}

#[test]
fn title_and_subtitle_come_from_the_same_provider() {
    // openBD に副書名がなく、NDL にある場合: 書名が openBD なら、副書名は空（NDL の副書名は候補）。
    let mut a = Candidate::empty(ProviderId::Openbd, ISBN_ONIGIRI);
    a.title = Some("書名A".into());
    let mut b = Candidate::empty(ProviderId::Ndl, ISBN_ONIGIRI);
    b.title = Some("書名B".into());
    b.subtitle = Some("副題B".into());
    let m = merge(ISBN_ONIGIRI, &[a, b], None, &MetaPriorities::default());
    assert_eq!(m.text(MetaField::Title), Some("書名A"));
    assert!(m.chosen(MetaField::Subtitle).is_none());
    assert_eq!(
        m.choice(MetaField::Subtitle).unwrap().others[0].value,
        MetaValue::Text("副題B".into())
    );
}

#[test]
fn volume_is_removed_from_the_title_when_the_title_ends_with_it() {
    // ONE PIECE: openBD の書名は「One piece 巻1」（巻次は空）、NDL は書名「One piece」・巻次「巻1」
    let isbn = "9784088725093";
    let m = merge(
        isbn,
        &[openbd(isbn), ndl(isbn)],
        None,
        &MetaPriorities::default(),
    );
    assert_eq!(m.text(MetaField::Title), Some("One piece"));
    assert_eq!(m.source(MetaField::Title), Some(MetaSource::Openbd));
    assert_eq!(m.text(MetaField::Volume), Some("巻1"));
    assert_eq!(m.source(MetaField::Volume), Some(MetaSource::Ndl));
    assert_eq!(m.text(MetaField::Series), Some("ジャンプ・コミックス"));

    // 巻次が書名の一部（「巻1」を含む別の語）なら切らない
    let mut a = Candidate::empty(ProviderId::Openbd, isbn);
    a.title = Some("第1巻".into());
    a.volume = Some("巻".into());
    let m = merge(isbn, &[a], None, &MetaPriorities::default());
    assert_eq!(m.text(MetaField::Title), Some("第1巻"));
}

#[test]
fn old_ndl_derived_openbd_and_ndl_agree_and_tax_included_price_is_a_reference() {
    let isbn = "9784003101117";
    let m = merge(
        isbn,
        &[openbd(isbn), ndl(isbn)],
        None,
        &MetaPriorities::default(),
    );
    assert_eq!(m.number(MetaField::ListPrice), None);
    assert_eq!(m.number(MetaField::RefPriceIncl), Some(310));
    assert_eq!(m.text(MetaField::CCode), None, "どこにもなければ空");
    assert_eq!(m.text(MetaField::Ndc), Some("913.6"));
    // 著者: openBD は役割なし、NDL は「作」。優先順位どおり openBD を採用し、NDL を候補に残す
    assert_eq!(m.authors()[0].role, None);
    let others = &m.choice(MetaField::Authors).unwrap().others;
    assert_eq!(others.len(), 1);
    assert!(
        matches!(&others[0].value, MetaValue::Authors(a) if a[0].role.as_deref() == Some("作"))
    );
}

#[test]
fn nothing_found_is_empty_and_cover_urls_alone_do_not_count() {
    let m = merge(
        ISBN_ONIGIRI,
        &[cover_only(
            ProviderId::Openbd,
            ISBN_ONIGIRI,
            "https://cover.openbd.jp/9784780802047.jpg",
        )],
        None,
        &MetaPriorities::default(),
    );
    assert!(m.is_empty(), "書影の URL だけでは書誌があるとは言えない");
    assert_eq!(m.covers_in_order().len(), 1);
    let m = merge(ISBN_ONIGIRI, &[], None, &MetaPriorities::default());
    assert!(m.is_empty());
    assert!(m.covers_in_order().is_empty());
    assert!(m.authors().is_empty());
}

#[test]
fn duplicate_provider_candidates_use_the_first() {
    let mut a = Candidate::empty(ProviderId::Openbd, ISBN_ONIGIRI);
    a.title = Some("一つ目".into());
    let mut b = Candidate::empty(ProviderId::Openbd, ISBN_ONIGIRI);
    b.title = Some("二つ目".into());
    let m = merge(ISBN_ONIGIRI, &[a, b], None, &MetaPriorities::default());
    assert_eq!(m.text(MetaField::Title), Some("一つ目"));
}

#[test]
fn default_priority_table() {
    use MetaSource::*;
    let p = MetaPriorities::default();
    // Open Library は、取れる項目の行の最後
    assert_eq!(
        p.order(MetaGroup::Title),
        vec![Openbd, Ndl, Google, OpenLibrary]
    );
    assert_eq!(p.order(MetaGroup::TitleKana), vec![Ndl]);
    assert_eq!(
        p.order(MetaGroup::Authors),
        vec![Openbd, Ndl, Google, OpenLibrary]
    );
    assert_eq!(
        p.order(MetaGroup::Publisher),
        vec![Openbd, Ndl, Google, OpenLibrary]
    );
    assert_eq!(
        p.order(MetaGroup::PublishedOn),
        vec![Openbd, Ndl, Google, OpenLibrary]
    );
    assert_eq!(p.order(MetaGroup::ListPrice), vec![Barcode, Openbd, Ndl]);
    assert_eq!(p.order(MetaGroup::CCode), vec![Barcode, Openbd]);
    assert_eq!(p.order(MetaGroup::Ndc), vec![Ndl]);
    assert_eq!(
        p.order(MetaGroup::PagesAndSize),
        vec![Ndl, Openbd, Google, OpenLibrary]
    );
    assert_eq!(p.order(MetaGroup::Series), vec![Openbd, Ndl]);
    assert_eq!(
        p.order(MetaGroup::Description),
        vec![Openbd, Google, OpenLibrary]
    );
    assert_eq!(p.order(MetaGroup::Cover), vec![Openbd, Google, OpenLibrary]);
    assert_eq!(
        p.order(MetaGroup::Language),
        vec![Openbd, Ndl, Google, OpenLibrary]
    );
    assert_eq!(p.order(MetaGroup::RefPriceIncl), vec![Openbd, Ndl]);
    // 重複して書かれていても 1 回だけ
    let mut p = MetaPriorities::default();
    p.set(MetaGroup::Ndc, vec![Ndl, Ndl]);
    assert_eq!(p.order(MetaGroup::Ndc), vec![Ndl]);
}

#[test]
fn priorities_roundtrip_through_json() {
    let mut p = MetaPriorities::default();
    p.set(MetaGroup::Title, vec![MetaSource::Ndl, MetaSource::Openbd]);
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"title\":[\"ndl\",\"openbd\"]"), "{json}");
    assert!(json.contains("\"pages_and_size\""), "{json}");
    let back: MetaPriorities = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}

#[test]
fn ipc_json_shapes() {
    let m = merge(
        ISBN_ONIGIRI,
        &[openbd(ISBN_ONIGIRI)],
        None,
        &MetaPriorities::default(),
    );
    let v = serde_json::to_value(&m).unwrap();
    assert_eq!(v["isbn13"], ISBN_ONIGIRI);
    let title = v["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"] == "title")
        .unwrap();
    assert_eq!(title["chosen"]["source"], "openbd");
    assert_eq!(title["chosen"]["value"]["type"], "text");
    assert_eq!(title["chosen"]["value"]["value"], "おにぎりレシピ101");
    let authors = v["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"] == "authors")
        .unwrap();
    assert_eq!(authors["chosen"]["value"]["type"], "authors");
    assert_eq!(authors["chosen"]["value"]["value"][0]["name"], "山田玲子");
}

// ---------------------------------------------------------------------------
// Open Library
// ---------------------------------------------------------------------------

#[test]
fn open_library_fills_only_the_fields_the_higher_priority_sources_lack() {
    // openBD は書名だけ（ほかは欠けている）、Google は出版社と書影を持つ。Open Library は全部持つ。
    let isbn = "9780132350884";
    let mut a = Candidate::empty(ProviderId::Openbd, isbn);
    a.title = Some("Clean Code (openBD)".into());
    let mut g = Candidate::empty(ProviderId::Google, isbn);
    g.publisher = Some("Prentice Hall PTR".into());
    g.covers = vec![CandidateCover {
        provider: ProviderId::Google,
        url: "https://books.google.com/x".into(),
    }];
    let ol = clean_code_open_library();
    let m = merge(isbn, &[a, g, ol], None, &MetaPriorities::default());

    // 書名: openBD が採用。副書名は書名と同じ取得元（openBD）にないので空で、Open Library の副書名は候補に回る
    assert_eq!(m.text(MetaField::Title), Some("Clean Code (openBD)"));
    assert!(m.chosen(MetaField::Subtitle).is_none());
    let sub = m.choice(MetaField::Subtitle).unwrap();
    assert_eq!(sub.others[0].source, MetaSource::OpenLibrary);
    assert_eq!(
        sub.others[0].value,
        MetaValue::Text("A Handbook of Agile Software Craftsmanship".into())
    );
    // 出版社: Google が先。Open Library の値は候補に残る
    assert_eq!(m.text(MetaField::Publisher), Some("Prentice Hall PTR"));
    assert_eq!(m.source(MetaField::Publisher), Some(MetaSource::Google));
    assert_eq!(
        m.choice(MetaField::Publisher).unwrap().others,
        vec![MetaPick {
            value: MetaValue::Text("Prentice Hall".into()),
            source: MetaSource::OpenLibrary
        }]
    );
    // ほかの取得元にない項目は、Open Library が埋める
    assert_eq!(m.source(MetaField::Authors), Some(MetaSource::OpenLibrary));
    assert_eq!(m.authors()[0].name, "Robert C. Martin");
    assert_eq!(m.text(MetaField::PublishedOn), Some("2008-07"));
    assert_eq!(
        m.source(MetaField::PublishedOn),
        Some(MetaSource::OpenLibrary)
    );
    assert_eq!(m.number(MetaField::Pages), Some(431));
    assert_eq!(m.source(MetaField::Pages), Some(MetaSource::OpenLibrary));
    assert_eq!(m.text(MetaField::Language), Some("en"));
    assert_eq!(
        m.source(MetaField::Description),
        Some(MetaSource::OpenLibrary)
    );
    // Open Library が持たない項目（ヨミ・NDC・価格・Cコード・シリーズ）は、空のまま
    for f in [
        MetaField::TitleKana,
        MetaField::Ndc,
        MetaField::ListPrice,
        MetaField::CCode,
        MetaField::Series,
        MetaField::Volume,
        MetaField::SizeText,
    ] {
        assert!(m.chosen(f).is_none(), "{f:?}");
    }
}

#[test]
fn open_library_is_last_in_the_cover_order() {
    let isbn = "9780132350884";
    let mut b = Candidate::empty(ProviderId::Openbd, isbn);
    b.covers = vec![CandidateCover {
        provider: ProviderId::Openbd,
        url: "https://cover.openbd.jp/x.jpg".into(),
    }];
    let mut g = Candidate::empty(ProviderId::Google, isbn);
    g.covers = vec![CandidateCover {
        provider: ProviderId::Google,
        url: "https://books.google.com/x".into(),
    }];
    // 渡す順を変えても、優先順位の順（openBD > Google > Open Library）
    let m = merge(
        isbn,
        &[clean_code_open_library(), g, b],
        None,
        &MetaPriorities::default(),
    );
    let covers = m.covers_in_order();
    let providers: Vec<ProviderId> = covers.iter().map(|c| c.provider).collect();
    assert_eq!(
        providers,
        vec![
            ProviderId::Openbd,
            ProviderId::Google,
            ProviderId::OpenLibrary
        ]
    );
    assert_eq!(
        covers[2].url,
        "https://covers.openlibrary.org/b/id/15126503-L.jpg?default=false"
    );
    // Open Library だけなら、その書影を使う
    let m = merge(
        isbn,
        &[clean_code_open_library()],
        None,
        &MetaPriorities::default(),
    );
    assert_eq!(m.source(MetaField::Cover), Some(MetaSource::OpenLibrary));
    assert_eq!(m.covers_in_order().len(), 1);
}

#[test]
fn open_library_is_appended_to_priorities_that_were_saved_without_it() {
    // Open Library を知らない古い設定（3 つの取得元だけ）でも、末尾に足されて使える
    let old: MetaPriorities = serde_json::from_str(
        r#"{"groups":{"title":["ndl","openbd","google"],"cover":["google","openbd"],"pages_and_size":["openbd","ndl","google"]}}"#,
    )
    .unwrap();
    assert_eq!(
        old.order(MetaGroup::Title),
        vec![
            MetaSource::Ndl,
            MetaSource::Openbd,
            MetaSource::Google,
            MetaSource::OpenLibrary
        ]
    );
    assert_eq!(
        old.order(MetaGroup::Cover),
        vec![
            MetaSource::Google,
            MetaSource::Openbd,
            MetaSource::OpenLibrary
        ]
    );
    // 設定にない行（description）は既定のまま
    assert_eq!(
        old.order(MetaGroup::Description),
        vec![
            MetaSource::Openbd,
            MetaSource::Google,
            MetaSource::OpenLibrary
        ]
    );
    // 設定の順を保つ。Open Library を先頭にした設定も読める
    let json = r#"{"groups":{"title":["openlibrary","ndl"]}}"#;
    let p: MetaPriorities = serde_json::from_str(json).unwrap();
    assert_eq!(
        p.order(MetaGroup::Title),
        vec![
            MetaSource::OpenLibrary,
            MetaSource::Ndl,
            MetaSource::Openbd,
            MetaSource::Google
        ]
    );
    assert_eq!(MetaSource::OpenLibrary.as_str(), "openlibrary");
}

// ---------------------------------------------------------------------------
// 区分（和書・洋書など）ごとの優先順位
// ---------------------------------------------------------------------------

/// 978-4（和書）の ISBN と、978-0（洋書）の ISBN。チェックデジットは合っている。
const JP_ISBN: &str = "9784003101117";
const US_ISBN: &str = "9780132350884";

#[test]
fn the_profile_follows_the_isbn_registration_group() {
    // 和書: 978-4（ISBN-10 は 4 で始まる）。ハイフン・全角・空白があっても同じ
    for isbn in [
        "9784003101117",
        "978-4-00-310111-7",
        "４-００-３１０１１１-１",
        "4003101111",
        " 9784003101117 ",
        "９７８４００３１０１１１７",
    ] {
        assert_eq!(PriorityProfile::for_isbn(Some(isbn)), Japanese, "{isbn}");
    }
    // 洋書など: それ以外のすべての ISBN（978-0・978-1・978-3・979-…）
    for isbn in [
        "9780132350884",
        "0-13-235088-2",
        "0132350882",
        "9781493934362",
        "9783161484100",
        "9791032300060",
        "979-10-323-0006-0",
    ] {
        assert_eq!(PriorityProfile::for_isbn(Some(isbn)), Foreign, "{isbn}");
    }
    // 読めない・ない: 和書
    for isbn in [
        None,
        Some(""),
        Some("abc"),
        Some("9784003101118"), // チェックデジットの不一致
        Some("97800000000"),
        Some("9780132350885"), // 洋書の形でもチェックデジットが合わなければ読めない
    ] {
        assert_eq!(PriorityProfile::for_isbn(isbn), Japanese, "{isbn:?}");
    }
    assert_eq!(PriorityProfile::default(), Japanese);
    // IPC の名前
    assert_eq!(serde_json::to_string(&Japanese).unwrap(), "\"japanese\"");
    assert_eq!(serde_json::to_string(&Foreign).unwrap(), "\"foreign\"");
}

#[test]
fn the_japanese_defaults_are_the_orders_before_profiles_existed() {
    use MetaSource::{Barcode, Google, Ndl, OpenLibrary, Openbd};
    let j = MetaPriorities::default_for(Japanese);
    assert_eq!(j, MetaPriorities::default());
    assert_eq!(MetaPrioritySets::default().japanese, j);
    let order = |g| j.order(g);
    for g in [
        MetaGroup::Title,
        MetaGroup::Authors,
        MetaGroup::Publisher,
        MetaGroup::Language,
        MetaGroup::PublishedOn,
    ] {
        assert_eq!(order(g), vec![Openbd, Ndl, Google, OpenLibrary], "{g:?}");
    }
    assert_eq!(order(MetaGroup::TitleKana), vec![Ndl]);
    assert_eq!(order(MetaGroup::ListPrice), vec![Barcode, Openbd, Ndl]);
    assert_eq!(order(MetaGroup::RefPriceIncl), vec![Openbd, Ndl]);
    assert_eq!(order(MetaGroup::CCode), vec![Barcode, Openbd]);
    assert_eq!(order(MetaGroup::Ndc), vec![Ndl]);
    assert_eq!(
        order(MetaGroup::PagesAndSize),
        vec![Ndl, Openbd, Google, OpenLibrary]
    );
    assert_eq!(order(MetaGroup::Series), vec![Openbd, Ndl]);
    assert_eq!(
        order(MetaGroup::Description),
        vec![Openbd, Google, OpenLibrary]
    );
    assert_eq!(order(MetaGroup::Cover), vec![Openbd, Google, OpenLibrary]);
}

#[test]
fn the_foreign_defaults_put_google_and_open_library_first_in_the_rows_that_matter() {
    use MetaSource::{Google, Ndl, OpenLibrary, Openbd};
    let jp = MetaPriorities::default_for(Japanese);
    let f = MetaPriorities::default_for(Foreign);
    assert_eq!(MetaPrioritySets::default().foreign, f);
    let changed = [
        MetaGroup::Title,
        MetaGroup::Authors,
        MetaGroup::Publisher,
        MetaGroup::Language,
        MetaGroup::PublishedOn,
        MetaGroup::Description,
        MetaGroup::Cover,
        MetaGroup::Series,
    ];
    for g in MetaGroup::ALL {
        if changed.contains(&g) {
            continue;
        }
        assert_eq!(f.order(g), jp.order(g), "{g:?} は和書と同じ");
    }
    for g in [
        MetaGroup::Title,
        MetaGroup::Authors,
        MetaGroup::Publisher,
        MetaGroup::Language,
        MetaGroup::PublishedOn,
    ] {
        assert_eq!(
            f.order_in(Foreign, g),
            vec![Google, OpenLibrary, Ndl, Openbd],
            "{g:?}"
        );
    }
    for g in [MetaGroup::Description, MetaGroup::Cover] {
        assert_eq!(
            f.order_in(Foreign, g),
            vec![Google, OpenLibrary, Openbd],
            "{g:?}"
        );
    }
    // シリーズ・巻次は、洋書では NDL を先にする（Google・Open Library はシリーズを返さない）
    assert_eq!(f.order_in(Foreign, MetaGroup::Series), vec![Ndl, Openbd]);
    // どの行も、取得元の顔ぶれは和書と同じ（並びだけが違う）
    for g in MetaGroup::ALL {
        let mut a = jp.order(g);
        let mut b = f.order_in(Foreign, g);
        a.sort_by_key(|s| s.as_str());
        b.sort_by_key(|s| s.as_str());
        assert_eq!(a, b, "{g:?}");
    }
}

/// 同じ内容の候補を、取得元ごとに値がはっきり分かる形で作る。`isbn` だけが違う。
fn labelled_candidates(isbn: &str) -> Vec<Candidate> {
    fn author(name: &str) -> Vec<CandidateAuthor> {
        vec![CandidateAuthor {
            name: name.into(),
            kana: None,
            role: None,
        }]
    }
    fn cover(provider: ProviderId, url: &str) -> Vec<CandidateCover> {
        vec![CandidateCover {
            provider,
            url: url.into(),
        }]
    }
    let mut a = Candidate::empty(ProviderId::Openbd, isbn);
    a.title = Some("T-openbd".into());
    a.subtitle = Some("S-openbd".into());
    a.authors = author("A-openbd");
    a.publisher = Some("P-openbd".into());
    a.published_on = Some("2001".into());
    a.list_price = Some(1000);
    a.ref_price_incl = Some(1100);
    a.c_code = Some("0077".into());
    a.pages = Some(100);
    a.size_text = Some("20cm".into());
    a.language = Some("ja".into());
    a.description = Some("D-openbd".into());
    a.series = Some("Series-openbd".into());
    a.volume = Some("1".into());
    a.covers = cover(ProviderId::Openbd, "https://example.org/openbd.jpg");

    let mut n = Candidate::empty(ProviderId::Ndl, isbn);
    n.title = Some("T-ndl".into());
    n.subtitle = Some("S-ndl".into());
    n.title_kana = Some("ヨミ".into());
    n.authors = author("A-ndl");
    n.publisher = Some("P-ndl".into());
    n.published_on = Some("2002".into());
    n.list_price = Some(1001);
    n.ref_price_incl = Some(1101);
    n.ndc = Some("913.6".into());
    n.pages = Some(101);
    n.size_text = Some("21cm".into());
    n.language = Some("ja".into());
    n.series = Some("Series-ndl".into());
    n.volume = Some("2".into());

    let mut g = Candidate::empty(ProviderId::Google, isbn);
    g.title = Some("T-google".into());
    g.subtitle = Some("S-google".into());
    g.authors = author("A-google");
    g.publisher = Some("P-google".into());
    g.published_on = Some("2003".into());
    g.pages = Some(102);
    g.language = Some("en".into());
    g.description = Some("D-google".into());
    g.covers = cover(ProviderId::Google, "https://example.org/google.jpg");

    let mut o = Candidate::empty(ProviderId::OpenLibrary, isbn);
    o.title = Some("T-ol".into());
    o.subtitle = Some("S-ol".into());
    o.authors = author("A-ol");
    o.publisher = Some("P-ol".into());
    o.published_on = Some("2004".into());
    o.pages = Some(103);
    o.language = Some("en-ol".into());
    o.description = Some("D-ol".into());
    o.covers = cover(ProviderId::OpenLibrary, "https://example.org/ol.jpg");

    vec![a, n, g, o]
}

fn merge_by_isbn(
    isbn: &str,
    candidates: &[Candidate],
    tier: Option<&MetaSecondTier>,
) -> MetaMerged {
    let sets = MetaPrioritySets::default();
    merge(isbn, candidates, tier, sets.for_isbn(Some(isbn)))
}

#[test]
fn the_same_candidates_pick_different_sources_for_a_japanese_and_a_foreign_isbn() {
    let tier = MetaSecondTier {
        c_code: Some("0098".into()),
        list_price: Some(1234),
    };
    let jp = merge_by_isbn(JP_ISBN, &labelled_candidates(JP_ISBN), Some(&tier));
    let us = merge_by_isbn(US_ISBN, &labelled_candidates(US_ISBN), Some(&tier));

    // 書名・副書名・著者・出版社・発行日・言語・内容紹介・書影は、和書は openBD、洋書は Google
    for f in [
        MetaField::Title,
        MetaField::Subtitle,
        MetaField::Authors,
        MetaField::Publisher,
        MetaField::PublishedOn,
        MetaField::Language,
        MetaField::Description,
        MetaField::Cover,
    ] {
        assert_eq!(jp.source(f), Some(MetaSource::Openbd), "和書 {f:?}");
        assert_eq!(us.source(f), Some(MetaSource::Google), "洋書 {f:?}");
    }
    assert_eq!(jp.text(MetaField::Title), Some("T-openbd"));
    assert_eq!(us.text(MetaField::Title), Some("T-google"));
    assert_eq!(us.text(MetaField::Subtitle), Some("S-google"));
    assert_eq!(us.authors()[0].name, "A-google");
    assert_eq!(us.text(MetaField::Publisher), Some("P-google"));
    // 洋書の他の取得元の値は、Open Library、NDL、openBD の順に候補へ残る
    let others: Vec<MetaSource> = us
        .choice(MetaField::Title)
        .unwrap()
        .others
        .iter()
        .map(|p| p.source)
        .collect();
    assert_eq!(
        others,
        vec![MetaSource::OpenLibrary, MetaSource::Ndl, MetaSource::Openbd]
    );
    // 書影は Google > Open Library > openBD の順に試す
    let providers: Vec<ProviderId> = us.covers_in_order().iter().map(|c| c.provider).collect();
    assert_eq!(
        providers,
        vec![
            ProviderId::Google,
            ProviderId::OpenLibrary,
            ProviderId::Openbd
        ]
    );
    let providers: Vec<ProviderId> = jp.covers_in_order().iter().map(|c| c.provider).collect();
    assert_eq!(
        providers,
        vec![
            ProviderId::Openbd,
            ProviderId::Google,
            ProviderId::OpenLibrary
        ]
    );

    // ヨミ・NDC・価格・Cコード・ページ数・大きさは、区分によらず同じ取得元・同じ値
    for f in [
        MetaField::TitleKana,
        MetaField::Ndc,
        MetaField::ListPrice,
        MetaField::RefPriceIncl,
        MetaField::CCode,
        MetaField::Pages,
        MetaField::SizeText,
    ] {
        assert_eq!(jp.source(f), us.source(f), "{f:?}");
        assert_eq!(jp.chosen(f), us.chosen(f), "{f:?}");
    }
    assert_eq!(us.source(MetaField::ListPrice), Some(MetaSource::Barcode));
    assert_eq!(us.number(MetaField::ListPrice), Some(1234));
    assert_eq!(us.source(MetaField::CCode), Some(MetaSource::Barcode));
    assert_eq!(us.text(MetaField::CCode), Some("0098"));
    assert_eq!(us.source(MetaField::Ndc), Some(MetaSource::Ndl));
    assert_eq!(us.source(MetaField::TitleKana), Some(MetaSource::Ndl));
    assert_eq!(us.source(MetaField::Pages), Some(MetaSource::Ndl));
    // シリーズ・巻次は、和書は openBD、洋書は NDL
    assert_eq!(jp.source(MetaField::Series), Some(MetaSource::Openbd));
    assert_eq!(us.source(MetaField::Series), Some(MetaSource::Ndl));
    assert_eq!(us.source(MetaField::RefPriceIncl), Some(MetaSource::Openbd));
}

#[test]
fn a_foreign_book_falls_back_along_google_open_library_ndl_openbd() {
    // Google がなければ Open Library、それもなければ NDL、最後に openBD
    let all = labelled_candidates(US_ISBN);
    let without = |gone: &[ProviderId]| -> Vec<Candidate> {
        all.iter()
            .filter(|c| !gone.contains(&c.provider))
            .cloned()
            .collect()
    };
    let title_source =
        |cands: &[Candidate]| merge_by_isbn(US_ISBN, cands, None).source(MetaField::Title);
    assert_eq!(
        title_source(&without(&[ProviderId::Google])),
        Some(MetaSource::OpenLibrary)
    );
    assert_eq!(
        title_source(&without(&[ProviderId::Google, ProviderId::OpenLibrary])),
        Some(MetaSource::Ndl)
    );
    assert_eq!(
        title_source(&without(&[
            ProviderId::Google,
            ProviderId::OpenLibrary,
            ProviderId::Ndl
        ])),
        Some(MetaSource::Openbd)
    );
    // 和書は、Google・Open Library があっても openBD、なければ NDL
    let jp_all = labelled_candidates(JP_ISBN);
    assert_eq!(
        merge_by_isbn(JP_ISBN, &jp_all, None).source(MetaField::Title),
        Some(MetaSource::Openbd)
    );
    let no_openbd: Vec<Candidate> = jp_all
        .into_iter()
        .filter(|c| c.provider != ProviderId::Openbd)
        .collect();
    assert_eq!(
        merge_by_isbn(JP_ISBN, &no_openbd, None).source(MetaField::Title),
        Some(MetaSource::Ndl)
    );
}

#[test]
fn a_book_without_an_isbn_uses_the_japanese_set() {
    let sets = MetaPrioritySets::default();
    assert_eq!(sets.for_isbn(None), &sets.japanese);
    assert_eq!(sets.for_isbn(Some("読めない")), &sets.japanese);
    assert_eq!(sets.for_isbn(Some(US_ISBN)), &sets.foreign);
    assert_eq!(sets.for_profile(Foreign), &sets.foreign);
}

#[test]
fn the_real_clean_code_fixtures_choose_the_google_title_for_a_foreign_isbn() {
    // NDL の実際の応答（`Clean code`、副書名は小文字）と Open Library の実際の応答、Google は合成の volume
    let google = {
        let volume: serde_json::Value =
            serde_json::from_str(&fixture("google_synthetic_volume_hjEFCAAAQBAJ.json")).unwrap();
        let search =
            serde_json::json!({"kind": "books#volumes", "totalItems": 1, "items": [volume]});
        let raw = RawResponse {
            provider: ProviderId::Google,
            isbn13: US_ISBN.into(),
            body: search.to_string(),
        };
        GoogleBooks::new(None).parse(&raw).unwrap().unwrap()
    };
    let cands = vec![ndl(US_ISBN), google, clean_code_open_library()];
    let ndl_title = cands[0].title.clone();
    assert_eq!(ndl_title.as_deref(), Some("Clean code"));

    let m = merge_by_isbn(US_ISBN, &cands, None);
    assert_eq!(m.text(MetaField::Title), Some("Clean Code"));
    assert_eq!(
        m.text(MetaField::Subtitle),
        Some("A Handbook of Agile Software Craftsmanship")
    );
    assert_eq!(m.source(MetaField::Title), Some(MetaSource::Google));
    assert_eq!(m.source(MetaField::Subtitle), Some(MetaSource::Google));
    assert_eq!(m.source(MetaField::Cover), Some(MetaSource::Google));
    // 和書の順で統合すると NDL の書名になってしまう（これまでの動き）
    let old = merge(US_ISBN, &cands, None, &MetaPriorities::default());
    assert_eq!(old.text(MetaField::Title), Some("Clean code"));
    assert_eq!(old.source(MetaField::Title), Some(MetaSource::Ndl));
}

#[test]
fn each_set_is_normalized_with_its_own_defaults() {
    use MetaSource::{Google, Ndl, OpenLibrary, Openbd};
    let mut sets = MetaPrioritySets::default();
    sets.japanese.set(MetaGroup::Title, vec![Ndl]);
    sets.foreign.set(MetaGroup::Title, vec![Ndl]);
    sets.foreign.groups.remove(&MetaGroup::Cover);
    sets.japanese
        .set(MetaGroup::Authors, vec![Google, Google, Ndl]);
    let n = sets.normalized();
    // 足りない取得元は、その区分の既定の順で末尾に足される
    assert_eq!(
        n.japanese.groups[&MetaGroup::Title],
        vec![Ndl, Openbd, Google, OpenLibrary]
    );
    assert_eq!(
        n.foreign.groups[&MetaGroup::Title],
        vec![Ndl, Google, OpenLibrary, Openbd]
    );
    // 行がなければ、その区分の既定
    assert_eq!(
        n.foreign.groups[&MetaGroup::Cover],
        vec![Google, OpenLibrary, Openbd]
    );
    // 重複は除く
    assert_eq!(
        n.japanese.groups[&MetaGroup::Authors],
        vec![Google, Ndl, Openbd, OpenLibrary]
    );
    // 全部の行がそろう。2 回かけても変わらない
    assert_eq!(n.japanese.groups.len(), MetaGroup::ALL.len());
    assert_eq!(n.foreign.groups.len(), MetaGroup::ALL.len());
    assert_eq!(n.normalized(), n);
    // 区分を取り違えない: 和書の組を洋書の既定で、洋書の組を和書の既定でそろえると別物になる
    assert_ne!(
        sets.japanese.normalized(Foreign).groups[&MetaGroup::Title],
        n.japanese.groups[&MetaGroup::Title]
    );
}

#[test]
fn the_sets_roundtrip_through_json_in_the_shape_the_ui_sees() {
    let mut sets = MetaPrioritySets::default();
    sets.foreign
        .set(MetaGroup::Title, vec![MetaSource::Ndl, MetaSource::Google]);
    let v = serde_json::to_value(&sets).unwrap();
    assert_eq!(
        v["japanese"]["groups"]["title"],
        serde_json::json!(["openbd", "ndl", "google", "openlibrary"])
    );
    assert_eq!(
        v["foreign"]["groups"]["title"],
        serde_json::json!(["ndl", "google"])
    );
    assert_eq!(
        v["foreign"]["groups"]["cover"],
        serde_json::json!(["google", "openlibrary", "openbd"])
    );
    let back: MetaPrioritySets = serde_json::from_value(v).unwrap();
    assert_eq!(back, sets);
}
