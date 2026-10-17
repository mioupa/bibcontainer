//! 取得元のトレイト（FR-META-01）と、生の応答のキャッシュのトレイト（FR-META-10）。
//!
//! 取得を「生の応答を取る（`fetch_raw`）」と「候補に直す（`parse`）」に分けてある。
//! 生の応答を保存しておけば、マッピングを直したときに再ダウンロードせず、`parse` だけをやり直せる。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::error::{FetchError, ParseError, SearchError};
use crate::normalize::clean_text;
use crate::transport::{ProviderRateLimit, Transport};
use crate::types::{Candidate, MetaField, ProviderId, SearchResults};

/// 取得元が必要とする認証情報。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ProviderAuth {
    /// 不要
    None,
    /// あれば使う（`label` は「API キー」など、設定画面の見出し）
    Optional { label: String },
    /// 必須。設定されたときだけ有効にできる。
    Required { label: String },
}

/// 取得元の能力情報（FR-META-01）。設定画面と「このアプリについて」の表示に使う。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub display_name: String,
    pub auth: ProviderAuth,
    /// 取れる項目
    pub fields: Vec<MetaField>,
    pub rate_limit: ProviderRateLimit,
    /// クレジット表記の文言（10.6）
    pub credit: String,
    /// 複数 ISBN をまとめて取れる（`fetch_raw_batch` が 1 回の通信で済む）
    pub supports_batch: bool,
    /// キーワード検索に対応している（フェーズ 3）
    pub supports_search: bool,
    /// 書誌を取るときに通信する。今の取得元はすべて `true`（通信せず URL を組み立てるだけの取得元を
    /// 足すときのために残してある。`false` の取得元は、生の応答をキャッシュせず、`offline` の判定にも数えない）。
    pub needs_network: bool,
}

/// 取得元から取った生の応答。`body` はキャッシュにそのまま保存する文字列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponse {
    pub provider: ProviderId,
    /// 問い合わせた ISBN-13
    pub isbn13: String,
    pub body: String,
}

/// 書名・著者それぞれの長さの上限（整えたあとの文字数）。
pub const MAX_SEARCH_TERM_CHARS: usize = 200;

/// キーワード検索の条件（書名・著者）。1 回に取る件数は、取得元ごとに決めてある。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub title: Option<String>,
    pub author: Option<String>,
}

impl SearchQuery {
    /// 条件を整えて確かめる。書名・著者は、NFKC にそろえ、前後の空白を除き、連続する空白を 1 つにする
    /// （空になったものは `None`）。書名も著者もなければ `InvalidQuery`。どちらかが
    /// `MAX_SEARCH_TERM_CHARS` 文字を超えても `InvalidQuery`。整えたあとの条件を返す（何度通しても同じ）。
    pub fn validated(&self) -> Result<SearchQuery, SearchError> {
        let title = self.title.as_deref().and_then(clean_text);
        let author = self.author.as_deref().and_then(clean_text);
        if title.is_none() && author.is_none() {
            return Err(SearchError::InvalidQuery(
                "書名か著者を入れてください".to_string(),
            ));
        }
        for (label, term) in [("書名", &title), ("著者", &author)] {
            if term
                .as_ref()
                .is_some_and(|t| t.chars().count() > MAX_SEARCH_TERM_CHARS)
            {
                return Err(SearchError::InvalidQuery(format!(
                    "{label}は {MAX_SEARCH_TERM_CHARS} 文字までにしてください"
                )));
            }
        }
        Ok(SearchQuery { title, author })
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    fn info(&self) -> ProviderInfo;

    /// ISBN-13 で 1 件、生の応答を取る。通信の失敗は `FetchError`。
    /// 「該当なし」は失敗ではなく、該当なしを表す応答が返る（`parse` が `Ok(None)` にする）。
    async fn fetch_raw(
        &self,
        transport: &Transport,
        isbn13: &str,
    ) -> Result<RawResponse, FetchError>;

    /// 複数の ISBN-13 をまとめて取る。既定は 1 件ずつ順に取る（レート制限は `Transport` が守る）。
    /// openBD はカンマ区切りで 100 件まで 1 回で取れるので、これを上書きする。
    async fn fetch_raw_batch(
        &self,
        transport: &Transport,
        isbns: &[String],
    ) -> Vec<(String, Result<RawResponse, FetchError>)> {
        let mut out = Vec::with_capacity(isbns.len());
        for isbn in isbns {
            out.push((isbn.clone(), self.fetch_raw(transport, isbn).await));
        }
        out
    }

    /// 生の応答を候補にする。`Ok(None)` は「該当なし」。
    fn parse(&self, raw: &RawResponse) -> Result<Option<Candidate>, ParseError>;

    /// キーワード検索（任意。フェーズ 3）。既定は未対応。
    /// 実装は、最初に `query.validated()` で条件を整えて確かめる（`MetadataService::search` も通すが、取得元を直接呼ぶ側のため）。
    /// 通信は `fetch_raw` と同じレート制限のグループで行う（ISBN の取得と制限を分け合う）。
    async fn search(
        &self,
        _transport: &Transport,
        _query: &SearchQuery,
    ) -> Result<SearchResults, SearchError> {
        Err(SearchError::Unsupported)
    }
}

// ---------------------------------------------------------------------------
// 生の応答のキャッシュ
// ---------------------------------------------------------------------------

/// キャッシュに入れる 1 件。`src-tauri` では `provider_cache`（`provider, isbn13, fetched_at, status, payload`）に対応する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub body: String,
    /// 取得した時刻（UNIX 秒、UTC）
    pub fetched_at: i64,
    /// `true`: 書誌が見つかった（`status = 'ok'`）、`false`: 該当なし（`status = 'not_found'`）
    pub found: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("キャッシュの読み書きに失敗しました: {0}")]
pub struct CacheError(pub String);

/// 生の応答のキャッシュ。呼び出し側（`src-tauri`）が DB で実装する。
/// 失敗しても取得は止めない（呼び出し側はログを出して続ける）。
#[async_trait]
pub trait RawCache: Send + Sync {
    async fn get(
        &self,
        provider: ProviderId,
        isbn13: &str,
    ) -> Result<Option<CacheEntry>, CacheError>;
    async fn put(
        &self,
        provider: ProviderId,
        isbn13: &str,
        entry: CacheEntry,
    ) -> Result<(), CacheError>;
}

/// メモリ上のキャッシュ（テストと、キャッシュなしでの動作確認用）。
#[derive(Default)]
pub struct MemoryRawCache {
    entries: Mutex<HashMap<(ProviderId, String), CacheEntry>>,
}

impl MemoryRawCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().expect("キャッシュ").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn entry(&self, provider: ProviderId, isbn13: &str) -> Option<CacheEntry> {
        self.entries
            .lock()
            .expect("キャッシュ")
            .get(&(provider, isbn13.to_string()))
            .cloned()
    }
}

#[async_trait]
impl RawCache for MemoryRawCache {
    async fn get(
        &self,
        provider: ProviderId,
        isbn13: &str,
    ) -> Result<Option<CacheEntry>, CacheError> {
        Ok(self.entry(provider, isbn13))
    }

    async fn put(
        &self,
        provider: ProviderId,
        isbn13: &str,
        entry: CacheEntry,
    ) -> Result<(), CacheError> {
        self.entries
            .lock()
            .expect("キャッシュ")
            .insert((provider, isbn13.to_string()), entry);
        Ok(())
    }
}
