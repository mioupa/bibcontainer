//! Open Library（https://openlibrary.org/）。API キーは要らない。
//!
//! 書誌は 2 回の通信で取る（検索が先）。
//!
//! 1. `GET https://openlibrary.org/search.json?isbn=<ISBN-13>&fields=…` — ISBN から作品（work）と、その ISBN の
//!    版（edition）を探す。`numFound` が 0（`docs` が空）なら該当なしで、2 回目の通信はしない。
//!    検索の応答の `editions.docs` から、ISBN を持つ版を選ぶ。版が 1 件もなければ該当なしとして扱う。
//! 2. `GET https://openlibrary.org<版の key>.json`（例 `/books/OL26222911M.json`）— ページ数と内容紹介のために、
//!    版の JSON を取る。404 と、`type.key` が `/type/redirect` の応答（統合された版）は、検索の結果だけで続ける
//!    （リダイレクト先は追わない）。そのほかのエラー（通信エラー・5xx・429 など）は、取得全体の失敗にする
//!    （キャッシュされず、あとで取り直される）。
//!
//! キャッシュに残す生の応答（`RawResponse.body`）は、この取得元の包み（`{"search": 検索の JSON, "edition": 版の JSON または null}`）。
//! `parse` はこの包みだけを読むので、キャッシュからのやり直しは通信なしでできる（FR-META-10）。
//!
//! 書影は、版の JSON の `covers`（`-1` が入ることがある）の最初の正の ID、なければ検索の `cover_i` から、
//! `https://covers.openlibrary.org/b/id/<ID>-L.jpg?default=false` を組み立てる。書影の ID を指定した取得には、
//! Open Library のレート制限（ISBN を指定した書影 URL にある制限）がない。`default=false` は、書影がないとき
//! 空白の画像ではなく 404 を返させる。
//!
//! Open Library はアプリからのリンク（クレジット）を求めている。レート制限は、書誌・書影とも同じグループ
//! （`openlibrary`、1 秒に 1 件、同時接続 1）。
//!
//! # キーワード検索（フェーズ 3）
//!
//! `GET /search.json?title=<書名>&author=<著者>&fields=…&limit=20`（あるものだけ）。結果は Open Library の順のまま、
//! 作品（work）1 件につき 1 件。版は、検索に合った先頭の 1 件（`editions.docs[0]`。`fields` で `editions.*` を頼む）で、
//! 書名・副書名・著者・出版社・発行日・言語・ISBN・書影をそこから取る（ない項目は作品の `title`・`subtitle`・
//! `author_name`・`cover_i` で補う。版がなければ作品の項目だけ）。作品全体の `isbn`（ほかの版の ISBN が混ざる）は頼まず、
//! 使わない。ISBN は版の `isbn` から（13 桁を先に、なければ 10 桁を直す）。本のページ（`link`）は
//! `https://openlibrary.org<版の key>`（`/books/OL…M`）、版の key がなければ作品の key（`/works/OL…W`）。
//! 通信は書誌と同じレート制限のグループ（`openlibrary`）。
//!
//! 日付は自由な英語の書式（`July 2008`、`Mar 05, 2016`、`c1995`、`[2003]` など）で、`normalize_date` が読む。

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::error::{FetchError, ParseError, SearchError};
use crate::http::{HttpRequest, percent_encode};
use crate::isbn::{is_valid_isbn13, to_isbn13};
use crate::normalize::{
    clean_description, clean_text, normalize_date, normalize_language, parse_extent, strip_html,
};
use crate::provider::{Provider, ProviderAuth, ProviderInfo, RawResponse, SearchQuery};
use crate::transport::{ProviderRateLimit, Transport, http_error};
use crate::types::{
    Candidate, CandidateAuthor, CandidateCover, MetaField, ProviderId, SearchHit, SearchResults,
};

pub const DEFAULT_BASE_URL: &str = "https://openlibrary.org";
pub const DEFAULT_COVERS_BASE_URL: &str = "https://covers.openlibrary.org";

/// 検索で取る項目。作品の `key`・`title`・`author_name` と、版（`editions`）の項目。
const SEARCH_FIELDS: &str = "key,title,author_name,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.publisher,editions.publish_date,editions.language,editions.cover_i,editions.isbn";

