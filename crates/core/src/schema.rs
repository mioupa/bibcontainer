//! 同期対象エンティティと、Op で変更できる項目の定義。
//!
//! 項目名（Op 上の名前）と列名は、`authors` → `authors_json` のように一部が異なる。
//! 同期する項目は、各テーブルの列のうち `id`、`extra_json`、`updated_at` 以外すべて。

use std::collections::BTreeMap;

use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::ids::is_valid_entity_id;

/// 項目名 → 値。キーは辞書順に並ぶので、Op の JSON が安定する。
pub type Fields = BTreeMap<String, Value>;

/// 同期対象のエンティティ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Entity {
    /// 書籍（`books`）。
    Book,
    /// シリーズ（`series`）。
    Series,
    /// 保管場所（`locations`）。
    Location,
    /// タグの属性（`tags`）。
    Tag,
    /// スマート本棚（`shelves`）。
    Shelf,
}

impl Entity {
    /// すべてのエンティティ（`dump_canonical` の出力順でもある）。
    pub const ALL: [Entity; 5] = [
        Entity::Book,
        Entity::Series,
        Entity::Location,
        Entity::Tag,
        Entity::Shelf,
    ];

    /// Op の `ent` と `field_clocks.entity` に使う名前。
    pub fn as_str(self) -> &'static str {
        match self {
            Entity::Book => "book",
            Entity::Series => "series",
            Entity::Location => "location",
            Entity::Tag => "tag",
            Entity::Shelf => "shelf",
        }
    }

    /// テーブル名。
    pub fn table(self) -> &'static str {
        match self {
            Entity::Book => "books",
            Entity::Series => "series",
            Entity::Location => "locations",
            Entity::Tag => "tags",
            Entity::Shelf => "shelves",
        }
    }

    /// 同期する項目の一覧。
    pub fn fields(self) -> &'static [FieldDef] {
        match self {
            Entity::Book => BOOK_FIELDS,
            Entity::Series => SERIES_FIELDS,
            Entity::Location => LOCATION_FIELDS,
            Entity::Tag => TAG_FIELDS,
            Entity::Shelf => SHELF_FIELDS,
        }
    }

    /// Op 上の項目名から定義を引く。知らない項目名なら `None`。
    pub fn field(self, name: &str) -> Option<&'static FieldDef> {
        self.fields().iter().find(|f| f.name == name)
    }
}

/// 項目の型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// 文字列。
    Text,
    /// i64。
    Integer,
    /// 実数（f64）。
    Real,
    /// DB には 0/1、Op では JSON の true/false。
    Bool,
    /// 配列またはオブジェクト。JSON 文字列として列に保存する。
    Json,
}

/// 列の既定値（NOT NULL 列に null が来たときに書く値）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColDefault {
    /// NULL 可の列。
    Null,
    /// 文字列の既定値。
    Text(&'static str),
    /// 整数（真偽値を含む）の既定値。
    Int(i64),
}

/// 項目の定義。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldDef {
    /// Op 上の項目名。
    pub name: &'static str,
    /// DB の列名。
    pub column: &'static str,
    /// 値の型。
    pub ty: FieldType,
    /// `Null` なら NULL 可。それ以外は NOT NULL 列で、これが DEFAULT と同じ値。
    pub default: ColDefault,
}

impl FieldDef {
    /// NULL を許す列か。
    pub fn is_nullable(&self) -> bool {
        self.default == ColDefault::Null
    }

    /// 列の既定値（NULL 可の列では NULL）。
    pub fn default_sql(&self) -> SqlValue {
        match self.default {
            ColDefault::Null => SqlValue::Null,
            ColDefault::Text(s) => SqlValue::Text(s.to_owned()),
            ColDefault::Int(i) => SqlValue::Integer(i),
        }
    }

    /// null 以外の値が、この項目の型に合うか。
    pub fn accepts(&self, v: &Value) -> bool {
        match self.ty {
            FieldType::Text => v.is_string(),
            FieldType::Integer => v.as_i64().is_some(),
            FieldType::Real => v.is_number(),
            FieldType::Bool => v.is_boolean(),
            FieldType::Json => v.is_array() || v.is_object(),
        }
    }

    /// 列の既定値を Op の値（JSON）で表したもの。NULL 可の列では null、
    /// Bool は真偽値、Json は配列かオブジェクト。
    pub fn default_value(&self) -> Value {
        match self.default {
            ColDefault::Null => Value::Null,
            ColDefault::Text(s) if self.ty == FieldType::Json => {
                serde_json::from_str(s).unwrap_or(Value::Null)
            }
            ColDefault::Text(s) => Value::String(s.to_owned()),
            ColDefault::Int(i) if self.ty == FieldType::Bool => Value::Bool(i != 0),
            ColDefault::Int(i) => Value::from(i),
        }
    }

