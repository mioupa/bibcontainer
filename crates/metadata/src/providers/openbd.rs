//! openBD（https://openbd.jp/）。
//!
//! `GET https://api.openbd.jp/v1/get?isbn=<ISBN>,<ISBN>,…`（最大 100 件）。応答は ISBN ごとの配列で、
//! 未収録は `null`。生の応答として保存するのは「その ISBN の要素 1 つの JSON 文字列」（未収録は `null`）。
//!
//! 2023 年 8 月以降、版元ドットコム会員社以外のデータは国立国会図書館の書誌を ONIX 形式に簡易変換したものになっている。
//! そのため、書名に副書名や巻次が含まれる・著者が `姓,名,生没年` の形・価格が `310 (税込)` の形・Cコードや書影がない、
//! といった欠けやゆれがある。ここで可能な範囲を吸収する。

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{FetchError, ParseError};
use crate::http::HttpRequest;
use crate::normalize::{
    attach_kana, clean_description, clean_person_name, clean_text, normalize_ccode, normalize_date,
    normalize_language, parse_authors, parse_price, split_title, upgrade_to_https,
};
use crate::provider::{Provider, ProviderAuth, ProviderInfo, RawResponse};
use crate::transport::{ProviderRateLimit, Transport};
use crate::types::{Candidate, CandidateAuthor, CandidateCover, MetaField, ProviderId};

pub const DEFAULT_BASE_URL: &str = "https://api.openbd.jp/v1";
/// 1 回の問い合わせで取れる ISBN の数。
pub const MAX_BATCH: usize = 100;

pub struct OpenBd {
    base_url: String,
}

impl Default for OpenBd {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenBd {
    pub fn new() -> Self {
        OpenBd {
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// テスト用に接続先を変える。
    pub fn with_base_url(base_url: &str) -> Self {
        OpenBd {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub(crate) fn rate_limit() -> ProviderRateLimit {
        // 規約や仕様書に数値の上限はないが、1 秒に 1 回程度にする（要件書 11.1）
        ProviderRateLimit::new("openbd", 1000, 1)
    }

    async fn fetch_chunk(
        &self,
        transport: &Transport,
        isbns: &[String],
    ) -> Result<Vec<Value>, FetchError> {
        let url = format!("{}/get?isbn={}", self.base_url, isbns.join(","));
        let resp = transport
            .get_ok(&Self::rate_limit(), HttpRequest::get(url))
            .await?;
        let v: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| FetchError::Parse(format!("openBD の応答が JSON ではありません: {e}")))?;
        match v {
            Value::Array(items) if items.len() == isbns.len() => Ok(items),
            Value::Array(items) => Err(FetchError::Parse(format!(
                "openBD の応答の件数が合いません（問い合わせ {} 件、応答 {} 件）",
                isbns.len(),
                items.len()
            ))),
            _ => Err(FetchError::Parse(
                "openBD の応答が配列ではありません".to_string(),
            )),
        }
    }
}

#[async_trait]
impl Provider for OpenBd {
    fn id(&self) -> ProviderId {
        ProviderId::Openbd
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ProviderId::Openbd,
            display_name: "openBD".to_string(),
            auth: ProviderAuth::None,
            fields: vec![
                MetaField::Title,
                MetaField::Subtitle,
                MetaField::Authors,
                MetaField::Publisher,
                MetaField::PublishedOn,
                MetaField::ListPrice,
                MetaField::RefPriceIncl,
                MetaField::CCode,
                MetaField::Pages,
                MetaField::Language,
                MetaField::Description,
                MetaField::Series,
                MetaField::Volume,
                MetaField::Cover,
            ],
            rate_limit: Self::rate_limit(),
            credit: "書誌情報・書影: openBD (https://openbd.jp/)".to_string(),
            supports_batch: true,
            supports_search: false,
            needs_network: true,
        }
    }

    async fn fetch_raw(
        &self,
        transport: &Transport,
        isbn13: &str,
    ) -> Result<RawResponse, FetchError> {
        let mut results = self.fetch_raw_batch(transport, &[isbn13.to_string()]).await;
        match results.pop() {
            Some((_, r)) => r,
            None => Err(FetchError::Parse("openBD の応答が空でした".to_string())),
        }
    }

    async fn fetch_raw_batch(
        &self,
        transport: &Transport,
        isbns: &[String],
    ) -> Vec<(String, Result<RawResponse, FetchError>)> {
        let mut out = Vec::with_capacity(isbns.len());
        for chunk in isbns.chunks(MAX_BATCH) {
            match self.fetch_chunk(transport, chunk).await {
                Ok(items) => {
                    for (isbn, item) in chunk.iter().zip(items) {
                        let body = item.to_string();
                        out.push((
                            isbn.clone(),
                            Ok(RawResponse {
                                provider: ProviderId::Openbd,
                                isbn13: isbn.clone(),
                                body,
                            }),
                        ));
                    }
                }
                Err(e) => {
                    for isbn in chunk {
                        out.push((isbn.clone(), Err(e.clone())));
                    }
                }
            }
        }
        out
    }

    fn parse(&self, raw: &RawResponse) -> Result<Option<Candidate>, ParseError> {
        parse_openbd(&raw.isbn13, &raw.body)
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

fn one_or_many(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(o @ Value::Object(_)) => vec![o],
        _ => Vec::new(),
    }
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn ptr_str<'a>(v: &'a Value, pointer: &str) -> Option<&'a str> {
    v.pointer(pointer).and_then(Value::as_str)
}

/// ONIX の ContributorRole（コードリスト 17）のうち、よく使うものだけ。著者の文字列が空のときの代替。
fn onix_role(code: &str) -> Option<&'static str> {
    match code {
        "A01" | "A14" => Some("著"),
        "A03" => Some("脚本"),
        "A12" | "A35" => Some("絵"),
        "A13" => Some("写真"),
        "A38" => Some("原作"),
        "B01" | "B21" => Some("編"),
        "B06" => Some("訳"),
        "B10" => Some("編訳"),
        "B20" => Some("監修"),
        _ => None,
    }
}

/// `PriceAmount` は文字列（`"1300"`、`"310 (税込)"`）のことも数値のこともある。
fn price_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub(crate) fn parse_openbd(isbn13: &str, body: &str) -> Result<Option<Candidate>, ParseError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| ParseError::new(format!("openBD の応答が JSON ではありません: {e}")))?;
    if v.is_null() {
        return Ok(None);
    }
    if !v.is_object() {
        return Err(ParseError::new("openBD の応答がオブジェクトではありません"));
    }
    let mut c = Candidate::empty(ProviderId::Openbd, isbn13);
    let summary = v.get("summary");
    let onix = v.get("onix");
    let desc = onix.and_then(|o| o.get("DescriptiveDetail"));