/// キーワード検索で取る項目。作品の `key`・`title`・`subtitle`・`author_name`・`cover_i` と、版（`editions`）の項目。
/// 作品全体の `isbn` は頼まない（ほかの版の ISBN が混ざっていて、表示する版の ISBN と食い違う）。
const KEYWORD_FIELDS: &str = "key,title,subtitle,author_name,cover_i,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.isbn,editions.publisher,editions.publish_date,editions.language,editions.cover_i";
/// キーワード検索で 1 回に取る作品の数。
const KEYWORD_LIMIT: u32 = 20;

pub struct OpenLibrary {
    base_url: String,
    covers_base_url: String,
}

impl Default for OpenLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenLibrary {
    pub fn new() -> Self {
        OpenLibrary {
            base_url: DEFAULT_BASE_URL.to_string(),
            covers_base_url: DEFAULT_COVERS_BASE_URL.to_string(),
        }
    }

    /// テスト用に接続先を変える。書影の URL も同じ接続先（`<接続先>/b/id/<ID>-L.jpg?default=false`）になる。
    pub fn with_base_url(base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        OpenLibrary {
            covers_base_url: base_url.clone(),
            base_url,
        }
    }

    /// 書誌と書影を別の接続先にするテスト用。
    pub fn with_base_urls(base_url: &str, covers_base_url: &str) -> Self {
        OpenLibrary {
            base_url: base_url.trim_end_matches('/').to_string(),
            covers_base_url: covers_base_url.trim_end_matches('/').to_string(),
        }
    }

    /// 1 秒に 1 件、同時接続 1。書影のダウンロード（covers.openlibrary.org）も同じグループで数える。
    pub(crate) fn rate_limit() -> ProviderRateLimit {
        ProviderRateLimit::new("openlibrary", 1000, 1)
    }

    fn search_url(&self, isbn13: &str) -> String {
        format!(
            "{}/search.json?isbn={}&fields={}",
            self.base_url, isbn13, SEARCH_FIELDS
        )
    }

    /// キーワード検索の URL。書名・著者はあるものだけ入れる。
    fn keyword_url(&self, query: &SearchQuery) -> String {
        let mut params: Vec<String> = Vec::new();
        if let Some(title) = &query.title {
            params.push(format!("title={}", percent_encode(title)));
        }
        if let Some(author) = &query.author {
            params.push(format!("author={}", percent_encode(author)));
        }
        params.push(format!("fields={KEYWORD_FIELDS}"));
        params.push(format!("limit={KEYWORD_LIMIT}"));
        format!("{}/search.json?{}", self.base_url, params.join("&"))
    }

    fn cover_url(&self, id: i64) -> String {
        format!("{}/b/id/{}-L.jpg?default=false", self.covers_base_url, id)
    }

    /// 版の JSON を取る。404 と、統合された版（`/type/redirect`）は `None`（検索の結果だけで続ける）。
    async fn fetch_edition(
        &self,
        transport: &Transport,
        key: &str,
    ) -> Result<Option<Value>, FetchError> {
        let url = format!("{}{}.json", self.base_url, key);
        let resp = transport
            .get(&Self::rate_limit(), HttpRequest::get(url))
            .await?;
        if resp.status == 404 {
            return Ok(None);
        }
        if !resp.is_success() {
            return Err(http_error(&resp));
        }
        let v: Value = serde_json::from_slice(&resp.body).map_err(|e| {
            FetchError::Parse(format!(
                "Open Library の版の応答が JSON ではありません: {e}"
            ))
        })?;
        Ok((!is_redirect(&v)).then_some(v))
    }
}

