//! 取得の流れ（オーケストレーション）。
//!
//! ISBN 1 件（または複数件）について、有効な取得元に並行して問い合わせ、結果を統合し、
//! 取得元ごとの状態（成功／該当なし／通信エラー／HTTP エラー／解析エラー）を返す。
//!
//! - 取得元どうしは並行に動く。同じ取得元への問い合わせの間隔と同時接続数は `Transport` が守る。
//! - 生の応答は `RawCache` に保存する。有効期限内（既定 30 日。該当なしは 1 日）ならネットワークに出ない（FR-META-10）。
//!   `force_refresh` を立てると読まずに取り直す（FR-META-13「書誌を再取得」）。
//!   通信エラー・HTTP エラー・解析エラーの応答は保存しない。
//! - 1 つの取得元が落ちても、ほかの取得元の結果で続ける（FR-META-11）。
//! - 有効な取得元のうち書誌の取得で通信するものが、すべて通信エラーなら `offline`（AT-11。呼び出し側が `meta_status` を
//!   `pending` にする）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cover::{self, MetaCoverResult, StoredCover};
use crate::error::{CoverError, FetchError};
use crate::isbn::to_isbn13;
use crate::merge::{MetaMerged, MetaPriorities, merge};
use crate::provider::{CacheEntry, Provider, ProviderInfo, RawCache, RawResponse};
use crate::transport::Transport;
use crate::types::{Candidate, MetaSecondTier, ProviderId};

/// 生の応答の有効期限の既定値（30 日）。
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// 「該当なし」の応答の有効期限の既定値（1 日）。新刊は少し遅れて収録されるため、短くする。
pub const DEFAULT_NOT_FOUND_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// 1 つの取得元の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ProviderOutcome {
    /// 取れた
    Found,
    /// 該当なし（その取得元に収録されていない）
    NotFound,
    /// つながらない・タイムアウト
    NetworkError { message: String },
    /// サーバが 2xx 以外を返した
    HttpError { status: u16, message: String },
    /// 応答の形が期待と違った
    ParseError { message: String },
}

impl ProviderOutcome {
    fn from_fetch_error(e: FetchError) -> Self {
        match e {
            FetchError::Network(message) => ProviderOutcome::NetworkError { message },
            FetchError::Http { status, message } => ProviderOutcome::HttpError { status, message },
            FetchError::Parse(message) => ProviderOutcome::ParseError { message },
        }
    }

    pub fn is_network_error(&self) -> bool {
        matches!(self, ProviderOutcome::NetworkError { .. })
    }
}

/// 取得元ごとの状態。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderReport {
    pub provider: ProviderId,
    pub outcome: ProviderOutcome,
    /// キャッシュ済みの生の応答を使った（通信していない）
    pub from_cache: bool,
    /// 書誌の取得で通信する取得元か（`ProviderInfo::needs_network`）
    pub network: bool,
}

/// ISBN 1 件の取得結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaFetchResult {
    pub isbn13: String,
    pub merged: MetaMerged,
    /// 取得元ごとの候補（統合前）
    pub candidates: Vec<Candidate>,
    pub reports: Vec<ProviderReport>,
    /// 通信する取得元がすべて通信エラーだった（オフラインとみなす）
    pub offline: bool,
}

impl MetaFetchResult {
    /// 取得元のうち、取れたもの（`Found`）の数。
    pub fn found_count(&self) -> usize {
        self.reports
            .iter()
            .filter(|r| r.outcome == ProviderOutcome::Found)
            .count()
    }

    /// 1 つでも取得に失敗した（通信・HTTP・解析のエラー）。`meta_status` を `partial` にする目安。
    pub fn has_error(&self) -> bool {
        self.reports.iter().any(|r| {
            matches!(
                r.outcome,
                ProviderOutcome::NetworkError { .. }
                    | ProviderOutcome::HttpError { .. }
                    | ProviderOutcome::ParseError { .. }
            )
        })
    }
}

