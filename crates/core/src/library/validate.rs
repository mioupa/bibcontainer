//! ローカルの変更の検証と、保存する値の正規化（書籍の項目）。
//!
//! 他端末から届いた Op（`apply_op`）には使わない。自分の変更だけを厳しく検べて、誤りを早く見つける。

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::ids::is_valid_entity_id;
use crate::normalize::{is_valid_isbn13, nfkc};
use crate::schema::Entity;
use crate::values::{META_STATUSES, OWNERSHIPS, PURCHASE_CONDS, READ_STATUSES, WISH_PRIORITIES};

/// `update_book` などで直接変えられない項目（専用の操作がある）。
const RESERVED: &[&str] = &["created_at", "deleted", "purged"];

/// 項目名が、書籍の項目として変更を受け付けるものか。
pub fn is_changeable_book_field(field: &str) -> bool {
    Entity::Book.field(field).is_some() && !RESERVED.contains(&field)
}

/// `YYYY`、`YYYY-MM`、`YYYY-MM-DD` のどれかで、実在する日付か。
pub fn is_valid_partial_date(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |from: usize, to: usize| b[from..to].iter().all(u8::is_ascii_digit);
    let num = |from: usize, to: usize| s[from..to].parse::<u32>().ok();
    match b.len() {
        4 => digits(0, 4),
        7 => {
            digits(0, 4)
                && b[4] == b'-'
                && digits(5, 7)
                && num(5, 7).is_some_and(|m| (1..=12).contains(&m))
        }
        10 => {
            digits(0, 4)
                && b[4] == b'-'
                && digits(5, 7)
                && b[7] == b'-'
                && digits(8, 10)
                && match (num(0, 4), num(5, 7), num(8, 10)) {
                    (Some(y), Some(m), Some(d)) => {
                        NaiveDate::from_ymd_opt(i32::try_from(y).unwrap_or(0), m, d).is_some()
                    }
                    _ => false,
                }
        }
        _ => false,
    }
}

fn bad(field: &str, message: impl Into<String>) -> Error {
    Error::validation(field, message)
}

fn as_string<'a>(field: &str, v: &'a Value) -> Result<&'a str> {
    v.as_str()
        .ok_or_else(|| bad(field, format!("文字列である必要があります（値: {v}）")))
}

/// 空白を除いた文字列。空なら null。
fn nullable_text(field: &str, v: &Value, multiline: bool) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let s = as_string(field, v)?;
    if s.trim().is_empty() {
        return Ok(Value::Null);
    }
    Ok(Value::String(if multiline {
        s.trim_end().to_owned()
    } else {
        s.trim().to_owned()
    }))
}

fn nullable_int(field: &str, v: &Value, min: i64, max: i64) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let n = v
        .as_i64()
        .ok_or_else(|| bad(field, format!("整数である必要があります（値: {v}）")))?;
    if !(min..=max).contains(&n) {
        return Err(bad(
            field,
            format!("{min} から {max} の範囲で指定してください（値: {n}）"),
        ));
    }
    Ok(json!(n))
}

fn enumeration(field: &str, v: &Value, allowed: &[&str], nullable: bool) -> Result<Value> {
    if v.is_null() {
        return if nullable {
            Ok(Value::Null)
        } else {
            Err(bad(field, "空にできません"))
        };
    }
    let s = as_string(field, v)?;
    if allowed.contains(&s) {
        Ok(Value::String(s.to_owned()))
    } else {
        Err(bad(
            field,
            format!(
                "{} のどれかを指定してください（値: {s:?}）",
                allowed.join(" / ")
            ),
        ))
    }
}

fn date(field: &str, v: &Value) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let s = nfkc(as_string(field, v)?);
    let s = s.trim();
    if s.is_empty() {
        return Ok(Value::Null);
    }
    if is_valid_partial_date(s) {
        Ok(Value::String(s.to_owned()))
    } else {
        Err(bad(
            field,
            format!("YYYY、YYYY-MM、YYYY-MM-DD のどれかで指定してください（値: {s:?}）"),
        ))
    }
}

fn isbn13(field: &str, v: &Value) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let compact: String = nfkc(as_string(field, v)?)
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if compact.is_empty() {
        return Ok(Value::Null);
    }
    if is_valid_isbn13(&compact) {
        Ok(Value::String(compact))
    } else {
        Err(bad(
            field,
            format!(
                "13 桁でチェックデジットが正しい ISBN-13 を指定してください（値: {compact:?}）"
            ),
        ))
    }
}

fn c_code(field: &str, v: &Value) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let s = nfkc(as_string(field, v)?);
    let s = s.trim();
    if s.is_empty() {
        return Ok(Value::Null);
    }
    if s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit()) {
        Ok(Value::String(s.to_owned()))
    } else {
        Err(bad(
            field,
            format!("4 桁の数字で指定してください（値: {s:?}）"),
        ))
    }
}