#[async_trait]
impl Provider for OpenLibrary {
    fn id(&self) -> ProviderId {
        ProviderId::OpenLibrary
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ProviderId::OpenLibrary,
            display_name: "Open Library".to_string(),
            auth: ProviderAuth::None,
            fields: vec![
                MetaField::Title,
                MetaField::Subtitle,
                MetaField::Authors,
                MetaField::Publisher,
                MetaField::PublishedOn,
                MetaField::Pages,
                MetaField::Language,
                MetaField::Description,
                MetaField::Cover,
            ],
            rate_limit: Self::rate_limit(),
            credit: "書誌情報・書影: Open Library (https://openlibrary.org/)".to_string(),
            supports_batch: false,
            supports_search: true,
            needs_network: true,
        }
    }

    async fn fetch_raw(
        &self,
        transport: &Transport,
        isbn13: &str,
    ) -> Result<RawResponse, FetchError> {
        let resp = transport
            .get_ok(
                &Self::rate_limit(),
                HttpRequest::get(self.search_url(isbn13)),
            )
            .await?;
        let search: Value = serde_json::from_slice(&resp.body).map_err(|e| {
            FetchError::Parse(format!("Open Library の応答が JSON ではありません: {e}"))
        })?;
        let docs = search_docs(&search)
            .ok_or_else(|| FetchError::Parse("Open Library の応答に docs がありません".into()))?;

        // 該当なし（docs が空）か、版が 1 件もない: 2 回目の通信はしない（parse が該当なしにする）
        let edition_key = match docs
            .first()
            .and_then(|work| pick_search_edition(work, isbn13))
        {
            None => None,
            Some(doc) => {
                let key = doc.get("key").and_then(Value::as_str).unwrap_or_default();
                if !is_edition_key(key) {
                    return Err(FetchError::Parse(format!(
                        "Open Library の版の key が想定の形ではありません: {key:?}"
                    )));
                }
                Some(key.to_string())
            }
        };
        let edition = match edition_key {
            Some(key) => self.fetch_edition(transport, &key).await?,
            None => None,
        };
        Ok(RawResponse {
            provider: ProviderId::OpenLibrary,
            isbn13: isbn13.to_string(),
            body: json!({ "search": search, "edition": edition }).to_string(),
        })
    }

    fn parse(&self, raw: &RawResponse) -> Result<Option<Candidate>, ParseError> {
        self.parse_envelope(&raw.isbn13, &raw.body)
    }

    async fn search(
        &self,
        transport: &Transport,
        query: &SearchQuery,
    ) -> Result<SearchResults, SearchError> {
        let query = query.validated()?;
        let resp = transport
            .get_ok(
                &Self::rate_limit(),
                HttpRequest::get(self.keyword_url(&query)),
            )
            .await?;
        self.parse_keyword_search(&resp.text())
            .map_err(|e| FetchError::Parse(e.0).into())
    }
}

/// 検索の応答の作品の一覧（`docs`）。`docs` がなくても `numFound` が 0 なら、該当なし（空の一覧）として読む。
fn search_docs(search: &Value) -> Option<&[Value]> {
    match search.get("docs").and_then(Value::as_array) {
        Some(docs) => Some(docs),
        None if search.get("numFound").and_then(Value::as_u64) == Some(0) => Some(&[]),
        None => None,
    }
}

/// 版の key（`/books/OL26222911M`）の形か。URL に入れるので、想定の形だけを通す。
fn is_edition_key(key: &str) -> bool {
    key.strip_prefix("/books/OL")
        .and_then(|rest| rest.strip_suffix('M'))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// 記録（作品・版）の `key`。
fn key_of(record: &Value) -> Option<&str> {
    record.get("key").and_then(Value::as_str)
}

/// 作品の key（`/works/OL17618370W`）の形か。URL に入れるので、想定の形だけを通す。
fn is_work_key(key: &str) -> bool {
    key.strip_prefix("/works/OL")
        .and_then(|rest| rest.strip_suffix('W'))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// 版の `isbn`（10 桁と 13 桁が混ざる）から ISBN-13 を 1 つ。チェックデジットが合う 13 桁を先に、なければ 10 桁を直す。
fn edition_isbn13(edition: &Value) -> Option<String> {
    let isbns: Vec<&str> = edition
        .get("isbn")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    isbns
        .iter()
        .find(|s| is_valid_isbn13(s))
        .map(|s| (*s).to_string())
        .or_else(|| isbns.iter().find_map(|s| to_isbn13(s)))
}

/// 統合されて別の版に移った記録（`type.key` が `/type/redirect`）か。
fn is_redirect(edition: &Value) -> bool {
    edition.pointer("/type/key").and_then(Value::as_str) == Some("/type/redirect")
}

/// 検索の作品（`docs[0]`）から、版の項目を選ぶ。ISBN（ISBN-10 の表記も）が合う版、なければ先頭の版。版がなければ `None`。
fn pick_search_edition<'a>(work: &'a Value, isbn13: &str) -> Option<&'a Value> {
    let docs = work.pointer("/editions/docs")?.as_array()?;
    docs.iter()
        .find(|d| {
            d.get("isbn")
                .and_then(Value::as_array)
                .is_some_and(|isbns| {
                    isbns
                        .iter()
                        .filter_map(Value::as_str)
                        .filter_map(to_isbn13)
                        .any(|x| x == isbn13)
                })
        })
        .or_else(|| docs.first())
}

/// 文字列の配列の、最初に使える（空でない）要素。
fn first_text(v: Option<&Value>) -> Option<String> {
    v?.as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find_map(clean_text)
}

