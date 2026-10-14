//! Google Books（`https://www.googleapis.com/books/v1/volumes?q=isbn:<ISBN>`）。
//!
//! API キーは呼び出し側（OS のキーチェーン）が渡す。このクレートはキーを保存せず、ログにも出さない。
//!
//! 2026-10-14 の確認では、キーなしのリクエストは共有プロジェクトの割り当て（`quota_limit_value` が 0）に当たり、
//! 常に 429 になった。実際にはキーが必要とみてよい。詳しくは `docs/decisions/0007-metadata-providers.md`。

use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;

use crate::error::{FetchError, ParseError};
use crate::http::HttpRequest;
use crate::isbn::to_isbn13;
use crate::normalize::{
    clean_description, clean_text, normalize_date, normalize_language, upgrade_to_https,
};
use crate::provider::{Provider, ProviderAuth, ProviderInfo, RawResponse};
use crate::transport::{ProviderRateLimit, Transport, http_error};
use crate::types::{Candidate, CandidateAuthor, CandidateCover, MetaField, ProviderId};

pub const DEFAULT_BASE_URL: &str = "https://www.googleapis.com";

pub struct GoogleBooks {
    base_url: String,
    api_key: Option<String>,
}

impl GoogleBooks {
    pub fn new(api_key: Option<String>) -> Self {
        GoogleBooks {
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
        }
    }

    /// テスト用に接続先を変える。
    pub fn with_base_url(base_url: &str, api_key: Option<String>) -> Self {
        GoogleBooks {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
        }
    }

    pub(crate) fn rate_limit() -> ProviderRateLimit {
        // 1 秒に 1 件（FR-META-08）。書影のダウンロード（books.google.com）も同じグループで数える
        ProviderRateLimit::new("google", 1000, 1)
    }

