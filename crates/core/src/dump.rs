//! 全同期対象テーブルの正規化した書き出し。

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::{Map, Number, Value, json};

use crate::error::Result;
use crate::schema::{Entity, FIELD_SOURCES_COLUMN, FieldType, MANUAL_FIELDS_COLUMN};

/// 全同期対象テーブルの行と `field_clocks` を、比較しやすい形の JSON にする。
///
/// 端末ごとに異なる `updated_at` は含めない。`extra_json` は `extra` として含める。
/// 行は id 順、時計は (entity, id, field) 順で、項目名は Op 上の名前を使う。
/// 書籍の `field_sources`（オブジェクト）と `manual_fields`（項目名の配列）は、項目ごとの Op キー
/// （`field_sources.<項目名>`、`manual_fields.<項目名>`）から組み立てた保存形式の値で、
/// 時計は `field_clocks` にドット付きのキーで入る。
/// 値は型に従って、Bool は真偽値、Json は JSON 値に戻して出す。
///
/// ```text
/// {
///   "books": [{"id": "...", "title": "...", "authors": [...], "deleted": false, ..., "extra": {}}],
///   "series": [...], "locations": [...], "tags": [...], "shelves": [...],
///   "field_clocks": [{"ent": "book", "id": "...", "field": "title", "hlc": "..."}]
/// }
/// ```
///
/// 2 つの DB に同じ変更が行き渡ったかの判定（収束テスト）や、JSON 書き出しの土台に使う。
pub fn dump_canonical(conn: &Connection) -> Result<Value> {
    let mut root = Map::new();
    for ent in Entity::ALL {
        root.insert(ent.table().to_owned(), Value::Array(dump_table(conn, ent)?));
    }
    root.insert("field_clocks".to_owned(), Value::Array(dump_clocks(conn)?));
    Ok(Value::Object(root))
}

/// 書き出す 1 項目の定義（Op 上の名前、列名、型）。
pub(crate) struct ColSpec {
    pub(crate) name: &'static str,
    pub(crate) column: &'static str,
    pub(crate) ty: FieldType,
}

/// エンティティの書き出し項目。書籍は、項目ごとの Op キーから組み立てる 2 列
/// （`field_sources` は JSON オブジェクト、`manual_fields` は項目名の配列）も出す。
pub(crate) fn col_specs(ent: Entity) -> Vec<ColSpec> {
    let mut specs: Vec<ColSpec> = ent
        .fields()
        .iter()
        .map(|f| ColSpec {
            name: f.name,
            column: f.column,
            ty: f.ty,
        })
        .collect();
    if ent == Entity::Book {
        specs.push(ColSpec {
            name: "field_sources",
            column: FIELD_SOURCES_COLUMN,
            ty: FieldType::Json,
        });
        specs.push(ColSpec {
            name: "manual_fields",
            column: MANUAL_FIELDS_COLUMN,
            ty: FieldType::Json,
        });
    }
    specs
}

pub(crate) fn row_object(row: &rusqlite::Row<'_>, specs: &[ColSpec]) -> Result<Map<String, Value>> {
    let mut obj = Map::new();
    obj.insert("id".to_owned(), Value::String(row.get(0)?));
    let extra: String = row.get(1)?;
    obj.insert("extra".to_owned(), serde_json::from_str(&extra)?);
    for (i, f) in specs.iter().enumerate() {
        obj.insert(f.name.to_owned(), column_to_json(row.get_ref(i + 2)?, f.ty));
    }
    Ok(obj)
}

pub(crate) fn select_sql(ent: Entity, specs: &[ColSpec]) -> String {
    let columns: Vec<String> = specs.iter().map(|f| format!("\"{}\"", f.column)).collect();
    format!(
        "SELECT id, extra_json, {} FROM {}",
        columns.join(", "),
        ent.table()
    )
}

fn dump_table(conn: &Connection, ent: Entity) -> Result<Vec<Value>> {
    let specs = col_specs(ent);
    let sql = format!("{} ORDER BY id", select_sql(ent, &specs));
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Value::Object(row_object(row, &specs)?));
    }
    Ok(out)
}

/// 1 行の項目の値（Op 上の名前 → 値）を読む。`id` と `extra` も含む。行がなければ `None`。
///
/// 書籍には `field_sources`（オブジェクト）と `manual_fields`（配列）も入る。
pub(crate) fn load_row(
    conn: &Connection,
    ent: Entity,
    id: &str,
) -> Result<Option<Map<String, Value>>> {
    let specs = col_specs(ent);
    let sql = format!("{} WHERE id = ?1", select_sql(ent, &specs));
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query([id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_object(row, &specs)?)),
        None => Ok(None),
    }
}

fn column_to_json(v: ValueRef<'_>, ty: FieldType) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) if ty == FieldType::Bool => Value::Bool(i != 0),
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
        ValueRef::Text(t) => {
            let s = String::from_utf8_lossy(t);
            if ty == FieldType::Json {
                // 壊れた JSON は文字列のまま出して、比較で差が出るようにする。
                serde_json::from_str(&s).unwrap_or_else(|_| Value::String(s.into_owned()))
            } else {
                Value::String(s.into_owned())
            }
        }
        ValueRef::Blob(b) => json!(b),
    }
}

fn dump_clocks(conn: &Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT entity, entity_id, field, hlc FROM field_clocks
         ORDER BY entity, entity_id, field",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "ent": r.get::<_, String>(0)?,
            "id": r.get::<_, String>(1)?,
            "field": r.get::<_, String>(2)?,
            "hlc": r.get::<_, String>(3)?,
        }))
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}
