//! 統合（FR-META-04／05）のテスト。取得元の候補は `tests/fixtures/` の実際の応答から作る。

mod common;

use bibcontainer_metadata::merge::{MetaPick, MetaPriorities, merge};
use bibcontainer_metadata::provider::{Provider, RawResponse};
use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd};
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
    // 設定にない取得元は既定の順で末尾に足される（Cover に Google だけを書くと、openBD が続く）
    assert_eq!(
        p.order(MetaGroup::Cover),
        vec![MetaSource::Google, MetaSource::Openbd]
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
    assert_eq!(p.order(MetaGroup::Title), vec![Openbd, Ndl, Google]);
    assert_eq!(p.order(MetaGroup::TitleKana), vec![Ndl]);
    assert_eq!(p.order(MetaGroup::Authors), vec![Openbd, Ndl, Google]);
    assert_eq!(p.order(MetaGroup::Publisher), vec![Openbd, Ndl, Google]);
    assert_eq!(p.order(MetaGroup::PublishedOn), vec![Openbd, Ndl, Google]);
    assert_eq!(p.order(MetaGroup::ListPrice), vec![Barcode, Openbd, Ndl]);
    assert_eq!(p.order(MetaGroup::CCode), vec![Barcode, Openbd]);
    assert_eq!(p.order(MetaGroup::Ndc), vec![Ndl]);
    assert_eq!(p.order(MetaGroup::PagesAndSize), vec![Ndl, Openbd, Google]);
    assert_eq!(p.order(MetaGroup::Series), vec![Openbd, Ndl]);
    assert_eq!(p.order(MetaGroup::Description), vec![Openbd, Google]);
    assert_eq!(p.order(MetaGroup::Cover), vec![Openbd, Google]);
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
