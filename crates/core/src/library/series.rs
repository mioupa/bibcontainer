//! シリーズ。FR-BOOK-07。シリーズ名の候補は取得元から出すので、名前で探して無ければ作る。

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::Serialize;
use serde_json::json;
use ts_rs::TS;

use super::Env;
use crate::db::write_tx;
use crate::error::{Error, Result};
use crate::ids::new_entity_id;
use crate::normalize::normalize_for_search;
use crate::schema::{Entity, Fields};
use crate::store::record_change;

/// シリーズ 1 件。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SeriesInfo {
    pub id: String,
    pub name: String,
    pub name_kana: Option<String>,
    /// ゴミ箱に入れていない本の数。
    pub book_count: i64,
}

fn require_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::validation("name", "シリーズ名が空です"));
    }
    Ok(name.to_owned())
}

/// 正規化した名前（NFKC・小文字・ひらがな）が一致するシリーズを探し、無ければ作る。ID を返す。
/// 一致が複数あるときは ID が最小（最古）のもの。削除したシリーズは探さない。
pub fn find_or_create_series(conn: &mut Connection, env: &Env<'_>, name: &str) -> Result<String> {
    let tx = write_tx(conn)?;
    let id = find_or_create_series_in(&tx, env, name)?;
    tx.commit()?;
    Ok(id)
}

/// `find_or_create_series` の、呼び出し側のトランザクションの中で動く版。
pub fn find_or_create_series_in(tx: &Transaction<'_>, env: &Env<'_>, name: &str) -> Result<String> {
    let name = require_name(name)?;
    let key = normalize_for_search(&name);
    let mut stmt = tx.prepare("SELECT id, name FROM series WHERE deleted = 0 ORDER BY id")?;
    let found = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .find(|(_, existing)| normalize_for_search(existing.trim()) == key);
    if let Some((id, _)) = found {
        return Ok(id);
    }
    let id = new_entity_id();
    let set: Fields = Entity::Series
        .fields()
        .iter()
        .map(|def| (def.name.to_owned(), def.default_value()))
        .chain([
            ("name".to_owned(), json!(name)),
            ("created_at".to_owned(), json!(env.now_iso()?)),
        ])
        .collect();
    record_change(tx, env.clock, Entity::Series, &id, set)?;
    Ok(id)
}

/// シリーズ名を変える。そのシリーズの本の検索用の派生データも更新される。
pub fn rename_series(conn: &mut Connection, env: &Env<'_>, id: &str, name: &str) -> Result<()> {
    let name = require_name(name)?;
    let tx = write_tx(conn)?;
    let current: Option<String> = tx
        .query_row(
            "SELECT name FROM series WHERE id = ?1 AND deleted = 0",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    match current {
        None => {
            return Err(Error::NotFound {
                what: "シリーズ",
                id: id.to_owned(),
            });
        }
        Some(current) if current == name => {}
        Some(_) => {
            record_change(
                &tx,
                env.clock,
                Entity::Series,
                id,
                [("name".to_owned(), json!(name))],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// シリーズに属する本の ID（完全削除したものを除く。ゴミ箱の本を含む）を、登録順に返す。
/// シリーズ名の変更で一覧の行（`seriesName`）が変わる本を知らせるのに使う。
pub fn book_ids_in_series(conn: &Connection, series_id: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM books WHERE series_id = ?1 AND purged = 0 ORDER BY id")?;
    let ids = stmt
        .query_map([series_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// シリーズの一覧（削除していないもの）。ヨミ（なければ名前）を正規化した順。
pub fn list_series(conn: &Connection) -> Result<Vec<SeriesInfo>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.name_kana,
                (SELECT count(*) FROM books b
                 WHERE b.series_id = s.id AND b.deleted = 0 AND b.purged = 0)
         FROM series s WHERE s.deleted = 0",
    )?;
    let mut out = stmt
        .query_map([], |r| {
            Ok(SeriesInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                name_kana: r.get(2)?,
                book_count: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    out.sort_by_cached_key(|s| {
        (
            normalize_for_search(s.name_kana.as_deref().unwrap_or(&s.name).trim()),
            s.id.clone(),
        )
    });
    Ok(out)
}
