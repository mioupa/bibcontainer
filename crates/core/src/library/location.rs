//! 保管場所（ツリー）。FR-BOOK-05。
//!
//! 同期の単位は 1 つの保管場所で、親（`parent_id`）と並び順（`sort_order`）を項目として持つ。
//! 2 台が同時に親を変えると循環ができることがあるので、読み出し側は循環があっても落ちない。

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, Transaction};
use serde::Serialize;
use serde_json::{Value, json};
use ts_rs::TS;

use super::Env;
use crate::db::write_tx;
use crate::error::{Error, Result};
use crate::ids::new_entity_id;
use crate::schema::{Entity, Fields};
use crate::store::record_change;

/// パスの区切り（例: `自宅 > 書斎の本棚 > 3段目`）。
pub const PATH_SEPARATOR: &str = " > ";

/// 保管場所 1 件。`list_locations` は親が子より先に来る順（深さ優先、並び順）で返す。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocationInfo {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: Option<f64>,
    /// ルートからのパス（例: `自宅 > 書斎の本棚 > 3段目`）。
    pub path: String,
    /// ルートが 0。
    pub depth: u32,
}

#[derive(Debug, Clone)]
struct Loc {
    id: String,
    name: String,
    parent_id: Option<String>,
    sort_order: Option<f64>,
    deleted: bool,
}

/// 保管場所の全件（削除したものを含む）を読んだもの。パスの組み立てに使う。
pub struct LocationIndex {
    all: HashMap<String, Loc>,
}

impl LocationIndex {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut stmt =
            conn.prepare("SELECT id, name, parent_id, sort_order, deleted FROM locations")?;
        let all = stmt
            .query_map([], |r| {
                Ok(Loc {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    parent_id: r.get(2)?,
                    sort_order: r.get(3)?,
                    deleted: r.get(4)?,
                })
            })?
            .map(|l| l.map(|l| (l.id.clone(), l)))
            .collect::<rusqlite::Result<_>>()?;
        Ok(LocationIndex { all })
    }

    /// ルートからのパス。保管場所が無ければ `None`。親が見つからなければ、見つかった所までのパス。
    /// 循環があっても止まる。
    pub fn path(&self, id: &str) -> Option<String> {
        let mut names: Vec<&str> = Vec::new();
        let mut visited = HashSet::new();
        let mut cur = self.all.get(id)?;
        loop {
            if !visited.insert(cur.id.as_str()) {
                break;
            }
            names.push(&cur.name);
            match cur.parent_id.as_deref().and_then(|p| self.all.get(p)) {
                Some(parent) => cur = parent,
                None => break,
            }
        }
        names.reverse();
        Some(names.join(PATH_SEPARATOR))
    }

    fn active(&self, id: &str) -> Option<&Loc> {
        self.all.get(id).filter(|l| !l.deleted)
    }

    /// `ancestor` が `id` の祖先（または `id` 自身）か。循環があっても止まる。
    fn is_self_or_descendant(&self, id: &str, ancestor: &str) -> bool {
        let mut visited = HashSet::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            if !visited.insert(c) {
                return false;
            }
            cur = self.all.get(c).and_then(|l| l.parent_id.as_deref());
        }
        false
    }

    /// 削除していない子（親が `parent`）を、並び順（無ければ最後）・名前・ID の順で返す。
    fn children_of(&self, parent: Option<&str>) -> Vec<&Loc> {
        let mut kids: Vec<&Loc> = self
            .all
            .values()
            .filter(|l| !l.deleted && l.parent_id.as_deref() == parent)
            .collect();
        kids.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
        kids
    }
}

fn order_key(l: &Loc) -> (bool, u64, &str, &str) {
    // f64 の並びを u64 に写す（NaN は無い前提。あっても最後）。
    let (missing, bits) = match l.sort_order.filter(|v| v.is_finite()) {
        Some(v) => {
            let b = v.to_bits();
            (
                false,
                if v.is_sign_negative() {
                    !b
                } else {
                    b | (1 << 63)
                },
            )
        }
        None => (true, 0),
    };
    (missing, bits, l.name.as_str(), l.id.as_str())
}

