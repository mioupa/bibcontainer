//! 一覧の行（`ListRow`）と、詳細（`BookDetail`）の読み出し。IPC に出す型もここ。
//!
//! 一覧は起動時に全件をフロントへ渡し、絞り込み・並べ替え・件数はフロントで行う
//! （`docs/decisions/0005` §4）。変わった行は `get_rows` で取り直す。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::types::FromSql;
use rusqlite::{Connection, Row, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::{Error, Result};
use crate::normalize::normalize_for_search;
use crate::values::{
    META_STATUSES, OWNERSHIPS, PURCHASE_CONDS, READ_STATUSES, WISH_PRIORITIES, pick_known,
};

/// 著者 1 人。`authors_json` の要素。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Author {
    pub name: String,
    pub kana: Option<String>,
    /// 役割（著、訳、編、監修など）。
    pub role: Option<String>,
}

/// 一覧用の 1 行。形は `docs/decisions/0005-phase1-architecture.md` §4 のとおり。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListRow {
    pub id: String,
    pub isbn13: Option<String>,
    pub title: String,
    pub title_kana: Option<String>,
    pub subtitle: Option<String>,
    pub volume: Option<String>,
    pub volume_sort: Option<f64>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    /// 作品の手動の指定。null は自動（書名と著者から決める）、`"none"` は作品に入れない、
    /// それ以外は手動でまとめた作品の ID（同じ ID の本が 1 つの作品になる）。
    pub work_id: Option<String>,
    pub authors: Vec<Author>,
    pub publisher: Option<String>,
    /// `YYYY` / `YYYY-MM` / `YYYY-MM-DD`。
    pub published_on: Option<String>,
    /// 4 桁。
    pub c_code: Option<String>,
    pub ndc: Option<String>,
    /// 本体価格（税抜）。
    pub list_price: Option<i64>,
    pub pages: Option<i64>,
    #[ts(type = "\"owned\" | \"wanted\" | \"released\"")]
    pub ownership: String,
    #[ts(type = "\"high\" | \"mid\" | \"low\" | null")]
    pub wish_priority: Option<String>,
    pub location_id: Option<String>,
    pub tags: Vec<String>,
    #[ts(type = "\"unread\" | \"reading\" | \"read\" | \"paused\"")]
    pub read_status: String,
    pub read_finished_on: Option<String>,
    pub rating: Option<i64>,
    pub purchased_on: Option<String>,
    pub purchase_price: Option<i64>,
    pub purchase_store: Option<String>,
    pub cover_hash: Option<String>,
    #[ts(type = "\"ok\" | \"pending\" | \"partial\" | \"failed\"")]
    pub meta_status: String,
    /// ゴミ箱にある。
    pub deleted: bool,
    /// 登録日時（ISO 8601 UTC ミリ秒）。
    pub created_at: String,
    /// 並べ替え用。ヨミがあればヨミ、なければ書名を正規化したもの。
    pub sort_title: String,
    /// 並べ替え用。先頭の著者のヨミ（なければ名前）を正規化したもの。
    pub sort_authors: String,
}

/// 書籍 1 冊の詳細。同期する全項目と、項目ごとの取得元・手修正の記録。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BookDetail {
    pub id: String,
    pub isbn13: Option<String>,
    pub title: String,
    pub title_kana: Option<String>,
    pub subtitle: Option<String>,
    pub series_id: Option<String>,
    pub volume: Option<String>,
    pub volume_sort: Option<f64>,
    /// 作品の手動の指定（`ListRow::work_id` と同じ）。
    pub work_id: Option<String>,
    pub authors: Vec<Author>,
    pub publisher: Option<String>,
    pub published_on: Option<String>,
    pub c_code: Option<String>,
    pub ndc: Option<String>,
    pub list_price: Option<i64>,
    /// 参考の税込価格（楽天など）。
    pub ref_price_incl: Option<i64>,
    pub pages: Option<i64>,
    pub size_text: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub cover_hash: Option<String>,
    #[ts(type = "\"owned\" | \"wanted\" | \"released\"")]
    pub ownership: String,
    #[ts(type = "\"high\" | \"mid\" | \"low\" | null")]
    pub wish_priority: Option<String>,
    pub location_id: Option<String>,
    pub tags: Vec<String>,
    #[ts(type = "\"unread\" | \"reading\" | \"read\" | \"paused\"")]
    pub read_status: String,
    pub read_started_on: Option<String>,
    pub read_finished_on: Option<String>,
    pub rating: Option<i64>,
    pub review: Option<String>,
    pub purchased_on: Option<String>,
    pub purchase_store: Option<String>,
    pub purchase_price: Option<i64>,
    #[ts(type = "\"new\" | \"used\" | null")]
    pub purchase_cond: Option<String>,
    pub purchase_memo: Option<String>,
    pub memo: Option<String>,
    #[ts(type = "\"ok\" | \"pending\" | \"partial\" | \"failed\"")]
    pub meta_status: String,
    pub deleted: bool,
    pub purged: bool,
    pub created_at: String,
    /// 項目名 → 取得元（`barcode`、`openbd`、`ndl`、`google`、`manual` など）。
    pub field_sources: BTreeMap<String, String>,
    /// 手で直した項目名（辞書順）。再取得しても上書きしない。
    pub manual_fields: Vec<String>,
    /// 新しい版の端末から届いた、この版が知らない項目。
    #[ts(type = "Record<string, unknown>")]
    pub extra: Value,
}

