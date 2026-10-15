//! 書籍の作成・更新・ゴミ箱・完全削除。
//!
//! 変更は「値」と「由来（`Origin`）」の組で受ける。
//!
//! - `Manual`: 値を書き、`field_sources.<項目> = "manual"`、`manual_fields.<項目> = true` を同じ Op に入れる。
//! - `Provider`・`Barcode`・`Import`: その項目が手修正済みなら書かない（FR-META-07、AT-16）。
//!   `UpdateOptions::override_manual` のときだけ書き、`manual_fields.<項目>` を外す。
//!   書いたら `field_sources.<項目>` に取得元の名前を入れる。値が空（null、空文字、空の配列）のときは
//!   何もしない（取得できなかった項目で、既にある値を消さないため）。ただし `Provider` の変更で
//!   `UpdateOptions::clear_stale` のときは、その項目の今の値が同じ取得元のものなら消す（再取得で、取得元が
//!   もう値を返さなくなった項目の古い値を残さない）。
//! - 取得元・手修正の記録は、外部から取得する書誌の項目（`values::TRACKED_FIELDS`）にだけ付ける。
//!   読書記録・購入情報・所有状態・保管場所・タグ・メモなどは、どの由来でも値だけを書く。

use std::collections::BTreeSet;

use rusqlite::{Connection, Transaction};
use serde_json::{Map, Value, json};

use super::validate::canonical_book_value;
use super::{ChangeReport, Env, FieldChange, Origin, UpdateOptions};
use crate::db::write_tx;
use crate::dump::load_row;
use crate::error::{Error, Result};
use crate::ids::new_entity_id;
use crate::schema::{
    Entity, FIELD_SOURCES_PREFIX, Fields, MANUAL_FIELDS_PREFIX, field_sources_key,
    manual_fields_key,
};
use crate::store::record_change;
use crate::values::{SOURCE_MANUAL, TRACKED_FIELDS};
use crate::volume::volume_sort;

/// 検証・正規化を済ませた変更。
struct Prepared {
    field: String,
    value: Value,
    origin: Origin,
}

fn is_tracked(field: &str) -> bool {
    TRACKED_FIELDS.contains(&field)
}

/// 値が空（null、空文字、空の配列）か。取得元が「取得できなかった」ときに渡してくる値で、
/// 既にある値を消さないために使う。
fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// 検証して正規化する。同じ項目が複数あれば、後のものを使う。
fn prepare(changes: &[FieldChange]) -> Result<Vec<Prepared>> {
    let mut out: Vec<Prepared> = Vec::with_capacity(changes.len());
    for c in changes {
        if let Origin::Provider(name) = &c.origin
            && (name.trim().is_empty() || name == SOURCE_MANUAL)
        {
            return Err(Error::validation(
                c.field.clone(),
                format!("取得元の名前が不正です: {name:?}"),
            ));
        }
        let value = canonical_book_value(&c.field, &c.value)?;
        let prepared = Prepared {
            field: c.field.clone(),
            value,
            origin: c.origin.clone(),
        };
        match out.iter_mut().find(|p| p.field == c.field) {
            Some(slot) => *slot = prepared,
            None => out.push(prepared),
        }
    }
    Ok(out)
}

/// `series_id`・`location_id` が、存在する（削除していない）シリーズ・保管場所を指すか確かめる。
fn check_refs(tx: &Transaction<'_>, prepared: &[Prepared]) -> Result<()> {
    for p in prepared {
        let (table, what) = match p.field.as_str() {
            "series_id" => ("series", "シリーズ"),
            "location_id" => ("locations", "保管場所"),
            _ => continue,
        };
        let Some(id) = p.value.as_str() else { continue };
        let exists = tx
            .prepare_cached(&format!(
                "SELECT 1 FROM {table} WHERE id = ?1 AND deleted = 0"
            ))?
            .exists([id])?;
        if !exists {
            return Err(Error::validation(
                p.field.clone(),
                format!("{what}が見つかりません: {id}"),
            ));
        }
    }
    Ok(())
}

/// 巻次（`volume`）の変更から、並べ替え用の数値（`volume_sort`）の変更を作る。由来は巻次と同じ。
/// 数値が読み取れなければ null（前の数値を残さない）。
fn derived_volume_sort(volume: &Prepared) -> Prepared {
    let value = volume
        .value
        .as_str()
        .and_then(volume_sort)
        .map_or(Value::Null, |n| json!(n));
    Prepared {
        field: "volume_sort".to_owned(),
        value,
        origin: volume.origin.clone(),
    }
}

