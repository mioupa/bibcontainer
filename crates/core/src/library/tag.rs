//! タグ。FR-BOOK-06。
//!
//! 本の `tags` 項目が名前の配列で、tag エンティティは色などの属性を持つ。タグ名の `/` は階層を表し、
//! 名前の変更（`rename_tag`）は、そのタグと子孫（`親/子`）をまとめて変える（フォルダの改名と同じ）。
//! 同じ名前の tag エンティティが複数あるとき（2 台が別々に作った場合）は、ID が最小
//! （UUIDv7 なので最古）のものを使う。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, Transaction};
use serde::Serialize;
use serde_json::{Value, json};
use ts_rs::TS;

use super::validate::canonical_tags;
use super::{Env, FieldChange, UpdateOptions, book::update_book_in};
use crate::db::write_tx;
use crate::dump::load_row;
use crate::error::{Error, Result};
use crate::ids::new_entity_id;
use crate::schema::{Entity, Fields};
use crate::store::record_change;

/// タグ 1 件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TagInfo {
    pub name: String,
    /// ゴミ箱に入れていない本の数。
    pub count: i64,
    pub color: Option<String>,
    /// tag エンティティの ID（色を設定していないタグでは null）。
    pub id: Option<String>,
}

/// 削除していない tag エンティティを、名前ごとに ID が最小のものだけ集める。
fn canonical_entities(conn: &Connection) -> Result<BTreeMap<String, (String, Option<String>)>> {
    let mut stmt =
        conn.prepare("SELECT id, name, color FROM tags WHERE deleted = 0 ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut map = BTreeMap::new();
    for (id, name, color) in rows {
        map.entry(name).or_insert((id, color));
    }
    Ok(map)
}

/// タグの一覧（名前順）。件数はゴミ箱に入れていない本の数。本に付いていなくても tag エンティティが
/// あれば（色を設定したタグ）、件数 0 で載る。
pub fn list_tags(conn: &Connection) -> Result<Vec<TagInfo>> {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT t.tag, count(*) FROM book_tags t JOIN books b ON b.id = t.book_id
         WHERE b.deleted = 0 AND b.purged = 0 GROUP BY t.tag",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (tag, n) = row?;
        counts.insert(tag, n);
    }
    let entities = canonical_entities(conn)?;
    let names: BTreeSet<&String> = counts.keys().chain(entities.keys()).collect();
    Ok(names
        .into_iter()
        .map(|name| {
            let entity = entities.get(name);
            TagInfo {
                name: name.clone(),
                count: counts.get(name).copied().unwrap_or(0),
                color: entity.and_then(|(_, c)| c.clone()),
                id: entity.map(|(id, _)| id.clone()),
            }
        })
        .collect())
}

fn require_tag_name(field: &str, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::validation(field, "タグ名が空です"));
    }
    Ok(name.to_owned())
}

/// 本のタグを書き換える。変わったら `true`。
fn rewrite_tags(
    tx: &Transaction<'_>,
    env: &Env<'_>,
    book_id: &str,
    f: impl FnOnce(Vec<String>) -> Vec<String>,
) -> Result<bool> {
    let cur = load_row(tx, Entity::Book, book_id)?.ok_or_else(|| Error::NotFound {
        what: "書籍",
        id: book_id.to_owned(),
    })?;
    let before: Vec<String> = cur
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let after = f(before.clone());
    if after == before {
        return Ok(false);
    }
    update_book_in(
        tx,
        env,
        book_id,
        &[FieldChange::manual("tags", json!(after))],
        UpdateOptions::default(),
    )?;
    Ok(true)
}

/// 改名の対象になるタグ名なら、変更後の名前を返す。`old` そのものと、その子孫（`old/…`）が対象で、
/// 子孫は `old` の部分だけを `new` に置き換える（`研究/SiC` を `研究` → `研究2` で `研究2/SiC`）。
/// 名前が `old` で始まっていても、`/` で区切られていないもの（`研究室`）は対象外。
fn rename_target(name: &str, old: &str, new: &str) -> Option<String> {
    if name == old {
        return Some(new.to_owned());
    }
    name.strip_prefix(old)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(|rest| format!("{new}/{rest}"))
}