    // 書名・副書名・巻次
    let title_details = one_or_many(desc.and_then(|d| d.get("TitleDetail")));
    let title_detail = title_details
        .iter()
        .find(|t| str_at(t, "TitleType") == Some("01"))
        .or(title_details.first())
        .copied();
    let elements = one_or_many(title_detail.and_then(|t| t.get("TitleElement")));
    let element = elements
        .iter()
        .find(|e| str_at(e, "TitleElementLevel") == Some("01"))
        .or(elements.first())
        .copied();
    let title_raw = element
        .and_then(|e| ptr_str(e, "/TitleText/content"))
        .filter(|s| !s.trim().is_empty())
        .or_else(|| summary.and_then(|s| str_at(s, "title")));
    let subtitle_raw = element.and_then(|e| ptr_str(e, "/Subtitle/content"));
    if let Some(title) = title_raw.and_then(clean_text) {
        match subtitle_raw.and_then(clean_text) {
            Some(sub) => {
                c.title = Some(title);
                c.subtitle = Some(sub);
            }
            None => {
                // NDL 由来のデータは「書名 : 副書名」の形で入っている
                let (main, sub) = split_title(&title);
                c.title = Some(main);
                c.subtitle = sub;
            }
        }
    }

    // シリーズ・巻次
    let collection_elements: Vec<&Value> = one_or_many(desc.and_then(|d| d.get("Collection")))
        .into_iter()
        .flat_map(|col| one_or_many(col.pointer("/TitleDetail/TitleElement")))
        .collect();
    let collection_element = collection_elements
        .iter()
        .find(|e| ptr_str(e, "/TitleText/content").is_some_and(|s| !s.trim().is_empty()))
        .copied();
    c.series = summary
        .and_then(|s| str_at(s, "series"))
        .and_then(clean_text)
        .or_else(|| {
            collection_element
                .and_then(|e| ptr_str(e, "/TitleText/content"))
                .and_then(clean_text)
        });
    c.volume = summary
        .and_then(|s| str_at(s, "volume"))
        .and_then(clean_text)
        .or_else(|| {
            collection_element
                .and_then(|e| str_at(e, "PartNumber"))
                .and_then(clean_text)
        })
        .or_else(|| {
            element
                .and_then(|e| str_at(e, "PartNumber"))
                .and_then(clean_text)
        });

    // 著者（名前と役割は summary.author の文字列から。ヨミは ONIX の Contributor から）
    let contributors = one_or_many(desc.and_then(|d| d.get("Contributor")));
    let mut authors = summary
        .and_then(|s| str_at(s, "author"))
        .map(parse_authors)
        .unwrap_or_default();
    if authors.is_empty() {
        for contributor in &contributors {
            let Some(name) = ptr_str(contributor, "/PersonName/content") else {
                continue;
            };
            let role = contributor
                .get("ContributorRole")
                .and_then(Value::as_array)
                .and_then(|roles| roles.iter().filter_map(Value::as_str).find_map(onix_role))
                .map(str::to_string);
            // Contributor は 1 件が 1 人
            if let Some(name) = clean_person_name(name) {
                let a = CandidateAuthor {
                    name,
                    kana: None,
                    role,
                };
                if !authors.contains(&a) {
                    authors.push(a);
                }
            }
        }
    }
    let kana_pairs: Vec<(String, String)> = contributors
        .iter()
        .filter_map(|c| {
            Some((
                ptr_str(c, "/PersonName/content")?.to_string(),
                ptr_str(c, "/PersonName/collationkey")?.to_string(),
            ))
        })
        .collect();
    attach_kana(&mut authors, &kana_pairs);
    c.authors = authors;

