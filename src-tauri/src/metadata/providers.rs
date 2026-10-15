//! 取得元の設定・一覧・接続テスト・API キーの操作（FR-CONF-02／07）。

use std::time::Instant;

use bibcontainer_metadata::providers::google::{self, GoogleFailure};
use bibcontainer_metadata::{ProviderAuth, ProviderId, ProviderInfo, ProviderOutcome};
use serde::Serialize;
use ts_rs::TS;

use super::keychain::clean_key;
use super::settings::{self, ProviderSettings};
use super::state::MetadataState;
use crate::error::AppError;

/// 接続テストに使う ISBN を、取得元ごとに最大 3 件。見つかるまで順に試す（`test_provider`）。
///
/// 取得元に収録されている可能性が高い本を選ぶ。どれも実在し、ISBN-13 のチェックデジットは正しい。
/// - openBD・NDL（和書）: `おにぎりレシピ101`（AT-10。両方にある）、岩波文庫『こころ』、新潮文庫『雪国』。
/// - Google Books（和書と洋書）: 岩波文庫『こころ』、新潮文庫『吾輩は猫である』、Penguin Classics『Pride and
///   Prejudice』。和書の 2 冊は openBD・NDL で書誌が取れることを確かめてある（2026-10-15）。洋書は Open Library
///   で確かめた。Google 自体は、キーがなく 429 になるため、収録を確かめていない。
/// - Open Library（洋書。和書はほとんど収録がない）: Penguin Classics『Pride and Prejudice』、『Clean Code』、
///   Springer『Fundamentals of Radiation Materials Science』。3 冊とも、検索で版が見つかり、版の JSON が取れることを
///   確かめてある（2026-10-15）。
pub fn test_isbns(id: ProviderId) -> &'static [&'static str] {
    match id {
        ProviderId::Openbd | ProviderId::Ndl => {
            &["9784780802047", "9784003101117", "9784101001012"]
        }
        ProviderId::Google => &["9784003101117", "9784101010014", "9780141439518"],
        ProviderId::OpenLibrary => &["9780141439518", "9780132350884", "9781493934362"],
    }
}

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

/// 接続テストの結果のまとめ（画面に出す文言を決める）。
///
/// `outcome` は取得元の応答そのもの（HTTP のステータスや通信エラーのメッセージ）。こちらは、そこから
/// 「利用者が知りたいこと」（通信できたか、キーは有効か）を決めたもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProviderTestVerdict {
    /// 書誌が取れた（通信できた。キーを使う取得元ではキーも有効）
    Found,
    /// 応答は 200 だが、試したどの本も見つからなかった（通信できた。キーを使う取得元ではキーも有効）
    NotFound,
    /// API キーが無効（期限切れを含む）、または Books API が有効になっていない（Google。400・401・403）
    InvalidKey,
    /// 1 日の利用上限に達した（Google。429。キーは通っている）
    QuotaExceeded,
    /// 割り当ての上限が 0（Google。429。キーが使われていない、または割り当てのないプロジェクト）
    QuotaZero,
    /// つながらない・タイムアウト
    NetworkError,
    /// 上のどれでもない HTTP エラー（`outcome` にステータスとメッセージ）
    HttpError,
    /// 応答の形が期待と違った
    ParseError,
}

/// 接続テストの結果。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderTestResult {
    pub provider: ProviderId,
    /// 結果を決めた ISBN-13（取れた本。取れなければ、最後に試した本）
    pub isbn13: String,
    /// 試した ISBN-13（順に。取れたか、エラーになったら、そこで止める）
    pub tried: Vec<String>,
    /// 結果（`found`・`notFound`・`networkError`・`httpError`・`parseError`）
    pub outcome: ProviderOutcome,
    /// `outcome` を、利用者に伝える形にまとめたもの
    pub verdict: ProviderTestVerdict,
    /// 取れた書名（`found` のとき）
    pub title: Option<String>,
    pub publisher: Option<String>,
    pub c_code: Option<String>,
    /// 書影の URL がある
    pub has_cover: bool,
    /// かかった時間（ミリ秒。試した本すべての合計。取得元のレート制限による待ちを含む）
    pub elapsed_ms: u64,
}

/// 取得元の応答から、利用者に伝える結果を決める。HTTP エラーの意味づけは Google だけ（キーを使う取得元）。
fn verdict_of(id: ProviderId, outcome: &ProviderOutcome) -> ProviderTestVerdict {
    match outcome {
        ProviderOutcome::Found => ProviderTestVerdict::Found,
        ProviderOutcome::NotFound => ProviderTestVerdict::NotFound,
        ProviderOutcome::NetworkError { .. } => ProviderTestVerdict::NetworkError,
        ProviderOutcome::ParseError { .. } => ProviderTestVerdict::ParseError,
        ProviderOutcome::HttpError { status, message } if id == ProviderId::Google => {
            match google::classify_failure(*status, message) {
                GoogleFailure::InvalidKey => ProviderTestVerdict::InvalidKey,
                GoogleFailure::DailyQuota => ProviderTestVerdict::QuotaExceeded,
                GoogleFailure::ZeroQuota => ProviderTestVerdict::QuotaZero,
                GoogleFailure::Other => ProviderTestVerdict::HttpError,
            }
        }
        ProviderOutcome::HttpError { .. } => ProviderTestVerdict::HttpError,
    }
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

    /// 取得元ごとの既知の ISBN（`test_isbns`）を、キャッシュを使わずに順に取得して、結果を返す。
    /// 取れたか、エラー（通信・HTTP・解析）になったら、そこで止める。「該当なし」なら次の本を試す
    /// （収録されていない本があるのは普通のため）。レート制限は、ふだんの取得と同じ仕組みに従う。
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
        let started = Instant::now();
        let mut tried = Vec::new();
        let mut last = None;
        for isbn in test_isbns(id) {
            let req = bibcontainer_metadata::FetchRequest::new(isbn)
                .with_enabled(vec![id])
                .with_priorities(active.settings.priorities.clone())
                .force_refresh();
            let result = active.service.fetch(&req).await;
            let outcome = result
                .reports
                .iter()
                .find(|r| r.provider == id)
                .map(|r| r.outcome.clone())
                .ok_or_else(|| AppError::Internal("取得元の結果が返りませんでした".to_owned()))?;
            tried.push(result.isbn13.clone());
            let stop = outcome != ProviderOutcome::NotFound;
            last = Some((result, outcome));
            if stop {
                break;
            }
        }
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (result, outcome) =
            last.ok_or_else(|| AppError::Internal("テスト用の ISBN がありません".to_owned()))?;
        let candidate = result.candidates.iter().find(|c| c.provider == id);
        Ok(ProviderTestResult {
            provider: id,
            isbn13: result.isbn13.clone(),
            tried,
            verdict: verdict_of(id, &outcome),
            outcome,
            title: candidate.and_then(|c| c.title.clone()),
            publisher: candidate.and_then(|c| c.publisher.clone()),
            c_code: candidate.and_then(|c| c.c_code.clone()),
            has_cover: candidate.is_some_and(|c| !c.covers.is_empty()),
            elapsed_ms,
        })
    }
}
