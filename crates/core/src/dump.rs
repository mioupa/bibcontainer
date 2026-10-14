//! 全同期対象テーブルの正規化した書き出し。

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::{Map, Number, Value, json};

use crate::error::Result;
use crate::schema::{Entity, FieldType};

/// 全同期対象テーブルの行と `field_clocks` を、比較しやすい形の JSON にする。
///
/// 端末ごとに異なる `updated_at` は含めない。`extra_json` は `extra` として含める。
/// 行は id 順、時計は (entity, id, field) 順で、項目名は Op 上の名前を使う。
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

fn dump_table(conn: &Connection, ent: Entity) -> Result<Vec<Value>> {
    let fields = ent.fields();
    let columns: Vec<String> = fields.iter().map(|f| format!("\"{}\"", f.column)).collect();
    let sql = format!(
        "SELECT id, extra_json, {} FROM {} ORDER BY id",
        columns.join(", "),
        ent.table()
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut obj = Map::new();
        obj.insert("id".to_owned(), Value::String(row.get(0)?));
        let extra: String = row.get(1)?;
        obj.insert("extra".to_owned(), serde_json::from_str(&extra)?);
        for (i, f) in fields.iter().enumerate() {
            obj.insert(f.name.to_owned(), column_to_json(row.get_ref(i + 2)?, f.ty));
        }
        out.push(Value::Object(obj));
    }
    Ok(out)
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