/// `authors_json` を読む。壊れた要素や名前のない要素は捨てる（他端末から届いた値でも落ちないように）。
pub fn parse_authors(json: &str) -> Vec<Author> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }
            let opt = |key: &str| {
                item.get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            };
            Some(Author {
                name: name.to_owned(),
                kana: opt("kana"),
                role: opt("role"),
            })
        })
        .collect()
}

/// `tags_json` を読む。文字列以外と空文字は捨てる。
pub fn parse_tags(json: &str) -> Vec<String> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 並べ替え用の書名のキー。ヨミがあればヨミ、なければ書名を正規化したもの。
pub fn sort_title_key(title: &str, title_kana: Option<&str>) -> String {
    let base = title_kana.filter(|k| !k.trim().is_empty()).unwrap_or(title);
    normalize_for_search(base.trim())
}

/// 並べ替え用の著者のキー。先頭の著者のヨミ（なければ名前）を正規化したもの。
pub fn sort_authors_key(authors: &[Author]) -> String {
    authors.first().map_or_else(String::new, |a| {
        normalize_for_search(a.kana.as_deref().unwrap_or(&a.name).trim())
    })
}

/// 列を順に読む小さな補助。SELECT の列の並びと読む順序を一致させる。
struct Cursor<'r, 'stmt> {
    row: &'r Row<'stmt>,
    next: usize,
}

impl<'r, 'stmt> Cursor<'r, 'stmt> {
    fn new(row: &'r Row<'stmt>) -> Self {
        Cursor { row, next: 0 }
    }

    fn get<T: FromSql>(&mut self) -> rusqlite::Result<T> {
        let v = self.row.get(self.next);
        self.next += 1;
        v
    }
}

const LIST_SELECT: &str =
    "SELECT b.id, b.isbn13, b.title, b.title_kana, b.subtitle, b.volume, b.volume_sort,
        b.series_id, s.name, b.work_id, b.authors_json, b.publisher, b.published_on, b.c_code, b.ndc,
        b.list_price, b.pages, b.ownership, b.wish_priority, b.location_id, b.tags_json,
        b.read_status, b.read_finished_on, b.rating, b.purchased_on, b.purchase_price,
        b.purchase_store, b.cover_hash, b.meta_status, b.deleted, b.created_at,
        i.sort_title, i.sort_authors
     FROM books b
     LEFT JOIN series s ON s.id = b.series_id
     LEFT JOIN book_idx i ON i.book_id = b.id
     WHERE b.purged = 0";