/// 取得の依頼。
#[derive(Debug, Clone)]
pub struct FetchRequest {
    /// ISBN（ハイフン・全角・ISBN-10 でもよい。ISBN-13 にそろえて使う）
    pub isbn: String,
    /// 2段目から分かった Cコードと本体価格
    pub second_tier: Option<MetaSecondTier>,
    /// 有効な取得元
    pub enabled: Vec<ProviderId>,
    /// 項目ごとの優先順位
    pub priorities: MetaPriorities,
    /// キャッシュを読まずに取り直す（手で直した項目を守るのは呼び出し側の役目。FR-META-07）
    pub force_refresh: bool,
}

impl FetchRequest {
    /// 4 つの取得元すべてを有効にし、既定の優先順位で取る依頼。
    pub fn new(isbn: &str) -> Self {
        FetchRequest {
            isbn: isbn.to_string(),
            second_tier: None,
            enabled: ProviderId::ALL.to_vec(),
            priorities: MetaPriorities::default(),
            force_refresh: false,
        }
    }

    pub fn with_second_tier(mut self, tier: MetaSecondTier) -> Self {
        self.second_tier = Some(tier);
        self
    }

    pub fn with_enabled(mut self, enabled: Vec<ProviderId>) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_priorities(mut self, priorities: MetaPriorities) -> Self {
        self.priorities = priorities;
        self
    }

    pub fn force_refresh(mut self) -> Self {
        self.force_refresh = true;
        self
    }
}

/// 取得元と、その取得元で取る (ISBN, 強制再取得) の一覧。
type Job = (Arc<dyn Provider>, Vec<(String, bool)>);

/// 1 つの取得元の、1 つの ISBN の結果。
#[derive(Clone)]
struct Run {
    outcome: ProviderOutcome,
    candidate: Option<Candidate>,
    from_cache: bool,
}

pub struct MetadataService {
    providers: Vec<Arc<dyn Provider>>,
    transport: Arc<Transport>,
    cache: Option<Arc<dyn RawCache>>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    cache_ttl: Duration,
    not_found_ttl: Duration,
}

