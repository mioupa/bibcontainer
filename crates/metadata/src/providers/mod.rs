//! 取得元の実装。
//!
//! | ID | 取得元 | 取れるもの |
//! |---|---|---|
//! | `openbd` | openBD（`api.openbd.jp`） | 書名・著者・出版社・発行日・価格・Cコード・内容紹介・書影 |
//! | `ndl` | NDLサーチ SRU（DC-NDL） | 書名とヨミ・著者・出版社・発行日・価格・ページ数と大きさ・NDC・シリーズ・巻次 |
//! | `google` | Google Books | 書名・著者・出版社・発行日・内容紹介・ページ数・言語・書影 |
//! | `openlibrary` | Open Library（`openlibrary.org`） | 書名・副書名・著者・出版社・発行日・ページ数・言語・内容紹介・書影 |

use std::sync::Arc;

use crate::provider::Provider;

pub mod google;
pub mod ndl;
pub mod openbd;
pub mod openlibrary;

pub use google::GoogleBooks;
pub use ndl::Ndl;
pub use openbd::OpenBd;
pub use openlibrary::OpenLibrary;

/// 4 つの取得元（openBD・NDL・Google Books・Open Library）を既定の設定で作る。`google_api_key` は呼び出し側（キーチェーン）から渡す。
pub fn default_providers(google_api_key: Option<String>) -> Vec<Arc<dyn Provider>> {
    vec![
        Arc::new(OpenBd::new()),
        Arc::new(Ndl::new()),
        Arc::new(GoogleBooks::new(google_api_key)),
        Arc::new(OpenLibrary::new()),
    ]
}

/// 取得元のレート制限（書影のダウンロードでも同じ制限を使う）。
pub fn default_rate_limit(id: crate::types::ProviderId) -> crate::transport::ProviderRateLimit {
    use crate::types::ProviderId;
    match id {
        ProviderId::Openbd => openbd::OpenBd::rate_limit(),
        ProviderId::Ndl => ndl::Ndl::rate_limit(),
        ProviderId::Google => google::GoogleBooks::rate_limit(),
        ProviderId::OpenLibrary => openlibrary::OpenLibrary::rate_limit(),
    }
}