/// 書籍を 1 冊作る（自前のトランザクション）。ID を返す。
pub fn create_book(
    conn: &mut Connection,
    env: &Env<'_>,
    changes: &[FieldChange],
) -> Result<String> {
    let tx = write_tx(conn)?;
    let id = create_book_in(&tx, env, changes)?;
    tx.commit()?;
    Ok(id)
}

/// 書籍を複数冊、1 つのトランザクションで作る（連続登録の「まとめて保存」）。ID を同じ順で返す。
/// 1 冊でも失敗したら全体を取り消す。
pub fn create_books(
    conn: &mut Connection,
    env: &Env<'_>,
    books: &[Vec<FieldChange>],
) -> Result<Vec<String>> {
    let tx = write_tx(conn)?;
    let ids = books
        .iter()
        .map(|changes| create_book_in(&tx, env, changes))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;
    Ok(ids)
}

/// 書籍を作る（呼び出し側のトランザクションの中）。ID を返す。
///
/// 全項目を `set` に入れた Op を 1 つ書く。指定のない項目は既定値（NULL 可の項目は null、
/// 所有状態は `owned`、読書状況は `unread`、取得状態は `ok`）。`created_at` は現在時刻
/// （ISO 8601 UTC ミリ秒）、ID は UUIDv7。読書状況が `read` で読了日が空なら、読了日に今日を入れる。
/// 巻次（`volume`）があり、同じ変更に `volume_sort` がなければ、巻次から並べ替え用の数値を作って
/// 同じ Op に入れる（由来は巻次と同じ。読み取れなければ入れない）。
pub fn create_book_in(
    tx: &Transaction<'_>,
    env: &Env<'_>,
    changes: &[FieldChange],
) -> Result<String> {
    let mut prepared = prepare(changes)?;
    check_refs(tx, &prepared)?;
    if !prepared.iter().any(|p| p.field == "volume_sort")
        && let Some(volume) = prepared.iter().find(|p| p.field == "volume")
    {
        let derived = derived_volume_sort(volume);
        if !derived.value.is_null() {
            prepared.push(derived);
        }
    }

    let mut set: Fields = Entity::Book
        .fields()
        .iter()
        .map(|def| (def.name.to_owned(), def.default_value()))
        .collect();
    set.insert("created_at".to_owned(), Value::String(env.now_iso()?));

    for p in &prepared {
        set.insert(p.field.clone(), p.value.clone());
        if is_tracked(&p.field) && !is_empty_value(&p.value) {
            set.insert(field_sources_key(&p.field), json!(p.origin.source_name()));
            if p.origin == Origin::Manual {
                set.insert(manual_fields_key(&p.field), json!(true));
            }
        }
    }
    if set["read_status"] == json!("read") && set["read_finished_on"].is_null() {
        set.insert("read_finished_on".to_owned(), json!(env.today()));
    }

    let id = new_entity_id();
    record_change(tx, env.clock, Entity::Book, &id, set)?;
    Ok(id)
}

/// 書籍の項目を変更する（自前のトランザクション）。
pub fn update_book(
    conn: &mut Connection,
    env: &Env<'_>,
    id: &str,
    changes: &[FieldChange],
    opts: UpdateOptions,
) -> Result<ChangeReport> {
    let tx = write_tx(conn)?;
    let report = update_book_in(&tx, env, id, changes, opts)?;
    tx.commit()?;
    Ok(report)
}

/// 複数の書籍に同じ変更を、1 つのトランザクションで行う（FR-BOOK-08）。
/// 1 冊でも失敗したら全体を取り消す。結果は、重複を除いた ID の順。
pub fn update_books(
    conn: &mut Connection,
    env: &Env<'_>,
    ids: &[String],
    changes: &[FieldChange],
    opts: UpdateOptions,
) -> Result<Vec<ChangeReport>> {
    let tx = write_tx(conn)?;
    let mut seen = BTreeSet::new();
    let mut reports = Vec::with_capacity(ids.len());
    for id in ids.iter().filter(|id| seen.insert(id.as_str())) {
        reports.push(update_book_in(&tx, env, id, changes, opts)?);
    }
    tx.commit()?;
    Ok(reports)
}