/// タグ名を変える。`/` で区切った階層も、フォルダの改名と同じように動く。`old` そのものと子孫
/// （`old/…`）のタグが付いた本すべての `tags` と、tag エンティティ（色など）を、1 つのトランザクションで
/// 書き換える。変わった本の ID を返す。
///
/// 例: `rename_tag("研究", "研究2")` で `研究` は `研究2` に、`研究/SiC` は `研究2/SiC` になる
/// （`研究室` のような、`/` で区切られていない名前は変わらない）。
///
/// 変更先の名前のタグが既にあれば、そのタグと 1 つにまとめる（本の `tags` は重複を除き、変更先の
/// tag エンティティが残って、古い方は削除する）。`old` とその子孫のどれにも、付いている本も tag
/// エンティティも無ければ `NotFound`。
pub fn rename_tag(
    conn: &mut Connection,
    env: &Env<'_>,
    old: &str,
    new: &str,
) -> Result<Vec<String>> {
    let old = require_tag_name("old", old)?;
    let new = require_tag_name("new", new)?;
    if old == new {
        return Ok(Vec::new());
    }
    let tx = write_tx(conn)?;
    let book_ids = tx
        .prepare(
            "SELECT DISTINCT book_id FROM book_tags
             WHERE tag = ?1 OR substr(tag, 1, length(?1) + 1) = ?1 || '/'
             ORDER BY book_id",
        )?
        .query_map([&old], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // 対象の tag エンティティ（削除していないもの）。名前ごとに ID 順。
    let all_entities: Vec<(String, String)> = tx
        .prepare("SELECT id, name FROM tags WHERE deleted = 0 ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let sources: Vec<(String, String, String)> = all_entities
        .iter()
        .filter_map(|(id, name)| {
            rename_target(name, &old, &new).map(|target| (id.clone(), name.clone(), target))
        })
        .collect();
    if book_ids.is_empty() && sources.is_empty() {
        return Err(Error::NotFound {
            what: "タグ",
            id: old,
        });
    }

    let mut changed = Vec::new();
    for id in &book_ids {
        let rewritten = rewrite_tags(&tx, env, id, |tags| {
            let mut out: Vec<String> = Vec::with_capacity(tags.len());
            for t in tags {
                let t = rename_target(&t, &old, &new).unwrap_or(t);
                if !out.contains(&t) {
                    out.push(t);
                }
            }
            out
        })?;
        if rewritten {
            changed.push(id.clone());
        }
    }

    // 変更先の名前が「改名されずに残るタグ」と重なるときは、そちらを残して古い方を削除する。
    // 改名の対象どうし（`a` → `a/b` のとき、もともとある `a/b` は `a/b/b` に動く）は重ならない。
    let moving: BTreeSet<&str> = sources.iter().map(|(_, name, _)| name.as_str()).collect();
    let staying: BTreeSet<&str> = all_entities
        .iter()
        .map(|(_, name)| name.as_str())
        .filter(|name| !moving.contains(name))
        .collect();
    let mut renamed_names: BTreeSet<&str> = BTreeSet::new();
    for (entity_id, name, target) in &sources {
        // 同じ名前のエンティティが複数あるときは、ID が最小のもの（先頭）だけを改名し、残りは削除する。
        let first_of_name = renamed_names.insert(name.as_str());
        let set: Vec<(String, Value)> = if first_of_name && !staying.contains(target.as_str()) {
            vec![("name".to_owned(), json!(target))]
        } else {
            vec![("deleted".to_owned(), json!(true))]
        };
        record_change(&tx, env.clock, Entity::Tag, entity_id, set)?;
    }
    tx.commit()?;
    Ok(changed)
}

/// タグの色を設定する（`None` で色を外す）。tag エンティティが無ければ作る。エンティティの ID を返す。
pub fn set_tag_color(
    conn: &mut Connection,
    env: &Env<'_>,
    name: &str,
    color: Option<&str>,
) -> Result<String> {
    let name = require_tag_name("name", name)?;
    let color = match color.map(str::trim) {
        Some("") => return Err(Error::validation("color", "色が空です")),
        other => other,
    };
    let color_value = color.map_or(Value::Null, |c| json!(c));
    let tx = write_tx(conn)?;
    let existing = canonical_entities(&tx)?.remove(&name);
    let id = match existing {
        Some((id, current)) => {
            if current.as_deref() != color {
                record_change(
                    &tx,
                    env.clock,
                    Entity::Tag,
                    &id,
                    [("color".to_owned(), color_value)],
                )?;
            }
            id
        }
        None => {
            let id = new_entity_id();
            let set: Fields = [
                ("name".to_owned(), json!(name)),
                ("color".to_owned(), color_value),
                ("deleted".to_owned(), json!(false)),
                ("created_at".to_owned(), json!(env.now_iso()?)),
            ]
            .into_iter()
            .collect();
            record_change(&tx, env.clock, Entity::Tag, &id, set)?;
            id
        }
    };
    tx.commit()?;
    Ok(id)
}

/// 複数の本のタグを、1 つのトランザクションで一括変更する（FR-BOOK-08）。各本に `add` を足し
/// （既にあれば足さない）、`remove` を外す。変わった本の ID を返す。
pub fn modify_tags(
    conn: &mut Connection,
    env: &Env<'_>,
    book_ids: &[String],
    add: &[String],
    remove: &[String],
) -> Result<Vec<String>> {
    let add = canonical_tags("add", &json!(add))?;
    let remove: BTreeSet<String> = canonical_tags("remove", &json!(remove))?
        .into_iter()
        .collect();
    let tx = write_tx(conn)?;
    let mut changed = Vec::new();
    let mut seen = BTreeSet::new();
    for id in book_ids.iter().filter(|id| seen.insert(id.as_str())) {
        let rewritten = rewrite_tags(&tx, env, id, |mut tags| {
            tags.retain(|t| !remove.contains(t));
            for t in &add {
                if !tags.contains(t) {
                    tags.push(t.clone());
                }
            }
            tags
        })?;
        if rewritten {
            changed.push(id.clone());
        }
    }
    tx.commit()?;
    Ok(changed)
}
