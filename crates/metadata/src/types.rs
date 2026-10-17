//! 取得元・項目・候補の型。
//!
//! IPC で使う型には `#[ts(export)]` を付けている（`cargo test` で `src/bindings/` に出る）。
//! コア側の型名と重ならないよう、名前は `Candidate`・`Meta`・`Provider` で始める。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 書誌・書影の取得元。文字列の形（`openbd` など）は `provider_cache.provider` にそのまま使える。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderId {
    Openbd,
    Ndl,
    Google,
    /// Open Library。serde の既定（snake_case）では `open_library` になるので、`as_str()` に合わせた名前にする。
    #[serde(rename = "openlibrary")]
    OpenLibrary,
}

impl ProviderId {
    pub const ALL: [ProviderId; 4] = [
        ProviderId::Openbd,
        ProviderId::Ndl,
        ProviderId::Google,
        ProviderId::OpenLibrary,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Openbd => "openbd",
            ProviderId::Ndl => "ndl",
            ProviderId::Google => "google",
            ProviderId::OpenLibrary => "openlibrary",
        }
    }

    /// `as_str` の逆。知らない文字列は `None`。
    pub fn parse(s: &str) -> Option<ProviderId> {
        ProviderId::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// 値の出どころ。取得元に加えて、2段目のバーコード（`barcode`）がある。
/// `books.field_sources_json` に入れる文字列は `as_str()` と同じ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MetaSource {
    Barcode,
    Openbd,
    Ndl,
    Google,
    /// serde の名前は `ProviderId::OpenLibrary` と同じ（`openlibrary`）。
    #[serde(rename = "openlibrary")]
    OpenLibrary,
}

impl MetaSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MetaSource::Barcode => "barcode",
            MetaSource::Openbd => "openbd",
            MetaSource::Ndl => "ndl",
            MetaSource::Google => "google",
            MetaSource::OpenLibrary => "openlibrary",
        }
    }
}

impl From<ProviderId> for MetaSource {
    fn from(p: ProviderId) -> Self {
        match p {
            ProviderId::Openbd => MetaSource::Openbd,
            ProviderId::Ndl => MetaSource::Ndl,
            ProviderId::Google => MetaSource::Google,
            ProviderId::OpenLibrary => MetaSource::OpenLibrary,
        }
    }
}

/// 書籍の項目。名前（snake_case）は `books` の列名と、`field_sources.<項目名>` のキーに合わせた。
/// `series` は系列名（`series_id` ではない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MetaField {
    Title,
    Subtitle,
    TitleKana,
    Authors,
    Publisher,
    PublishedOn,
    ListPrice,
    RefPriceIncl,
    CCode,
    Ndc,
    Pages,
    SizeText,
    Language,
    Description,
    Series,
    Volume,
    Cover,
}

impl MetaField {
    /// 統合の処理順。
    pub const ALL: [MetaField; 17] = [
        MetaField::Title,
        MetaField::Subtitle,
        MetaField::TitleKana,
        MetaField::Authors,
        MetaField::Publisher,
        MetaField::PublishedOn,
        MetaField::ListPrice,
        MetaField::RefPriceIncl,
        MetaField::CCode,
        MetaField::Ndc,
        MetaField::Pages,
        MetaField::SizeText,
        MetaField::Language,
        MetaField::Description,
        MetaField::Series,
        MetaField::Volume,
        MetaField::Cover,
    ];

    /// 優先順位の設定単位（要件書 FR-META-04 の表の行）。
    pub fn group(self) -> MetaGroup {
        match self {
            MetaField::Title | MetaField::Subtitle => MetaGroup::Title,
            MetaField::TitleKana => MetaGroup::TitleKana,
            MetaField::Authors => MetaGroup::Authors,
            MetaField::Publisher => MetaGroup::Publisher,
            MetaField::PublishedOn => MetaGroup::PublishedOn,
            MetaField::ListPrice => MetaGroup::ListPrice,
            MetaField::RefPriceIncl => MetaGroup::RefPriceIncl,
            MetaField::CCode => MetaGroup::CCode,
            MetaField::Ndc => MetaGroup::Ndc,
            MetaField::Pages | MetaField::SizeText => MetaGroup::PagesAndSize,
            MetaField::Language => MetaGroup::Language,
            MetaField::Description => MetaGroup::Description,
            MetaField::Series | MetaField::Volume => MetaGroup::Series,
            MetaField::Cover => MetaGroup::Cover,
        }
    }

    /// `field_sources` / `manual_fields` のキーに使う名前。
    pub fn key(self) -> &'static str {
        match self {
            MetaField::Title => "title",
            MetaField::Subtitle => "subtitle",
            MetaField::TitleKana => "title_kana",
            MetaField::Authors => "authors",
            MetaField::Publisher => "publisher",
            MetaField::PublishedOn => "published_on",
            MetaField::ListPrice => "list_price",
            MetaField::RefPriceIncl => "ref_price_incl",
            MetaField::CCode => "c_code",
            MetaField::Ndc => "ndc",
            MetaField::Pages => "pages",
            MetaField::SizeText => "size_text",
            MetaField::Language => "language",
            MetaField::Description => "description",
            MetaField::Series => "series",
            MetaField::Volume => "volume",
            MetaField::Cover => "cover",
        }
    }
}

/// 優先順位を設定する単位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MetaGroup {
    /// 書名・副書名（同じ取得元から取る）
    Title,
    TitleKana,
    Authors,
    Publisher,
    PublishedOn,
    ListPrice,
    RefPriceIncl,
    CCode,
    Ndc,
    PagesAndSize,
    /// シリーズ・巻次
    Series,
    Language,
    Description,
    Cover,
}