fn require_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::validation("name", "保管場所の名前が空です"));
    }
    Ok(name.to_owned())
}

fn require_active<'i>(index: &'i LocationIndex, id: &str) -> Result<&'i Loc> {
    index.active(id).ok_or_else(|| Error::NotFound {
        what: "保管場所",
        id: id.to_owned(),
    })
}

/// 保管場所の一覧（削除していないもの）。親が子より先に来る深さ優先の順で、パスと深さを付ける。
/// 親が見つからない（または削除済みの）保管場所と、循環に入った保管場所は、ルートとして末尾に並べる。
pub fn list_locations(conn: &Connection) -> Result<Vec<LocationInfo>> {
    let index = LocationIndex::load(conn)?;
    let mut out = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    fn walk(
        index: &LocationIndex,
        loc: &Loc,
        depth: u32,
        visited: &mut HashSet<String>,
        out: &mut Vec<LocationInfo>,
    ) {
        if !visited.insert(loc.id.clone()) {
            return;
        }
        out.push(LocationInfo {
            id: loc.id.clone(),
            name: loc.name.clone(),
            parent_id: loc.parent_id.clone(),
            sort_order: loc.sort_order,
            path: index.path(&loc.id).unwrap_or_else(|| loc.name.clone()),
            depth,
        });
        for child in index.children_of(Some(&loc.id)) {
            walk(index, child, depth + 1, visited, out);
        }
    }

    for root in index.children_of(None) {
        walk(&index, root, 0, &mut visited, &mut out);
    }
    // 親が無い・削除済み・循環で、ここまでに届かなかったもの
    let mut rest: Vec<&Loc> = index
        .all
        .values()
        .filter(|l| !l.deleted && !visited.contains(&l.id))
        .collect();
    rest.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    for loc in rest {
        walk(&index, loc, 0, &mut visited, &mut out);
    }
    Ok(out)
}

/// 親の下の、次の並び順の値（最大 + 1）。
fn next_sort_order(index: &LocationIndex, parent: Option<&str>) -> f64 {
    index
        .children_of(parent)
        .iter()
        .filter_map(|l| l.sort_order.filter(|v| v.is_finite()))
        .fold(0.0_f64, f64::max)
        + 1.0
}

/// 保管場所を作る。親の下の末尾に並ぶ。ID を返す。
pub fn create_location(
    conn: &mut Connection,
    env: &Env<'_>,
    name: &str,
    parent_id: Option<&str>,
) -> Result<String> {
    let name = require_name(name)?;
    let tx = write_tx(conn)?;
    let index = LocationIndex::load(&tx)?;
    if let Some(parent) = parent_id {
        require_active(&index, parent)?;
    }
    let id = new_entity_id();
    let set: Fields = [
        ("name".to_owned(), json!(name)),
        (
            "parent_id".to_owned(),
            parent_id.map_or(Value::Null, |p| json!(p)),
        ),
        (
            "sort_order".to_owned(),
            json!(next_sort_order(&index, parent_id)),
        ),
        ("deleted".to_owned(), json!(false)),
        ("created_at".to_owned(), json!(env.now_iso()?)),
    ]
    .into_iter()
    .collect();
    record_change(&tx, env.clock, Entity::Location, &id, set)?;
    tx.commit()?;
    Ok(id)
}

