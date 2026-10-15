//! bibcontainer の書誌取得クレート。
//!
//! - `provider`・`providers`: 取得元（openBD・NDL・Google Books・Open Library）。生の応答の取得（`fetch_raw`）と解析（`parse`）を分けてある。
//! - `normalize`: 正規化（NFKC、著者の役割の分離、日付、価格、ページ数と大きさ）。
//! - `merge`: 項目ごとの優先順位による統合。
//! - `service`: 有効な取得元への並行した問い合わせ、キャッシュ、取得元ごとの状態。
//! - `transport`・`http`: レート制限、リトライ、タイムアウト、User-Agent、ログの URL の伏せ字。
//! - `cover`: 書影の取得・ダミー画像の判定・JPEG への変換・保存・サムネイル。
//!
//! DB にも Tauri にも依存しない。生の応答の保存先は `RawCache` トレイトで呼び出し側が渡す。

pub mod cover;
pub mod error;
pub mod http;
pub mod isbn;
pub mod merge;
pub mod normalize;
pub mod provider;
pub mod providers;
pub mod service;
pub mod transport;
pub mod types;
pub mod xml;

pub use error::{CoverError, FetchError, ParseError, SearchError};
pub use http::{
    HttpClient, HttpRequest, HttpResponse, ReqwestClient, TransportError, mask_url, user_agent,
};
pub use merge::{MetaFieldChoice, MetaMerged, MetaPick, MetaPriorities, merge};
pub use provider::{
    CacheEntry, CacheError, MemoryRawCache, Provider, ProviderAuth, ProviderInfo, RawCache,
    RawResponse, SearchQuery,
};
pub use providers::{default_providers, default_rate_limit};
pub use service::{
    FetchRequest, MetaFetchResult, MetadataService, ProviderOutcome, ProviderReport,
};
pub use transport::{ProviderRateLimit, RetryPolicy, Transport};
pub use types::*;
