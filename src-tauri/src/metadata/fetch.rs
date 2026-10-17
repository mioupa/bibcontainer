//! ISBN 1 件の書誌取得（`fetch_metadata`）。登録フォームの下書きを作る（DB には書かない）。
//!
//! 流れ: 有効な取得元に並行して問い合わせ → 統合 → 書影を取得して保存（`covers/`）→ 取得状態を決める → 下書きにする。
//! 通信できなかったとき（オフライン）も失敗にはしない。ISBN と 2段目の情報だけの下書き（`meta_status = pending`）を返す
//! ので、そのまま保存でき、あとで自動で再取得される（FR-SCAN-16、AT-11）。

use bibcontainer_metadata::isbn::to_isbn13;
use bibcontainer_metadata::{
    Candidate, MetaFetchResult, MetaMerged, MetaSecondTier, ProviderId, ProviderOutcome,
    ProviderReport, merge,
};
use serde::Serialize;
use ts_rs::TS;

use super::draft::{BookDraft, SavedCover, build_draft, decide_status};
use super::state::{Active, MetadataState};
use crate::error::AppError;

/// `fetch_metadata` の結果。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FetchMetadataResult {
    /// 統合結果（項目ごとの採用値と、他の取得元の値。編集フォームの候補に使う）
    pub merged: MetaMerged,
    /// 登録フォームの下書き（`meta_status` を含む）
    pub draft: BookDraft,
    /// 取得元ごとの状態
    pub reports: Vec<ProviderReport>,
    /// 通信する取得元がすべて通信エラーだった（オフライン）。取得待ちとして保存する。
    pub offline: bool,
    /// 取得して保存した書影のハッシュ。書影がなければ null。
    pub cover_hash: Option<String>,
}

/// 書影の取得結果。
#[derive(Debug, Clone, Default)]
pub(super) struct CoverOutcome {
    pub hash: Option<String>,
    pub provider: Option<ProviderId>,
    /// 試した URL がすべて通信エラーだった
    pub offline: bool,
}

impl CoverOutcome {
    pub fn saved(&self) -> Option<SavedCover<'_>> {
        Some(SavedCover {
            hash: self.hash.as_deref()?,
            provider: self.provider?,
        })
    }
}

impl MetadataState {
    /// 書影の URL を優先順位の順に試して、最初に使えた画像を保存する。URL がなければ何もしない。
    pub(super) async fn fetch_cover(&self, active: &Active, merged: &MetaMerged) -> CoverOutcome {
        if merged.covers_in_order().is_empty() {
            return CoverOutcome::default();
        }
        let result = active
            .service
            .acquire_cover(merged, &self.dirs.covers)
            .await;
        CoverOutcome {
            hash: result.saved.as_ref().map(|s| s.hash.clone()),
            provider: result.saved.as_ref().map(|s| s.provider),
            offline: result.offline,
        }
    }

    /// ISBN-13（ISBN-10・ハイフン・全角も可）の書誌を取得する。
    ///
    /// `second_tier` は 2段目のバーコードから分かった Cコードと本体価格（最優先の値として統合する）。
    pub async fn fetch_metadata(
        &self,
        isbn: &str,
        second_tier: Option<MetaSecondTier>,
    ) -> Result<FetchMetadataResult, AppError> {
        let isbn13 = to_isbn13(isbn).ok_or_else(|| {
            AppError::Validation(format!(
                "ISBN として読めません（チェックデジットを確かめてください）: {isbn}"
            ))
        })?;
        self.fetch_isbn13(&isbn13, second_tier, None).await
    }

    /// ISBN-13 の書誌を取得して、下書きにする（`fetch_metadata` と `fetch_search_hit` の共通の流れ）。
    ///
    /// `search_hit` は、キーワード検索で選ばれた 1 件の候補。ISBN での取得で、その取得元が「取れた」にならなかったとき
    /// （該当なし・通信エラー・取得元が無効、など）だけ、候補に足して統合し直す（`add_candidate`）。
    pub(super) async fn fetch_isbn13(
        &self,
        isbn13: &str,
        second_tier: Option<MetaSecondTier>,
        search_hit: Option<Candidate>,
    ) -> Result<FetchMetadataResult, AppError> {
        let active = self.active().await?;
        let req = active.request(isbn13, second_tier.clone(), false);
        let mut result = active.service.fetch(&req).await;
        if let Some(candidate) = search_hit {
            add_candidate(&mut result, &active, candidate, second_tier.as_ref());
        }
        let cover = self.fetch_cover(&active, &result.merged).await;
        Ok(assemble(result, isbn13, &cover))
    }
}

/// 取得結果に、検索結果の候補を足して、同じ優先順位で統合し直す。候補の取得元が、この取得で「取れた」なら、
/// その取得元の候補（ISBN での取得のほうが項目が多い）を使い、何もしない。オフラインの判定（`offline`）は変えない。
/// 取得の報告（`reports`）は、候補の取得元が「該当なし」だったときだけ「取れた」に直す（その取得元の値を使うので。
/// 直さないと、どの取得元も ISBN では見つからなかった本が、値があるのに `failed` になる）。通信・HTTP・解析の
/// 失敗はそのまま残す（`pending`・`partial` にして、あとで取り直させるため）。
fn add_candidate(
    result: &mut MetaFetchResult,
    active: &Active,
    candidate: Candidate,
    second_tier: Option<&MetaSecondTier>,
) {
    let found = result
        .reports
        .iter()
        .any(|r| r.provider == candidate.provider && r.outcome == ProviderOutcome::Found);
    if found {
        return;
    }
    for report in &mut result.reports {
        if report.provider == candidate.provider && report.outcome == ProviderOutcome::NotFound {
            report.outcome = ProviderOutcome::Found;
        }
    }
    result
        .candidates
        .retain(|c| c.provider != candidate.provider);
    result.candidates.push(candidate);
    let priorities = active
        .settings
        .priorities
        .for_profile(result.profile)
        .normalized(result.profile);
    result.merged = merge(&result.isbn13, &result.candidates, second_tier, &priorities);
}

/// 取得結果と書影から、返す形を組み立てる。
pub(super) fn assemble(
    result: MetaFetchResult,
    isbn13: &str,
    cover: &CoverOutcome,
) -> FetchMetadataResult {
    let status = decide_status(&result, cover.offline);
    let draft = build_draft(&result.merged, Some(isbn13), cover.saved(), status);
    tracing::info!(
        isbn13,
        status = status.as_str(),
        offline = result.offline,
        cover = cover.hash.is_some(),
        "書誌を取得しました"
    );
    FetchMetadataResult {
        merged: result.merged,
        draft,
        reports: result.reports,
        offline: result.offline,
        cover_hash: cover.hash.clone(),
    }
}
