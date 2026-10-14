//! 一覧・検索・詳細・重複チェックなどの参照（DB は変えない）。

use std::sync::Arc;

use bibcontainer_core::library::{self, IsbnMatch, StoreCount};
use bibcontainer_core::{BookDetail, ListRow};
use tauri::State;

use crate::database::Database;
use crate::error::AppError;

/// 全書籍の一覧用の行（完全削除したものを除く。ゴミ箱のものは `deleted: true` で含む）。起動時に一度取る。
#[tauri::command]
pub async fn list_books(db: State<'_, Arc<Database>>) -> Result<Vec<ListRow>, AppError> {
    db.run(|conn, _| Ok(bibcontainer_core::list_rows(conn)?))
        .await
}

/// 指定した本の行（指定した順）。存在しない ID・完全削除した ID は含めない。
/// `library-changed` を受けたフロントが、変わった行だけ取り直すのに使う。
#[tauri::command]
pub async fn get_rows(
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
) -> Result<Vec<ListRow>, AppError> {
    db.run(move |conn, _| Ok(bibcontainer_core::get_rows(conn, &ids)?))
        .await
}

/// 書籍 1 冊の詳細（同期する全項目と、取得元・手修正の記録）。無ければ `not_found`。
#[tauri::command]
pub async fn get_book_detail(
    db: State<'_, Arc<Database>>,
    id: String,
) -> Result<BookDetail, AppError> {
    db.run(move |conn, _| Ok(bibcontainer_core::get_book_detail(conn, &id)?))
        .await
}

/// 検索語に一致した本の ID（登録順）。語は空白で区切って AND、空なら全冊。
#[tauri::command]
pub async fn search_books(
    db: State<'_, Arc<Database>>,
    query: String,
) -> Result<Vec<String>, AppError> {
    db.run(move |conn, _| Ok(bibcontainer_core::search_books(conn, &query)?))
        .await
}

/// ISBN が一致する本（重複チェック）。ハイフン・ISBN-10 を許す。
#[tauri::command]
pub async fn find_by_isbn(
    db: State<'_, Arc<Database>>,
    isbn: String,
) -> Result<Vec<IsbnMatch>, AppError> {
    db.run(move |conn, _| Ok(library::find_by_isbn(conn, &isbn)?))
        .await
}

/// 購入店の候補（使った回数の多い順）。
#[tauri::command]
pub async fn purchase_store_suggestions(
    db: State<'_, Arc<Database>>,
) -> Result<Vec<StoreCount>, AppError> {
    db.run(|conn, _| Ok(library::purchase_store_suggestions(conn)?))
        .await
}

/// 書誌の取得待ちの本の ID。
#[tauri::command]
pub async fn pending_book_ids(db: State<'_, Arc<Database>>) -> Result<Vec<String>, AppError> {
    db.run(|conn, _| Ok(library::pending_book_ids(conn)?)).await
}
