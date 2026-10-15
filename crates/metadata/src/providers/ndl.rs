//! NDLサーチ（SRU、DC-NDL）。
//!
//! `GET https://ndlsearch.ndl.go.jp/api/sru?operation=searchRetrieve&recordSchema=dcndl&recordPacking=xml&onlyBib=true&maximumRecords=10&query=isbn%3D<ISBN>`
//!
//! - `isbn=` の検索は完全一致で、10 桁・13 桁のどちらで入れても両方に変換して探す（公式の仕様書 3 章）。
//! - `onlyBib=true` で所蔵（`dcndl:Item`）を省き、応答が 1/3 ほどになる（公式の仕様書）。
//! - 同じ ISBN で複数のレコード（NDL 自身、CiNii、公共図書館の MARC など）が返る。誤って登録された ISBN
//!   （`dcterms:identifier` の型が `ErrorISBN`）でも当たるので、ISBN の型が `ISBN` で、探した ISBN と合うレコードだけを使う。
//!   シリーズ全体を 1 件にまとめたレコード（ISBN が何十個も並ぶ）は避け、NDL 自身のレコードと、項目が多いレコードを優先する。
//! - 該当なしは HTTP 200 で、`<diagnostics>` に `Record does not exist` が入る。
//! - 著者: 責任表示（`dc:creator`）が日本語（漢字・かなを含む）なら、名前と役割を責任表示から、ヨミを `foaf:Agent` から取る。
//!   英字だけの責任表示（`Robert C. Martin ... [et al.] ; foreword by James O. Coplien`）は、空白で人を分けられないので、
//!   構造化された人名（`dcterms:creator` の `foaf:name`、`Martin, Robert C.`）を使い、役割だけ責任表示から読む
//!   （`normalize::parse_latin_statement_authors`）。

use async_trait::async_trait;

use crate::error::{FetchError, ParseError};
use crate::http::HttpRequest;
use crate::isbn::to_isbn13;
use crate::normalize::{
    attach_kana, clean_text, has_kana, latin_reference_names, normalize_date, normalize_language,
    parse_authors, parse_extent, parse_latin_statement_authors, parse_price,
    restore_latin_name_spacing, split_title,
};
use crate::provider::{Provider, ProviderAuth, ProviderInfo, RawResponse};
use crate::transport::{ProviderRateLimit, Transport};
use crate::types::{Candidate, MetaField, ProviderId};
use crate::xml::{Node, parse_document};

pub const DEFAULT_BASE_URL: &str = "https://ndlsearch.ndl.go.jp";

const SRW: &str = "http://www.loc.gov/zing/srw/";
const SRW_DIAG: &str = "http://www.loc.gov/zing/srw/diagnostic/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const DCNDL: &str = "http://ndl.go.jp/dcndl/terms/";
const FOAF: &str = "http://xmlns.com/foaf/0.1/";

/// 国立国会図書館自身の書誌レコードのカテゴリ。
const NDL_OWN_CATEGORY: &str = "R100000002";
/// ISBN がこの数を超えるレコードは、シリーズ全体をまとめたレコードとみなす。
const MAX_ISBNS_PER_BOOK: usize = 4;

pub struct Ndl {
    base_url: String,
}

impl Default for Ndl {
    fn default() -> Self {
        Self::new()
    }
}

