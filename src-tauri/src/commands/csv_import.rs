//! CSV の取り込み（FR-IO-02）。処理はコア（`bibcontainer_core::csv_import`）にあり、ここはファイルの読み込みと
//! DB・イベントへのつなぎだけをする。
//!
//! ファイルの場所は、フロントがダイアログで選んだ絶対パス。ファイルは呼び出しごとに読み直す
//! （上限は `MAX_CSV_BYTES`）。画面は、開く → 確認 → 実行の順に、同じ `options` で呼ぶ。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bibcontainer_core::csv_import::{
    self, CsvEncoding, CsvImportOptions, CsvImportPlan, CsvImportResult, CsvPreview, MAX_CSV_BYTES,
};
use tauri::{AppHandle, Manager, Runtime, State};

use super::env;
use crate::database::Database;
use crate::error::AppError;
use crate::events::emit_library_changed;
use crate::metadata::MetadataState;

/// ファイルを読む（大きすぎるものは読まずに断る）。
fn read_csv(path: &Path) -> Result<Vec<u8>, AppError> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_CSV_BYTES as u64 {
        return Err(AppError::Validation(format!(
            "入力が不正です（path）: ファイルが大きすぎます（{} MB まで）",
            MAX_CSV_BYTES / 1024 / 1024
        )));
    }
    Ok(std::fs::read(path)?)
}

/// 重い処理（ファイルの読み込みと CSV の解析）を別スレッドで行う。
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|err| AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}")))?
}

/// CSV を開いて、文字コード・列・見本・行数と、列の割り当ての案を返す。DB は使わない。
/// `encoding` を省くと自動で判定する。
#[tauri::command]
pub async fn csv_import_open(
    path: String,
    encoding: Option<CsvEncoding>,
    has_header: bool,
) -> Result<CsvPreview, AppError> {
    let path = PathBuf::from(path);
    blocking(move || {
        let bytes = read_csv(&path)?;
        Ok(csv_import::preview(&bytes, encoding, has_header)?)
    })
    .await
}

/// 取り込みの下見。何冊取り込み、何行を飛ばし、どの行が誤りかを返す。DB は読むだけで、書かない。
#[tauri::command]
pub async fn csv_import_check(
    db: State<'_, Arc<Database>>,
    path: String,
    options: CsvImportOptions,
) -> Result<CsvImportPlan, AppError> {
    let path = PathBuf::from(path);
    let bytes = blocking(move || read_csv(&path)).await?;
    db.run(move |conn, _| Ok(csv_import::plan(conn, &bytes, &options)?))
        .await
}

/// 取り込む。1 つのトランザクションで、保管場所・シリーズ・本を作る。取り込んだ本があれば `library-changed`
/// を送り（保管場所・シリーズも増えうるので、ほかの一覧も取り直させる）、書誌の取得待ちの本があれば、
/// 取得待ちの再取得をすぐ始める（書誌と書影は、取り込んだ値を守ったまま補う。0028）。
#[tauri::command]
pub async fn csv_import_run<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    path: String,
    options: CsvImportOptions,
) -> Result<CsvImportResult, AppError> {
    let path = PathBuf::from(path);
    let bytes = blocking(move || read_csv(&path)).await?;
    let result = db
        .run(move |conn, _| Ok(csv_import::execute(conn, &env(), &bytes, &options)?))
        .await?;
    if result.imported > 0 {
        emit_library_changed(&app, result.book_ids.clone(), true);
    }
    if result.fetch_queued > 0
        && let Some(state) = app.try_state::<Arc<MetadataState>>()
    {
        state.inner().trigger_now();
    }
    Ok(result)
}

/// 取り込めなかった行（誤りの行）を、理由の列を足した CSV として `out_path` に書く。書いた行数を返す。DB は使わない。
#[tauri::command]
pub async fn csv_import_save_rejected(
    path: String,
    options: CsvImportOptions,
    out_path: String,
) -> Result<u32, AppError> {
    let path = PathBuf::from(path);
    let out_path = PathBuf::from(out_path);
    if !out_path.is_absolute() {
        return Err(AppError::Validation(
            "入力が不正です（outPath）: 保存先は絶対パスで指定してください".to_owned(),
        ));
    }
    blocking(move || {
        let bytes = read_csv(&path)?;
        Ok(csv_import::write_rejected_to_file(
            &bytes, &options, &out_path,
        )?)
    })
    .await
}
