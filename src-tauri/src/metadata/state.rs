//! 書誌取得の状態（managed state）。
//!
//! 設定（`providers`）とキー（キーチェーン）から `MetadataService` を組み立てて持ち、設定やキーが変わったら
//! 作り直す（`reload`）。最初の取得のときに遅れて組み立てる（起動を、キーチェーンの確認で止めないため）。
//!
//! 実際に使う取得元（`Active::active_providers`）は「利用者が有効にしている」かつ「認証が必須ならキーがある」もの。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use bibcontainer_metadata::{
    FetchRequest, MetaSecondTier, MetadataService, ProviderAuth, ProviderId, ProviderInfo,
    Transport, default_providers, user_agent,
};

use super::cache::DbRawCache;
use super::keychain::{ApiKeys, KeyStore};
use super::settings::{self, ProviderSettings};
use crate::database::Database;
use crate::error::AppError;

/// 蔵書が変わったことを知らせる関数（本の ID の一覧、本以外（シリーズなど）も変わったか）。
/// 本番は `events::emit_library_changed`。
pub type Notifier = Arc<dyn Fn(Vec<String>, bool) + Send + Sync>;

/// User-Agent（`bibcontainer/<版> (+<連絡先>)`）から通信の層を作る関数。テストでは偽の HTTP に差し替える。
pub type TransportFactory = Arc<dyn Fn(&str) -> Result<Arc<Transport>, AppError> + Send + Sync>;

/// 書影の置き場所。
#[derive(Debug, Clone)]
pub struct CoverDirs {
    /// 書影の本体（`app_local_data_dir/covers`）。`<先頭 2 文字>/<ハッシュ>.jpg`。
    pub covers: PathBuf,
    /// サムネイルのキャッシュ（`app_cache_dir/thumbs`）。`<長辺>/<先頭 2 文字>/<ハッシュ>.jpg`。
    pub thumbs: PathBuf,
}

/// ある時点の設定とキーから組み立てた取得の道具一式。
pub struct Active {
    pub service: Arc<MetadataService>,
    pub settings: ProviderSettings,
    /// 実際に問い合わせる取得元（有効で、認証が必須ならキーがある）。
    pub active_providers: Vec<ProviderId>,
    pub infos: Vec<ProviderInfo>,
    /// キーが保存されている取得元（認証を使うもののうち）。
    pub keys_present: BTreeSet<ProviderId>,
}

impl Active {
    pub fn info(&self, id: ProviderId) -> Option<&ProviderInfo> {
        self.infos.iter().find(|i| i.id == id)
    }

    /// 取得の依頼を作る。
    pub fn request(
        &self,
        isbn: &str,
        second_tier: Option<MetaSecondTier>,
        force_refresh: bool,
    ) -> FetchRequest {
        let mut req = FetchRequest::new(isbn)
            .with_enabled(self.active_providers.clone())
            .with_priorities(self.settings.priorities.clone());
        if let Some(tier) = second_tier {
            req = req.with_second_tier(tier);
        }
        if force_refresh {
            req = req.force_refresh();
        }
        req
    }

    /// 書誌で通信する取得元を 1 つでも使うか。
    pub fn has_network_provider(&self) -> bool {
        self.active_providers
            .iter()
            .any(|id| self.info(*id).is_some_and(|i| i.needs_network))
    }
}

pub struct MetadataState {
    pub(super) db: Arc<Database>,
    pub(super) dirs: CoverDirs,
    pub(super) keys: ApiKeys,
    app_version: String,
    transport_factory: TransportFactory,
    pub(super) notifier: Notifier,
    current: Mutex<Option<Arc<Active>>>,
    build_lock: tokio::sync::Mutex<()>,
    /// 再取得（手動・取得待ちの自動）が同時に 2 つ走らないようにする。
    pub(super) refetch_lock: tokio::sync::Mutex<()>,
    /// 取得待ちの自動再取得を最後に始めた時刻（`trigger` が間を空けるために見る）。
    pub(super) last_pass: Mutex<Option<Instant>>,
}

impl MetadataState {
    pub fn new(
        db: Arc<Database>,
        dirs: CoverDirs,
        key_store: Arc<dyn KeyStore>,
        app_version: String,
        transport_factory: TransportFactory,
        notifier: Notifier,
    ) -> Arc<Self> {
        Arc::new(MetadataState {
            db,
            dirs,
            keys: ApiKeys::new(key_store),
            app_version,
            transport_factory,
            notifier,
            current: Mutex::new(None),
            build_lock: tokio::sync::Mutex::new(()),
            refetch_lock: tokio::sync::Mutex::new(()),
            last_pass: Mutex::new(None),
        })
    }

    pub fn dirs(&self) -> &CoverDirs {
        &self.dirs
    }

    fn current(&self) -> Option<Arc<Active>> {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_current(&self, active: Option<Arc<Active>>) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = active;
    }

    /// 今の道具一式。まだ組み立てていなければ組み立てる。
    pub async fn active(&self) -> Result<Arc<Active>, AppError> {
        if let Some(a) = self.current() {
            return Ok(a);
        }
        let _guard = self.build_lock.lock().await;
        // 待っている間に、別の呼び出しが組み立てたかもしれない
        if let Some(a) = self.current() {
            return Ok(a);
        }
        let active = Arc::new(self.build().await?);
        self.set_current(Some(Arc::clone(&active)));
        Ok(active)
    }

    /// 設定やキーが変わったあとに、道具一式を作り直す。
    pub async fn reload(&self) -> Result<Arc<Active>, AppError> {
        let _guard = self.build_lock.lock().await;
        let active = Arc::new(self.build().await?);
        self.set_current(Some(Arc::clone(&active)));
        Ok(active)
    }

    async fn build(&self) -> Result<Active, AppError> {
        let settings = self.db.run(|conn, _| settings::load(conn)).await?;
        // キーを読めなくても、キーなしで使える取得元は動かす
        let google_key = match self.keys.get(ProviderId::Google).await {
            Ok(key) => key,
            Err(err) => {
                tracing::warn!(error = %err, "Google Books の API キーを読めませんでした。キーなしとして続けます");
                None
            }
        };
        let transport =
            (self.transport_factory)(&user_agent(&self.app_version, &settings.contact_url))?;
        let service = MetadataService::new(
            default_providers(google_key.as_ref().map(|k| k.expose().to_owned())),
            transport,
        )
        .with_cache(Arc::new(DbRawCache::new(Arc::clone(&self.db))));
        let infos = service.providers_info();

        let mut keys_present = BTreeSet::new();
        for info in &infos {
            if info.auth == ProviderAuth::None {
                continue;
            }
            match self.keys.has(info.id).await {
                Ok(true) => {
                    keys_present.insert(info.id);
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(provider = info.id.as_str(), error = %err, "API キーの有無を確かめられませんでした");
                }
            }
        }

        let active_providers: Vec<ProviderId> = infos
            .iter()
            .filter(|info| settings.is_enabled(info.id))
            .filter(|info| {
                !matches!(info.auth, ProviderAuth::Required { .. })
                    || keys_present.contains(&info.id)
            })
            .map(|info| info.id)
            .collect();
        tracing::info!(
            providers = ?active_providers.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "書誌の取得元を組み立てました"
        );
        Ok(Active {
            service: Arc::new(service),
            settings,
            active_providers,
            infos,
            keys_present,
        })
    }
}