impl Ndl {
    pub fn new() -> Self {
        Ndl {
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// テスト用に接続先を変える。
    pub fn with_base_url(base_url: &str) -> Self {
        Ndl {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// NDL は同時接続 1、1 秒に 1 件（FR-META-08、11.2）。
    pub(crate) fn rate_limit() -> ProviderRateLimit {
        ProviderRateLimit::new("ndl", 1000, 1)
    }
}

#[async_trait]
impl Provider for Ndl {
    fn id(&self) -> ProviderId {
        ProviderId::Ndl
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ProviderId::Ndl,
            display_name: "NDLサーチ".to_string(),
            auth: ProviderAuth::None,
            fields: vec![
                MetaField::Title,
                MetaField::Subtitle,
                MetaField::TitleKana,
                MetaField::Authors,
                MetaField::Publisher,
                MetaField::PublishedOn,
                MetaField::ListPrice,
                MetaField::RefPriceIncl,
                MetaField::Ndc,
                MetaField::Pages,
                MetaField::SizeText,
                MetaField::Language,
                MetaField::Series,
                MetaField::Volume,
            ],
            rate_limit: Self::rate_limit(),
            credit: "書誌情報: 国立国会図書館サーチ API (https://ndlsearch.ndl.go.jp/)".to_string(),
            supports_batch: false,
            supports_search: false,
            needs_network: true,
        }
    }

    async fn fetch_raw(
        &self,
        transport: &Transport,
        isbn13: &str,
    ) -> Result<RawResponse, FetchError> {
        let url = format!(
            "{}/api/sru?operation=searchRetrieve&recordSchema=dcndl&recordPacking=xml&maximumRecords=10&onlyBib=true&query=isbn%3D{}",
            self.base_url, isbn13
        );
        let resp = transport
            .get_ok(&Self::rate_limit(), HttpRequest::get(url))
            .await?;
        Ok(RawResponse {
            provider: ProviderId::Ndl,
            isbn13: isbn13.to_string(),
            body: resp.text(),
        })
    }

    fn parse(&self, raw: &RawResponse) -> Result<Option<Candidate>, ParseError> {
        parse_sru(&raw.isbn13, &raw.body)
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

/// SRU の 1 レコードの中の、書誌（`dcndl:BibResource`）と、そのレコードの出どころ。
struct Rec<'a> {
    bib: &'a Node,
    /// `R100000002`（NDL 自身）など
    category: Option<String>,
    isbns: Vec<String>,
    is_book: bool,
}

fn text_of(node: Option<&Node>) -> Option<String> {
    node.and_then(|n| clean_text(n.text()))
}

/// `<dc:title><rdf:Description><rdf:value>…</rdf:value><dcndl:transcription>…</dcndl:transcription></rdf:Description></dc:title>`
/// の形の項目から、値とヨミを取る。
fn described(node: &Node) -> (Option<String>, Option<String>) {
    match node.child(RDF, "Description") {
        Some(d) => (
            text_of(d.child(RDF, "value")),
            text_of(d.child(DCNDL, "transcription")),
        ),
        None => (clean_text(node.text()), None),
    }
}

fn first_described(bib: &Node, ns: &str, name: &str) -> (Option<String>, Option<String>) {
    bib.children_named(ns, name)
        .map(described)
        .find(|(value, _)| value.is_some())
        .unwrap_or((None, None))
}

fn category_of(admin: Option<&Node>, bib: &Node) -> Option<String> {
    if let Some(c) = text_of(admin.and_then(|a| a.child(DCNDL, "bibRecordCategory"))) {
        return Some(c);
    }
    // `https://ndlsearch.ndl.go.jp/books/R100000002-I025359164#material` の `R100000002`
    let about = bib.attr(RDF, "about")?;
    let tail = about.split("/books/").nth(1)?;
    let cat = tail.split('-').next()?;
    (cat.starts_with('R') && cat[1..].chars().all(|c| c.is_ascii_digit())).then(|| cat.to_string())
}

fn isbns_of(bib: &Node) -> Vec<String> {
    let mut out = Vec::new();
    for id in bib.children_named(DCTERMS, "identifier") {
        // 型が `…/ISBN` のものだけ（`…/ErrorISBN` は誤って付けられた ISBN）
        let Some(datatype) = id.attr(RDF, "datatype") else {
            continue;
        };
        if !datatype.ends_with("/ISBN") {
            continue;
        }
        if let Some(isbn) = to_isbn13(id.text())
            && !out.contains(&isbn)
        {
            out.push(isbn);
        }
    }
    out
}

fn is_book(bib: &Node) -> bool {
    bib.children_named(DCNDL, "materialType").any(|m| {
        m.attr(RDF, "resource")
            .is_some_and(|r| r.ends_with("/Book"))
            || m.attr(RDFS, "label") == Some("図書")
    })
}

fn has_title(bib: &Node) -> bool {
    bib.child(DCTERMS, "title").is_some() || bib.child(DC, "title").is_some()
}

/// 候補にしたレコードの順位づけ。大きいほど良い。
fn score(rec: &Rec<'_>) -> usize {
    let bib = rec.bib;
    let mut s = 0;
    if rec.category.as_deref() == Some(NDL_OWN_CATEGORY) {
        s += 1000;
    }
    let present = [
        first_described(bib, DC, "title").1.is_some(),
        bib.child(DCNDL, "price").is_some(),
        bib.child(DCTERMS, "extent")
            .is_some_and(|e| e.text().contains('p')),
        bib.children_named(DCTERMS, "subject").any(|n| {
            n.attr(RDF, "resource")
                .is_some_and(|r| r.contains("/class/ndc"))
        }),
        bib.child(DCTERMS, "publisher").is_some(),
        bib.child(DCTERMS, "date").is_some(),
        bib.child(DCNDL, "seriesTitle").is_some(),
        bib.child(DC, "creator").is_some(),
    ];
    s + present.iter().filter(|p| **p).count()
}

pub(crate) fn parse_sru(isbn13: &str, body: &str) -> Result<Option<Candidate>, ParseError> {
    let root = parse_document(body)
        .map_err(|e| ParseError::new(format!("NDL の応答を読めません: {e}")))?;
    if !root.is(SRW, "searchRetrieveResponse") {
        return Err(ParseError::new("NDL の応答が SRU の形ではありません"));
    }

    let records: Vec<&Node> = root
        .child(SRW, "records")
        .map(|r| r.children_named(SRW, "record").collect())
        .unwrap_or_default();

    if records.is_empty() {
        let messages: Vec<String> = root
            .child(SRW, "diagnostics")
            .into_iter()
            .flat_map(|d| d.children_named(SRW_DIAG, "diagnostic"))
            .filter_map(|d| text_of(d.child(SRW_DIAG, "message")))
            .collect();
        return match messages.first() {
            None => Ok(None),
            Some(m) if m == "Record does not exist" => Ok(None),
            Some(m) => Err(ParseError::new(format!("NDL がエラーを返しました: {m}"))),
        };
    }

    // 洋名の綴りの参照（英字の `foaf:name`）。選ばれなかったレコード（CiNii など）の `Martin, Robert C.` も使うので、
    // 絞り込みの前に、応答のすべてのレコードから集める。
    let references = latin_reference_names(creator_names(&records));

    // レコードごとに、書誌（BibResource のうち書名のあるもの）を取り出す
    let mut recs: Vec<Rec<'_>> = Vec::new();
    for record in records {
        let Some(data) = record.child(SRW, "recordData") else {
            continue;
        };
        let Some(rdf) = data.children.iter().find(|n| n.is(RDF, "RDF")) else {
            continue;
        };
        let admin = rdf.child(DCNDL, "BibAdminResource");
        for bib in rdf
            .children_named(DCNDL, "BibResource")
            .filter(|b| has_title(b))
        {
            recs.push(Rec {
                bib,
                category: category_of(admin, bib),
                isbns: isbns_of(bib),
                is_book: is_book(bib),
            });
        }
    }

    // 探した ISBN を、型 `ISBN` の識別子として持つレコードだけ（誤 ISBN での一致を除く）
    recs.retain(|r| r.isbns.iter().any(|i| i == isbn13));
    // シリーズ全体のレコードは、ほかにあれば使わない
    if recs.iter().any(|r| r.isbns.len() <= MAX_ISBNS_PER_BOOK) {
        recs.retain(|r| r.isbns.len() <= MAX_ISBNS_PER_BOOK);
    }
    // 図書のレコードを選ぶ
    if recs.iter().any(|r| r.is_book) {
        recs.retain(|r| r.is_book);
    }
    // 先に出てきたものを優先（同点のとき）
    let best = recs
        .iter()
        .enumerate()
        .max_by_key(|(i, r)| (score(r), std::cmp::Reverse(*i)))
        .map(|(_, r)| r);
    let Some(best) = best else { return Ok(None) };
    Ok(Some(candidate_from(isbn13, best.bib, &references)))
}

/// 応答のすべてのレコードの、人名（`dcterms:creator` の `foaf:Agent` の `foaf:name`）。
fn creator_names(records: &[&Node]) -> Vec<String> {
    records
        .iter()
        .filter_map(|record| record.child(SRW, "recordData"))
        .filter_map(|data| data.children.iter().find(|n| n.is(RDF, "RDF")))
        .flat_map(|rdf| rdf.children_named(DCNDL, "BibResource"))
        .flat_map(|bib| bib.children_named(DCTERMS, "creator"))
        .filter_map(|c| c.child(FOAF, "Agent"))
        .filter_map(|agent| agent.child(FOAF, "name"))
        .map(|name| name.text().to_string())
        .collect()
}

/// `NDC10 > NDC9 > NDC8` の順で新しい版を優先する。
fn ndc_of(bib: &Node) -> Option<String> {
    let mut found: Vec<(u32, String)> = Vec::new();
    for s in bib.children_named(DCTERMS, "subject") {
        if let Some(res) = s.attr(RDF, "resource") {
            for (edition, marker) in [
                (10, "/class/ndc10/"),
                (9, "/class/ndc9/"),
                (8, "/class/ndc8/"),
            ] {
                if let Some(code) = res.split(marker).nth(1).and_then(clean_text) {
                    found.push((edition, code));
                }
            }
        }
    }
    for s in bib.children_named(DC, "subject") {
        let Some(datatype) = s.attr(RDF, "datatype") else {
            continue;
        };
        for (edition, suffix) in [(10, "/NDC10"), (9, "/NDC9"), (8, "/NDC8")] {
            if datatype.ends_with(suffix)
                && let Some(code) = clean_text(s.text())
            {
                found.push((edition, code));
            }
        }
    }
    found.into_iter().max_by_key(|(e, _)| *e).map(|(_, c)| c)
}

/// `name_references` は、応答のすべてのレコードから集めた英字の人名（`名 姓` の形。洋名の空白を直すときの参照）。
fn candidate_from(isbn13: &str, bib: &Node, name_references: &[String]) -> Candidate {
    let mut c = Candidate::empty(ProviderId::Ndl, isbn13);

    // 書名・ヨミ・副書名。`dc:title` の値は「書名 : 副書名」の形
    let (value, transcription) = first_described(bib, DC, "title");
    let title_full = value.or_else(|| text_of(bib.child(DCTERMS, "title")));
    if let Some(full) = title_full {
        let (main, sub) = split_title(&full);
        c.title = Some(main);
        c.subtitle = sub;
    }
    // ヨミは、かなを含むものだけ（`One piece` のように原文の綴りが入っていることがある）
    c.title_kana = transcription
        .map(|t| split_title(&t).0)
        .filter(|t| has_kana(t));

    // 巻次・シリーズ
    c.volume = first_described(bib, DCNDL, "volume").0;
    c.series = first_described(bib, DCNDL, "seriesTitle").0;

    // 著者。英字の責任表示（`Robert C. Martin ... [et al.] ; foreword by James O. Coplien`）は、空白で人を分けられず
    // 役割の語も日本語でないので、構造化された人名（dcterms:creator の foaf:name）を使い、役割だけ責任表示から読む。
    // 日本語の本（責任表示に漢字・かながある）は、名前と役割を dc:creator から、ヨミを foaf:Agent から取る。
    let statement = bib
        .children_named(DC, "creator")
        .map(|creator| creator.text())
        .collect::<Vec<_>>()
        .join(" ; ");
    let foaf_names: Vec<String> = bib
        .children_named(DCTERMS, "creator")
        .filter_map(|c| c.child(FOAF, "Agent"))
        .filter_map(|agent| text_of(agent.child(FOAF, "name")))
        .collect();
    c.authors = match parse_latin_statement_authors(&statement, &foaf_names) {
        Some(authors) => authors,
        None => {
            let mut authors = Vec::new();
            for creator in bib.children_named(DC, "creator") {
                for a in parse_authors(creator.text()) {
                    if !authors.contains(&a) {
                        authors.push(a);
                    }
                }
            }
            let kana_pairs: Vec<(String, String)> = bib
                .children_named(DCTERMS, "creator")
                .filter_map(|c| c.child(FOAF, "Agent"))
                .filter_map(|agent| {
                    Some((
                        agent.child(FOAF, "name")?.text().to_string(),
                        agent.child(DCNDL, "transcription")?.text().to_string(),
                    ))
                })
                .collect();
            attach_kana(&mut authors, &kana_pairs);
            authors
        }
    };
    // 日本語訳の本では、責任表示も NDL 自身の `foaf:name` も洋名の空白を落としている（`ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ`）。
    // 同じ応答のほかのレコード（CiNii）の `Martin, Robert C.` があればその綴りに、なければ頭文字のまわりだけ直す。
    restore_latin_name_spacing(&mut c.authors, name_references);

    // 出版社（「発売」は発行元がほかにあるときは避ける）
    let publishers: Vec<(String, bool)> = bib
        .children_named(DCTERMS, "publisher")
        .filter_map(|p| p.child(FOAF, "Agent"))
        .filter_map(|agent| {
            let name = text_of(agent.child(FOAF, "name"))?;
            let sales = agent
                .child(DCTERMS, "description")
                .is_some_and(|d| d.text().contains("発売"));
            Some((name, sales))
        })
        .collect();
    c.publisher = publishers
        .iter()
        .find(|(_, sales)| !sales)
        .or(publishers.first())
        .map(|(name, _)| name.clone());

    // 発行日（`dcterms:date` は `2014.4` の形。なければ `dcterms:issued` の年）
    c.published_on = bib
        .children_named(DCTERMS, "date")
        .find_map(|d| normalize_date(d.text()))
        .or_else(|| {
            bib.children_named(DCTERMS, "issued")
                .find_map(|d| normalize_date(d.text()))
        });

    // 価格（「(税込)」とあれば参考の税込価格）
    if let Some(p) = bib
        .child(DCNDL, "price")
        .and_then(|n| parse_price(n.text()))
    {
        if p.tax_included {
            c.ref_price_incl = Some(p.amount);
        } else {
            c.list_price = Some(p.amount);
        }
    }

    // ページ数・大きさ（`218p ; 15cm`）
    if let Some(extent) = bib
        .children_named(DCTERMS, "extent")
        .map(|e| parse_extent(e.text()))
        .find(|e| e.pages.is_some() || e.size.is_some())
    {
        c.pages = extent.pages;
        c.size_text = extent.size;
    }

    c.ndc = ndc_of(bib);
    c.language = bib
        .children_named(DCTERMS, "language")
        .find_map(|l| normalize_language(l.text()));
    c
}