    /// 列に書く値。null なら列の既定値。型が合わなければ `None`。
    pub fn to_sql(&self, v: &Value) -> Option<SqlValue> {
        if v.is_null() {
            return Some(self.default_sql());
        }
        if !self.accepts(v) {
            return None;
        }
        Some(match self.ty {
            FieldType::Text => SqlValue::Text(v.as_str()?.to_owned()),
            FieldType::Integer => SqlValue::Integer(v.as_i64()?),
            FieldType::Real => SqlValue::Real(v.as_f64()?),
            FieldType::Bool => SqlValue::Integer(i64::from(v.as_bool()?)),
            FieldType::Json => SqlValue::Text(v.to_string()),
        })
    }
}

const fn nullable(name: &'static str, ty: FieldType) -> FieldDef {
    FieldDef {
        name,
        column: name,
        ty,
        default: ColDefault::Null,
    }
}

const fn required(name: &'static str, ty: FieldType, default: ColDefault) -> FieldDef {
    FieldDef {
        name,
        column: name,
        ty,
        default,
    }
}

/// Op 上の名前と列名が違う項目に、列名を指定する。
const fn column(mut def: FieldDef, column: &'static str) -> FieldDef {
    def.column = column;
    def
}

use ColDefault::{Int, Text as Txt};
use FieldType::{Bool, Integer, Json, Real, Text};

const NO: ColDefault = Int(0);

const BOOK_FIELDS: &[FieldDef] = &[
    nullable("isbn13", Text),
    required("title", Text, Txt("")),
    nullable("title_kana", Text),
    nullable("subtitle", Text),
    nullable("series_id", Text),
    nullable("volume", Text),
    nullable("volume_sort", Real),
    column(required("authors", Json, Txt("[]")), "authors_json"),
    nullable("publisher", Text),
    nullable("published_on", Text),
    nullable("c_code", Text),
    nullable("ndc", Text),
    nullable("list_price", Integer),
    nullable("ref_price_incl", Integer),
    nullable("pages", Integer),
    nullable("size_text", Text),
    nullable("language", Text),
    nullable("description", Text),
    nullable("cover_hash", Text),
    required("ownership", Text, Txt("owned")),
    nullable("wish_priority", Text),
    nullable("location_id", Text),
    column(required("tags", Json, Txt("[]")), "tags_json"),
    required("read_status", Text, Txt("unread")),
    nullable("read_started_on", Text),
    nullable("read_finished_on", Text),
    nullable("rating", Integer),
    nullable("review", Text),
    nullable("purchased_on", Text),
    nullable("purchase_store", Text),
    nullable("purchase_price", Integer),
    nullable("purchase_cond", Text),
    nullable("purchase_memo", Text),
    nullable("memo", Text),
    required("meta_status", Text, Txt("ok")),
    required("deleted", Bool, NO),
    required("purged", Bool, NO),
    required("created_at", Text, Txt("")),
];

const SERIES_FIELDS: &[FieldDef] = &[
    required("name", Text, Txt("")),
    nullable("name_kana", Text),
    nullable("memo", Text),
    required("deleted", Bool, NO),
    required("created_at", Text, Txt("")),
];

const LOCATION_FIELDS: &[FieldDef] = &[
    required("name", Text, Txt("")),
    nullable("parent_id", Text),
    nullable("sort_order", Real),
    required("deleted", Bool, NO),
    required("created_at", Text, Txt("")),
];

const TAG_FIELDS: &[FieldDef] = &[
    required("name", Text, Txt("")),
    nullable("color", Text),
    required("deleted", Bool, NO),
    required("created_at", Text, Txt("")),
];

const SHELF_FIELDS: &[FieldDef] = &[
    required("name", Text, Txt("")),
    column(required("query", Json, Txt("{}")), "query_json"),
    nullable("sort_order", Real),
    required("deleted", Bool, NO),
    required("created_at", Text, Txt("")),
];

/// 書籍の取得元の記録の Op キーの接頭辞（`field_sources.<項目名>`）。値は取得元の文字列か null。
pub const FIELD_SOURCES_PREFIX: &str = "field_sources.";
/// 書籍の手修正の記録の Op キーの接頭辞（`manual_fields.<項目名>`）。値は `true` か null。
pub const MANUAL_FIELDS_PREFIX: &str = "manual_fields.";

