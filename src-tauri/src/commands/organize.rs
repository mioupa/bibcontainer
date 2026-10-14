//! 保管場所・タグ・シリーズ。
//!
//! 変更したら `library-changed`（`others: true`）を出す。本の行が変わるもの（タグの改名、シリーズ名の
//! 変更）は、その本の ID も入れる。

use std::sync::Arc;

use bibcontainer_core::library::{self, LocationInfo, SeriesInfo, TagInfo};
use tauri::{AppHandle, Runtime, State};

use super::env;
use crate::database::Database;
use crate::error::AppError;
use crate::events::emit_library_changed;

// ---- 保管場所 ----

/// 保管場所の一覧（親が子より先に来る順。パスと深さ付き）。
#[tauri::command]
pub async fn list_locations(db: State<'_, Arc<Database>>) -> Result<Vec<LocationInfo>, AppError> {
    db.run(|conn, _| Ok(library::list_locations(conn)?)).await
}

/// 保管場所を作る（親の下の末尾）。ID を返す。
#[tauri::command]
pub async fn create_location<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    name: String,
    parent_id: Option<String>,
) -> Result<String, AppError> {
    let id = db
        .run(move |conn, _| {
            Ok(library::create_location(
                conn,
                &env(),
                &name,
                parent_id.as_deref(),
            )?)
        })
        .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(id)
}

/// 保管場所の名前を変える。
#[tauri::command]
pub async fn rename_location<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    id: String,
    name: String,
) -> Result<(), AppError> {
    db.run(move |conn, _| Ok(library::rename_location(conn, &env(), &id, &name)?))
        .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(())
}

/// 保管場所の親を変える（`null` でルートへ）。自分自身やその下へは移せない（`conflict`）。
#[tauri::command]
pub async fn move_location<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    id: String,
    new_parent_id: Option<String>,
) -> Result<(), AppError> {
    db.run(move |conn, _| {
        Ok(library::move_location(
            conn,
            &env(),
            &id,
            new_parent_id.as_deref(),
        )?)
    })
    .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(())
}

/// 同じ親の下の並び順を決める。
#[tauri::command]
pub async fn reorder_locations<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    parent_id: Option<String>,
    ordered_ids: Vec<String>,
) -> Result<(), AppError> {
    db.run(move |conn, _| {
        Ok(library::reorder_locations(
            conn,
            &env(),
            parent_id.as_deref(),
            &ordered_ids,
        )?)
    })
    .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(())
}

/// 保管場所を削除する。子の保管場所や本が残っていれば `conflict`。
#[tauri::command]
pub async fn delete_location<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    id: String,
) -> Result<(), AppError> {
    db.run(move |conn, _| Ok(library::delete_location(conn, &env(), &id)?))
        .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(())
}

// ---- タグ ----

/// タグの一覧（名前順。件数はゴミ箱を除く）。
#[tauri::command]
pub async fn list_tags(db: State<'_, Arc<Database>>) -> Result<Vec<TagInfo>, AppError> {
    db.run(|conn, _| Ok(library::list_tags(conn)?)).await
}

/// タグの名前を変える。`/` で区切った子孫（`研究/SiC`）もまとめて変わる。変わった本の ID を返す。
#[tauri::command]
pub async fn rename_tag<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    old: String,
    new: String,
) -> Result<Vec<String>, AppError> {
    let changed = db
        .run(move |conn, _| Ok(library::rename_tag(conn, &env(), &old, &new)?))
        .await?;
    emit_library_changed(&app, changed.clone(), true);
    Ok(changed)
}

/// タグの色を設定する（`null` で外す）。tag エンティティの ID を返す。
#[tauri::command]
pub async fn set_tag_color<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    name: String,
    color: Option<String>,
) -> Result<String, AppError> {
    let id = db
        .run(move |conn, _| {
            Ok(library::set_tag_color(
                conn,
                &env(),
                &name,
                color.as_deref(),
            )?)
        })
        .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(id)
}

// ---- シリーズ ----

/// シリーズの一覧（ヨミ順。冊数はゴミ箱を除く）。
#[tauri::command]
pub async fn list_series(db: State<'_, Arc<Database>>) -> Result<Vec<SeriesInfo>, AppError> {
    db.run(|conn, _| Ok(library::list_series(conn)?)).await
}

/// 名前が一致するシリーズの ID を返す。無ければ作る。
#[tauri::command]
pub async fn find_or_create_series<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    name: String,
) -> Result<String, AppError> {
    let id = db
        .run(move |conn, _| Ok(library::find_or_create_series(conn, &env(), &name)?))
        .await?;
    emit_library_changed(&app, Vec::new(), true);
    Ok(id)
}

/// シリーズの名前を変える。そのシリーズの本の行（`seriesName`）も変わるので、本の ID も知らせる。
#[tauri::command]
pub async fn rename_series<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
    id: String,
    name: String,
) -> Result<(), AppError> {
    let books = db
        .run(move |conn, _| {
            library::rename_series(conn, &env(), &id, &name)?;
            Ok(library::book_ids_in_series(conn, &id)?)
        })
        .await?;
    emit_library_changed(&app, books, true);
    Ok(())
}
