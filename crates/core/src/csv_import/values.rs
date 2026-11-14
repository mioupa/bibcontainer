//! 欄の値を、書籍の項目の値に読み替える。DB には触らない。

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Map, Value, json};

use super::parse::RawRow;
use super::{ImportTarget, IssueReason};
use crate::barcode::{self, ScanCode};
use crate::export::{COND_JA, OWNERSHIP_JA, PRIORITY_JA, READ_STATUS_JA, WORK_NONE_JA};
use crate::ids::is_valid_entity_id;
use crate::library::validate::canonical_book_value;
use crate::library::works::WORK_NONE;
use crate::normalize::nfkc;
use crate::values::{OWNERSHIPS, PURCHASE_CONDS, READ_STATUSES, WISH_PRIORITIES};

/// 1 行を読んだ結果。DB の状態には依らない。
#[derive(Debug, Clone, Default)]
pub(super) struct ParsedRow {
    pub line: u32,
    pub isbn13: Option<String>,
    pub title: Option<String>,
    pub id: Option<String>,
    pub trashed: bool,
    /// 書籍の項目（`series_id`・`location_id`・`created_at` を除く）。検証・正規化済み。
    pub fields: Vec<(&'static str, Value)>,
    /// 保管場所のパス（根から）。
    pub location: Option<Vec<String>>,
    pub series: Option<String>,
    pub created_at: Option<String>,
    /// エラーの行ならその理由（`InvalidIsbn`・`NoTitleOrIsbn`）。
    pub error: Option<IssueReason>,
    /// 読めなかった値（その項目だけ取り込まない）。
    pub warnings: Vec<IssueReason>,
}

/// 1 つの欄の読み替え結果。
enum Conv {
    Empty,
    Val(Value),
    Bad,
}

/// 1 行を読む。
pub(super) fn parse_row(row: &RawRow, mapping: &[(usize, ImportTarget)]) -> ParsedRow {
    let mut out = ParsedRow {
        line: row.line,
        ..ParsedRow::default()
    };
    for &(col, target) in mapping {
        let cell = row.cell(col);
        if cell.is_empty() {
            continue;
        }
        fn bad(out: &mut ParsedRow, target: ImportTarget, cell: &str) {
            out.warnings.push(IssueReason::UnreadableValue {
                target,
                value: cell.to_owned(),
            });
        }
        match target {
            ImportTarget::Isbn13 => match parse_isbn(&cell) {
                Ok(isbn) => out.isbn13 = isbn,
                Err(reason) => out.error = Some(reason),
            },
            ImportTarget::Id => {
                if is_valid_entity_id(&cell) {
                    out.id = Some(cell);
                } else {
                    bad(&mut out, target, &cell);
                }
            }
            ImportTarget::Trashed => match parse_bool(&cell) {
                Some(b) => out.trashed = b,
                None => bad(&mut out, target, &cell),
            },
            ImportTarget::Location => {
                let path = parse_location(&cell);
                if !path.is_empty() {
                    out.location = Some(path);
                }
            }
            ImportTarget::Series => out.series = Some(cell),
            ImportTarget::CreatedAt => match parse_created_at(&cell) {
                Some(c) => out.created_at = Some(c),
                None => bad(&mut out, target, &cell),
            },
            _ => match convert(target, &cell) {
                Conv::Empty => {}
                Conv::Bad => bad(&mut out, target, &cell),
                Conv::Val(v) => {
                    let field = target.key();
                    match canonical_book_value(field, &v) {
                        Ok(v) => {
                            if target == ImportTarget::Title {
                                out.title = v.as_str().map(str::to_owned);
                            }
                            out.fields.push((field, v));
                        }
                        Err(_) => bad(&mut out, target, &cell),
                    }
                }
            },
        }
    }
    // 書名が空の文字列に正規化された場合などは、書名なしとして扱う
    if out.title.as_deref().is_some_and(str::is_empty) {
        out.title = None;
    }
    if out.error.is_none() && out.isbn13.is_none() && out.title.is_none() {
        out.error = Some(IssueReason::NoTitleOrIsbn);
    }
    out
}

fn convert(target: ImportTarget, cell: &str) -> Conv {
    use ImportTarget as T;
    match target {
        T::Authors => {
            let authors = parse_authors(cell);
            if authors.is_empty() {
                Conv::Empty
            } else {
                Conv::Val(Value::Array(authors))
            }
        }
        T::Tags => {
            let tags = parse_tags(cell);
            if tags.is_empty() {
                Conv::Empty
            } else {
                Conv::Val(json!(tags))
            }
        }
        T::Ownership => enumeration(cell, OWNERSHIPS, OWNERSHIP_JA),
        T::WishPriority => enumeration(cell, WISH_PRIORITIES, PRIORITY_JA),
        T::ReadStatus => enumeration(cell, READ_STATUSES, READ_STATUS_JA),
        T::PurchaseCond => enumeration(cell, PURCHASE_CONDS, COND_JA),
        T::PublishedOn | T::ReadStartedOn | T::ReadFinishedOn | T::PurchasedOn => {
            parse_date(cell).map_or(Conv::Bad, |d| Conv::Val(json!(d)))
        }
        T::ListPrice | T::RefPriceIncl | T::Pages | T::PurchasePrice => {
            parse_amount(cell).map_or(Conv::Bad, |n| Conv::Val(json!(n)))
        }
        T::Rating => match parse_integer(&nfkc(cell)) {
            Some(0) => Conv::Empty,
            Some(n) => Conv::Val(json!(n)),
            None => Conv::Bad,
        },
        T::VolumeSort => match nfkc(cell).trim().parse::<f64>() {
            Ok(n) if n.is_finite() => Conv::Val(json!(n)),
            _ => Conv::Bad,
        },
        T::WorkId => {
            let s = cell.trim();
            if s == WORK_NONE_JA || s.eq_ignore_ascii_case(WORK_NONE) {
                Conv::Val(json!(WORK_NONE))
            } else if is_valid_entity_id(s) {
                Conv::Val(json!(s))
            } else {
                Conv::Bad
            }
        }
        _ => Conv::Val(json!(cell)),
    }
}

/// ISBN の欄。空なら `None`。ISBN-13・ISBN-10（ハイフン・全角可）以外はエラー。
pub(super) fn parse_isbn(cell: &str) -> Result<Option<String>, IssueReason> {
    if barcode::normalize(cell).is_empty() {
        return Ok(None);
    }
    match barcode::classify(cell) {
        ScanCode::Isbn13 { isbn13, .. } => Ok(Some(isbn13)),
        _ => Err(IssueReason::InvalidIsbn {
            value: cell.to_owned(),
            excel_number: is_excel_number(cell),
        }),
    }
}

/// Excel が長い数字を指数表記にした形（`9.78478E+12`）か。
fn is_excel_number(s: &str) -> bool {
    let s = s.trim();
    let Some((mantissa, exp)) = s.split_once(['E', 'e']) else {
        return false;
    };
    let Some(exp) = exp.strip_prefix('+') else {
        return false;
    };
    let mut parts = mantissa.splitn(2, '.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next();
    int.len() == 1
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
        && !exp.is_empty()
        && exp.bytes().all(|b| b.is_ascii_digit())
}

/// 著者の欄。`;` `；` `、` 改行で区切り、末尾の `（役割）` を役割にする。
pub(super) fn parse_authors(cell: &str) -> Vec<Value> {
    cell.split([';', '；', '、', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut entry = Map::new();
            let (name, role) = split_role(s);
            entry.insert("name".to_owned(), json!(name));
            if let Some(role) = role {
                entry.insert("role".to_owned(), json!(role));
            }
            Value::Object(entry)
        })
        .collect()
}

fn split_role(s: &str) -> (&str, Option<&str>) {
    let Some(inner) = s.strip_suffix(['）', ')']) else {
        return (s, None);
    };
    let Some(open) = inner.rfind(['（', '(']) else {
        return (s, None);
    };
    let name = inner[..open].trim();
    let open_len = inner[open..].chars().next().map_or(1, char::len_utf8);
    let role = inner[open + open_len..].trim();
    if name.is_empty() {
        return (s, None);
    }
    (name, Some(role).filter(|r| !r.is_empty()))
}

/// タグの欄。`;` `；` `,` `，` `、` 改行で区切る。
pub(super) fn parse_tags(cell: &str) -> Vec<String> {
    cell.split([';', '；', ',', '，', '、', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 保管場所の欄。NFKC をかけ、`>` で区切って空の部分を捨てる。
pub(super) fn parse_location(cell: &str) -> Vec<String> {
    nfkc(cell)
        .split('>')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 列挙値。英語のキー（大文字小文字を問わない）か、書き出しの日本語の表示名。
fn enumeration(cell: &str, keys: &[&str], labels: &[(&str, &str)]) -> Conv {
    let s = nfkc(cell).trim().to_lowercase();
    if let Some(k) = keys.iter().find(|k| **k == s) {
        return Conv::Val(json!(k));
    }
    if let Some((k, _)) = labels.iter().find(|(_, ja)| nfkc(ja).to_lowercase() == s) {
        return Conv::Val(json!(k));
    }
    Conv::Bad
}

/// 真偽。真は `はい` `true` `1` `yes` `y` `○` `✓`、偽は `いいえ` `false` `0` `no`（空は呼ばない）。
fn parse_bool(cell: &str) -> Option<bool> {
    match nfkc(cell).trim().to_lowercase().as_str() {
        "はい" | "true" | "1" | "yes" | "y" | "○" | "◯" | "✓" => Some(true),
        "いいえ" | "false" | "0" | "no" | "n" => Some(false),
        _ => None,
    }
}

/// 日付。`YYYY` `YYYY-M` `YYYY-M-D`（区切りは `-` `/` `.`）、`YYYY年` `YYYY年M月` `YYYY年M月D日`。
/// 後ろの時刻は捨てる。ゼロ埋めして返す（実在の日付かは、あとの検証が確かめる）。
pub(super) fn parse_date(cell: &str) -> Option<String> {
    let s = nfkc(cell);
    let s = s.trim();
    let s = s.split([' ', 'T']).next().unwrap_or("");
    let (y, m, d): (&str, Option<&str>, Option<&str>) = if s.contains('年') {
        let (y, rest) = s.split_once('年')?;
        if rest.is_empty() {
            (y, None, None)
        } else {
            let (m, rest) = rest.split_once('月')?;
            if rest.is_empty() {
                (y, Some(m), None)
            } else {
                (y, Some(m), Some(rest.strip_suffix('日')?))
            }
        }
    } else {
        let mut parts = s.split(['-', '/', '.']);
        let y = parts.next()?;
        let m = parts.next();
        let d = parts.next();
        if parts.next().is_some() {
            return None;
        }
        (y, m, d)
    };
    let num = |t: &str, max_len: usize| -> Option<u32> {
        (!t.is_empty() && t.len() <= max_len && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse().ok())
            .flatten()
    };
    if y.len() != 4 {
        return None;
    }
    let y = num(y, 4)?;
    match (m, d) {
        (None, _) => Some(format!("{y:04}")),
        (Some(m), None) => Some(format!("{y:04}-{:02}", num(m, 2)?)),
        (Some(m), Some(d)) => Some(format!("{y:04}-{:02}-{:02}", num(m, 2)?, num(d, 2)?)),
    }
}

/// 0 以上の整数（`4.0` も可）。
fn parse_integer(s: &str) -> Option<i64> {
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some(f) = frac
        && (f.is_empty() || !f.bytes().all(|b| b == b'0'))
    {
        return None;
    }
    int.parse().ok()
}

/// 金額・ページ数。`円` `¥` `,` 空白と、末尾の `p` `P` `ページ` `頁` を除いて整数にする。
fn parse_amount(cell: &str) -> Option<i64> {
    let s: String = nfkc(cell)
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '円' | '¥' | ','))
        .collect();
    let s = s
        .strip_suffix("ページ")
        .or_else(|| s.strip_suffix('頁'))
        .or_else(|| s.strip_suffix(['p', 'P']))
        .unwrap_or(&s);
    parse_integer(s)
}

/// 登録日時。`env.now_iso()` と同じ形（UTC のミリ秒）にする。
///
/// ゾーンつき（`Z`・オフセット）は UTC に直す。ゾーンなし（`YYYY-MM-DD HH:MM[:SS[.fff]]`、日付だけ）は
/// この PC のローカル時刻として読む（日付だけは 0 時）。区切りに `/` も使える。
pub(super) fn parse_created_at(cell: &str) -> Option<String> {
    let mut s = nfkc(cell).trim().to_owned();
    if s.len() >= 10 && s.as_bytes()[4] == b'/' && s.as_bytes()[7] == b'/' {
        s.replace_range(4..5, "-");
        s.replace_range(7..8, "-");
    }
    if let Some(i) = s.find(' ') {
        s.replace_range(i..=i, "T");
    }
    let utc: DateTime<Utc> = if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        dt.with_timezone(&Utc)
    } else {
        let naive = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M"))
            .ok()
            .or_else(|| {
                NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
            })?;
        Local
            .from_local_datetime(&naive)
            .earliest()?
            .with_timezone(&Utc)
    };
    if !(0..=9999).contains(&utc.format("%Y").to_string().parse::<i32>().ok()?) {
        return None;
    }
    Some(utc.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}
