//! 重複チェック、購入店の候補、取得待ちの本の一覧。

use rusqlite::Connection;
use serde::Serialize;
use ts_rs::TS;

use super::location::LocationIndex;
use crate::error::Result;
use crate::normalize::{is_valid_isbn10, isbn10_to_isbn13, nfkc};

/// ISBN が一致した本（重複チェックの結果。FR-SCAN-17／18）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IsbnMatch {
    pub id: String,
    pub title: String,
    #[ts(type = "\"owned\" | \"wanted\" | \"released\"")]
    pub ownership: String,
    pub location_id: Option<String>,
    /// 保管場所のパス（例: `自宅 > 書斎の本棚 > 3段目`）。保管場所が無ければ null。
    pub location_path: Option<String>,
    /// ゴミ箱にある。
    pub deleted: bool,
}

/// ISBN-13 が一致する本を探す（完全削除した本は除く）。ゴミ箱の本も含めて `deleted` で知らせる。
///
/// ハイフンと空白は無視する。ISBN-10 を渡したときは ISBN-13 に直して探す。
/// 並びは、ゴミ箱でない本が先、所有状態は所有・欲しい本・手放した、その中は登録順。
pub fn find_by_isbn(conn: &Connection, isbn: &str) -> Result<Vec<IsbnMatch>> {
    let compact: String = nfkc(isbn)
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    let isbn13 = if is_valid_isbn10(&compact) {
        isbn10_to_isbn13(&compact).unwrap_or(compact)
    } else {
        compact
    };
    let mut stmt = conn.prepare(
        "SELECT id, title, ownership, location_id, deleted FROM books
         WHERE isbn13 = ?1 AND purged = 0
         ORDER BY deleted,
                  CASE ownership WHEN 'owned' THEN 0 WHEN 'wanted' THEN 1 ELSE 2 END,
                  id",
    )?;
    let rows = stmt
        .query_map([&isbn13], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, bool>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let locations = LocationIndex::load(conn)?;
    Ok(rows
        .into_iter()
        .map(|(id, title, ownership, location_id, deleted)| IsbnMatch {
            location_path: location_id.as_deref().and_then(|l| locations.path(l)),
            id,
            title,
            ownership,
            location_id,
            deleted,
        })
        .collect())
}

/// 購入店の候補 1 件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreCount {
    pub name: String,
    /// 過去に使った回数（ゴミ箱の本を含む）。
    pub count: i64,
}

/// 過去の `purchase_store` を、使った回数の多い順（同数なら名前順）に返す。FR-BUY-01。
pub fn purchase_store_suggestions(conn: &Connection) -> Result<Vec<StoreCount>> {
    let mut stmt = conn.prepare(
        "SELECT purchase_store, count(*) AS n FROM books
         WHERE purged = 0 AND purchase_store IS NOT NULL AND purchase_store <> ''
         GROUP BY purchase_store ORDER BY n DESC, purchase_store",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(StoreCount {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 書誌の取得待ち（`meta_status = 'pending'`）の本の ID を登録順に返す。ゴミ箱の本は含めない。
pub fn pending_book_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM books WHERE meta_status = 'pending' AND purged = 0 AND deleted = 0
         ORDER BY id",
    )?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}
