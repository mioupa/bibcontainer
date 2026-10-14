//! 書き出し（FR-IO-01 CSV、FR-IO-02 JSON）。
//!
//! - CSV: UTF-8（BOM 付き）、改行 CRLF、ヘッダーは日本語の項目名。1 冊 1 行で、同期する全項目と
//!   シリーズ名・保管場所のパスを含む。著者は `名前（役割）` を `; ` で連結、タグも `; ` で連結する。
//!   列挙値（所有状態など）は日本語の表示名で書く。対象の ID を渡すと、その ID の本だけを渡した順に書く
//!   （表示中の絞り込み結果だけの書き出し）。
//! - JSON: 全エンティティ（ゴミ箱・完全削除を含む）を、ID・項目の値・項目ごとの HLC つきで書く。

use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::dump::dump_canonical;
use crate::error::{Error, Result};
use crate::hlc::{Clock, ms_to_iso8601};
use crate::library::location::LocationIndex;
use crate::meta;
use crate::migrate::user_version;
use crate::rows::{BookDetail, all_details, details_for};
use crate::schema::Entity;

/// CSV の先頭に付ける UTF-8 の BOM（Excel が UTF-8 と判定するため）。
pub const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// JSON 書き出しの形式名。
pub const JSON_FORMAT: &str = "bibcontainer-library";
/// JSON 書き出しの形式の版。
pub const JSON_FORMAT_VERSION: u32 = 1;

struct Lookups {
    series: HashMap<String, String>,
    locations: LocationIndex,
}