/// 著者名の配列。名前だけ（役割・ヨミなし）。Google と同じく、役割の分離はしない。
fn names(v: Option<&Value>) -> Vec<CandidateAuthor> {
    let mut out: Vec<CandidateAuthor> = Vec::new();
    for name in v.and_then(Value::as_array).into_iter().flatten() {
        if let Some(name) = name.as_str().and_then(clean_text) {
            let a = CandidateAuthor {
                name,
                kana: None,
                role: None,
            };
            if !out.contains(&a) {
                out.push(a);
            }
        }
    }
    out
}

/// 版の JSON の `description`。文字列か、`{ "type": "/type/text", "value": "…" }`。
fn edition_description(edition: &Value) -> Option<String> {
    let d = edition.get("description")?;
    let text = match d {
        Value::String(s) => s.as_str(),
        Value::Object(_) => d.get("value")?.as_str()?,
        _ => return None,
    };
    clean_description(&strip_html(text))
}

/// 版の JSON の `title_prefix`（`The `）を付けた書名。
fn edition_title(edition: &Value) -> Option<String> {
    let title = edition.get("title").and_then(Value::as_str)?;
    let prefix = edition
        .get("title_prefix")
        .and_then(Value::as_str)
        .unwrap_or_default();
    clean_text(&format!("{prefix}{title}"))
}

fn positive(v: &Value) -> Option<i64> {
    v.as_i64().filter(|n| *n > 0)
}

impl OpenLibrary {
    /// キーワード検索の応答を結果にする。作品 1 件が 1 件の結果で、順番も件数も Open Library のまま。
    fn parse_keyword_search(&self, body: &str) -> Result<SearchResults, ParseError> {
        let search: Value = serde_json::from_str(body).map_err(|e| {
            ParseError::new(format!("Open Library の応答が JSON ではありません: {e}"))
        })?;
        let docs = search_docs(&search)
            .ok_or_else(|| ParseError::new("Open Library の応答に docs がありません"))?;
        Ok(SearchResults {
            total: search.get("numFound").and_then(Value::as_u64),
            hits: docs.iter().map(|work| self.hit_from_work(work)).collect(),
        })
    }

    /// 検索の作品 1 件から、結果 1 件を作る。版は検索に合った先頭の 1 件（なければ作品の項目だけ）。
    fn hit_from_work(&self, work: &Value) -> SearchHit {
        let edition = work.pointer("/editions/docs/0").filter(|e| e.is_object());
        let isbn13 = edition.and_then(edition_isbn13);
        let mut c = Candidate::empty(ProviderId::OpenLibrary, isbn13.as_deref().unwrap_or(""));

        // 書名と副書名は、同じ記録から取る（版、なければ作品）
        let text =
            |rec: &Value, key: &str| rec.get(key).and_then(Value::as_str).and_then(clean_text);
        match edition.and_then(edition_title) {
            Some(title) => {
                c.title = Some(title);
                c.subtitle = edition.and_then(|e| text(e, "subtitle"));
            }
            None => {
                c.title = text(work, "title");
                c.subtitle = text(work, "subtitle");
            }
        }
        c.authors = names(edition.and_then(|e| e.get("author_name")));
        if c.authors.is_empty() {
            c.authors = names(work.get("author_name"));
        }
        c.publisher = first_text(edition.and_then(|e| e.get("publisher")));
        c.published_on = edition
            .and_then(|e| e.get("publish_date"))
            .and_then(Value::as_array)
            .and_then(|dates| {
                dates
                    .iter()
                    .filter_map(Value::as_str)
                    .find_map(normalize_date)
            });
        c.language = edition
            .and_then(|e| e.pointer("/language/0"))
            .and_then(Value::as_str)
            .and_then(normalize_language);
        let cover_id = edition
            .and_then(|e| e.get("cover_i"))
            .and_then(positive)
            .or_else(|| work.get("cover_i").and_then(positive));
        if let Some(id) = cover_id {
            c.covers.push(CandidateCover {
                provider: ProviderId::OpenLibrary,
                url: self.cover_url(id),
            });
        }

        // ページは、サイトの本物の URL（テストで接続先を変えても、利用者のブラウザで開く先は変わらない）
        let link = edition
            .and_then(key_of)
            .filter(|k| is_edition_key(k))
            .or_else(|| key_of(work).filter(|k| is_work_key(k)))
            .map(|k| format!("{DEFAULT_BASE_URL}{k}"));
        SearchHit {
            provider: ProviderId::OpenLibrary,
            isbn13,
            candidate: c,
            link,
        }
    }