    // 出版社・発行日
    let publishing = onix.and_then(|o| o.get("PublishingDetail"));
    c.publisher = summary
        .and_then(|s| str_at(s, "publisher"))
        .and_then(clean_text)
        .or_else(|| {
            publishing
                .and_then(|p| ptr_str(p, "/Publisher/PublisherName"))
                .and_then(clean_text)
        })
        .or_else(|| {
            publishing
                .and_then(|p| ptr_str(p, "/Imprint/ImprintName"))
                .and_then(clean_text)
        });
    let dates = one_or_many(publishing.and_then(|p| p.get("PublishingDate")));
    let onix_date = ["01", "11"]
        .iter()
        .find_map(|role| {
            dates
                .iter()
                .find(|d| str_at(d, "PublishingDateRole") == Some(role))
        })
        .or(dates.first())
        .and_then(|d| str_at(d, "Date"));
    c.published_on = summary
        .and_then(|s| str_at(s, "pubdate"))
        .and_then(normalize_date)
        .or_else(|| onix_date.and_then(normalize_date));

    // Cコード（SubjectSchemeIdentifier 78）
    c.c_code = one_or_many(desc.and_then(|d| d.get("Subject")))
        .iter()
        .filter(|s| str_at(s, "SubjectSchemeIdentifier") == Some("78"))
        .find_map(|s| str_at(s, "SubjectCode").and_then(normalize_ccode));

    // 価格（PriceType 01）。「(税込)」とあれば参考の税込価格、なければ本体価格
    let supply = one_or_many(onix.and_then(|o| o.pointer("/ProductSupply/SupplyDetail")));
    let price = supply
        .iter()
        .flat_map(|d| one_or_many(d.get("Price")))
        .filter(|p| str_at(p, "PriceType") == Some("01"))
        .filter(|p| str_at(p, "CurrencyCode").is_none_or(|cur| cur == "JPY"))
        .find_map(|p| {
            p.get("PriceAmount")
                .and_then(price_text)
                .and_then(|t| parse_price(&t))
        });
    if let Some(p) = price {
        if p.tax_included {
            c.ref_price_incl = Some(p.amount);
        } else {
            c.list_price = Some(p.amount);
        }
    }

    // ページ数（ExtentType 11 = 本文のページ数、ExtentUnit 03 = ページ）
    c.pages = one_or_many(desc.and_then(|d| d.get("Extent")))
        .iter()
        .filter(|e| str_at(e, "ExtentType") == Some("11") && str_at(e, "ExtentUnit") == Some("03"))
        .find_map(|e| str_at(e, "ExtentValue").and_then(|s| s.trim().parse::<u32>().ok()))
        .filter(|p| *p > 0);

    // 言語（LanguageRole 01 = 本文の言語）
    c.language = one_or_many(desc.and_then(|d| d.get("Language")))
        .iter()
        .filter(|l| str_at(l, "LanguageRole") == Some("01"))
        .find_map(|l| str_at(l, "LanguageCode").and_then(normalize_language));

    // 内容紹介（TextType 03）。なければ短い説明（02）
    let texts = one_or_many(onix.and_then(|o| o.pointer("/CollateralDetail/TextContent")));
    c.description = ["03", "02"].iter().find_map(|kind| {
        texts
            .iter()
            .filter(|t| str_at(t, "TextType") == Some(kind))
            .find_map(|t| str_at(t, "Text").and_then(clean_description))
    });

    // 書影
    let mut cover_urls: Vec<String> = Vec::new();
    if let Some(url) = summary
        .and_then(|s| str_at(s, "cover"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        cover_urls.push(upgrade_to_https(url));
    }
    for resource in
        one_or_many(onix.and_then(|o| o.pointer("/CollateralDetail/SupportingResource")))
    {
        if str_at(resource, "ResourceContentType").is_some_and(|t| t != "01") {
            continue;
        }
        for version in one_or_many(resource.get("ResourceVersion")) {
            if let Some(link) = str_at(version, "ResourceLink")
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let url = upgrade_to_https(link);
                if !cover_urls.contains(&url) {
                    cover_urls.push(url);
                }
            }
        }
    }
    c.covers = cover_urls
        .into_iter()
        .map(|url| CandidateCover {
            provider: ProviderId::Openbd,
            url,
        })
        .collect();

    Ok(Some(c))
}
