//! 検索・並べ替え用の派生データ（端末ローカル。同期しない）。
//!
//! - `books_fts`（FTS5 trigram）: 書名・ヨミ・著者・出版社・シリーズ名・タグ・メモ・ISBN を
//!   `normalize_for_search` で正規化した文字列。
//! - `book_idx`: 書籍 ID と FTS の rowid の対応、並べ替えキー（`sort_title`、`sort_authors`）。
//! - `book_tags`: `(book_id, tag)`。タグの件数と名前の変更に使う。
//!
//! 書籍・シリーズが変わったときは `apply_op` の後処理（`after_apply`）で更新するので、他端末の Op を
//! 取り込んだときも最新になる。完全削除した本は載せない（ゴミ箱の本は載せる）。
//! 全体を作り直す `rebuild` は、マイグレーションの後や、正規化の仕様を変えたときに呼ぶ
//! （`ensure_built` が `DERIVED_VERSION` を見て自動で呼ぶ）。

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::write_tx;
use crate::error::Result;
use crate::meta;
use crate::normalize::{isbn13_to_isbn10, normalize_for_search};
use crate::rows::{Author, parse_authors, parse_tags, sort_authors_key, sort_title_key};
use crate::schema::Entity;

/// 派生データの作り方の版。正規化や列の構成を変えたら上げる（次の起動で作り直される）。
pub const DERIVED_VERSION: &str = "1";
/// `local_meta` に、最後に作った派生データの版を覚えておくキー。
pub const KEY_DERIVED_VERSION: &str = "derived_version";

/// 変わったら派生データの再計算が要る書籍の項目（`purged` は載せる・外すの判断に使う）。
const BOOK_SEARCH_FIELDS: &[&str] = &[
    "isbn13",
    "title",
    "title_kana",
    "subtitle",
    "series_id",
    "authors",
    "publisher",
    "tags",
    "memo",
    "purged",
];

/// `apply_op` が項目を書き換えた後に呼ぶ。`touched` は書き換えた項目名。
pub(crate) fn after_apply(
    conn: &Connection,
    ent: Entity,
    id: &str,
    touched: &[&str],
) -> Result<()> {
    match ent {
        Entity::Book => {
            let relevant = touched.iter().any(|t| BOOK_SEARCH_FIELDS.contains(t));
            if relevant || !is_indexed(conn, id)? {
                refresh_book(conn, id)?;
            }
        }
        Entity::Series if touched.contains(&"name") => refresh_series_books(conn, id)?,
        _ => {}
    }
    Ok(())
}

fn is_indexed(conn: &Connection, book_id: &str) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT 1 FROM book_idx WHERE book_id = ?1")?
        .exists([book_id])?)
}

/// 書籍 1 冊の派生データを作り直す。完全削除した本・存在しない本は派生データから外す。
pub fn refresh_book(conn: &Connection, id: &str) -> Result<()> {
    let source = conn
        .prepare_cached(
            "SELECT b.isbn13, b.title, b.title_kana, b.subtitle, b.authors_json, b.publisher,
                    b.tags_json, b.memo, s.name, b.purged
             FROM books b LEFT JOIN series s ON s.id = b.series_id WHERE b.id = ?1",
        )?
        .query_row([id], |r| {
            Ok((
                Source {
                    isbn13: r.get(0)?,
                    title: r.get(1)?,
                    title_kana: r.get(2)?,
                    subtitle: r.get(3)?,
                    authors_json: r.get(4)?,
                    publisher: r.get(5)?,
                    tags_json: r.get(6)?,
                    memo: r.get(7)?,
                    series_name: r.get(8)?,
                },
                r.get::<_, bool>(9)?,
            ))
        })
        .optional()?;
    match source {
        Some((source, false)) => write_index(conn, id, &build(&source)),
        _ => remove_index(conn, id),
    }
}

/// そのシリーズに属する本の派生データを作り直す（シリーズ名が変わったとき）。
pub fn refresh_series_books(conn: &Connection, series_id: &str) -> Result<()> {
    let ids = conn
        .prepare_cached("SELECT id FROM books WHERE series_id = ?1 AND purged = 0")?
        .query_map([series_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        refresh_book(conn, &id)?;
    }
    Ok(())
}

struct Source {
    isbn13: Option<String>,
    title: String,
    title_kana: Option<String>,
    subtitle: Option<String>,
    authors_json: String,
    publisher: Option<String>,
    tags_json: String,
    memo: Option<String>,
    series_name: Option<String>,
}

/// 派生データの 1 冊分。
struct Built {
    /// FTS の列（title, title_kana, authors, publisher, series, tags, memo, isbn の順）。
    fts: [String; 8],
    sort_title: String,
    sort_authors: String,
    tags: Vec<String>,
}

fn build(src: &Source) -> Built {
    let authors: Vec<Author> = parse_authors(&src.authors_json);
    let tags = parse_tags(&src.tags_json);

    let mut title = src.title.trim().to_owned();
    if let Some(sub) = src
        .subtitle
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        title.push(' ');
        title.push_str(sub);
    }
    let authors_text = authors
        .iter()
        .flat_map(|a| std::iter::once(a.name.as_str()).chain(a.kana.as_deref()))
        .collect::<Vec<_>>()
        .join(" ");
    let isbn = src
        .isbn13
        .as_deref()
        .map_or_else(String::new, |i13| match isbn13_to_isbn10(i13) {
            Some(i10) => format!("{i13} {i10}"),
            None => i13.to_owned(),
        });
    let opt = |s: &Option<String>| normalize_for_search(s.as_deref().unwrap_or("").trim());

    Built {
        fts: [
            normalize_for_search(&title),
            opt(&src.title_kana),
            normalize_for_search(&authors_text),
            opt(&src.publisher),
            opt(&src.series_name),
            normalize_for_search(&tags.join(" ")),
            opt(&src.memo),
            normalize_for_search(&isbn),
        ],
        sort_title: sort_title_key(&src.title, src.title_kana.as_deref()),
        sort_authors: sort_authors_key(&authors),
        tags,
    }
}

