//! 書誌取得の IPC コマンド。処理は `MetadataState` のメソッドにあり、ここは引数の受け渡しだけをする。
//!
//! 引数名は JS 側では camelCase（`second_tier` → `secondTier`、`override_manual` → `overrideManual`）。
//! エラーは `{ kind, message }`（`AppError`）。キーを返すコマンドはない。

use std::sync::Arc;

use bibcontainer_metadata::{MetaSecondTier, ProviderId};
use tauri::State;
use tauri::ipc::{InvokeBody, Request};

use super::fetch::FetchMetadataResult;
use super::providers::{ProviderStatus, ProviderTestResult};
use super::refetch::RefetchSummary;
use super::register::NewBookDraft;
use super::settings::ProviderSettings;
use super::state::MetadataState;
use crate::error::AppError;

type Metadata<'a> = State<'a, Arc<MetadataState>>;

/// 取得元の設定（有効・無効、項目ごとの優先順位、User-Agent の連絡先）。欠けた項目は既定値で補う。
#[tauri::command]
pub async fn get_provider_settings(state: Metadata<'_>) -> Result<ProviderSettings, AppError> {
    state.get_provider_settings().await
}

/// 取得元の設定を保存する。検証・補完した設定を返す。保存したあと、取得の道具一式を作り直す。
#[tauri::command]
pub async fn set_provider_settings(
    settings: ProviderSettings,
    state: Metadata<'_>,
) -> Result<ProviderSettings, AppError> {
    let saved = state.set_provider_settings(settings).await?;
    // 取得元を有効にしたときに、取得待ちの本をすぐ試す（間もなければ見送られる）
    state.inner().trigger();
    Ok(saved)
}

/// 取得元の一覧（能力情報・クレジット表記に、有効・無効とキーの有無を足したもの）。
#[tauri::command]
pub async fn list_providers(state: Metadata<'_>) -> Result<Vec<ProviderStatus>, AppError> {
    state.list_providers().await
}

/// 既知の ISBN を、キャッシュを使わずに取得して、取得元の接続を確かめる。
#[tauri::command]
pub async fn test_provider(
    id: ProviderId,
    state: Metadata<'_>,
) -> Result<ProviderTestResult, AppError> {
    state.test_provider(id).await
}

/// API キーを OS のキーチェーンに保存する（上書き）。
#[tauri::command]
pub async fn set_api_key(
    provider: ProviderId,
    key: String,
    state: Metadata<'_>,
) -> Result<(), AppError> {
    state.set_api_key(provider, &key).await?;
    state.inner().trigger();
    Ok(())
}

/// API キーを削除する。
#[tauri::command]
pub async fn delete_api_key(provider: ProviderId, state: Metadata<'_>) -> Result<(), AppError> {
    state.delete_api_key(provider).await
}

/// API キーが保存されているか。
#[tauri::command]
pub async fn has_api_key(provider: ProviderId, state: Metadata<'_>) -> Result<bool, AppError> {
    state.has_api_key(provider).await
}

/// ISBN の書誌を取得して、登録フォームの下書きを返す（DB には書かない）。
#[tauri::command]
pub async fn fetch_metadata(
    isbn13: String,
    second_tier: Option<MetaSecondTier>,
    state: Metadata<'_>,
) -> Result<FetchMetadataResult, AppError> {
    state.fetch_metadata(&isbn13, second_tier).await
}

/// 下書きから本を作る（全体で 1 トランザクション）。新しい本の ID を渡した順に返す。
#[tauri::command]
pub async fn create_books_from_drafts(
    drafts: Vec<NewBookDraft>,
    state: Metadata<'_>,
) -> Result<Vec<String>, AppError> {
    state.create_books_from_drafts(drafts).await
}

/// 選んだ本の書誌を再取得する。`override_manual` が偽なら、手で直した項目は上書きしない。
#[tauri::command]
pub async fn refetch_books(
    ids: Vec<String>,
    override_manual: bool,
    state: Metadata<'_>,
) -> Result<RefetchSummary, AppError> {
    state.refetch_books(ids, override_manual).await
}

/// 取得待ちの本の再取得を、今すぐ試す（すぐ戻る。処理は裏で行う）。
#[tauri::command]
pub async fn trigger_pending_refetch(state: Metadata<'_>) -> Result<(), AppError> {
    state.inner().trigger();
    Ok(())
}

/// 画像ファイルから書影を取り込む。保存した書影のハッシュを返す。
#[tauri::command]
pub async fn import_cover_file(path: String, state: Metadata<'_>) -> Result<String, AppError> {
    state.import_cover_file(&path).await
}

/// 画像のバイト列から書影を取り込む。保存した書影のハッシュを返す。
///
/// 呼び出し方は 2 つ。`invoke("import_cover_bytes", uint8Array)`（バイナリのまま送る。大きい画像はこちら）か、
/// `invoke("import_cover_bytes", { bytes: [..] })`（数値の配列）。
#[tauri::command]
pub async fn import_cover_bytes(
    request: Request<'_>,
    state: Metadata<'_>,
) -> Result<String, AppError> {
    let bytes = bytes_from_body(request.body())?;
    state.import_cover_bytes(bytes).await
}

/// 利用者が入力した画像の URL（http・https）から書影を取り込む。保存した書影のハッシュを返す。
#[tauri::command]
pub async fn import_cover_url(url: String, state: Metadata<'_>) -> Result<String, AppError> {
    state.import_cover_url(&url).await
}

/// `import_cover_bytes` の引数から、画像のバイト列を取り出す。
pub fn bytes_from_body(body: &InvokeBody) -> Result<Vec<u8>, AppError> {
    match body {
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        InvokeBody::Json(value) => {
            let list = value
                .get("bytes")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    AppError::Validation(
                        "画像のデータを渡してください（Uint8Array、または { bytes: number[] }）"
                            .to_owned(),
                    )
                })?;
            list.iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u8::try_from(n).ok())
                        .ok_or_else(|| {
                            AppError::Validation(
                                "画像のデータに 0〜255 以外の値があります".to_owned(),
                            )
                        })
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn bytes_come_from_a_raw_body_or_a_json_array() {
        assert_eq!(
            bytes_from_body(&InvokeBody::Raw(vec![1, 2, 255])).unwrap(),
            vec![1, 2, 255]
        );
        assert_eq!(
            bytes_from_body(&InvokeBody::Json(json!({ "bytes": [0, 1, 255] }))).unwrap(),
            vec![0, 1, 255]
        );
        for bad in [
            json!({}),
            json!({ "bytes": "abc" }),
            json!({ "bytes": [256] }),
            json!({ "bytes": [-1] }),
            json!({ "bytes": [1.5] }),
            json!([1, 2]),
            json!(null),
        ] {
            assert!(
                matches!(
                    bytes_from_body(&InvokeBody::Json(bad.clone())),
                    Err(AppError::Validation(_))
                ),
                "{bad}"
            );
        }
    }
}