fn entity_ref(field: &str, v: &Value) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let s = as_string(field, v)?;
    if is_valid_entity_id(s) {
        Ok(Value::String(s.to_owned()))
    } else {
        Err(bad(
            field,
            format!("ID の書式が正しくありません（値: {s:?}）"),
        ))
    }
}

fn authors(field: &str, v: &Value) -> Result<Value> {
    if v.is_null() {
        return Ok(json!([]));
    }
    let items = v
        .as_array()
        .ok_or_else(|| bad(field, "配列である必要があります"))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = item.as_object().ok_or_else(|| {
            bad(
                field,
                "著者は {name, kana, role} のオブジェクトで指定してください",
            )
        })?;
        let text = |key: &str| -> Result<Option<String>> {
            match obj.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.trim().to_owned()).filter(|s| !s.is_empty())),
                Some(other) => Err(bad(
                    field,
                    format!("{key} は文字列である必要があります（値: {other}）"),
                )),
            }
        };
        let name = text("name")?.ok_or_else(|| bad(field, "著者の名前が空です"))?;
        let mut entry = Map::new();
        entry.insert("name".to_owned(), Value::String(name));
        if let Some(kana) = text("kana")? {
            entry.insert("kana".to_owned(), Value::String(kana));
        }
        if let Some(role) = text("role")? {
            entry.insert("role".to_owned(), Value::String(role));
        }
        out.push(Value::Object(entry));
    }
    Ok(Value::Array(out))
}

/// タグ名の配列を、前後の空白を除き、空と重複を取り除いた形にする（順序は保つ）。
pub fn canonical_tags(field: &str, v: &Value) -> Result<Vec<String>> {
    if v.is_null() {
        return Ok(Vec::new());
    }
    let items = v
        .as_array()
        .ok_or_else(|| bad(field, "配列である必要があります"))?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let tag = as_string(field, item)?.trim();
        if !tag.is_empty() && seen.insert(tag.to_owned()) {
            out.push(tag.to_owned());
        }
    }
    Ok(out)
}