/// 書籍の項目を変更する（呼び出し側のトランザクションの中）。
///
/// 値は検証・正規化する。現在の値と同じ変更は、記録しない（全部同じなら Op を書かない）。
/// 規則:
/// - 読書状況を `read` にしたとき（それまで `read` でなく）、読了日が空なら今日を入れる（FR-READ-02）。
/// - 所有状態を `wanted` から `owned` に移すとき、購入日が空なら今日を入れる（FR-SCAN-18）。
/// - 巻次（`volume`）を書いたとき、同じ変更に `volume_sort` がなければ、巻次から並べ替え用の数値を作って
///   同じ Op に入れる（由来は巻次と同じ）。読み取れなければ `volume_sort` を null にする（古い数値を残さない）。
///   巻次が手修正済みで書かれなかったときは、数値も作らない。
///
/// 完全削除した本は変更できない（`Conflict`）。存在しなければ `NotFound`。
pub fn update_book_in(
    tx: &Transaction<'_>,
    env: &Env<'_>,
    id: &str,
    changes: &[FieldChange],
    opts: UpdateOptions,
) -> Result<ChangeReport> {
    let cur = load_row(tx, Entity::Book, id)?.ok_or_else(|| Error::NotFound {
        what: "書籍",
        id: id.to_owned(),
    })?;
    if cur.get("purged") == Some(&Value::Bool(true)) {
        return Err(Error::Conflict(format!(
            "完全削除した本は変更できません: {id}"
        )));
    }
    let prepared = prepare(changes)?;
    check_refs(tx, &prepared)?;

    let sources = cur
        .get("field_sources")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let manual: BTreeSet<&str> = cur
        .get("manual_fields")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut entries = Fields::new();
    let mut skipped_manual = Vec::new();
    let mut ctx = ApplyCtx {
        cur: &cur,
        sources: &sources,
        manual: &manual,
        opts,
        entries: &mut entries,
        skipped_manual: &mut skipped_manual,
    };
    for p in &prepared {
        ctx.apply(p);
    }
    // 巻次を書いたのに `volume_sort` が明示されていなければ、巻次から作って同じ Op に入れる。
    // 巻次が手修正済みで書かれなかったときは、数値も作らない（組になる項目は一緒に扱う）。
    // 取得元が値を返さなくなった巻次を消したとき（`clear_stale`）は、並べ替え用の数値も消す。
    if !prepared.iter().any(|p| p.field == "volume_sort")
        && let Some(volume) = prepared.iter().find(|p| p.field == "volume")
        && ctx.entries.contains_key("volume")
    {
        if ctx.opts.clear_stale
            && matches!(volume.origin, Origin::Provider(_))
            && ctx.entries.get("volume").is_some_and(Value::is_null)
        {
            ctx.clear_paired("volume_sort");
        } else {
            ctx.apply(&derived_volume_sort(volume));
        }
    }

    apply_date_rules(env, &cur, &mut entries);
    entries.retain(|key, value| !is_noop(&cur, &sources, &manual, key, value));

    let mut written: Vec<String> = entries
        .keys()
        .map(|k| {
            k.strip_prefix(FIELD_SOURCES_PREFIX)
                .or_else(|| k.strip_prefix(MANUAL_FIELDS_PREFIX))
                .unwrap_or(k)
                .to_owned()
        })
        .collect();
    written.sort();
    written.dedup();

    if !entries.is_empty() {
        record_change(tx, env.clock, Entity::Book, id, entries)?;
    }
    Ok(ChangeReport {
        id: id.to_owned(),
        written,
        skipped_manual,
    })
}

/// 1 冊の更新で、項目ごとの変更を `entries`（Op の `set`）に積むときの状態。
struct ApplyCtx<'a> {
    cur: &'a Map<String, Value>,
    sources: &'a Map<String, Value>,
    manual: &'a BTreeSet<&'a str>,
    opts: UpdateOptions,
    entries: &'a mut Fields,
    skipped_manual: &'a mut Vec<String>,
}