impl MetaGroup {
    pub const ALL: [MetaGroup; 14] = [
        MetaGroup::Title,
        MetaGroup::TitleKana,
        MetaGroup::Authors,
        MetaGroup::Publisher,
        MetaGroup::PublishedOn,
        MetaGroup::ListPrice,
        MetaGroup::RefPriceIncl,
        MetaGroup::CCode,
        MetaGroup::Ndc,
        MetaGroup::PagesAndSize,
        MetaGroup::Series,
        MetaGroup::Language,
        MetaGroup::Description,
        MetaGroup::Cover,
    ];
}

/// 著者 1 人分。`role` は「著」「訳」「英訳」「監修」など（原文の表記を正規化したもの）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CandidateAuthor {
    pub name: String,
    pub kana: Option<String>,
    pub role: Option<String>,
}

/// 書影の URL。どの取得元の URL かを持つ（レート制限とダミー画像の判定に使う）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CandidateCover {
    pub provider: ProviderId,
    pub url: String,
}

/// 1 つの取得元から得た書誌情報。値は正規化済み（`normalize` モジュール）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Candidate {
    pub provider: ProviderId,
    pub isbn13: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub title_kana: Option<String>,
    pub authors: Vec<CandidateAuthor>,
    pub publisher: Option<String>,
    /// `YYYY` / `YYYY-MM` / `YYYY-MM-DD`
    pub published_on: Option<String>,
    /// 本体価格（税抜・円）
    pub list_price: Option<i64>,
    /// 参考の税込価格（円）。「(税込)」と明記されているものだけ入る。
    pub ref_price_incl: Option<i64>,
    /// 4 桁
    pub c_code: Option<String>,
    pub ndc: Option<String>,
    pub pages: Option<u32>,
    /// 例: `15cm`
    pub size_text: Option<String>,
    /// ISO 639-1（`ja` など）。分からなければ原文のコード。
    pub language: Option<String>,
    pub description: Option<String>,
    pub series: Option<String>,
    pub volume: Option<String>,
    pub covers: Vec<CandidateCover>,
}

impl Candidate {
    pub fn empty(provider: ProviderId, isbn13: &str) -> Self {
        Candidate {
            provider,
            isbn13: isbn13.to_string(),
            title: None,
            subtitle: None,
            title_kana: None,
            authors: Vec::new(),
            publisher: None,
            published_on: None,
            list_price: None,
            ref_price_incl: None,
            c_code: None,
            ndc: None,
            pages: None,
            size_text: None,
            language: None,
            description: None,
            series: None,
            volume: None,
            covers: Vec::new(),
        }
    }

    /// 項目の値。空（`None`・空文字・空の一覧）は `None`。
    pub fn value(&self, field: MetaField) -> Option<MetaValue> {
        fn text(v: &Option<String>) -> Option<MetaValue> {
            v.as_ref()
                .filter(|s| !s.is_empty())
                .map(|s| MetaValue::Text(s.clone()))
        }
        match field {
            MetaField::Title => text(&self.title),
            MetaField::Subtitle => text(&self.subtitle),
            MetaField::TitleKana => text(&self.title_kana),
            MetaField::Authors => {
                (!self.authors.is_empty()).then(|| MetaValue::Authors(self.authors.clone()))
            }
            MetaField::Publisher => text(&self.publisher),
            MetaField::PublishedOn => text(&self.published_on),
            MetaField::ListPrice => self.list_price.map(MetaValue::Number),
            MetaField::RefPriceIncl => self.ref_price_incl.map(MetaValue::Number),
            MetaField::CCode => text(&self.c_code),
            MetaField::Ndc => text(&self.ndc),
            MetaField::Pages => self.pages.map(|p| MetaValue::Number(i64::from(p))),
            MetaField::SizeText => text(&self.size_text),
            MetaField::Language => text(&self.language),
            MetaField::Description => text(&self.description),
            MetaField::Series => text(&self.series),
            MetaField::Volume => text(&self.volume),
            MetaField::Cover => {
                (!self.covers.is_empty()).then(|| MetaValue::Covers(self.covers.clone()))
            }
        }
    }
}

/// キーワード検索の結果の 1 件。ISBN があれば、`candidate.isbn13` に同じ値が入る（なければ空文字）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchHit {
    pub provider: ProviderId,
    /// ISBN-13（チェックデジットが合うものだけ。ISBN-10 は 13 桁に直す）。ISBN がない本は null。
    pub isbn13: Option<String>,
    pub candidate: Candidate,
    /// その本の、取得元のサイト上のページ（https のみ）。
    pub link: Option<String>,
}

/// キーワード検索の結果。`hits` は取得元が返した順（NDL は新しい順に並べ直してある）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchResults {
    /// 該当の総件数（取得元が教えてくれるとき。`hits` より多いことがある）。
    pub total: Option<u64>,
    pub hits: Vec<SearchHit>,
}

/// 統合結果の 1 項目の値。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
#[ts(export)]
pub enum MetaValue {
    Text(String),
    Number(i64),
    Authors(Vec<CandidateAuthor>),
    Covers(Vec<CandidateCover>),
}

/// 2段目のバーコードから分かった値（FR-META-04: 本体価格と Cコードで最優先）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaSecondTier {
    /// 4 桁
    pub c_code: Option<String>,
    /// 本体価格（円）。「不明」は `None`。
    pub list_price: Option<i64>,
}