/// 取得元の記録を保存する列（`books.field_sources_json`。オブジェクト）。
/// Op のキーは項目ごと（`field_sources.<項目名>`）で、この列は Op から組み立てる保存形式。
pub const FIELD_SOURCES_COLUMN: &str = "field_sources_json";
/// 手修正の記録を保存する列（`books.manual_fields_json`。項目名の配列）。
pub const MANUAL_FIELDS_COLUMN: &str = "manual_fields_json";

/// `field_sources.<項目名>` のキーを作る。
pub fn field_sources_key(field: &str) -> String {
    format!("{FIELD_SOURCES_PREFIX}{field}")
}

/// `manual_fields.<項目名>` のキーを作る。
pub fn manual_fields_key(field: &str) -> String {
    format!("{MANUAL_FIELDS_PREFIX}{field}")
}

/// Op の `set` のキーの種類。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpKey {
    /// 通常の項目。列に保存する。
    Column(&'static FieldDef),
    /// 書籍の `field_sources.<項目名>`。中身は対象の項目の定義。
    Source(&'static FieldDef),
    /// 書籍の `manual_fields.<項目名>`。中身は対象の項目の定義。
    Manual(&'static FieldDef),
    /// `field_sources.` か `manual_fields.` で始まるが、項目名が書籍の既知の項目でない。
    BadProvenance,
    /// 知らない項目。`extra_json` に保持する。
    Unknown,
}

/// Op の `set` のキーを分類する。`field_sources.` と `manual_fields.` は書籍だけで意味を持つ。
pub fn classify_key(ent: Entity, name: &str) -> OpKey {
    if let Some(def) = ent.field(name) {
        return OpKey::Column(def);
    }
    if ent == Entity::Book {
        let target = |prefix: &str| name.strip_prefix(prefix).map(|f| Entity::Book.field(f));
        if let Some(found) = target(FIELD_SOURCES_PREFIX) {
            return found.map_or(OpKey::BadProvenance, OpKey::Source);
        }
        if let Some(found) = target(MANUAL_FIELDS_PREFIX) {
            return found.map_or(OpKey::BadProvenance, OpKey::Manual);
        }
    }
    OpKey::Unknown
}

/// ローカルの変更として記録してよいか検証する。
///
/// 他端末から届く Op と違い、未知の項目・型の不一致・NOT NULL 列への null はエラーにする
/// （自分のコードの誤りを早く見つけるため）。
pub fn validate_local_change(ent: Entity, id: &str, set: &Fields) -> Result<()> {
    let bad = |msg: String| Err(Error::InvalidOp(msg));
    if !is_valid_entity_id(id) {
        return bad(format!("id が UUID の書式でない: {id:?}"));
    }
    if set.is_empty() {
        return bad("set が空".to_owned());
    }
    for (name, value) in set {
        let def = match classify_key(ent, name) {
            OpKey::Column(def) => def,
            OpKey::Source(_) => {
                if !value.is_null() && value.as_str().is_none_or(str::is_empty) {
                    return bad(format!("{name} は空でない文字列か null（値: {value}）"));
                }
                continue;
            }
            OpKey::Manual(_) => {
                if !value.is_null() && *value != Value::Bool(true) {
                    return bad(format!("{name} は true か null（値: {value}）"));
                }
                continue;
            }
            OpKey::BadProvenance => {
                return bad(format!(
                    "{name} の項目名が書籍の既知の項目でない（{} に項目 {name:?} はない）",
                    ent.as_str()
                ));
            }
            OpKey::Unknown => {
                return bad(format!("{} に項目 {name:?} はない", ent.as_str()));
            }
        };
        if value.is_null() {
            if !def.is_nullable() {
                return bad(format!(
                    "{}.{name} は NOT NULL なので null にできない",
                    ent.as_str()
                ));
            }
        } else if !def.accepts(value) {
            return bad(format!(
                "{}.{name} の型が合わない（期待: {:?}、値: {value}）",
                ent.as_str(),
                def.ty
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;
    use crate::db;

    /// 項目定義が実際のスキーマ（0001_init.sql）と食い違っていないことを確かめる。
    #[test]
    fn field_defs_match_the_migrated_schema() {
        let conn = db::open_in_memory().unwrap();
        for ent in Entity::ALL {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({})", ent.table()))
                .unwrap();
            // name -> (type, notnull, dflt_value)
            let cols: BTreeMap<String, (String, bool, Option<String>)> = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>("name")?,
                        (
                            r.get::<_, String>("type")?,
                            r.get::<_, bool>("notnull")?,
                            r.get::<_, Option<String>>("dflt_value")?,
                        ),
                    ))
                })
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap();

            // field_sources_json / manual_fields_json は、項目ごとの Op キー
            // （field_sources.<項目名> など）から組み立てる保存形式なので、項目定義には含めない。
            let composed = if ent == Entity::Book {
                vec![FIELD_SOURCES_COLUMN, MANUAL_FIELDS_COLUMN]
            } else {
                Vec::new()
            };
            let synced: BTreeSet<&str> = cols
                .keys()
                .map(String::as_str)
                .filter(|c| !["id", "extra_json", "updated_at"].contains(c))
                .filter(|c| !composed.contains(c))
                .collect();
            let defined: BTreeSet<&str> = ent.fields().iter().map(|f| f.column).collect();
            assert_eq!(synced, defined, "{} の列と項目定義が一致する", ent.table());
            assert_eq!(
                ent.fields().len(),
                defined.len(),
                "{} の列名が重複していない",
                ent.table()
            );
            let names: BTreeSet<&str> = ent.fields().iter().map(|f| f.name).collect();
            assert_eq!(names.len(), ent.fields().len(), "項目名が重複していない");

            for f in ent.fields() {
                let (ty, notnull, dflt) = &cols[f.column];
                let want_ty = match f.ty {
                    Text | Json => "TEXT",
                    Integer | Bool => "INTEGER",
                    Real => "REAL",
                };
                assert_eq!(ty, want_ty, "{}.{} の型", ent.table(), f.column);
                assert_eq!(
                    *notnull,
                    !f.is_nullable(),
                    "{}.{} の NOT NULL",
                    ent.table(),
                    f.column
                );
                let want_default = match f.default {
                    ColDefault::Null => None,
                    ColDefault::Text(s) => Some(format!("'{s}'")),
                    ColDefault::Int(i) => Some(i.to_string()),
                };
                assert_eq!(
                    *dflt,
                    want_default,
                    "{}.{} の DEFAULT",
                    ent.table(),
                    f.column
                );
            }

            if ent == Entity::Book {
                assert_eq!(
                    cols[FIELD_SOURCES_COLUMN],
                    ("TEXT".to_owned(), true, Some("'{}'".to_owned()))
                );
                assert_eq!(
                    cols[MANUAL_FIELDS_COLUMN],
                    ("TEXT".to_owned(), true, Some("'[]'".to_owned()))
                );
            }

            // extra_json と id もスキーマどおり
            assert_eq!(cols["extra_json"].2.as_deref(), Some("'{}'"));
            assert!(cols["id"].1, "id は NOT NULL");
            assert_eq!(cols["updated_at"].2.as_deref(), Some("''"));
        }
    }

    #[test]
    fn op_names_that_differ_from_column_names() {
        let book = |n: &str| Entity::Book.field(n).map(|f| f.column);
        assert_eq!(book("authors"), Some("authors_json"));
        assert_eq!(book("tags"), Some("tags_json"));
        // 項目全体のキーは廃止した（項目ごとの field_sources.<項目名> に置き換えた）
        assert_eq!(book("field_sources"), None);
        assert_eq!(book("manual_fields"), None);
        assert_eq!(book("title"), Some("title"));
        assert_eq!(book("authors_json"), None);
        assert_eq!(
            Entity::Shelf.field("query").map(|f| f.column),
            Some("query_json")
        );
        assert!(Entity::Book.field("id").is_none());
        assert!(Entity::Book.field("extra_json").is_none());
        assert!(Entity::Book.field("updated_at").is_none());
    }

    #[test]
    fn field_types_follow_the_design() {
        let ty = |e: Entity, n: &str| e.field(n).unwrap().ty;
        for n in [
            "list_price",
            "ref_price_incl",
            "pages",
            "rating",
            "purchase_price",
        ] {
            assert_eq!(ty(Entity::Book, n), Integer, "{n}");
        }
        assert_eq!(ty(Entity::Book, "volume_sort"), Real);
        for n in ["deleted", "purged"] {
            assert_eq!(ty(Entity::Book, n), Bool, "{n}");
        }
        for n in ["authors", "tags"] {
            assert_eq!(ty(Entity::Book, n), Json, "{n}");
        }
        for n in ["created_at", "meta_status", "title", "memo", "isbn13"] {
            assert_eq!(ty(Entity::Book, n), Text, "{n}");
        }
        assert_eq!(ty(Entity::Location, "sort_order"), Real);
        assert_eq!(ty(Entity::Shelf, "sort_order"), Real);
        assert_eq!(ty(Entity::Shelf, "query"), Json);
    }

    #[test]
    fn classify_key_recognizes_per_field_provenance_keys() {
        let title = Entity::Book.field("title").unwrap();
        assert_eq!(classify_key(Entity::Book, "title"), OpKey::Column(title));
        assert_eq!(
            classify_key(Entity::Book, "field_sources.title"),
            OpKey::Source(title)
        );
        assert_eq!(
            classify_key(Entity::Book, "manual_fields.title"),
            OpKey::Manual(title)
        );
        // 項目名が書籍の既知の項目でない
        for bad in [
            "field_sources.nope",
            "manual_fields.nope",
            "field_sources.",
            "field_sources.field_sources",
            "manual_fields.manual_fields",
            "field_sources.id",
        ] {
            assert_eq!(
                classify_key(Entity::Book, bad),
                OpKey::BadProvenance,
                "{bad}"
            );
        }
        // 項目全体のキーは廃止した（知らない項目として extra に入る）
        assert_eq!(classify_key(Entity::Book, "field_sources"), OpKey::Unknown);
        assert_eq!(classify_key(Entity::Book, "manual_fields"), OpKey::Unknown);
        // 書籍以外では意味を持たない
        assert_eq!(
            classify_key(Entity::Series, "field_sources.name"),
            OpKey::Unknown
        );
    }

    #[test]
    fn local_validation_of_provenance_keys() {
        let id = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55";
        let check = |pairs: &[(&str, Value)]| {
            let set: Fields = pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect();
            validate_local_change(Entity::Book, id, &set)
        };
        assert!(check(&[("field_sources.title", json!("openbd"))]).is_ok());
        assert!(check(&[("field_sources.title", json!(null))]).is_ok());
        assert!(check(&[("manual_fields.title", json!(true))]).is_ok());
        assert!(check(&[("manual_fields.title", json!(null))]).is_ok());
        assert!(check(&[("field_sources.title", json!(1))]).is_err());
        assert!(check(&[("field_sources.title", json!(""))]).is_err());
        assert!(check(&[("manual_fields.title", json!(false))]).is_err());
        assert!(check(&[("manual_fields.title", json!("x"))]).is_err());
        assert!(check(&[("field_sources.nope", json!("x"))]).is_err());
        assert!(check(&[("manual_fields.nope", json!(true))]).is_err());
        assert!(check(&[("field_sources", json!({"title": "x"}))]).is_err());
    }

    #[test]
    fn entity_serializes_in_lowercase() {
        for (e, s) in [
            (Entity::Book, "book"),
            (Entity::Series, "series"),
            (Entity::Location, "location"),
            (Entity::Tag, "tag"),
            (Entity::Shelf, "shelf"),
        ] {
            assert_eq!(serde_json::to_string(&e).unwrap(), format!("\"{s}\""));
            assert_eq!(e.as_str(), s);
        }
    }

    #[test]
    fn to_sql_converts_values() {
        let f = |n: &str| Entity::Book.field(n).unwrap();
        assert_eq!(
            f("title").to_sql(&json!("x")),
            Some(SqlValue::Text("x".into()))
        );
        assert_eq!(f("rating").to_sql(&json!(4)), Some(SqlValue::Integer(4)));
        assert_eq!(f("rating").to_sql(&json!(4.5)), None);
        assert_eq!(f("rating").to_sql(&json!("4")), None);
        assert_eq!(
            f("volume_sort").to_sql(&json!(2)),
            Some(SqlValue::Real(2.0))
        );
        assert_eq!(
            f("deleted").to_sql(&json!(true)),
            Some(SqlValue::Integer(1))
        );
        assert_eq!(f("deleted").to_sql(&json!(1)), None);
        assert_eq!(
            f("authors").to_sql(&json!([{"name": "a"}])),
            Some(SqlValue::Text("[{\"name\":\"a\"}]".into()))
        );
        assert_eq!(f("authors").to_sql(&json!("a")), None);
        // null は NOT NULL 列では既定値、NULL 可の列では NULL
        assert_eq!(
            f("title").to_sql(&json!(null)),
            Some(SqlValue::Text(String::new()))
        );
        assert_eq!(
            f("authors").to_sql(&json!(null)),
            Some(SqlValue::Text("[]".into()))
        );
        assert_eq!(
            f("deleted").to_sql(&json!(null)),
            Some(SqlValue::Integer(0))
        );
        assert_eq!(f("rating").to_sql(&json!(null)), Some(SqlValue::Null));
    }
}