impl ApplyCtx<'_> {
    /// 1 項目の変更を、由来の規則（0006 §2）に従って `entries` に積む。書かない場合は何も積まない。
    fn apply(&mut self, p: &Prepared) {
        let field = p.field.as_str();
        let unchanged = same_value(self.cur.get(field), &p.value);
        if !is_tracked(field) {
            self.entries.insert(p.field.clone(), p.value.clone());
            return;
        }
        let is_manual = self.manual.contains(field);
        match &p.origin {
            Origin::Manual => {
                if unchanged {
                    return;
                }
                self.entries.insert(p.field.clone(), p.value.clone());
                self.entries
                    .insert(field_sources_key(field), json!(SOURCE_MANUAL));
                self.entries.insert(manual_fields_key(field), json!(true));
            }
            other => {
                if is_empty_value(&p.value) {
                    self.clear_if_stale(field, other);
                    return;
                }
                if is_manual && !self.opts.override_manual {
                    self.skipped_manual.push(p.field.clone());
                    return;
                }
                if unchanged && !is_manual && self.sources.contains_key(field) {
                    return;
                }
                self.entries.insert(p.field.clone(), p.value.clone());
                self.entries
                    .insert(field_sources_key(field), json!(other.source_name()));
                if is_manual {
                    self.entries.insert(manual_fields_key(field), Value::Null);
                }
            }
        }
    }
}

impl ApplyCtx<'_> {
    /// 取得元が空の値を返したときの規則（`UpdateOptions::clear_stale`）。その項目が手修正でなく、今の値があり、
    /// その取得元の記録が `origin` の取得元と同じなら、値と取得元の記録を消す。そうでなければ何もしない。
    fn clear_if_stale(&mut self, field: &str, origin: &Origin) {
        if !self.opts.clear_stale {
            return;
        }
        let Origin::Provider(name) = origin else {
            return;
        };
        if self.sources.get(field).and_then(Value::as_str) != Some(name.as_str()) {
            return;
        }
        self.clear_paired(field);
    }

    /// 手修正でなく、今の値がある項目の値と取得元の記録を消す（巻次を消したときの `volume_sort` など、
    /// 取得元の記録が一致するかを見ずに、組になる項目を消すときにも使う）。
    fn clear_paired(&mut self, field: &str) {
        if self.manual.contains(field) || self.cur.get(field).is_none_or(is_empty_value) {
            return;
        }
        // 値は列の既定値（NULL 可の項目は null、`authors` は空の配列）。NOT NULL の項目には null を書けない。
        let empty = Entity::Book
            .field(field)
            .map_or(Value::Null, |def| def.default_value());
        self.entries.insert(field.to_owned(), empty);
        self.entries.insert(field_sources_key(field), Value::Null);
    }
}

/// 読書状況・所有状態の変更に伴う日付の自動入力。
fn apply_date_rules(env: &Env<'_>, cur: &Map<String, Value>, entries: &mut Fields) {
    let cur_str = |k: &str| cur.get(k).and_then(Value::as_str);
    let effective_is_empty = |entries: &Fields, k: &str| {
        entries
            .get(k)
            .map_or_else(|| cur.get(k).is_none_or(Value::is_null), Value::is_null)
    };
    let new_str =
        |entries: &Fields, k: &str| entries.get(k).and_then(Value::as_str).map(str::to_owned);

    if new_str(entries, "read_status").as_deref() == Some("read")
        && cur_str("read_status") != Some("read")
        && effective_is_empty(entries, "read_finished_on")
    {
        entries.insert("read_finished_on".to_owned(), json!(env.today()));
    }
    if new_str(entries, "ownership").as_deref() == Some("owned")
        && cur_str("ownership") == Some("wanted")
        && effective_is_empty(entries, "purchased_on")
    {
        entries.insert("purchased_on".to_owned(), json!(env.today()));
    }
}

/// 数値は値で比べる（`2` と `2.0` は同じ）。それ以外は JSON として等しいか。
fn same_value(current: Option<&Value>, new: &Value) -> bool {
    match (current, new) {
        (None, v) | (Some(Value::Null), v) if v.is_null() => true,
        (Some(Value::Number(a)), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Some(a), b) => a == b,
        (None, _) => false,
    }
}

