//! Google Books（`https://www.googleapis.com/books/v1/volumes?q=isbn:<ISBN>`）。
//!
//! API キーは呼び出し側（OS のキーチェーン）が渡す。このクレートはキーを保存せず、ログにも出さない。
//!
//! 2026-10-14 の確認では、キーなしのリクエストは共有プロジェクトの割り当て（`quota_limit_value` が 0）に当たり、
//! 常に 429 になった。実際にはキーが必要とみてよい。詳しくは `docs/decisions/0007-metadata-providers.md`。
//!
//! # ISBN 検索の障害と、Dynamic Links による回避（2026-10-16）
//!
//! 2026-10-01 ごろから、Google Books API の `q=isbn:<ISBN>`（`intitle:`・`inauthor:` も）が、キーが有効でも
//! HTTP 200・`totalItems: 0` を返し続けている（Google 側の不具合で、直っていない。Issue Tracker の課題は
//! 「Won't Fix (Infeasible)」で閉じられ、公式の報告窓口がない）。一方、次の 2 つは動く。
//!
//! - **Dynamic Links**（公式。<https://developers.google.com/books/docs/dynamic-links>。キーは要らない）:
//!   `GET https://books.google.com/books?bibkeys=ISBN:<ISBN>&jscmd=viewapi&callback=cb` は JSONP
//!   （`cb({"ISBN:…": {"info_url": "https://books.google.com/books?id=<ID>&…", …}});`）で、その ISBN の本の
//!   volume ID を返す。知らない ISBN は `cb({});`。ドキュメントは「サーバー側やオフラインの問い合わせ向けではない
//!   （閲覧の可否が利用者の場所で変わるため）」としているが、利用者の PC から 1 冊ずつ問い合わせるこのアプリは
//!   想定の範囲内と判断した。Google Books への帰属とリンクを保つこと（`ProviderInfo.credit`）。
//!   大量の問い合わせは captcha の対象になりうるので、書誌・書影と同じレート制限（`google`、1 秒に 1 件）に入れる。
//! - **volume ID を指定した取得**: `GET /books/v1/volumes/<ID>?key=…` は 1 冊の volume を返す。
//!
//! そこで、`fetch_raw` は次の順に取る。
//!
//! 1. これまでどおり `volumes?q=isbn:…` を 1 回。HTTP エラーなら、そのエラーを返す。解析して、書誌が取れるとき
//!    （`Ok(Some)`）と、解析できないとき（`Err`）は、その応答をそのまま返す（回避策には進まない）。
//!    **「該当なし」（`Ok(None)`）のときだけ**、2 に進む。Google の障害が直れば、1 で見つかるので、2 以降は動かない
//!    （本当に Google にない本は、毎回、2 の 1 回の通信（`cb({});`）が増える）。
//! 2. Dynamic Links を 1 回（**API キーは付けない。books.google.com にキーを送らない**）。`{}`・該当する項目なし
//!    は「該当なし」で、1 の応答をそのまま返す（`parse` が `None` にし、1 日キャッシュされる）。
//!    項目があれば、`info_url`（なければ `preview_url`・`thumbnail_url`）の `id` を volume ID とする。
//!    ID は `^[A-Za-z0-9_-]{4,32}$` を確かめてから URL に入れる（合わなければ解析エラー）。
//!    HTTP エラーや JSONP でない応答は、Google の取得全体の失敗にする（キャッシュされず、あとで取り直される）。
//! 3. `volumes/<ID>?key=…` を 1 回。404 は「該当なし」。そのほかの HTTP エラーは 1 と同じ意味づけ
//!    （`google_http_error`。接続テストの `invalidKey` などの判定が効く）で失敗にする。200 のときは、
//!    `industryIdentifiers` に ISBN（ISBN-10 は 13 に直す）があるか、識別子が 1 つもなければ採用し、
//!    合わなければ「該当なし」にする（検索の応答と同じ規則）。採用したら、**volume の JSON を受け取ったまま**
//!    `RawResponse.body` にする。
//!
//! 障害のあいだの 1 冊あたりの通信は、検索・Dynamic Links・volume の 3 回（1 秒ずつあける）。
//!
//! `parse_google` は、検索の応答（`items`）と、volume 1 件の応答（`kind` が `books#volume`、または最上位に
//! `volumeInfo`）の両方を読む。書影は、volume 1 件の応答では、`imageLinks` があるとき、最上位の `id` から
//! `books/content?id=<ID>&printsec=frontcover&img=1&zoom=0`（大きい）と `zoom=1`（128px ほど）を組み立てる。
//! volume の `imageLinks` の URL（`books/publisher/content?…&imgtk=…`）は使わない。`zoom=0` が和書で既知のダミー画像
//! （575x750）のとき、書影の取得は次の URL（`zoom=1`）に進む（0007 の 8）。

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{FetchError, ParseError};
use crate::http::HttpRequest;
use crate::isbn::to_isbn13;
use crate::normalize::{
    clean_description, clean_text, normalize_date, normalize_language, strip_html, upgrade_to_https,
};
use crate::provider::{Provider, ProviderAuth, ProviderInfo, RawResponse};
use crate::transport::{ProviderRateLimit, Transport, http_error};
use crate::types::{Candidate, CandidateAuthor, CandidateCover, MetaField, ProviderId};

