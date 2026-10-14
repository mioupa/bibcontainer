//! API キーの保存（OS のキーチェーン）。
//!
//! - サービス名はバンドル ID（`io.github.mioupa.bibcontainer`）、アカウント名は取得元 ID（`google` など）。
//! - キーを返すコマンドは作らない。Rust の中でだけ読み、取得元（`GoogleBooks::new(Some(key))`）に渡す。
//! - キーをログ・エラーメッセージ・設定・DB に出さない（`Secret` は `Debug` で伏せる）。
//! - キーチェーンの呼び出しはブロックする（macOS では許可の確認が出ることもある）ので、別のスレッドで呼ぶ。
//!   読んだキーは、プロセスの間だけメモリに持つ（呼ぶたびにキーチェーンへ問い合わせて確認を出さないため）。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use bibcontainer_metadata::ProviderId;

use crate::error::AppError;

/// キーチェーンのサービス名（`tauri.conf.json` の `identifier` と同じ）。
pub const KEYCHAIN_SERVICE: &str = "io.github.mioupa.bibcontainer";
/// キーの長さの上限（バイト）。実際の API キーは 40 バイト前後。
const MAX_KEY_LEN: usize = 512;

/// 秘密の文字列。`Debug` と `Display` では伏せる。
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// 取得元に渡すときだけ使う。
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// キーの保存先。本番は OS のキーチェーン、テストはメモリ。
pub trait KeyStore: Send + Sync {
    /// なければ `Ok(None)`。
    fn get(&self, account: &str) -> Result<Option<Secret>, AppError>;
    fn set(&self, account: &str, key: &Secret) -> Result<(), AppError>;
    /// なくてもエラーにしない。
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

/// OS のキーチェーン（macOS: Keychain、Windows: 資格情報マネージャー）。
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeyStore;

fn keychain_error(err: &keyring::Error) -> AppError {
    // keyring のエラーの文章にはキーは入らない（アカウント名と原因だけ）
    AppError::Keychain(err.to_string())
}

fn entry(account: &str) -> Result<keyring::Entry, AppError> {
    keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| keychain_error(&e))
}

impl KeyStore for OsKeyStore {
    fn get(&self, account: &str) -> Result<Option<Secret>, AppError> {
        match entry(account)?.get_password() {
            Ok(key) => Ok(Some(Secret::new(key)).filter(|s| !s.expose().is_empty())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_error(&e)),
        }
    }

    fn set(&self, account: &str, key: &Secret) -> Result<(), AppError> {
        entry(account)?
            .set_password(key.expose())
            .map_err(|e| keychain_error(&e))
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_error(&e)),
        }
    }
}

/// メモリ上の保存先（テスト用）。
#[cfg(test)]
#[derive(Debug, Default)]
pub struct MemoryKeyStore {
    keys: Mutex<HashMap<String, String>>,
}

#[cfg(test)]
impl KeyStore for MemoryKeyStore {
    fn get(&self, account: &str) -> Result<Option<Secret>, AppError> {
        let keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(keys.get(account).cloned().map(Secret::new))
    }

    fn set(&self, account: &str, key: &Secret) -> Result<(), AppError> {
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.insert(account.to_owned(), key.expose().to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.remove(account);
        Ok(())
    }
}

/// 利用者が入力したキーを検べて整える。空・長すぎる・空白や制御文字を含むものは受け付けない。
pub fn clean_key(raw: &str) -> Result<Secret, AppError> {
    let key = raw.trim();
    if key.is_empty() {
        return Err(AppError::Validation("API キーが空です".to_owned()));
    }
    if key.len() > MAX_KEY_LEN {
        return Err(AppError::Validation(format!(
            "API キーが長すぎます（{MAX_KEY_LEN} バイト以内）"
        )));
    }
    if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::Validation(
            "API キーに空白や改行が含まれています。コピーし直してください".to_owned(),
        ));
    }
    Ok(Secret::new(key))
}

/// キーの取得・保存・削除。読んだ結果はメモリに持つ。
pub struct ApiKeys {
    store: Arc<dyn KeyStore>,
    cache: Mutex<HashMap<ProviderId, Option<Secret>>>,
}