fn list_row(row: &Row<'_>) -> rusqlite::Result<ListRow> {
    let mut c = Cursor::new(row);
    let id: String = c.get()?;
    let isbn13 = c.get()?;
    let title: String = c.get()?;
    let title_kana: Option<String> = c.get()?;
    let subtitle = c.get()?;
    let volume = c.get()?;
    let volume_sort = c.get()?;
    let series_id = c.get()?;
    let series_name = c.get()?;
    let work_id = c.get()?;
    let authors = parse_authors(&c.get::<String>()?);
    let publisher = c.get()?;
    let published_on = c.get()?;
    let c_code = c.get()?;
    let ndc = c.get()?;
    let list_price = c.get()?;
    let pages = c.get()?;
    let ownership: String = c.get()?;
    let wish_priority: Option<String> = c.get()?;
    let location_id = c.get()?;
    let tags = parse_tags(&c.get::<String>()?);
    let read_status: String = c.get()?;
    let read_finished_on = c.get()?;
    let rating = c.get()?;
    let purchased_on = c.get()?;
    let purchase_price = c.get()?;
    let purchase_store = c.get()?;
    let cover_hash = c.get()?;
    let meta_status: String = c.get()?;
    let deleted: bool = c.get()?;
    let created_at = c.get()?;
    let sort_title: Option<String> = c.get()?;
    let sort_authors: Option<String> = c.get()?;
    // 派生データは apply_op と同じトランザクションで更新するので、通常は必ずある。
    // 無いときは（派生データの作り直しの前など）その場で作る。
    let sort_title = sort_title.unwrap_or_else(|| sort_title_key(&title, title_kana.as_deref()));
    let sort_authors = sort_authors.unwrap_or_else(|| sort_authors_key(&authors));
    Ok(ListRow {
        id,
        isbn13,
        title,
        title_kana,
        subtitle,
        volume,
        volume_sort,
        series_id,
        series_name,
        work_id,
        authors,
        publisher,
        published_on,
        c_code,
        ndc,
        list_price,
        pages,
        ownership: pick_known(ownership, OWNERSHIPS, "owned"),
        wish_priority: wish_priority.filter(|p| WISH_PRIORITIES.contains(&p.as_str())),
        location_id,
        tags,
        read_status: pick_known(read_status, READ_STATUSES, "unread"),
        read_finished_on,
        rating,
        purchased_on,
        purchase_price,
        purchase_store,
        cover_hash,
        meta_status: pick_known(meta_status, META_STATUSES, "ok"),
        deleted,
        created_at,
        sort_title,
        sort_authors,
    })
}

/// 完全削除していない全書籍の一覧用の行（ID 順＝登録順）。ゴミ箱の本も含む（`deleted` で判別する）。
pub fn list_rows(conn: &Connection) -> Result<Vec<ListRow>> {
    let sql = format!("{LIST_SELECT} ORDER BY b.id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], list_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 指定した ID の行を、指定した順に返す（重複は 1 つにする）。
///
/// 存在しない ID と、完全削除した ID は結果に含まれない。呼び出し側は「要求したのに無い」ものを
/// 一覧から取り除く（`library-changed` イベントの扱い）。
pub fn get_rows(conn: &Connection, ids: &[String]) -> Result<Vec<ListRow>> {
    let mut unique: Vec<&String> = Vec::with_capacity(ids.len());
    let mut seen = BTreeSet::new();
    for id in ids {
        if seen.insert(id.as_str()) {
            unique.push(id);
        }
    }
    let mut found: HashMap<String, ListRow> = HashMap::with_capacity(unique.len());
    for chunk in unique.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!("{LIST_SELECT} AND b.id IN ({marks})");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter().copied()), list_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for row in rows {
            found.insert(row.id.clone(), row);
        }
    }
    Ok(unique
        .into_iter()
        .filter_map(|id| found.remove(id))
        .collect())
}

const DETAIL_SELECT: &str =
    "SELECT id, isbn13, title, title_kana, subtitle, series_id, volume, volume_sort, work_id,
        authors_json, publisher, published_on, c_code, ndc, list_price, ref_price_incl, pages,
        size_text, language, description, cover_hash, ownership, wish_priority, location_id,
        tags_json, read_status, read_started_on, read_finished_on, rating, review, purchased_on,
        purchase_store, purchase_price, purchase_cond, purchase_memo, memo, meta_status,
        deleted, purged, created_at, field_sources_json, manual_fields_json, extra_json
     FROM books";