/// 書籍の項目の値を検証して、保存する形に正規化する。
///
/// - 空白だけの文字列は null にする（書名は空文字）。前後の空白は除く。
/// - 列挙値、評価（1〜5）、日付（`YYYY`／`YYYY-MM`／`YYYY-MM-DD`）、ISBN-13（13 桁・チェックデジット）、
///   Cコード（4 桁の数字）を検べる。金額とページ数は 0 以上の整数。
/// - 著者は `{name, kana, role}` の配列。タグは文字列の配列（重複は 1 つにする）。
///
/// `created_at`・`deleted`・`purged` は受け付けない（作成・ゴミ箱・完全削除の専用の操作を使う）。
pub fn canonical_book_value(field: &str, value: &Value) -> Result<Value> {
    let v = value;
    match field {
        "title" => {
            if v.is_null() {
                return Ok(json!(""));
            }
            Ok(json!(as_string(field, v)?.trim()))
        }
        "title_kana" | "subtitle" | "volume" | "publisher" | "ndc" | "size_text" | "language"
        | "cover_hash" | "purchase_store" => nullable_text(field, v, false),
        "description" | "review" | "memo" | "purchase_memo" => nullable_text(field, v, true),
        "isbn13" => isbn13(field, v),
        "c_code" => c_code(field, v),
        "series_id" | "location_id" => entity_ref(field, v),
        "volume_sort" => {
            if v.is_null() {
                return Ok(Value::Null);
            }
            match v.as_f64() {
                Some(n) if n.is_finite() => Ok(json!(n)),
                _ => Err(bad(field, format!("数値である必要があります（値: {v}）"))),
            }
        }
        "authors" => authors(field, v),
        "published_on" | "read_started_on" | "read_finished_on" | "purchased_on" => date(field, v),
        "list_price" | "ref_price_incl" | "pages" | "purchase_price" => {
            nullable_int(field, v, 0, i64::from(i32::MAX))
        }
        "rating" => nullable_int(field, v, 1, 5),
        "ownership" => enumeration(field, v, OWNERSHIPS, false),
        "read_status" => enumeration(field, v, READ_STATUSES, false),
        "meta_status" => enumeration(field, v, META_STATUSES, false),
        "wish_priority" => enumeration(field, v, WISH_PRIORITIES, true),
        "purchase_cond" => enumeration(field, v, PURCHASE_CONDS, true),
        "tags" => Ok(json!(canonical_tags(field, v)?)),
        f if RESERVED.contains(&f) => Err(bad(
            field,
            "この項目は直接変更できません（専用の操作を使ってください）",
        )),
        _ => Err(bad(field, "書籍にない項目です")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(field: &str, v: Value) -> Value {
        canonical_book_value(field, &v).unwrap_or_else(|e| panic!("{field}={v}: {e}"))
    }

    fn rejected(field: &str, v: Value) {
        assert!(
            canonical_book_value(field, &v).is_err(),
            "{field}={v} は拒否されるはず"
        );
    }

    #[test]
    fn dates() {
        for good in ["2026", "2026-10", "2026-10-14", "2024-02-29", "0001"] {
            assert!(is_valid_partial_date(good), "{good}");
        }
        for bad in [
            "",
            "26",
            "2026-1",
            "2026-13",
            "2026-00",
            "2026-02-30",
            "2025-02-29",
            "2026/10/14",
            "2026-10-1",
            "２０２６",
            "2026-10-14T00",
            "abcd",
        ] {
            assert!(!is_valid_partial_date(bad), "{bad}");
        }
        assert_eq!(
            ok("published_on", json!("２０２６－１０")),
            json!("2026-10")
        );
        rejected("published_on", json!("2026-02-30"));
        rejected("purchased_on", json!(2026));
    }

    #[test]
    fn enumerations_and_ranges() {
        assert_eq!(ok("ownership", json!("wanted")), json!("wanted"));
        rejected("ownership", json!("lost"));
        rejected("ownership", json!(null));
        assert_eq!(ok("wish_priority", json!(null)), json!(null));
        rejected("wish_priority", json!("urgent"));
        assert_eq!(ok("read_status", json!("paused")), json!("paused"));
        rejected("read_status", json!("done"));
        assert_eq!(ok("purchase_cond", json!("used")), json!("used"));
        rejected("purchase_cond", json!("old"));
        assert_eq!(ok("meta_status", json!("pending")), json!("pending"));
        rejected("meta_status", json!("unknown"));

        assert_eq!(ok("rating", json!(5)), json!(5));
        assert_eq!(ok("rating", json!(null)), json!(null));
        rejected("rating", json!(0));
        rejected("rating", json!(6));
        rejected("rating", json!(3.5));
        rejected("rating", json!("3"));
        assert_eq!(ok("list_price", json!(700)), json!(700));
        rejected("list_price", json!(-1));
        rejected("pages", json!(1.5));
    }

    #[test]
    fn isbn_and_c_code() {
        assert_eq!(
            ok("isbn13", json!("978-4-00-310111-7")),
            json!("9784003101117")
        );
        assert_eq!(
            ok("isbn13", json!("９７８４００３１０１１１７")),
            json!("9784003101117")
        );
        assert_eq!(ok("isbn13", json!("")), json!(null));
        rejected("isbn13", json!("9784003101118"));
        rejected("isbn13", json!("4003101111"));
        rejected("isbn13", json!("978400310111"));
        assert_eq!(ok("c_code", json!("0193")), json!("0193"));
        assert_eq!(ok("c_code", json!("００９３")), json!("0093"));
        rejected("c_code", json!("193"));
        rejected("c_code", json!("01930"));
        rejected("c_code", json!("01a3"));
    }

    #[test]
    fn text_authors_and_tags() {
        assert_eq!(ok("title", json!("  坊っちゃん ")), json!("坊っちゃん"));
        assert_eq!(ok("title", json!(null)), json!(""));
        assert_eq!(ok("publisher", json!("   ")), json!(null));
        assert_eq!(ok("memo", json!("a\nb\n")), json!("a\nb"));
        rejected("title", json!(1));

        assert_eq!(
            ok(
                "authors",
                json!([{"name": " 夏目漱石 ", "kana": "", "role": "著"}, {"name": "x", "kana": "え"}])
            ),
            json!([{"name": "夏目漱石", "role": "著"}, {"kana": "え", "name": "x"}])
        );
        rejected("authors", json!([{"name": ""}]));
        rejected("authors", json!(["夏目漱石"]));
        rejected("authors", json!({"name": "x"}));
        assert_eq!(ok("authors", json!(null)), json!([]));

        assert_eq!(
            ok("tags", json!([" 研究/SiC ", "小説", "", "小説"])),
            json!(["研究/SiC", "小説"])
        );
        rejected("tags", json!([1]));
        rejected("tags", json!("小説"));
    }

    #[test]
    fn references_and_reserved_fields() {
        assert_eq!(
            ok("series_id", json!("0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c01")),
            json!("0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c01")
        );
        rejected("series_id", json!("not-an-id"));
        rejected("deleted", json!(true));
        rejected("purged", json!(true));
        rejected("created_at", json!("2026-10-14T00:00:00.000Z"));
        rejected("no_such_field", json!(1));
        assert!(is_changeable_book_field("title"));
        assert!(!is_changeable_book_field("deleted"));
        assert!(!is_changeable_book_field("field_sources"));
    }
}