impl ApiKeys {
    pub fn new(store: Arc<dyn KeyStore>) -> Self {
        ApiKeys {
            store,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, id: ProviderId) -> Option<Option<Secret>> {
        let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache.get(&id).cloned()
    }

    fn remember(&self, id: ProviderId, key: Option<Secret>) {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache.insert(id, key);
    }

    /// キーを読む。キーチェーンの呼び出しは別のスレッドで行う。
    pub async fn get(&self, id: ProviderId) -> Result<Option<Secret>, AppError> {
        if let Some(known) = self.cached(id) {
            return Ok(known);
        }
        let store = Arc::clone(&self.store);
        let key = blocking(move || store.get(id.as_str())).await?;
        self.remember(id, key.clone());
        Ok(key)
    }

    pub async fn has(&self, id: ProviderId) -> Result<bool, AppError> {
        Ok(self.get(id).await?.is_some())
    }

    /// 保存する（上書き）。
    pub async fn set(&self, id: ProviderId, key: Secret) -> Result<(), AppError> {
        let store = Arc::clone(&self.store);
        let to_store = key.clone();
        blocking(move || store.set(id.as_str(), &to_store)).await?;
        self.remember(id, Some(key));
        Ok(())
    }

    /// 削除する（なくてもエラーにしない）。
    pub async fn delete(&self, id: ProviderId) -> Result<(), AppError> {
        let store = Arc::clone(&self.store);
        blocking(move || store.delete(id.as_str())).await?;
        self.remember(id, None);
        Ok(())
    }
}

async fn blocking<T, F>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("キーチェーンの処理が中断されました: {e}")))?
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tauri::async_runtime::block_on;

    use super::*;

    #[test]
    fn secret_never_shows_its_value() {
        let s = Secret::new("AIzaSyDUMMY-key");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(format!("{s}"), "***");
        assert_eq!(s.expose(), "AIzaSyDUMMY-key");
        // 入れ物の Debug にも出ない
        let nested = Some(s);
        assert!(!format!("{nested:?}").contains("AIza"));
    }

    #[test]
    fn keys_are_trimmed_and_bad_ones_rejected() {
        assert_eq!(clean_key("  abc123  ").unwrap().expose(), "abc123");
        for bad in ["", "   ", "a b", "a\nb", "a\tb", &"x".repeat(600)] {
            assert!(
                matches!(clean_key(bad), Err(AppError::Validation(_))),
                "{bad:?}"
            );
        }
        // エラーメッセージにキーの中身は入らない
        let err = clean_key("secret value").unwrap_err().to_string();
        assert!(!err.contains("secret"), "{err}");
    }

    /// 呼ばれた回数を数える保存先。
    #[derive(Default)]
    struct CountingStore {
        inner: MemoryKeyStore,
        gets: AtomicUsize,
    }

    impl KeyStore for CountingStore {
        fn get(&self, account: &str) -> Result<Option<Secret>, AppError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get(account)
        }
        fn set(&self, account: &str, key: &Secret) -> Result<(), AppError> {
            self.inner.set(account, key)
        }
        fn delete(&self, account: &str) -> Result<(), AppError> {
            self.inner.delete(account)
        }
    }

    #[test]
    fn keys_are_stored_under_the_provider_id_and_read_once() {
        let store = Arc::new(CountingStore::default());
        let keys = ApiKeys::new(store.clone());
        block_on(async {
            assert!(!keys.has(ProviderId::Google).await.unwrap());
            keys.set(ProviderId::Google, Secret::new("k1"))
                .await
                .unwrap();
            assert!(keys.has(ProviderId::Google).await.unwrap());
            assert_eq!(
                keys.get(ProviderId::Google)
                    .await
                    .unwrap()
                    .unwrap()
                    .expose(),
                "k1"
            );
            // アカウント名は取得元 ID
            assert_eq!(store.inner.get("google").unwrap().unwrap().expose(), "k1");
            // 読んだ結果はメモリに持つ（最初の 1 回だけキーチェーンに聞いた）
            assert_eq!(store.gets.load(Ordering::SeqCst), 1);

            keys.delete(ProviderId::Google).await.unwrap();
            assert!(!keys.has(ProviderId::Google).await.unwrap());
            assert!(store.inner.get("google").unwrap().is_none());
            // 無いものを消してもエラーにしない
            keys.delete(ProviderId::Google).await.unwrap();
        });
    }

    #[test]
    fn a_failing_keychain_is_reported_as_a_keychain_error_without_the_key() {
        struct Locked;
        impl KeyStore for Locked {
            fn get(&self, _: &str) -> Result<Option<Secret>, AppError> {
                Err(AppError::Keychain(
                    "キーチェーンがロックされています".into(),
                ))
            }
            fn set(&self, _: &str, _: &Secret) -> Result<(), AppError> {
                Err(AppError::Keychain(
                    "キーチェーンがロックされています".into(),
                ))
            }
            fn delete(&self, _: &str) -> Result<(), AppError> {
                Ok(())
            }
        }
        let keys = ApiKeys::new(Arc::new(Locked));
        block_on(async {
            let err = keys.has(ProviderId::Google).await.unwrap_err();
            assert_eq!(err.kind(), crate::error::ErrorKind::Keychain);
            let err = keys
                .set(ProviderId::Google, Secret::new("topsecret"))
                .await
                .unwrap_err();
            assert!(!err.to_string().contains("topsecret"));
            // 失敗したときは、保存したことにしない
            assert!(keys.cached(ProviderId::Google).is_none());
        });
    }

    /// 本物のキーチェーンに書いて読んで消す。macOS では許可の確認が出ることがあるので、既定では実行しない。
    /// `cargo test -p bibcontainer-app os_keychain -- --ignored`
    #[test]
    #[ignore = "OS のキーチェーンを使う"]
    fn os_keychain_roundtrip() {
        let account = format!("test-{}", std::process::id());
        let store = OsKeyStore;
        assert!(store.get(&account).unwrap().is_none());
        store.set(&account, &Secret::new("dummy-key-123")).unwrap();
        assert_eq!(
            store.get(&account).unwrap().unwrap().expose(),
            "dummy-key-123"
        );
        store.delete(&account).unwrap();
        assert!(store.get(&account).unwrap().is_none());
        store.delete(&account).unwrap();
    }
}