    fn request_url(&self, isbn13: &str) -> String {
        let mut url = format!(
            "{}/books/v1/volumes?q=isbn:{}&maxResults=5&printType=books",
            self.base_url, isbn13
        );
        if let Some(key) = &self.api_key {
            url.push_str("&key=");
            url.push_str(&percent_encode(key));
        }
        url
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[async_trait]
impl Provider for GoogleBooks {
    fn id(&self) -> ProviderId {
        ProviderId::Google
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ProviderId::Google,
            display_name: "Google Books".to_string(),
            // キーなしのリクエストは 429（割り当て 0）で必ず失敗するので、キーは必須（0007）
            auth: ProviderAuth::Required {
                label: "API キー".to_string(),
            },
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
            credit: "Powered by Google Books API (https://books.google.com/)".to_string(),
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
        let resp = transport
            .get(
                &Self::rate_limit(),
                HttpRequest::get(self.request_url(isbn13)),
            )
            .await?;
        if !resp.is_success() {
            return Err(google_http_error(&resp, self.api_key.is_some()));
        }
        Ok(RawResponse {
            provider: ProviderId::Google,
            isbn13: isbn13.to_string(),
            body: resp.text(),
        })
    }

    fn parse(&self, raw: &RawResponse) -> Result<Option<Candidate>, ParseError> {
        parse_google(&raw.isbn13, &raw.body)
    }
}

/// 429 で、割り当ての上限が 0 のとき（キーなしの共有プロジェクトなど）に、メッセージの末尾へ付ける印。
/// 応答の `error.details[].metadata.quota_limit_value` が `"0"` のとき。待っても回復しないので、
/// 「1 日の上限に達した」（上限が 0 より大きい）と区別できるようにする（`classify_failure`）。
pub const ZERO_QUOTA_NOTE: &str = "（割り当ての上限は 0 です）";

/// Google Books の HTTP エラーの意味づけ（設定画面の接続テストの表示用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoogleFailure {
    /// API キーが無効・期限切れ（400 `API_KEY_INVALID`）、または認証・権限のエラー（401・403。Books API が
    /// 有効になっていない、キーの利用制限に当たった、など）
    InvalidKey,
    /// 429 で、1 日あたりの上限に達した（キーは通っている。待てば戻る）
    DailyQuota,
    /// 429 で、割り当ての上限が 0（キーなし、または割り当てのないプロジェクト）
    ZeroQuota,
    /// 上のどれでもない
    Other,
}

/// `FetchError::Http` の `status` と `message`（`google_http_error` が作ったもの）から、意味を決める。
pub fn classify_failure(status: u16, message: &str) -> GoogleFailure {
    let lower = message.to_ascii_lowercase();
    match status {
        429 if message.contains(ZERO_QUOTA_NOTE) => GoogleFailure::ZeroQuota,
        429 if lower.contains("per day") => GoogleFailure::DailyQuota,
        // 403 でも、1 日の上限（旧形式の `dailyLimitExceeded` など）なら上限として扱う
        403 if lower.contains("per day") || lower.contains("daily limit") => {
            GoogleFailure::DailyQuota
        }
        401 | 403 => GoogleFailure::InvalidKey,
        // キーが無効だと、401 ではなく 400（`INVALID_ARGUMENT`、reason `API_KEY_INVALID`）が返る
        400 if lower.contains("api key not valid")
            || lower.contains("api key expired")
            || lower.contains("api_key_invalid") =>
        {
            GoogleFailure::InvalidKey
        }
        _ => GoogleFailure::Other,
    }
}

/// 429・403 のとき、API キーの案内を付ける。メッセージは応答の `error.message`。
/// 429 で割り当ての上限が 0 のときは、`ZERO_QUOTA_NOTE` も付ける。
fn google_http_error(resp: &crate::http::HttpResponse, has_key: bool) -> FetchError {
    let body = serde_json::from_slice::<Value>(&resp.body).ok();
    let message = body.as_ref().and_then(|v| {
        v.pointer("/error/message")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let zero_quota = resp.status == 429
        && body
            .as_ref()
            .and_then(|v| v.pointer("/error/details"))
            .and_then(Value::as_array)
            .is_some_and(|details| {
                details.iter().any(|d| {
                    d.pointer("/metadata/quota_limit_value")
                        .and_then(Value::as_str)
                        == Some("0")
                })
            });
    match message {
        Some(m) => {
            let mut m: String = m.chars().take(200).collect();
            if matches!(resp.status, 429 | 403) && !has_key {
                m.push_str("（API キーを設定すると解消する場合があります）");
            }
            if zero_quota {
                m.push_str(ZERO_QUOTA_NOTE);
            }
            FetchError::Http {
                status: resp.status,
                message: crate::http::mask_secrets(&m),
            }
        }
        None => http_error(resp),
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

static BLOCK_BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>|</p>|</div>|</li>|</h[1-6]>").expect("正規表現"));
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("正規表現"));
static BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").expect("正規表現"));

/// Google の `description` に含まれる簡単な HTML（`<br>`、`<b>`、`&amp;`）を取り除く。
fn strip_html(s: &str) -> String {
    let t = BLOCK_BREAK.replace_all(s, "\n");
    let t = TAG.replace_all(&t, "");
    let t = t
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    let t: String = t.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    BLANK_LINES.replace_all(&t, "\n\n").into_owned()
}

fn identifiers(info: &Value) -> Vec<String> {
    info.get("industryIdentifiers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| i.get("identifier").and_then(Value::as_str))
                .filter_map(to_isbn13)
                .collect()
        })
        .unwrap_or_default()
}

/// サムネイル URL の `edge=curl`（ページをめくったような加工）を外し、https にする。
fn clean_cover_url(url: &str) -> String {
    let url = upgrade_to_https(url.trim());
    url.replace("&edge=curl", "")
        .replace("?edge=curl&", "?")
        .replace("?edge=curl", "")
}

pub(crate) fn parse_google(isbn13: &str, body: &str) -> Result<Option<Candidate>, ParseError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| ParseError::new(format!("Google Books の応答が JSON ではありません: {e}")))?;
    if v.get("error").is_some() {
        return Err(ParseError::new("Google Books がエラーを返しました"));
    }
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Ok(None);
    }
    // ISBN が合う項目を選ぶ。どの項目にも識別子がなければ先頭（要件書 11.3）。識別子があって合わなければ該当なし。
    let chosen = items
        .iter()
        .find(|i| {
            i.get("volumeInfo")
                .is_some_and(|info| identifiers(info).iter().any(|x| x == isbn13))
        })
        .or_else(|| {
            items.iter().find(|i| {
                i.get("volumeInfo")
                    .is_some_and(|info| identifiers(info).is_empty())
            })
        });
    let Some(info) = chosen.and_then(|i| i.get("volumeInfo")) else {
        return Ok(None);
    };

