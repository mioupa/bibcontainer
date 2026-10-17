//! 作品の手動のまとめ。
//!
//! 書籍の `work_id`（同期する項目）は、一覧で巻をまとめる「作品」の、手動の指定。
//!
//! - null: 自動（フロントが、巻次を外した書名と著者から決める）。
//! - `"none"`（`WORK_NONE`）: この本はどの作品にも入れない。
//! - それ以外: 手動でまとめた作品の ID（同じ ID の本が 1 つの作品になる）。作品はエンティティではなく、
//!   ID はまとめたときに作る UUIDv7。存在の確認は要らない。
//!
//! 「まとめる」だけ専用の操作（`merge_works`）にする。作品から外す（`"none"`）・自動に戻す（null）は、
//! 1 冊ごとの規則がないので、`update_books` で `work_id` を変える。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::Env;
use crate::db::write_tx;
use crate::error::{Error, Result};
use crate::ids::new_entity_id;
use crate::schema::Entity;
use crate::store::record_change;

/// `work_id` の値。この本はどの作品にも入れない。
pub const WORK_NONE: &str = "none";

/// `merge_works` の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeWorksResult {
    /// まとめた先の作品の ID。
    pub work_id: String,
    /// `work_id` を書き換えた書籍の ID（選んだ本を渡した順に、続けて、同じ作品の残りを ID 順に）。
    pub changed: Vec<String>,
}

/// 選んだ本を 1 つの作品にまとめる。全体を 1 つのトランザクションで行う。
///
/// - `ids` は重複を除き、存在しない ID があれば `NotFound`（何も書かない）。完全削除した本は対象にしない。
/// - 対象が 2 冊に満たなければ `Validation`（`work_id`）。
/// - 対象が今の `work_id` で持つ手動の作品の ID の集合を M とする。M が空なら、作品の ID を新しく作る
///   （UUIDv7）。そうでなければ、M のうち、ライブラリ全体で所属する本（ゴミ箱の本を含む）が最も多いものを選ぶ。
///   同数なら ID の辞書順で最小のもの。
/// - 書き込む本は、対象と、M のどれかに入っている本すべて（対象でない本・ゴミ箱の本も含む。作品の
///   片方だけが残るのを避ける）。`work_id` が選んだ ID と違う本にだけ、1 冊 1 つの Op を書く。
pub fn merge_works(
    conn: &mut Connection,
    env: &Env<'_>,
    ids: &[String],
) -> Result<MergeWorksResult> {
    let tx = write_tx(conn)?;

    // 対象を読む。書き込みは、検査がすべて済んでからだけ行う。
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids.iter().filter(|id| seen.insert(id.as_str())) {
        let (purged, work_id): (bool, Option<String>) = tx
            .query_row(
                "SELECT purged, work_id FROM books WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::NotFound {
                what: "書籍",
                id: id.clone(),
            })?;
        if !purged {
            targets.push((id.clone(), work_id));
        }
    }
    if targets.len() < 2 {
        return Err(Error::validation("work_id", "2 冊以上を選んでください"));
    }

    // 対象が今入っている手動の作品（M）。
    let existing: BTreeSet<&str> = targets
        .iter()
        .filter_map(|(_, w)| w.as_deref())
        .filter(|w| *w != WORK_NONE)
        .collect();

    // M の作品に入っている本を、ライブラリ全体から ID 順に集める（ゴミ箱の本を含み、完全削除した本は含まない）。
    let mut members: Vec<(String, String)> = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    if !existing.is_empty() {
        let mut stmt = tx.prepare(
            "SELECT id, work_id FROM books
             WHERE purged = 0 AND work_id IS NOT NULL AND work_id <> ?1
             ORDER BY id",
        )?;
        let rows = stmt
            .query_map([WORK_NONE], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, work_id) in rows {
            if existing.contains(work_id.as_str()) {
                *counts.entry(work_id.clone()).or_default() += 1;
                members.push((id, work_id));
            }
        }
    }

    // 本が最も多い作品。同数なら ID が最小のもの（`counts` は ID 順で、`>` なので先のものが残る）。
    let chosen = counts
        .iter()
        .fold(None::<(&String, usize)>, |best, (work_id, &n)| match best {
            Some((_, best_n)) if n <= best_n => best,
            _ => Some((work_id, n)),
        })
        .map_or_else(new_entity_id, |(work_id, _)| work_id.clone());

    // 書く順: 対象（渡した順）、続けて、対象でない作品の残り（ID 順）。
    let target_ids: BTreeSet<&str> = targets.iter().map(|(id, _)| id.as_str()).collect();
    let rest = members
        .iter()
        .filter(|(id, _)| !target_ids.contains(id.as_str()))
        .map(|(id, work_id)| (id.as_str(), Some(work_id.as_str())));
    let order: Vec<(&str, Option<&str>)> = targets
        .iter()
        .map(|(id, work_id)| (id.as_str(), work_id.as_deref()))
        .chain(rest)
        .collect();

    let mut changed = Vec::new();
    for (id, current) in order {
        if current == Some(chosen.as_str()) {
            continue;
        }
        let set = [("work_id".to_owned(), Value::String(chosen.clone()))];
        record_change(&tx, env.clock, Entity::Book, id, set)?;
        changed.push(id.to_owned());
    }
    tx.commit()?;
    Ok(MergeWorksResult {
        work_id: chosen,
        changed,
    })
}