/// この `set` の 1 項目は、記録しても現在の状態から変わらないか。
fn is_noop(
    cur: &Map<String, Value>,
    sources: &Map<String, Value>,
    manual: &BTreeSet<&str>,
    key: &str,
    value: &Value,
) -> bool {
    if let Some(field) = key.strip_prefix(FIELD_SOURCES_PREFIX) {
        return match value {
            Value::Null => !sources.contains_key(field),
            v => sources.get(field) == Some(v),
        };
    }
    if let Some(field) = key.strip_prefix(MANUAL_FIELDS_PREFIX) {
        return match value {
            Value::Null => !manual.contains(field),
            _ => manual.contains(field),
        };
    }
    same_value(cur.get(key), value)
}

/// 書籍をゴミ箱に移す（`deleted = true`）。実際に変えた書籍の ID を返す（既にゴミ箱にある本・完全削除した本は飛ばす）。
/// 存在しない ID があれば `NotFound`（全体を取り消す）。
pub fn trash_books(conn: &mut Connection, env: &Env<'_>, ids: &[String]) -> Result<Vec<String>> {
    set_deleted(conn, env, ids, true)
}

/// 書籍をゴミ箱から戻す（`deleted = false`）。実際に変えた書籍の ID を返す。
/// 完全削除した本は戻せない（`Conflict`）。
pub fn restore_books(conn: &mut Connection, env: &Env<'_>, ids: &[String]) -> Result<Vec<String>> {
    set_deleted(conn, env, ids, false)
}

fn set_deleted(
    conn: &mut Connection,
    env: &Env<'_>,
    ids: &[String],
    deleted: bool,
) -> Result<Vec<String>> {
    let tx = write_tx(conn)?;
    let mut changed = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids.iter().filter(|id| seen.insert(id.as_str())) {
        let cur = load_row(&tx, Entity::Book, id)?.ok_or_else(|| Error::NotFound {
            what: "書籍",
            id: id.clone(),
        })?;
        if cur.get("purged") == Some(&Value::Bool(true)) {
            if deleted {
                continue;
            }
            return Err(Error::Conflict(format!("完全削除した本は戻せません: {id}")));
        }
        if cur.get("deleted") == Some(&Value::Bool(deleted)) {
            continue;
        }
        let set = [("deleted".to_owned(), Value::Bool(deleted))];
        record_change(&tx, env.clock, Entity::Book, id, set)?;
        changed.push(id.clone());
    }
    tx.commit()?;
    Ok(changed)
}

/// 書籍を完全削除する。実際に変えた書籍の ID を返す（完全削除済みの本は飛ばす）。
///
/// 行は残し（他端末に削除を伝える記録のため）、`purged = true`、`deleted = true` にして、
/// それ以外の内容の項目（`created_at` を除く）を既定値（NULL 可の項目は null）に戻す。
/// 取得元・手修正の記録（`field_sources.*`、`manual_fields.*`）も消す。検索用の派生データからも外れる。
pub fn purge_books(conn: &mut Connection, env: &Env<'_>, ids: &[String]) -> Result<Vec<String>> {
    let tx = write_tx(conn)?;
    let mut changed = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids.iter().filter(|id| seen.insert(id.as_str())) {
        let cur = load_row(&tx, Entity::Book, id)?.ok_or_else(|| Error::NotFound {
            what: "書籍",
            id: id.clone(),
        })?;
        if cur.get("purged") == Some(&Value::Bool(true)) {
            continue;
        }
        let mut set: Fields = Entity::Book
            .fields()
            .iter()
            .filter(|def| def.name != "created_at")
            .map(|def| (def.name.to_owned(), def.default_value()))
            .collect();
        set.insert("deleted".to_owned(), Value::Bool(true));
        set.insert("purged".to_owned(), Value::Bool(true));
        if let Some(sources) = cur.get("field_sources").and_then(Value::as_object) {
            for field in sources.keys().filter(|f| Entity::Book.field(f).is_some()) {
                set.insert(field_sources_key(field), Value::Null);
            }
        }
        if let Some(manual) = cur.get("manual_fields").and_then(Value::as_array) {
            for field in manual
                .iter()
                .filter_map(Value::as_str)
                .filter(|f| Entity::Book.field(f).is_some())
            {
                set.insert(manual_fields_key(field), Value::Null);
            }
        }
        record_change(&tx, env.clock, Entity::Book, id, set)?;
        changed.push(id.clone());
    }
    tx.commit()?;
    Ok(changed)
}