    /// 生の応答（検索と版の包み）を候補にする。
    fn parse_envelope(&self, isbn13: &str, body: &str) -> Result<Option<Candidate>, ParseError> {
        let env: Value = serde_json::from_str(body).map_err(|e| {
            ParseError::new(format!("Open Library の応答が JSON ではありません: {e}"))
        })?;
        let docs = env
            .get("search")
            .and_then(search_docs)
            .ok_or_else(|| ParseError::new("Open Library の応答に docs がありません"))?;
        let Some(work) = docs.first() else {
            return Ok(None);
        };
        let Some(doc) = pick_search_edition(work, isbn13) else {
            return Ok(None);
        };
        // 統合された版（redirect）の記録は使わない
        let edition = env
            .get("edition")
            .filter(|e| e.is_object() && !is_redirect(e));

        let mut c = Candidate::empty(ProviderId::OpenLibrary, isbn13);

        // 書名と副書名は、同じ記録から取る（版の JSON、なければ検索の版、なければ作品）
        let doc_text = |key: &str| doc.get(key).and_then(Value::as_str).and_then(clean_text);
        if let Some(title) = edition.and_then(edition_title) {
            c.title = Some(title);
            c.subtitle = edition
                .and_then(|e| e.get("subtitle"))
                .and_then(Value::as_str)
                .and_then(clean_text);
        } else if let Some(title) = doc_text("title") {
            c.title = Some(title);
            c.subtitle = doc_text("subtitle");
        } else {
            c.title = work
                .get("title")
                .and_then(Value::as_str)
                .and_then(clean_text);
        }

        c.authors = names(doc.get("author_name"));
        if c.authors.is_empty() {
            c.authors = names(work.get("author_name"));
        }

        c.publisher = first_text(edition.and_then(|e| e.get("publishers")))
            .or_else(|| first_text(doc.get("publisher")));

        c.published_on = edition
            .and_then(|e| e.get("publish_date"))
            .and_then(Value::as_str)
            .and_then(normalize_date)
            .or_else(|| {
                doc.get("publish_date")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                    .and_then(normalize_date)
            });

        c.pages = edition
            .and_then(|e| e.get("number_of_pages"))
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok())
            .filter(|p| *p > 0)
            .or_else(|| {
                edition
                    .and_then(|e| e.get("pagination"))
                    .and_then(Value::as_str)
                    .and_then(|p| parse_extent(p).pages)
            });

        c.language = edition
            .and_then(|e| e.pointer("/languages/0/key"))
            .and_then(Value::as_str)
            .map(|k| k.rsplit('/').next().unwrap_or(k))
            .and_then(normalize_language)
            .or_else(|| {
                doc.pointer("/language/0")
                    .and_then(Value::as_str)
                    .and_then(normalize_language)
            });

        c.description = edition.and_then(edition_description);

        // 書影: 版の JSON の covers（-1 が入ることがある）の最初の正の ID、なければ検索の cover_i
        let cover_id = edition
            .and_then(|e| e.get("covers"))
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find_map(positive))
            .or_else(|| doc.get("cover_i").and_then(positive));
        if let Some(id) = cover_id {
            c.covers.push(CandidateCover {
                provider: ProviderId::OpenLibrary,
                url: self.cover_url(id),
            });
        }
        Ok(Some(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edition_keys_are_checked_before_they_go_into_a_url() {
        for ok in ["/books/OL26222911M", "/books/OL1M"] {
            assert!(is_edition_key(ok), "{ok}");
        }
        for bad in [
            "",
            "/books/OLM",
            "/books/OL12W",
            "/works/OL12M",
            "/books/OL12M/../x",
            "/books/OL12M?x=1",
            "https://evil.example/books/OL1M",
            "books/OL1M",
        ] {
            assert!(!is_edition_key(bad), "{bad}");
        }
    }

    #[test]
    fn urls_are_built_from_the_base_urls() {
        let p = OpenLibrary::new();
        assert_eq!(
            p.search_url("9780132350884"),
            "https://openlibrary.org/search.json?isbn=9780132350884&fields=key,title,author_name,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.publisher,editions.publish_date,editions.language,editions.cover_i,editions.isbn"
        );
        assert_eq!(
            p.cover_url(15126503),
            "https://covers.openlibrary.org/b/id/15126503-L.jpg?default=false"
        );
        let p = OpenLibrary::with_base_url("http://127.0.0.1:1234/");
        assert_eq!(
            p.cover_url(7),
            "http://127.0.0.1:1234/b/id/7-L.jpg?default=false"
        );
    }
}