    let text = |key: &str| info.get(key).and_then(Value::as_str).and_then(clean_text);
    let mut c = Candidate::empty(ProviderId::Google, isbn13);
    c.title = text("title");
    c.subtitle = text("subtitle");
    c.publisher = text("publisher");
    c.published_on = info
        .get("publishedDate")
        .and_then(Value::as_str)
        .and_then(normalize_date);
    c.description = info
        .get("description")
        .and_then(Value::as_str)
        .map(strip_html)
        .and_then(|d| clean_description(&d));
    c.pages = info
        .get("pageCount")
        .and_then(Value::as_u64)
        .and_then(|p| u32::try_from(p).ok())
        .filter(|p| *p > 0);
    c.language = info
        .get("language")
        .and_then(Value::as_str)
        .and_then(normalize_language);
    // Google の著者は 1 要素が 1 人（「夏目 漱石」のように空白を含む）。役割の情報はない。
    for name in info
        .get("authors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = name.as_str().and_then(clean_text) {
            let a = CandidateAuthor {
                name,
                kana: None,
                role: None,
            };
            if !c.authors.contains(&a) {
                c.authors.push(a);
            }
        }
    }

    // 書影: thumbnail（zoom=1、幅 128px ほど）と、同じ URL の zoom=0（大きい画像）。大きい方を先にする。
    if let Some(links) = info.get("imageLinks") {
        let pick = |key: &str| links.get(key).and_then(Value::as_str).map(clean_cover_url);
        let mut urls: Vec<String> = Vec::new();
        for key in ["extraLarge", "large", "medium", "small"] {
            if let Some(u) = pick(key) {
                urls.push(u);
            }
        }
        if let Some(thumb) = pick("thumbnail").or_else(|| pick("smallThumbnail")) {
            if thumb.contains("zoom=1") {
                urls.push(thumb.replace("zoom=1", "zoom=0"));
            }
            urls.push(thumb);
        }
        let mut seen: Vec<String> = Vec::new();
        for u in urls {
            if !seen.contains(&u) {
                seen.push(u.clone());
                c.covers.push(CandidateCover {
                    provider: ProviderId::Google,
                    url: u,
                });
            }
        }
    }
    Ok(Some(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_stripped() {
        assert_eq!(
            strip_html("先生と私。<br><br><b>改版</b>。&quot;K&quot;と&amp;遺書"),
            "先生と私。\n\n改版。\"K\"と&遺書"
        );
        assert_eq!(strip_html("a<p>b</p><p>c</p>"), "ab\nc");
    }

    #[test]
    fn cover_urls_lose_the_curl_effect() {
        assert_eq!(
            clean_cover_url(
                "http://books.google.com/books/content?id=X&printsec=frontcover&img=1&zoom=1&edge=curl&source=gbs_api"
            ),
            "https://books.google.com/books/content?id=X&printsec=frontcover&img=1&zoom=1&source=gbs_api"
        );
    }

    #[test]
    fn key_is_percent_encoded_and_added_only_when_given() {
        let g = GoogleBooks::new(Some("AIza-_x y".to_string()));
        assert!(g.request_url("9784003101117").ends_with("&key=AIza-_x%20y"));
        let g = GoogleBooks::new(Some("  ".to_string()));
        assert!(!g.request_url("9784003101117").contains("key="));
        let g = GoogleBooks::new(None);
        assert_eq!(
            g.request_url("9784003101117"),
            "https://www.googleapis.com/books/v1/volumes?q=isbn:9784003101117&maxResults=5&printType=books"
        );
    }
}