fn detail_row(row: &Row<'_>) -> rusqlite::Result<BookDetail> {
    let mut c = Cursor::new(row);
    let id = c.get()?;
    let isbn13 = c.get()?;
    let title = c.get()?;
    let title_kana = c.get()?;
    let subtitle = c.get()?;
    let series_id = c.get()?;
    let volume = c.get()?;
    let volume_sort = c.get()?;
    let work_id = c.get()?;
    let authors = parse_authors(&c.get::<String>()?);
    let publisher = c.get()?;
    let published_on = c.get()?;
    let c_code = c.get()?;
    let ndc = c.get()?;
    let list_price = c.get()?;
    let ref_price_incl = c.get()?;
    let pages = c.get()?;
    let size_text = c.get()?;
    let language = c.get()?;
    let description = c.get()?;
    let cover_hash = c.get()?;
    let ownership: String = c.get()?;
    let wish_priority: Option<String> = c.get()?;
    let location_id = c.get()?;
    let tags = parse_tags(&c.get::<String>()?);
    let read_status: String = c.get()?;
    let read_started_on = c.get()?;
    let read_finished_on = c.get()?;
    let rating = c.get()?;
    let review = c.get()?;
    let purchased_on = c.get()?;
    let purchase_store = c.get()?;
    let purchase_price = c.get()?;
    let purchase_cond: Option<String> = c.get()?;
    let purchase_memo = c.get()?;
    let memo = c.get()?;
    let meta_status: String = c.get()?;
    let deleted = c.get()?;
    let purged = c.get()?;
    let created_at = c.get()?;
    let sources_json: String = c.get()?;
    let manual_json: String = c.get()?;
    let extra_json: String = c.get()?;
    Ok(BookDetail {
        id,
        isbn13,
        title,
        title_kana,
        subtitle,
        series_id,
        volume,
        volume_sort,
        work_id,
        authors,
        publisher,
        published_on,
        c_code,
        ndc,
        list_price,
        ref_price_incl,
        pages,
        size_text,
        language,
        description,
        cover_hash,
        ownership: pick_known(ownership, OWNERSHIPS, "owned"),
        wish_priority: wish_priority.filter(|p| WISH_PRIORITIES.contains(&p.as_str())),
        location_id,
        tags,
        read_status: pick_known(read_status, READ_STATUSES, "unread"),
        read_started_on,
        read_finished_on,
        rating,
        review,
        purchased_on,
        purchase_store,
        purchase_price,
        purchase_cond: purchase_cond.filter(|p| PURCHASE_CONDS.contains(&p.as_str())),
        purchase_memo,
        memo,
        meta_status: pick_known(meta_status, META_STATUSES, "ok"),
        deleted,
        purged,
        created_at,
        field_sources: parse_sources(&sources_json),
        manual_fields: parse_manual(&manual_json),
        extra: serde_json::from_str(&extra_json)
            .unwrap_or_else(|_| Value::Object(Default::default())),
    })
}

/// `field_sources_json`（オブジェクト）を読む。文字列でない値は捨てる。
pub(crate) fn parse_sources(json: &str) -> BTreeMap<String, String> {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_owned())))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// `manual_fields_json`（項目名の配列）を読む。辞書順にそろえる。
pub(crate) fn parse_manual(json: &str) -> Vec<String> {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// 書籍 1 冊の詳細。完全削除したものも返す（中身は空）。無ければ `None`。
pub fn find_book_detail(conn: &Connection, id: &str) -> Result<Option<BookDetail>> {
    let sql = format!("{DETAIL_SELECT} WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map([id], detail_row)?;
    Ok(rows.next().transpose()?)
}

/// 書籍 1 冊の詳細。無ければ `NotFound`。
pub fn get_book_detail(conn: &Connection, id: &str) -> Result<BookDetail> {
    find_book_detail(conn, id)?.ok_or_else(|| Error::NotFound {
        what: "書籍",
        id: id.to_owned(),
    })
}

/// 完全削除していない書籍の詳細を ID 順に全件返す（書き出し用）。
pub(crate) fn all_details(conn: &Connection) -> Result<Vec<BookDetail>> {
    let sql = format!("{DETAIL_SELECT} WHERE purged = 0 ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], detail_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 指定した ID の詳細を指定した順に返す（書き出し用。無い ID・完全削除した ID は飛ばす）。
pub(crate) fn details_for(conn: &Connection, ids: &[String]) -> Result<Vec<BookDetail>> {
    let mut seen = BTreeSet::new();
    let mut found: HashMap<String, BookDetail> = HashMap::new();
    let unique: Vec<&String> = ids.iter().filter(|id| seen.insert(id.as_str())).collect();
    for chunk in unique.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!("{DETAIL_SELECT} WHERE purged = 0 AND id IN ({marks})");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter().copied()), detail_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for d in rows {
            found.insert(d.id.clone(), d);
        }
    }
    Ok(unique
        .into_iter()
        .filter_map(|id| found.remove(id))
        .collect())
}