fn write_index(conn: &Connection, id: &str, built: &Built) -> Result<()> {
    let existing: Option<i64> = conn
        .prepare_cached("SELECT idx FROM book_idx WHERE book_id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?;
    let idx = match existing {
        Some(idx) => {
            conn.prepare_cached("DELETE FROM books_fts WHERE rowid = ?1")?
                .execute([idx])?;
            conn.prepare_cached(
                "UPDATE book_idx SET sort_title = ?1, sort_authors = ?2 WHERE idx = ?3",
            )?
            .execute(params![built.sort_title, built.sort_authors, idx])?;
            idx
        }
        None => {
            conn.prepare_cached(
                "INSERT INTO book_idx(book_id, sort_title, sort_authors) VALUES (?1, ?2, ?3)",
            )?
            .execute(params![id, built.sort_title, built.sort_authors])?;
            conn.last_insert_rowid()
        }
    };
    insert_fts(conn, idx, &built.fts)?;
    conn.prepare_cached("DELETE FROM book_tags WHERE book_id = ?1")?
        .execute([id])?;
    insert_tags(conn, id, &built.tags)?;
    Ok(())
}

fn insert_fts(conn: &Connection, idx: i64, fts: &[String; 8]) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO books_fts(rowid, title, title_kana, authors, publisher, series, tags, memo, isbn)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?
    .execute(params![
        idx, fts[0], fts[1], fts[2], fts[3], fts[4], fts[5], fts[6], fts[7]
    ])?;
    Ok(())
}

fn insert_tags(conn: &Connection, id: &str, tags: &[String]) -> Result<()> {
    let mut stmt =
        conn.prepare_cached("INSERT OR IGNORE INTO book_tags(book_id, tag) VALUES (?1, ?2)")?;
    for tag in tags {
        stmt.execute(params![id, tag])?;
    }
    Ok(())
}

fn remove_index(conn: &Connection, id: &str) -> Result<()> {
    let existing: Option<i64> = conn
        .prepare_cached("SELECT idx FROM book_idx WHERE book_id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?;
    if let Some(idx) = existing {
        conn.prepare_cached("DELETE FROM books_fts WHERE rowid = ?1")?
            .execute([idx])?;
        conn.prepare_cached("DELETE FROM book_idx WHERE idx = ?1")?
            .execute([idx])?;
    }
    conn.prepare_cached("DELETE FROM book_tags WHERE book_id = ?1")?
        .execute([id])?;
    Ok(())
}

/// 派生データを全部作り直す（トランザクションの中で呼ぶ）。載せた本の数を返す。
pub fn rebuild_in(conn: &Connection) -> Result<usize> {
    conn.execute_batch("DELETE FROM books_fts; DELETE FROM book_idx; DELETE FROM book_tags;")?;
    let series: HashMap<String, String> = conn
        .prepare("SELECT id, name FROM series")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT id, isbn13, title, title_kana, subtitle, authors_json, publisher, tags_json, memo, series_id
         FROM books WHERE purged = 0",
    )?;
    let mut rows = stmt.query([])?;
    let mut count = 0;
    while let Some(r) = rows.next()? {
        let id: String = r.get(0)?;
        let series_id: Option<String> = r.get(9)?;
        let source = Source {
            isbn13: r.get(1)?,
            title: r.get(2)?,
            title_kana: r.get(3)?,
            subtitle: r.get(4)?,
            authors_json: r.get(5)?,
            publisher: r.get(6)?,
            tags_json: r.get(7)?,
            memo: r.get(8)?,
            series_name: series_id.and_then(|sid| series.get(&sid).cloned()),
        };
        write_index(conn, &id, &build(&source))?;
        count += 1;
    }
    Ok(count)
}

/// 派生データを全部作り直して、版を記録する（自前のトランザクション）。載せた本の数を返す。
pub fn rebuild(conn: &mut Connection) -> Result<usize> {
    let tx = write_tx(conn)?;
    let count = rebuild_in(&tx)?;
    meta::set(&tx, KEY_DERIVED_VERSION, DERIVED_VERSION)?;
    tx.commit()?;
    Ok(count)
}

/// 派生データのテーブルがあって、版が古い（または未作成の）ときだけ作り直す。作り直したら `true`。
///
/// `open` がマイグレーションの後に呼ぶ。0001 だけの DB（派生データのテーブルがない）では何もしない。
pub fn ensure_built(conn: &mut Connection) -> Result<bool> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'books_fts')",
        [],
        |r| r.get(0),
    )?;
    if !has_table || meta::get(conn, KEY_DERIVED_VERSION)?.as_deref() == Some(DERIVED_VERSION) {
        return Ok(false);
    }
    let count = rebuild(conn)?;
    tracing::info!(books = count, "検索用の派生データを作り直した");
    Ok(true)
}
