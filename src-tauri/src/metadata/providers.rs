//! 取得元の設定・一覧・接続テスト・API キーの操作（FR-CONF-02／07）。

use std::time::Instant;

use bibcontainer_metadata::{ProviderAuth, ProviderId, ProviderInfo, ProviderOutcome};
use serde::Serialize;
use ts_rs::TS;

use super::keychain::clean_key;
use super::settings::{self, ProviderSettings};
use super::state::MetadataState;
use crate::error::AppError;

/// 接続テストに使う ISBN（AT-10 の `おにぎりレシピ101`。openBD・NDL にある）。
pub const TEST_ISBN: &str = "9784780802047";

/// 設定画面と「このアプリについて」に出す、取得元 1 つの情報。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderStatus {
    /// 能力情報（表示名・認証・取れる項目・レート制限・クレジット表記）
    #[serde(flatten)]
    #[ts(flatten)]
    pub info: ProviderInfo,
    /// 利用者が有効にしている（設定）
    pub enabled: bool,
    /// API キーが保存されている（認証を使わない取得元は常に false）
    pub has_key: bool,
    /// 実際に使う（有効で、認証が必須ならキーがある）
    pub active: bool,
}

/// 接続テストの結果。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderTestResult {
    pub provider: ProviderId,
    /// 問い合わせた ISBN-13
    pub isbn13: String,
    /// 結果（`found`・`notFound`・`networkError`・`httpError`・`parseError`）
    pub outcome: ProviderOutcome,
    /// 取れた書名（`found` のとき）
    pub title: Option<String>,
    pub publisher: Option<String>,
    pub c_code: Option<String>,
    /// 書影の URL がある
    pub has_cover: bool,
    /// かかった時間（ミリ秒）
    pub elapsed_ms: u64,
}

impl MetadataState {
    pub async fn get_provider_settings(&self) -> Result<ProviderSettings, AppError> {
        self.db.run(|conn, _| settings::load(conn)).await
    }

    /// 設定を保存して、取得の道具一式を作り直す。保存した（検証・補完済みの）設定を返す。
    pub async fn set_provider_settings(
        &self,
        new: ProviderSettings,
    ) -> Result<ProviderSettings, AppError> {
        let saved = self
            .db
            .run(move |conn, _| settings::save(conn, new))
            .await?;
        self.reload().await?;
        Ok(saved)
    }

    /// 取得元の一覧（有効・無効とキーの有無つき）。
    pub async fn list_providers(&self) -> Result<Vec<ProviderStatus>, AppError> {
        let active = self.active().await?;
        Ok(active
            .infos
            .iter()
            .map(|info| ProviderStatus {
                info: info.clone(),
                enabled: active.settings.is_enabled(info.id),
                has_key: active.keys_present.contains(&info.id),
                active: active.active_providers.contains(&info.id),
            })
            .collect())
    }

    /// キーを使う取得元か。使わないものにキーを保存させない。
    async fn require_key_provider(&self, id: ProviderId) -> Result<ProviderInfo, AppError> {
        let active = self.active().await?;
        let info = active
            .info(id)
            .cloned()
            .ok_or_else(|| AppError::Validation(format!("知らない取得元です: {}", id.as_str())))?;
        if info.auth == ProviderAuth::None {
            return Err(AppError::Validation(format!(
                "{} は API キーを使いません",
                info.display_name
            )));
        }
        Ok(info)
    }

    /// API キーを OS のキーチェーンに保存する（上書き）。保存したあと、取得の道具一式を作り直す。
    pub async fn set_api_key(&self, id: ProviderId, key: &str) -> Result<(), AppError> {
        self.require_key_provider(id).await?;
        let key = clean_key(key)?;
        self.keys.set(id, key).await?;
        self.reload().await?;
        tracing::info!(provider = id.as_str(), "API キーを保存しました");
        Ok(())
    }

    /// API キーを削除する（なくてもエラーにしない）。
    pub async fn delete_api_key(&self, id: ProviderId) -> Result<(), AppError> {
        self.require_key_provider(id).await?;
        self.keys.delete(id).await?;
        self.reload().await?;
        tracing::info!(provider = id.as_str(), "API キーを削除しました");
        Ok(())
    }

    /// API キーが保存されているか（キーそのものは返さない）。
    pub async fn has_api_key(&self, id: ProviderId) -> Result<bool, AppError> {
        let active = self.active().await?;
        let info = active
            .info(id)
            .ok_or_else(|| AppError::Validation(format!("知らない取得元です: {}", id.as_str())))?;
        if info.auth == ProviderAuth::None {
            return Ok(false);
        }
        self.keys.has(id).await
    }

    /// 既知の ISBN（`TEST_ISBN`）を、キャッシュを使わずに取得して結果を返す。
    /// 設定で無効にしている取得元でも試せる。認証が必須の取得元は、キーがなければ試さない。
    pub async fn test_provider(&self, id: ProviderId) -> Result<ProviderTestResult, AppError> {
        let active = self.active().await?;
        let info = active
            .info(id)
            .ok_or_else(|| AppError::Validation(format!("知らない取得元です: {}", id.as_str())))?;
        if matches!(info.auth, ProviderAuth::Required { .. }) && !active.keys_present.contains(&id)
        {
            return Err(AppError::Validation(format!(
                "{} の API キーが設定されていません",
                info.display_name
            )));
        }
        let req = bibcontainer_metadata::FetchRequest::new(TEST_ISBN)
            .with_enabled(vec![id])
            .force_refresh();
        let started = Instant::now();
        let result = active.service.fetch(&req).await;
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let outcome = result
            .reports
            .iter()
            .find(|r| r.provider == id)
            .map(|r| r.outcome.clone())
            .ok_or_else(|| AppError::Internal("取得元の結果が返りませんでした".to_owned()))?;
        let candidate = result.candidates.iter().find(|c| c.provider == id);
        Ok(ProviderTestResult {
            provider: id,
            isbn13: result.isbn13,
            outcome,
            title: candidate.and_then(|c| c.title.clone()),
            publisher: candidate.and_then(|c| c.publisher.clone()),
            c_code: candidate.and_then(|c| c.c_code.clone()),
            has_cover: candidate.is_some_and(|c| !c.covers.is_empty()),
            elapsed_ms,
        })
    }
}