type Column = (&'static str, fn(&BookDetail, &Lookups) -> String);

fn opt(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

fn num<T: ToString>(v: &Option<T>) -> String {
    v.as_ref().map(ToString::to_string).unwrap_or_default()
}

fn label(value: &str, table: &[(&str, &str)]) -> String {
    table
        .iter()
        .find(|(k, _)| *k == value)
        .map_or_else(|| value.to_owned(), |(_, ja)| (*ja).to_owned())
}

const OWNERSHIP_JA: &[(&str, &str)] = &[
    ("owned", "所有"),
    ("wanted", "欲しい本"),
    ("released", "手放した"),
];
const READ_STATUS_JA: &[(&str, &str)] = &[
    ("unread", "未読"),
    ("reading", "読書中"),
    ("read", "読了"),
    ("paused", "中断"),
];
const PRIORITY_JA: &[(&str, &str)] = &[("high", "高"), ("mid", "中"), ("low", "低")];
const COND_JA: &[(&str, &str)] = &[("new", "新品"), ("used", "中古")];
const META_JA: &[(&str, &str)] = &[
    ("ok", "取得済み"),
    ("pending", "取得待ち"),
    ("partial", "一部取得"),
    ("failed", "取得失敗"),
];

/// CSV の列（ヘッダー、値の取り出し方）。並びがそのまま CSV の列の並び。
const COLUMNS: &[Column] = &[
    ("ID", |b, _| b.id.clone()),
    ("ISBN", |b, _| opt(&b.isbn13)),
    ("書名", |b, _| b.title.clone()),
    ("書名のヨミ", |b, _| opt(&b.title_kana)),
    ("副書名", |b, _| opt(&b.subtitle)),
    ("シリーズ", |b, l| {
        b.series_id
            .as_ref()
            .and_then(|id| l.series.get(id))
            .cloned()
            .unwrap_or_default()
    }),
    ("巻次", |b, _| opt(&b.volume)),
    ("巻次（並べ替え用）", |b, _| num(&b.volume_sort)),
    ("著者", |b, _| {
        b.authors
            .iter()
            .map(|a| match &a.role {
                Some(role) => format!("{}（{role}）", a.name),
                None => a.name.clone(),
            })
            .collect::<Vec<_>>()
            .join("; ")
    }),
    ("出版社", |b, _| opt(&b.publisher)),
    ("発行日", |b, _| opt(&b.published_on)),
    ("Cコード", |b, _| opt(&b.c_code)),
    ("NDC", |b, _| opt(&b.ndc)),
    ("本体価格", |b, _| num(&b.list_price)),
    ("参考税込価格", |b, _| num(&b.ref_price_incl)),
    ("ページ数", |b, _| num(&b.pages)),
    ("大きさ", |b, _| opt(&b.size_text)),
    ("言語", |b, _| opt(&b.language)),
    ("内容紹介", |b, _| opt(&b.description)),
    ("書影", |b, _| opt(&b.cover_hash)),
    ("所有状態", |b, _| label(&b.ownership, OWNERSHIP_JA)),
    ("欲しい本の優先度", |b, _| {
        b.wish_priority
            .as_deref()
            .map(|p| label(p, PRIORITY_JA))
            .unwrap_or_default()
    }),
    ("保管場所", |b, l| {
        b.location_id
            .as_deref()
            .and_then(|id| l.locations.path(id))
            .unwrap_or_default()
    }),
    ("タグ", |b, _| b.tags.join("; ")),
    ("読書状況", |b, _| label(&b.read_status, READ_STATUS_JA)),
    ("読み始めた日", |b, _| opt(&b.read_started_on)),
    ("読了日", |b, _| opt(&b.read_finished_on)),
    ("評価", |b, _| num(&b.rating)),
    ("感想", |b, _| opt(&b.review)),
    ("購入日", |b, _| opt(&b.purchased_on)),
    ("購入店", |b, _| opt(&b.purchase_store)),
    ("購入価格", |b, _| num(&b.purchase_price)),
    ("新品・中古", |b, _| {
        b.purchase_cond
            .as_deref()
            .map(|c| label(c, COND_JA))
            .unwrap_or_default()
    }),
    ("購入メモ", |b, _| opt(&b.purchase_memo)),
    ("メモ", |b, _| opt(&b.memo)),
    ("取得状態", |b, _| label(&b.meta_status, META_JA)),
    ("ゴミ箱", |b, _| {
        if b.deleted {
            "はい".to_owned()
        } else {
            String::new()
        }
    }),
    ("登録日時", |b, _| b.created_at.clone()),
];

/// CSV のヘッダー（日本語の項目名）。
pub fn csv_headers() -> Vec<&'static str> {
    COLUMNS.iter().map(|(h, _)| *h).collect()
}

/// 表計算ソフトが数式として解釈しうる先頭の文字（`=`、`+`、`-`、`@`、タブ、CR）。
const FORMULA_PREFIXES: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// 数式注入の対策。取得元の書名などは信頼できない入力なので、数式の先頭になりうる文字で始まる欄は、
/// 先頭に `'` を付けて文字列として読ませる（OWASP の CSV Injection の推奨）。
fn neutralize_formula(s: &str) -> std::borrow::Cow<'_, str> {
    if s.starts_with(FORMULA_PREFIXES) {
        std::borrow::Cow::Owned(format!("'{s}"))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

fn csv_field(s: &str, out: &mut String) {
    let s = neutralize_formula(s);
    let s = s.as_ref();
    if s.contains(['"', ',', '\r', '\n']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

fn csv_line(fields: impl Iterator<Item = String>) -> String {
    let mut line = String::new();
    for (i, f) in fields.enumerate() {
        if i > 0 {
            line.push(',');
        }
        csv_field(&f, &mut line);
    }
    line.push_str("\r\n");
    line
}

/// CSV を書き出す。書いた本の数を返す。
///
/// `ids` が `None` なら、完全削除していない全冊を登録順に書く（ゴミ箱の本も含み、「ゴミ箱」列で分かる）。
/// `Some` なら、その ID の本を渡した順に書く（存在しない ID・完全削除した ID は飛ばす）。
pub fn write_csv<W: Write>(
    conn: &Connection,
    ids: Option<&[String]>,
    out: &mut W,
) -> Result<usize> {
    let books = match ids {
        Some(ids) => details_for(conn, ids)?,
        None => all_details(conn)?,
    };
    let lookups = Lookups {
        series: conn
            .prepare("SELECT id, name FROM series")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?,
        locations: LocationIndex::load(conn)?,
    };
    out.write_all(UTF8_BOM)?;
    out.write_all(csv_line(COLUMNS.iter().map(|(h, _)| (*h).to_owned())).as_bytes())?;
    for book in &books {
        out.write_all(csv_line(COLUMNS.iter().map(|(_, f)| f(book, &lookups))).as_bytes())?;
    }
    Ok(books.len())
}

/// CSV をバイト列で返す（BOM 付き）。
pub fn export_csv(conn: &Connection, ids: Option<&[String]>) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    write_csv(conn, ids, &mut buf)?;
    Ok(buf)
}

/// CSV をファイルに書き出す。書いた本の数を返す。途中で失敗しても、既存のファイルは壊さない。
pub fn export_csv_to_file(conn: &Connection, path: &Path, ids: Option<&[String]>) -> Result<usize> {
    write_file_atomically(path, |w| write_csv(conn, ids, w))
}

/// 全データの JSON 書き出し。キーは宣言の順に出る（形式名と版が先頭）。
///
/// ```text
/// { "format": "bibcontainer-library", "version": 1, "exportedAt": "...", "schemaVersion": 2,
///   "deviceId": "d-...",
///   "books": [{ "id": "...", "values": { "title": "...", ..., "field_sources": {..}, "manual_fields": [..] },
///               "extra": {..}, "clocks": { "title": "<HLC>", "field_sources.title": "<HLC>", .. } }],
///   "series": [..], "locations": [..], "tags": [..], "shelves": [..] }
/// ```
///
/// 完全削除した本・ゴミ箱の本を含む。`values` のキーは Op 上の項目名、`clocks` は項目ごとの HLC
/// （`field_sources.<項目名>` など、ドット付きのキーも）。`extra` は、この版が知らない項目。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryExport {
    pub format: &'static str,
    pub version: u32,
    /// 書き出した時刻（ISO 8601 UTC ミリ秒）。
    pub exported_at: String,
    /// 書き出した DB のスキーマ版。
    pub schema_version: u32,
    /// 書き出した端末の ID。
    pub device_id: Option<String>,
    pub books: Vec<Value>,
    pub series: Vec<Value>,
    pub locations: Vec<Value>,
    pub tags: Vec<Value>,
    pub shelves: Vec<Value>,
}

/// 全データを JSON 書き出しの形にする。
pub fn export_json(conn: &Connection, exported_at_ms: i64) -> Result<LibraryExport> {
    let dump = dump_canonical(conn)?;

    let mut clocks: HashMap<(String, String), Map<String, Value>> = HashMap::new();
    for c in dump["field_clocks"].as_array().into_iter().flatten() {
        let (Some(ent), Some(id), Some(field), Some(hlc)) = (
            c["ent"].as_str(),
            c["id"].as_str(),
            c["field"].as_str(),
            c["hlc"].as_str(),
        ) else {
            continue;
        };
        clocks
            .entry((ent.to_owned(), id.to_owned()))
            .or_default()
            .insert(field.to_owned(), json!(hlc));
    }

    let mut rows_of = |ent: Entity| -> Vec<Value> {
        dump[ent.table()]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .map(|row| {
                let mut values = row.clone();
                let id = values.remove("id").unwrap_or(Value::Null);
                let extra = values.remove("extra").unwrap_or_else(|| json!({}));
                let row_clocks = id
                    .as_str()
                    .and_then(|i| clocks.remove(&(ent.as_str().to_owned(), i.to_owned())))
                    .unwrap_or_default();
                json!({ "id": id, "values": values, "extra": extra, "clocks": row_clocks })
            })
            .collect()
    };

    Ok(LibraryExport {
        format: JSON_FORMAT,
        version: JSON_FORMAT_VERSION,
        exported_at: ms_to_iso8601(exported_at_ms)?,
        schema_version: user_version(conn)?,
        device_id: meta::get(conn, meta::KEY_DEVICE_ID)?,
        books: rows_of(Entity::Book),
        series: rows_of(Entity::Series),
        locations: rows_of(Entity::Location),
        tags: rows_of(Entity::Tag),
        shelves: rows_of(Entity::Shelf),
    })
}

/// JSON をファイルに書き出す（読みやすいよう整形する）。途中で失敗しても、既存のファイルは壊さない。
pub fn export_json_to_file(conn: &Connection, clock: &dyn Clock, path: &Path) -> Result<()> {
    let value = export_json(conn, clock.now_ms())?;
    write_file_atomically(path, |w| {
        serde_json::to_writer_pretty(&mut *w, &value)?;
        w.write_all(b"\n")?;
        Ok(())
    })
}

/// 同じフォルダの一時ファイルに書いてから、名前を付け替える。
fn write_file_atomically<T>(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<fs::File>) -> Result<T>,
) -> Result<T> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".tmp-{}", std::process::id()));
    let tmp: PathBuf = path.with_file_name(tmp_name);

    let result = (|| {
        let mut w = BufWriter::new(fs::File::create(&tmp)?);
        let out = write(&mut w)?;
        w.flush()?;
        w.into_inner()
            .map_err(std::io::IntoInnerError::into_error)?
            .sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(out)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutralizes_every_formula_prefix() {
        for prefix in ["=", "+", "-", "@", "\t", "\r"] {
            let field = format!("{prefix}SUM(A1)");
            assert_eq!(
                neutralize_formula(&field),
                format!("'{field}"),
                "{prefix:?}"
            );
        }
        for safe in ["", "書名", "1+1", "a=b", " =1", "'=1"] {
            assert_eq!(neutralize_formula(safe), safe, "{safe:?}");
        }
    }

    #[test]
    fn quote_comes_before_csv_escaping() {
        let mut out = String::new();
        csv_field("=A,\"B\"", &mut out);
        assert_eq!(out, "\"'=A,\"\"B\"\"\"");
        let mut out = String::new();
        csv_field("\r\n=1", &mut out);
        assert_eq!(out, "\"'\r\n=1\"");
    }
}