pub const DEFAULT_BASE_URL: &str = "https://www.googleapis.com";
/// Dynamic Links の接続先（キーを送らない）。
pub const DEFAULT_DYNAMIC_LINKS_BASE_URL: &str = "https://books.google.com";

pub struct GoogleBooks {
    base_url: String,
    dynamic_links_base_url: String,
    api_key: Option<String>,
}

impl GoogleBooks {
    pub fn new(api_key: Option<String>) -> Self {
        GoogleBooks {
            base_url: DEFAULT_BASE_URL.to_string(),
            dynamic_links_base_url: DEFAULT_DYNAMIC_LINKS_BASE_URL.to_string(),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
        }
    }

    /// テスト用に接続先を変える。Dynamic Links も同じ接続先（`<接続先>/books?bibkeys=…`）になるので、
    /// 本物の books.google.com には出ていかない。
    pub fn with_base_url(base_url: &str, api_key: Option<String>) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        GoogleBooks {
            dynamic_links_base_url: base_url.clone(),
            base_url,
            api_key: api_key.filter(|k| !k.trim().is_empty()),
        }
    }

    /// 検索・volume と、Dynamic Links を別の接続先にするテスト用（キーが Dynamic Links に行かないことの確認など）。
    pub fn with_base_urls(
        base_url: &str,
        dynamic_links_base_url: &str,
        api_key: Option<String>,
    ) -> Self {
        GoogleBooks {
            base_url: base_url.trim_end_matches('/').to_string(),
            dynamic_links_base_url: dynamic_links_base_url.trim_end_matches('/').to_string(),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
        }
    }

    pub(crate) fn rate_limit() -> ProviderRateLimit {
        // 1 秒に 1 件（FR-META-08）。書影のダウンロード（books.google.com）と Dynamic Links も同じグループで数える
        ProviderRateLimit::new("google", 1000, 1)
    }

    /// API キーがあれば、`key=` を付ける。`url` には、すでに `?` がある。
    fn with_key(&self, mut url: String, has_query: bool) -> String {
        if let Some(key) = &self.api_key {
            url.push(if has_query { '&' } else { '?' });
            url.push_str("key=");
            url.push_str(&percent_encode(key));
        }
        url
    }

    fn request_url(&self, isbn13: &str) -> String {
        self.with_key(
            format!(
                "{}/books/v1/volumes?q=isbn:{}&maxResults=5&printType=books",
                self.base_url, isbn13
            ),
            true,
        )
    }

    /// Dynamic Links の URL。キーは付けない（books.google.com にキーを送らない）。
    fn dynamic_links_url(&self, isbn13: &str) -> String {
        format!(
            "{}/books?bibkeys=ISBN:{}&jscmd=viewapi&callback=cb",
            self.dynamic_links_base_url, isbn13
        )
    }

    /// volume ID を指定した取得の URL。`id` は `is_valid_volume_id` を通ったものだけを渡す。
    fn volume_url(&self, id: &str) -> String {
        self.with_key(format!("{}/books/v1/volumes/{}", self.base_url, id), false)
    }

    /// 検索が「該当なし」だったときの回避策（ファイル先頭の 2・3）。
    /// 取れたら volume の JSON、該当なしなら `None`。失敗は `Err`（取得全体の失敗）。
    async fn fetch_volume_via_dynamic_links(
        &self,
        transport: &Transport,
        isbn13: &str,
    ) -> Result<Option<String>, FetchError> {
        tracing::debug!(
            isbn13,
            "Google Books の検索が該当なしなので、Dynamic Links で探す"
        );
        let resp = transport
            .get(
                &Self::rate_limit(),
                HttpRequest::get(self.dynamic_links_url(isbn13)),
            )
            .await?;
        if !resp.is_success() {
            return Err(dynamic_links_http_error(&resp));
        }
        let Some(id) = volume_id_from_jsonp(&resp.text(), isbn13)? else {
            return Ok(None);
        };

        let resp = transport
            .get(&Self::rate_limit(), HttpRequest::get(self.volume_url(&id)))
            .await?;
        if resp.status == 404 {
            return Ok(None);
        }
        if !resp.is_success() {
            return Err(google_http_error(&resp, self.api_key.is_some()));
        }
        let body = resp.text();
        let v: Value = serde_json::from_str(&body).map_err(|e| {
            FetchError::Parse(format!(
                "Google Books の volume の応答が JSON ではありません: {e}"
            ))
        })?;
        if v.get("error").is_some() || !v.get("volumeInfo").is_some_and(Value::is_object) {
            return Err(FetchError::Parse(
                "Google Books の volume の応答に volumeInfo がありません".to_string(),
            ));
        }
        // ISBN が合うか、識別子がなければ採用（検索の応答と同じ規則。`parse_google` の中で決めている）
        match parse_google(isbn13, &body) {
            Ok(Some(_)) => Ok(Some(body)),
            Ok(None) => Ok(None),
            Err(e) => Err(FetchError::Parse(e.0)),
        }
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
        let search_body = resp.text();
        // 書誌が取れる応答と、解析できない応答は、そのまま返す。「該当なし」のときだけ、回避策に進む（ファイル先頭の 1）
        let body = if matches!(parse_google(isbn13, &search_body), Ok(None)) {
            self.fetch_volume_via_dynamic_links(transport, isbn13)
                .await?
                // 該当なしなら、検索の応答を返す（`parse` が `None` にする）
                .unwrap_or(search_body)
        } else {
            search_body
        };
        Ok(RawResponse {
            provider: ProviderId::Google,
            isbn13: isbn13.to_string(),
            body,
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

/// Dynamic Links（books.google.com。API キーを使わない）の HTTP エラーのメッセージの末尾に付ける印。
/// このエラーは API キーの問題ではないので、`classify_failure` は、401・403 でも `InvalidKey` にしない。
pub const DYNAMIC_LINKS_NOTE: &str = "（Google Books のリンク取得）";

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
    if message.contains(DYNAMIC_LINKS_NOTE) {
        return GoogleFailure::Other;
    }
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

/// Dynamic Links の HTTP エラー。`http_error` の形で、末尾に `DYNAMIC_LINKS_NOTE` を付ける。
fn dynamic_links_http_error(resp: &crate::http::HttpResponse) -> FetchError {
    match http_error(resp) {
        FetchError::Http { status, message } => FetchError::Http {
            status,
            message: format!("{message}{DYNAMIC_LINKS_NOTE}"),
        },
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Dynamic Links（JSONP）
// ---------------------------------------------------------------------------

/// volume ID として URL に入れてよいか（`^[A-Za-z0-9_-]{4,32}$`）。応答の文字列をそのまま URL に入れない。
fn is_valid_volume_id(id: &str) -> bool {
    (4..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// URL のクエリから、`name` の値（パーセントエンコードのまま）を取る。
fn query_param<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    let query = url.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

/// JSONP（`cb({...});`）の、最初の `(` から最後の `)` までのあいだを JSON として読む。
fn parse_jsonp(text: &str) -> Result<Value, FetchError> {
    let not_jsonp =
        || FetchError::Parse("Google Books（Dynamic Links）の応答が JSONP ではありません".into());
    let open = text.find('(').ok_or_else(not_jsonp)?;
    let close = text.rfind(')').ok_or_else(not_jsonp)?;
    if close <= open {
        return Err(not_jsonp());
    }
    serde_json::from_str(&text[open + 1..close]).map_err(|e| {
        FetchError::Parse(format!(
            "Google Books（Dynamic Links）の応答の JSON を読めません: {e}"
        ))
    })
}

/// Dynamic Links の応答（JSONP）から、`ISBN:<isbn13>` の項目の volume ID を取る。
///
/// - `{}`、または該当する項目がない: `Ok(None)`（該当なし）。項目は、`ISBN:<isbn13>`、なければ唯一の項目。
/// - ID は `info_url`、なければ `preview_url`・`thumbnail_url` の `id` パラメータ。
///   ID がない・形が違う（`is_valid_volume_id`）ときは `Err`（解析エラー）。
fn volume_id_from_jsonp(text: &str, isbn13: &str) -> Result<Option<String>, FetchError> {
    let v = parse_jsonp(text)?;
    let map = v.as_object().ok_or_else(|| {
        FetchError::Parse("Google Books（Dynamic Links）の応答がオブジェクトではありません".into())
    })?;
    let entry = match map.get(&format!("ISBN:{isbn13}")) {
        Some(entry) => entry,
        None if map.len() == 1 => map.values().next().expect("項目が 1 つある"),
        None => return Ok(None),
    };
    let url_id = ["info_url", "preview_url", "thumbnail_url"]
        .iter()
        .filter_map(|key| entry.get(*key).and_then(Value::as_str))
        .find_map(|url| query_param(url, "id"));
    match url_id {
        Some(id) if is_valid_volume_id(id) => Ok(Some(id.to_string())),
        Some(_) => Err(FetchError::Parse(
            "Google Books（Dynamic Links）の volume ID が想定の形ではありません".into(),
        )),
        None => Err(FetchError::Parse(
            "Google Books（Dynamic Links）の応答に volume ID がありません".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

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

/// volume 1 件の応答か（検索の応答は `items` を持つ）。
fn is_single_volume(v: &Value) -> bool {
    v.get("items").is_none()
        && (v.get("kind").and_then(Value::as_str) == Some("books#volume")
            || v.get("volumeInfo").is_some())
}

/// volume 1 件の応答の書影。`imageLinks` があるとき、最上位の `id` から、大きい方（`zoom=0`）と
/// `zoom=1` の URL を組み立てる。`imageLinks` の URL（`books/publisher/content?…&imgtk=…`）は使わない。
fn single_volume_covers(volume: &Value, info: &Value) -> Vec<CandidateCover> {
    let has_links = info
        .get("imageLinks")
        .and_then(Value::as_object)
        .is_some_and(|links| !links.is_empty());
    let id = volume
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| is_valid_volume_id(id));
    let (true, Some(id)) = (has_links, id) else {
        return Vec::new();
    };
    [0, 1]
        .into_iter()
        .map(|zoom| CandidateCover {
            provider: ProviderId::Google,
            url: format!(
                "https://books.google.com/books/content?id={id}&printsec=frontcover&img=1&zoom={zoom}"
            ),
        })
        .collect()
}

/// 検索の応答（`items`）と、volume 1 件の応答の、どちらも読む。
pub(crate) fn parse_google(isbn13: &str, body: &str) -> Result<Option<Candidate>, ParseError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| ParseError::new(format!("Google Books の応答が JSON ではありません: {e}")))?;
    if v.get("error").is_some() {
        return Err(ParseError::new("Google Books がエラーを返しました"));
    }
    let single = is_single_volume(&v);
    let items: Vec<&Value> = if single {
        vec![&v]
    } else {
        v.get("items")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default()
    };
    if items.is_empty() {
        return Ok(None);
    }
    // ISBN が合う項目を選ぶ。どの項目にも識別子がなければ先頭（要件書 11.3）。識別子があって合わなければ該当なし。
    // volume 1 件の応答も同じ規則（合うか、識別子がなければ採用）。
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
        })
        .copied();
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

    // 書影
    if single {
        // volume 1 件の応答: 最上位の id から zoom=0（大きい）、zoom=1 の順
        c.covers = single_volume_covers(&v, info);
    } else if let Some(links) = info.get("imageLinks") {
        // 検索の応答: thumbnail（zoom=1、幅 128px ほど）と、同じ URL の zoom=0（大きい画像）。大きい方を先にする。
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

    const CLEAN_CODE: &str = "9780132350884";
    const KOKORO: &str = "9784003101117";

    // Dynamic Links の実際の応答（2026-10-16）
    const DL_CLEAN_CODE: &str =
        include_str!("../../tests/fixtures/google_dynamiclinks_9780132350884.jsonp");
    const DL_KOKORO: &str =
        include_str!("../../tests/fixtures/google_dynamiclinks_9784003101117.jsonp");
    const DL_BOGUS: &str =
        include_str!("../../tests/fixtures/google_dynamiclinks_9784000000000.jsonp");
    const DL_NOT_FOUND: &str =
        include_str!("../../tests/fixtures/google_dynamiclinks_notfound.jsonp");

    fn id_of(text: &str, isbn13: &str) -> Result<Option<String>, FetchError> {
        volume_id_from_jsonp(text, isbn13)
    }

    #[test]
    fn volume_ids_come_from_the_real_dynamic_links_responses() {
        assert_eq!(
            id_of(DL_CLEAN_CODE, CLEAN_CODE).unwrap().as_deref(),
            Some("hjEFCAAAQBAJ")
        );
        assert_eq!(
            id_of(DL_KOKORO, KOKORO).unwrap().as_deref(),
            Some("fz5TPAAACAAJ")
        );
        // 実際には存在しない ISBN でも、Google が項目を返すことがある（ID は取れる。ISBN の確認は volume を取ってから）
        assert_eq!(
            id_of(DL_BOGUS, "9784000000000").unwrap().as_deref(),
            Some("zT3GEAAAQBAJ")
        );
        // 知らない ISBN は `cb({});`
        assert_eq!(id_of(DL_NOT_FOUND, "9784123456784").unwrap(), None);
    }

    #[test]
    fn dynamic_links_entry_is_picked_by_isbn_or_the_only_entry() {
        let two = r#"cb({"ISBN:9784003101117":{"info_url":"https://books.google.com/books?id=AAAA1111&source=gbs_ViewAPI"},"ISBN:9780132350884":{"info_url":"https://books.google.com/books?id=BBBB2222&source=gbs_ViewAPI"}});"#;
        assert_eq!(id_of(two, KOKORO).unwrap().as_deref(), Some("AAAA1111"));
        assert_eq!(id_of(two, CLEAN_CODE).unwrap().as_deref(), Some("BBBB2222"));
        // 探した ISBN の項目がなく、項目が複数あるときは該当なし
        assert_eq!(id_of(two, "9784780802047").unwrap(), None);
        // 項目が 1 つなら、キーが違っていても、それを使う
        let one =
            r#"cb({"ISBN:4003101111":{"info_url":"https://books.google.com/books?id=CCCC3333"}});"#;
        assert_eq!(id_of(one, KOKORO).unwrap().as_deref(), Some("CCCC3333"));
    }

    #[test]
    fn volume_id_falls_back_to_the_other_urls_and_ignores_the_rest_of_the_query() {
        let wrap = |entry: &str| format!(r#"cb({{"ISBN:{KOKORO}":{entry}}});"#);
        // info_url がなければ preview_url、それもなければ thumbnail_url
        let preview = wrap(
            r#"{"preview_url":"https://books.google.com/books?printsec=frontcover&id=PPPP1111&source=x","thumbnail_url":"https://books.google.com/books/content?id=TTTT2222"}"#,
        );
        assert_eq!(
            id_of(&preview, KOKORO).unwrap().as_deref(),
            Some("PPPP1111")
        );
        let thumb = wrap(
            r#"{"thumbnail_url":"https://books.google.com/books/content?id=TTTT2222&zoom=5#x"}"#,
        );
        assert_eq!(id_of(&thumb, KOKORO).unwrap().as_deref(), Some("TTTT2222"));
        // `id` という名前の別のパラメータ（`uid=`・`xid=`）は ID ではない
        let other = wrap(r#"{"info_url":"https://books.google.com/books?uid=NOPE&xid=NOPE"}"#);
        assert!(matches!(id_of(&other, KOKORO), Err(FetchError::Parse(_))));
    }

    #[test]
    fn invalid_or_missing_volume_ids_are_parse_errors() {
        let wrap = |url: &str| format!(r#"cb({{"ISBN:{KOKORO}":{{"info_url":"{url}"}}}});"#);
        for bad in [
            "https://books.google.com/books?id=abc", // 短い（3 文字）
            "https://books.google.com/books?id=abcdefghijklmnopqrstuvwxyz0123456", // 長い（33 文字）
            "https://books.google.com/books?id=../../etc/passwd",
            "https://books.google.com/books?id=ab%2Fcd1234",
            "https://books.google.com/books?id=ab.cd1234",
            "https://books.google.com/books?id=ab cd1234",
            "https://books.google.com/books?id=あいうえおかきく",
            "https://books.google.com/books?id=",
            "https://books.google.com/books?source=gbs_ViewAPI", // id がない
            "https://books.google.com/books",
        ] {
            assert!(
                matches!(id_of(&wrap(bad), KOKORO), Err(FetchError::Parse(_))),
                "{bad}"
            );
        }
        // 項目に URL がまったくない
        let none = format!(r#"cb({{"ISBN:{KOKORO}":{{"bib_key":"ISBN:{KOKORO}"}}}});"#);
        assert!(matches!(id_of(&none, KOKORO), Err(FetchError::Parse(_))));
        // 形が合う ID は通る（`-`・`_`・4 文字・32 文字）
        assert!(is_valid_volume_id("a-_Z"));
        assert!(is_valid_volume_id(&"x".repeat(32)));
        assert!(!is_valid_volume_id(&"x".repeat(33)));
        assert!(!is_valid_volume_id(""));
    }

    #[test]
    fn jsonp_is_read_between_the_first_open_and_the_last_close_parenthesis() {
        let obj = |t: &str| parse_jsonp(t).unwrap();
        assert_eq!(obj("cb({});"), serde_json::json!({}));
        assert_eq!(obj("cb({})"), serde_json::json!({}));
        assert_eq!(
            obj("  /**/ cb({\"a\": \"(x)\"} ) ;\n"),
            serde_json::json!({"a": "(x)"})
        );
        assert_eq!(
            obj("anything_else({\"a\":1});"),
            serde_json::json!({"a": 1})
        );
        for bad in [
            "",
            "{}",
            "cb(",
            "cb)(",
            "cb()",
            "cb({);",
            "<html><body>unusual traffic (captcha)</body></html>",
        ] {
            assert!(
                matches!(parse_jsonp(bad), Err(FetchError::Parse(_))),
                "{bad:?}"
            );
        }
        // JSON だが、オブジェクトではない
        assert!(matches!(
            id_of("cb([1,2]);", KOKORO),
            Err(FetchError::Parse(_))
        ));
        assert!(matches!(
            id_of("cb(\"x\");", KOKORO),
            Err(FetchError::Parse(_))
        ));
    }

    #[test]
    fn urls_never_carry_the_key_to_dynamic_links() {
        let g = GoogleBooks::new(Some("AIza-_x y".to_string()));
        assert_eq!(
            g.dynamic_links_url(CLEAN_CODE),
            "https://books.google.com/books?bibkeys=ISBN:9780132350884&jscmd=viewapi&callback=cb"
        );
        assert_eq!(
            g.volume_url("hjEFCAAAQBAJ"),
            "https://www.googleapis.com/books/v1/volumes/hjEFCAAAQBAJ?key=AIza-_x%20y"
        );
        let g = GoogleBooks::new(None);
        assert_eq!(
            g.volume_url("hjEFCAAAQBAJ"),
            "https://www.googleapis.com/books/v1/volumes/hjEFCAAAQBAJ"
        );
        // テスト用の接続先: `with_base_url` は Dynamic Links も同じ接続先にする（本物の books.google.com に出ない）
        let g = GoogleBooks::with_base_url("http://127.0.0.1:9/", Some("K".into()));
        assert_eq!(
            g.dynamic_links_url(CLEAN_CODE),
            "http://127.0.0.1:9/books?bibkeys=ISBN:9780132350884&jscmd=viewapi&callback=cb"
        );
        let g = GoogleBooks::with_base_urls("http://a.test/", "http://b.test/", Some("K".into()));
        assert_eq!(
            g.request_url(CLEAN_CODE),
            "http://a.test/books/v1/volumes?q=isbn:9780132350884&maxResults=5&printType=books&key=K"
        );
        assert_eq!(
            g.dynamic_links_url(CLEAN_CODE),
            "http://b.test/books?bibkeys=ISBN:9780132350884&jscmd=viewapi&callback=cb"
        );
    }

    #[test]
    fn dynamic_links_errors_are_never_read_as_an_invalid_api_key() {
        // API キーを使わない通信の失敗。401・403 でも、キーのせいにしない
        let note = |m: &str| format!("{m}{DYNAMIC_LINKS_NOTE}");
        for status in [400, 401, 403, 429, 500] {
            assert_eq!(
                classify_failure(status, &note("forbidden")),
                GoogleFailure::Other,
                "{status}"
            );
        }
        // 印がなければ、これまでどおり
        assert_eq!(
            classify_failure(403, "forbidden"),
            GoogleFailure::InvalidKey
        );
    }
}
