//! 書籍の作成・変更・ゴミ箱・タグの一括変更。
//!
//! 変えたあとに `library-changed` を出す。`others` は、タグ・シリーズの件数が変わるとき（タグ・シリーズ
//! を含む変更、ゴミ箱への移動と復元、完全削除）に true にする。サイドバーのタグ・シリーズの取り直しのため。

use std::sync::Arc;

use bibcontainer_core::library::{self, ChangeReport, FieldChange, UpdateOptions};
use tauri::{AppHandle, Runtime, State};

use super::env;
use crate::database::Database;
use crate::error::AppError;
use crate::events::emit_library_changed;

/// 項目の変更が、タグ・シリーズの一覧（件数）に響くか。
fn touches_tags_or_series<'a>(fields: impl IntoIterator<Item = &'a str>) -> bool {
    fields
        .into_iter()
        .any(|f| matches!(f, "tags" | "series_id"))
}

/// 実際に書いた変更だけを通知する（同じ値の保存は Op も通知もなし）。
fn changed_reports(reports: &[ChangeReport]) -> (Vec<String>, bool) {
    let changed: Vec<&ChangeReport> = reports.iter().filter(|r| !r.written.is_empty()).collect();
    let others = changed
        .iter()
        .any(|r| touches_tags_or_series(r.written.iter().map(String::as_str)));
    (changed.iter().map(|r| r.id.clone()).collect(), others)
}

/// 書籍を 1 冊作る。ID を返す。
#[tauri::command]
pub async fn create_book<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    changes: Vec<FieldChange>,
) -> Result<String, AppError> {
    let others = touches_tags_or_series(changes.iter().map(|c| c.field.as_str()));
    let id = db
        .run(move |conn, _| Ok(library::create_book(conn, &env(), &changes)?))
        .await?;
    emit_library_changed(&app, vec![id.clone()], others);
    Ok(id)
}

/// 書籍を複数冊、1 つのトランザクションで作る（連続登録のまとめて保存）。ID を同じ順で返す。
#[tauri::command]
pub async fn create_books<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    books: Vec<Vec<FieldChange>>,
) -> Result<Vec<String>, AppError> {
    let others = books
        .iter()
        .any(|changes| touches_tags_or_series(changes.iter().map(|c| c.field.as_str())));
    let ids = db
        .run(move |conn, _| Ok(library::create_books(conn, &env(), &books)?))
        .await?;
    if !ids.is_empty() {
        emit_library_changed(&app, ids.clone(), others);
    }
    Ok(ids)
}

/// 書籍の項目を変える。手修正済みの項目を取得元の値が上書きしないかは、`changes` の由来と
/// `options.overrideManual` で決まる。
#[tauri::command]
pub async fn update_book<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    id: String,
    changes: Vec<FieldChange>,
    options: Option<UpdateOptions>,
) -> Result<ChangeReport, AppError> {
    let opts = options.unwrap_or_default();
    let report = db
        .run(move |conn, _| Ok(library::update_book(conn, &env(), &id, &changes, opts)?))
        .await?;
    let (books, others) = changed_reports(std::slice::from_ref(&report));
    if !books.is_empty() {
        emit_library_changed(&app, books, others);
    }
    Ok(report)
}

/// 複数の本に同じ変更を、1 つのトランザクションで行う。1 冊でも失敗したら全体を取り消す。
#[tauri::command]
pub async fn update_books<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
    changes: Vec<FieldChange>,
    options: Option<UpdateOptions>,
) -> Result<Vec<ChangeReport>, AppError> {
    let opts = options.unwrap_or_default();
    let reports = db
        .run(move |conn, _| Ok(library::update_books(conn, &env(), &ids, &changes, opts)?))
        .await?;
    let (books, others) = changed_reports(&reports);
    if !books.is_empty() {
        emit_library_changed(&app, books, others);
    }
    Ok(reports)
}

/// 複数の本のタグを一括変更する（`add` を足し、`remove` を外す）。変わった本の ID を返す。
#[tauri::command]
pub async fn modify_tags<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let changed = db
        .run(move |conn, _| Ok(library::modify_tags(conn, &env(), &ids, &add, &remove)?))
        .await?;
    if !changed.is_empty() {
        emit_library_changed(&app, changed.clone(), true);
    }
    Ok(changed)
}

/// 書籍をゴミ箱に移す。実際に変えた本の ID を返す。
#[tauri::command]
pub async fn trash_books<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let changed = db
        .run(move |conn, _| Ok(library::trash_books(conn, &env(), &ids)?))
        .await?;
    if !changed.is_empty() {
        emit_library_changed(&app, changed.clone(), true);
    }
    Ok(changed)
}

/// 書籍をゴミ箱から戻す。実際に変えた本の ID を返す。
#[tauri::command]
pub async fn restore_books<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let changed = db
        .run(move |conn, _| Ok(library::restore_books(conn, &env(), &ids)?))
        .await?;
    if !changed.is_empty() {
        emit_library_changed(&app, changed.clone(), true);
    }
    Ok(changed)
}

/// 書籍を完全削除する（戻せない）。実際に変えた本の ID を返す。
#[tauri::command]
pub async fn purge_books<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let changed = db
        .run(move |conn, _| Ok(library::purge_books(conn, &env(), &ids)?))
        .await?;
    if !changed.is_empty() {
        emit_library_changed(&app, changed.clone(), true);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: &str, written: &[&str]) -> ChangeReport {
        ChangeReport {
            id: id.to_owned(),
            written: written.iter().map(|s| (*s).to_owned()).collect(),
            skipped_manual: Vec::new(),
        }
    }

    #[test]
    fn only_books_with_written_fields_are_announced() {
        let reports = [
            report("a", &["title"]),
            report("b", &[]),
            report("c", &["memo"]),
        ];
        let (books, others) = changed_reports(&reports);
        assert_eq!(books, ["a", "c"]);
        assert!(
            !others,
            "書名・メモの変更は、タグ・シリーズの一覧に響かない"
        );
    }

    #[test]
    fn tag_and_series_changes_ask_for_the_other_lists_too() {
        assert!(changed_reports(&[report("a", &["title", "tags"])]).1);
        assert!(changed_reports(&[report("a", &["series_id"])]).1);
        assert!(!changed_reports(&[report("a", &["location_id", "rating"])]).1);
        assert!(touches_tags_or_series(["tags"]));
        assert!(!touches_tags_or_series(["title", "authors"]));
    }
}