impl MetadataService {
    pub fn new(providers: Vec<Arc<dyn Provider>>, transport: Arc<Transport>) -> Self {
        MetadataService {
            providers,
            transport,
            cache: None,
            clock: Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
                    .unwrap_or(0)
            }),
            cache_ttl: DEFAULT_CACHE_TTL,
            not_found_ttl: DEFAULT_NOT_FOUND_TTL,
        }
    }

    pub fn with_cache(mut self, cache: Arc<dyn RawCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn with_ttl(mut self, cache_ttl: Duration, not_found_ttl: Duration) -> Self {
        self.cache_ttl = cache_ttl;
        self.not_found_ttl = not_found_ttl;
        self
    }

    /// 現在時刻（UNIX 秒）を返す関数を差し替える（テスト用）。
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    pub fn transport(&self) -> &Arc<Transport> {
        &self.transport
    }

    /// 統合結果の書影の URL を優先順位の順に試し、最初に使えた画像を `cover_dir` に保存する（FR-COVER-01〜03）。
    /// 結果の `offline` が `true` なら、あとで取り直す。
    pub async fn acquire_cover(&self, merged: &MetaMerged, cover_dir: &Path) -> MetaCoverResult {
        cover::acquire_cover(&self.transport, &merged.covers_in_order(), cover_dir).await
    }

    /// ユーザーが入力した画像 URL から書影を保存する（FR-COVER-05）。
    pub async fn import_cover_url(
        &self,
        url: &str,
        cover_dir: &Path,
    ) -> Result<StoredCover, CoverError> {
        cover::import_cover_url(&self.transport, cover_dir, url).await
    }

    /// 登録されている取得元の能力情報（設定画面・クレジット表示用）。
    pub fn providers_info(&self) -> Vec<ProviderInfo> {
        self.providers.iter().map(|p| p.info()).collect()
    }

    /// ISBN 1 件を取得して統合する。
    pub async fn fetch(&self, req: &FetchRequest) -> MetaFetchResult {
        self.fetch_many(std::slice::from_ref(req))
            .await
            .into_iter()
            .next()
            .expect("依頼 1 件に対して結果 1 件")
    }

    /// 複数の ISBN をまとめて取得する（一括の再取得）。結果は依頼と同じ順。
    /// まとめて取れる取得元（openBD）は 1 回の通信にまとめ、そうでない取得元は 1 件ずつ、レート制限を守って取る。
    pub async fn fetch_many(&self, reqs: &[FetchRequest]) -> Vec<MetaFetchResult> {
        let normalized: Vec<Option<String>> = reqs.iter().map(|r| to_isbn13(&r.isbn)).collect();

        // 取得元ごとに、必要な (ISBN, 強制再取得) を集める
        let mut jobs: Vec<Job> = Vec::new();
        for provider in &self.providers {
            let mut wanted: Vec<(String, bool)> = Vec::new();
            for (req, isbn) in reqs.iter().zip(&normalized) {
                let Some(isbn) = isbn else { continue };
                if !req.enabled.contains(&provider.id()) {
                    continue;
                }
                match wanted.iter_mut().find(|(i, _)| i == isbn) {
                    Some(entry) => entry.1 |= req.force_refresh,
                    None => wanted.push((isbn.clone(), req.force_refresh)),
                }
            }
            if !wanted.is_empty() {
                jobs.push((provider.clone(), wanted));
            }
        }

        let finished: Vec<(ProviderId, HashMap<String, Run>)> =
            join_all(jobs.iter().map(|(provider, wanted)| async move {
                (
                    provider.id(),
                    self.run_provider(provider.as_ref(), wanted).await,
                )
            }))
            .await;
        let runs: HashMap<ProviderId, HashMap<String, Run>> = finished.into_iter().collect();

        reqs.iter()
            .zip(normalized)
            .map(|(req, isbn)| self.assemble(req, isbn, &runs))
            .collect()
    }

    fn assemble(
        &self,
        req: &FetchRequest,
        isbn: Option<String>,
        runs: &HashMap<ProviderId, HashMap<String, Run>>,
    ) -> MetaFetchResult {
        let Some(isbn) = isbn else {
            tracing::warn!(isbn = %req.isbn, "ISBN として読めないため、取得しません");
            return MetaFetchResult {
                isbn13: req.isbn.clone(),
                merged: merge(&req.isbn, &[], req.second_tier.as_ref(), &req.priorities),
                candidates: Vec::new(),
                reports: Vec::new(),
                offline: false,
            };
        };
        let mut candidates = Vec::new();
        let mut reports = Vec::new();
        for provider in &self.providers {
            let id = provider.id();
            if !req.enabled.contains(&id) {
                continue;
            }
            let Some(run) = runs.get(&id).and_then(|m| m.get(&isbn)) else {
                continue;
            };
            if let Some(c) = &run.candidate {
                candidates.push(c.clone());
            }
            reports.push(ProviderReport {
                provider: id,
                outcome: run.outcome.clone(),
                from_cache: run.from_cache,
                network: provider.info().needs_network,
            });
        }
        let network_reports: Vec<&ProviderReport> = reports.iter().filter(|r| r.network).collect();
        let offline = !network_reports.is_empty()
            && network_reports.iter().all(|r| r.outcome.is_network_error());
        let merged = merge(
            &isbn,
            &candidates,
            req.second_tier.as_ref(),
            &req.priorities,
        );
        MetaFetchResult {
            isbn13: isbn,
            merged,
            candidates,
            reports,
            offline,
        }
    }

    /// 1 つの取得元について、複数の ISBN の結果を作る。
    async fn run_provider(
        &self,
        provider: &dyn Provider,
        wanted: &[(String, bool)],
    ) -> HashMap<String, Run> {
        let id = provider.id();
        let cacheable = provider.info().needs_network;
        let mut results: HashMap<String, Run> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();

        for (isbn, force) in wanted {
            if cacheable
                && !force
                && let Some(run) = self.cached_run(provider, isbn).await
            {
                results.insert(isbn.clone(), run);
            } else {
                missing.push(isbn.clone());
            }
        }

        if !missing.is_empty() {
            let fetched: Vec<(String, Result<RawResponse, FetchError>)> =
                if provider.info().supports_batch && missing.len() > 1 {
                    provider.fetch_raw_batch(&self.transport, &missing).await
                } else {
                    let mut v = Vec::with_capacity(missing.len());
                    for isbn in &missing {
                        v.push((
                            isbn.clone(),
                            provider.fetch_raw(&self.transport, isbn).await,
                        ));
                    }
                    v
                };
            for (isbn, raw) in fetched {
                let run = match raw {
                    Err(e) => {
                        tracing::warn!(provider = id.as_str(), %isbn, error = %e, "取得に失敗しました");
                        Run {
                            outcome: ProviderOutcome::from_fetch_error(e),
                            candidate: None,
                            from_cache: false,
                        }
                    }
                    Ok(raw) => match provider.parse(&raw) {
                        Err(e) => {
                            tracing::warn!(provider = id.as_str(), %isbn, error = %e, "応答を解析できませんでした");
                            Run {
                                outcome: ProviderOutcome::ParseError {
                                    message: e.to_string(),
                                },
                                candidate: None,
                                from_cache: false,
                            }
                        }
                        Ok(candidate) => {
                            if cacheable {
                                self.store(id, &isbn, raw.body, candidate.is_some()).await;
                            }
                            Run {
                                outcome: if candidate.is_some() {
                                    ProviderOutcome::Found
                                } else {
                                    ProviderOutcome::NotFound
                                },
                                candidate,
                                from_cache: false,
                            }
                        }
                    },
                };
                results.insert(isbn, run);
            }
        }
        results
    }

    /// 有効期限内のキャッシュがあれば、その結果を返す。解析できないキャッシュは使わない。
    async fn cached_run(&self, provider: &dyn Provider, isbn: &str) -> Option<Run> {
        let cache = self.cache.as_ref()?;
        let id = provider.id();
        let entry = match cache.get(id, isbn).await {
            Ok(Some(e)) => e,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(provider = id.as_str(), %isbn, error = %e, "キャッシュを読めませんでした");
                return None;
            }
        };
        let ttl = if entry.found {
            self.cache_ttl
        } else {
            self.not_found_ttl
        };
        let age = (self.clock)().saturating_sub(entry.fetched_at);
        if age < 0 || age >= i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX) {
            return None;
        }
        let raw = RawResponse {
            provider: id,
            isbn13: isbn.to_string(),
            body: entry.body,
        };
        match provider.parse(&raw) {
            Ok(candidate) => Some(Run {
                outcome: if candidate.is_some() {
                    ProviderOutcome::Found
                } else {
                    ProviderOutcome::NotFound
                },
                candidate,
                from_cache: true,
            }),
            Err(e) => {
                tracing::warn!(provider = id.as_str(), %isbn, error = %e, "キャッシュの応答を解析できないため、取り直します");
                None
            }
        }
    }

    async fn store(&self, id: ProviderId, isbn: &str, body: String, found: bool) {
        let Some(cache) = &self.cache else { return };
        let entry = CacheEntry {
            body,
            fetched_at: (self.clock)(),
            found,
        };
        if let Err(e) = cache.put(id, isbn, entry).await {
            tracing::warn!(provider = id.as_str(), %isbn, error = %e, "キャッシュに書けませんでした");
        }
    }
}