/// 名前を変える。
pub fn rename_location(conn: &mut Connection, env: &Env<'_>, id: &str, name: &str) -> Result<()> {
    let name = require_name(name)?;
    let tx = write_tx(conn)?;
    let index = LocationIndex::load(&tx)?;
    let loc = require_active(&index, id)?;
    if loc.name != name {
        record_change(
            &tx,
            env.clock,
            Entity::Location,
            id,
            [("name".to_owned(), json!(name))],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 親を変える（`None` でルートへ）。新しい親の下の末尾に並ぶ。自分自身や子孫の下へは移せない（`Conflict`）。
/// 今の親と同じなら何もしない。
pub fn move_location(
    conn: &mut Connection,
    env: &Env<'_>,
    id: &str,
    new_parent: Option<&str>,
) -> Result<()> {
    let tx = write_tx(conn)?;
    let index = LocationIndex::load(&tx)?;
    let loc = require_active(&index, id)?;
    if loc.parent_id.as_deref() == new_parent {
        return Ok(());
    }
    if let Some(parent) = new_parent {
        require_active(&index, parent)?;
        if index.is_self_or_descendant(parent, id) {
            return Err(Error::Conflict(
                "自分自身かその下の保管場所の下には移せません".to_owned(),
            ));
        }
    }
    let set: Fields = [
        (
            "parent_id".to_owned(),
            new_parent.map_or(Value::Null, |p| json!(p)),
        ),
        (
            "sort_order".to_owned(),
            json!(next_sort_order(&index, new_parent)),
        ),
    ]
    .into_iter()
    .collect();
    record_change(&tx, env.clock, Entity::Location, id, set)?;
    tx.commit()?;
    Ok(())
}

/// 同じ親の下の並び順を決める。`ordered_ids` に挙げた保管場所を先頭から順に並べ、挙げなかった兄弟は
/// 今の順のまま後ろに続ける。変わった保管場所だけ記録する。`ordered_ids` は、すべて `parent` の
/// 直下の保管場所でなければならない。
pub fn reorder_locations(
    conn: &mut Connection,
    env: &Env<'_>,
    parent: Option<&str>,
    ordered_ids: &[String],
) -> Result<()> {
    let tx = write_tx(conn)?;
    let index = LocationIndex::load(&tx)?;
    let siblings = index.children_of(parent);
    let sibling_ids: HashSet<&str> = siblings.iter().map(|l| l.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut order: Vec<&str> = Vec::with_capacity(siblings.len());
    for id in ordered_ids {
        if !sibling_ids.contains(id.as_str()) {
            return Err(Error::validation(
                "ordered_ids",
                format!("この親の直下の保管場所ではありません: {id}"),
            ));
        }
        if seen.insert(id.as_str()) {
            order.push(id);
        }
    }
    order.extend(
        siblings
            .iter()
            .map(|l| l.id.as_str())
            .filter(|id| !seen.contains(id)),
    );
    for (i, id) in order.iter().enumerate() {
        let want = (i + 1) as f64;
        let current = index.all.get(*id).and_then(|l| l.sort_order);
        if current != Some(want) {
            record_change(
                &tx,
                env.clock,
                Entity::Location,
                id,
                [("sort_order".to_owned(), json!(want))],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// 保管場所を削除する。子の保管場所も本（ゴミ箱の本を含む）もないときだけ。あれば `Conflict`。
pub fn delete_location(conn: &mut Connection, env: &Env<'_>, id: &str) -> Result<()> {
    let tx = write_tx(conn)?;
    let index = LocationIndex::load(&tx)?;
    require_active(&index, id)?;
    let children = index.children_of(Some(id)).len();
    if children > 0 {
        return Err(Error::Conflict(format!(
            "子の保管場所が {children} 件あるので削除できません"
        )));
    }
    let books = book_count(&tx, id)?;
    if books > 0 {
        return Err(Error::Conflict(format!(
            "この保管場所に本が {books} 冊あるので削除できません（ゴミ箱の本を含む）"
        )));
    }
    record_change(
        &tx,
        env.clock,
        Entity::Location,
        id,
        [("deleted".to_owned(), json!(true))],
    )?;
    tx.commit()?;
    Ok(())
}

/// その保管場所に直接置いてある本の数（完全削除していないもの。ゴミ箱の本を含む）。
fn book_count(tx: &Transaction<'_>, location_id: &str) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT count(*) FROM books WHERE location_id = ?1 AND purged = 0",
        [location_id],
        |r| r.get(0),
    )?)
}

/// 保管場所のパス（`自宅 > 書斎の本棚 > 3段目`）。保管場所が無ければ `None`。
pub fn location_path(conn: &Connection, id: &str) -> Result<Option<String>> {
    Ok(LocationIndex::load(conn)?.path(id))
}
