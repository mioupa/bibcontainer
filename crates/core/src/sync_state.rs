//! 同期エンジン（`bibcontainer-sync`）が使う、DB 側の操作。
//!
//! 同期エンジンは、同期フォルダのファイルを読み書きしている間は DB の接続を持たない。そのため、DB への操作は
//! ここにある小さな関数に分けてあり、エンジンは 1 つずつ（接続を借りて、すぐ返して）呼ぶ。
//!
//! - ライブラリ ID と「スナップショットが必要」の印、スナップショットを書いた位置（`local_meta`）。
//! - 取り込み済みの範囲（`applied_segments`）。他端末のセグメントやスナップショットを取り込むたびに記録する。
//! - outbox（自分の Op の書き出し状況）。
//! - スナップショットの元になる状態の書き出しと、その適用。
//!
//! 設計は `docs/decisions/0013-phase2-sync.md` と `0014-sync-engine.md` を参照。

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

use crate::dump::{col_specs, row_object, select_sql};
use crate::error::{Error, Result};
use crate::hlc::Hlc;
use crate::meta;
use crate::op::{OP_VERSION, Op};
use crate::schema::{
    Entity, FIELD_SOURCES_PREFIX, Fields, MANUAL_FIELDS_PREFIX, OpKey, classify_key,
};
use crate::store::{ApplyOutcome, apply_op};

/// 参加しているライブラリの ID。同期フォルダを設定すると入る。
pub const KEY_LIBRARY_ID: &str = "library_id";
/// 「手元の全状態をスナップショットとして書き出す必要がある」印（`"1"`）。
/// バックアップの復元・ライブラリへの統合・ライブラリの新規作成で立て、同期がスナップショットを書いたら下ろす。
pub const KEY_NEEDS_SNAPSHOT: &str = "needs_snapshot";
/// 他端末のスナップショットのうち、中身を確かめた最新のファイル名（端末 ID → ファイル名）の JSON。
pub const KEY_SNAPSHOT_SEEN: &str = "snapshot_seen";
/// 自分が最後にスナップショットを書いたときの、自分の seq。
pub const KEY_LAST_SNAPSHOT_SEQ: &str = "last_snapshot_seq";
/// 自分が最後にスナップショットを書いた時刻（UNIX ミリ秒）。
pub const KEY_LAST_SNAPSHOT_MS: &str = "last_snapshot_ms";

// ---------------------------------------------------------------------------
// ライブラリ ID と印
// ---------------------------------------------------------------------------

/// 参加しているライブラリの ID。同期フォルダを設定していなければ `None`。
pub fn library_id(conn: &Connection) -> Result<Option<String>> {
    Ok(meta::get(conn, KEY_LIBRARY_ID)?.filter(|s| !s.is_empty()))
}

/// ライブラリ ID を書く（取り込み済みの範囲などは変えない）。ライブラリを切り替えるときは `join_library`。
pub fn set_library_id(conn: &Connection, id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(Error::InvalidOp("ライブラリ ID が空".to_owned()));
    }
    meta::set(conn, KEY_LIBRARY_ID, id)
}

/// ライブラリ ID を消す（取り込み済みの範囲などは変えない）。同期を解除するときは `leave_library`。
pub fn clear_library_id(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM local_meta WHERE key = ?1", [KEY_LIBRARY_ID])?;
    Ok(())
}

/// 「スナップショットが必要」の印が立っているか。
pub fn needs_snapshot(conn: &Connection) -> Result<bool> {
    Ok(meta::get(conn, KEY_NEEDS_SNAPSHOT)?.as_deref() == Some("1"))
}

/// 「スナップショットが必要」の印を立てる・下ろす。
pub fn set_needs_snapshot(conn: &Connection, needed: bool) -> Result<()> {
    if needed {
        meta::set(conn, KEY_NEEDS_SNAPSHOT, "1")
    } else {
        conn.execute(
            "DELETE FROM local_meta WHERE key = ?1",
            [KEY_NEEDS_SNAPSHOT],
        )?;
        Ok(())
    }
}

/// ライブラリに参加する。ライブラリ ID と印を 1 つのトランザクションで書く。
///
/// それまで別のライブラリ（または無所属）だったときは、取り込み済みの範囲・確かめたスナップショット・
/// スナップショットを書いた位置を消す（前のライブラリの記録を引き継がない）。同じライブラリへの再設定なら残す。
pub fn join_library(tx: &Transaction<'_>, library_id: &str, needs_snapshot: bool) -> Result<()> {
    if meta::get(tx, KEY_LIBRARY_ID)?.as_deref() != Some(library_id) {
        clear_membership_state(tx)?;
    }
    set_library_id(tx, library_id)?;
    set_needs_snapshot(tx, needs_snapshot)
}

/// 同期を解除する。ライブラリ ID・取り込み済みの範囲・印などを消す。手元のデータは変えない。
pub fn leave_library(tx: &Transaction<'_>) -> Result<()> {
    clear_membership_state(tx)?;
    clear_library_id(tx)?;
    set_needs_snapshot(tx, false)
}

fn clear_membership_state(tx: &Transaction<'_>) -> Result<()> {
    clear_applied(tx)?;
    tx.execute(
        "DELETE FROM local_meta WHERE key IN (?1, ?2, ?3)",
        params![
            KEY_SNAPSHOT_SEEN,
            KEY_LAST_SNAPSHOT_SEQ,
            KEY_LAST_SNAPSHOT_MS
        ],
    )?;
    Ok(())
}

/// 他端末のスナップショットのうち、中身を確かめた最新のファイル名（端末 ID → ファイル名）。
pub fn snapshot_seen(conn: &Connection) -> Result<BTreeMap<String, String>> {
    match meta::get(conn, KEY_SNAPSHOT_SEEN)? {
        None => Ok(BTreeMap::new()),
        Some(s) => Ok(serde_json::from_str(&s).unwrap_or_else(|_| {
            tracing::warn!("snapshot_seen が壊れているので、空として扱う");
            BTreeMap::new()
        })),
    }
}

/// 端末のスナップショットを確かめたことを覚える（古い名前には戻さない）。
pub fn mark_snapshot_seen(conn: &Connection, dev: &str, name: &str) -> Result<()> {
    let mut seen = snapshot_seen(conn)?;
    if seen.get(dev).is_some_and(|old| old.as_str() >= name) {
        return Ok(());
    }
    seen.insert(dev.to_owned(), name.to_owned());
    meta::set(conn, KEY_SNAPSHOT_SEEN, &serde_json::to_string(&seen)?)
}

/// 自分が最後にスナップショットを書いた位置（seq、時刻 ms）。まだ書いていなければ `None`。
pub fn last_snapshot(conn: &Connection) -> Result<Option<(i64, i64)>> {
    let seq = meta::get(conn, KEY_LAST_SNAPSHOT_SEQ)?.and_then(|s| s.parse::<i64>().ok());
    let ms = meta::get(conn, KEY_LAST_SNAPSHOT_MS)?.and_then(|s| s.parse::<i64>().ok());
    Ok(seq.zip(ms))
}

/// 自分がスナップショットを書いた位置を覚える。
pub fn set_last_snapshot(conn: &Connection, seq: i64, ms: i64) -> Result<()> {
    meta::set(conn, KEY_LAST_SNAPSHOT_SEQ, &seq.to_string())?;
    meta::set(conn, KEY_LAST_SNAPSHOT_MS, &ms.to_string())
}

// ---------------------------------------------------------------------------
// 取り込み済みの範囲
// ---------------------------------------------------------------------------

/// 端末ごとの、取り込み済みの seq の範囲（閉区間）。重なりと隣り合う範囲はまとめてある。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedRanges {
    by_device: BTreeMap<String, Vec<(i64, i64)>>,
}

impl AppliedRanges {
    /// 範囲を足す（メモリ上だけ。重なりはまとめる）。
    pub fn insert(&mut self, dev: &str, from: i64, to: i64) {
        if from > to {
            return;
        }
        let ranges = self.by_device.entry(dev.to_owned()).or_default();
        ranges.push((from, to));
        ranges.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::with_capacity(ranges.len());
        for &(f, t) in ranges.iter() {
            match merged.last_mut() {
                Some(last) if f <= last.1.saturating_add(1) => last.1 = last.1.max(t),
                _ => merged.push((f, t)),
            }
        }
        *ranges = merged;
    }

    /// `from..=to` の全体が、取り込み済みの範囲で覆われているか。
    pub fn covers(&self, dev: &str, from: i64, to: i64) -> bool {
        if from > to {
            return true;
        }
        self.by_device
            .get(dev)
            .is_some_and(|ranges| ranges.iter().any(|&(f, t)| f <= from && to <= t))
    }

    /// 1 から切れ目なく取り込んだ seq（何もなければ 0）。
    pub fn contiguous(&self, dev: &str) -> i64 {
        self.by_device
            .get(dev)
            .and_then(|ranges| ranges.first())
            .filter(|&&(f, _)| f <= 1)
            .map_or(0, |&(_, t)| t)
    }

    /// 1 つでも範囲を持つ端末の ID。
    pub fn devices(&self) -> impl Iterator<Item = &str> {
        self.by_device.keys().map(String::as_str)
    }

    /// 何も記録がないか。
    pub fn is_empty(&self) -> bool {
        self.by_device.is_empty()
    }

    /// 端末の範囲の一覧（昇順）。
    pub fn ranges(&self, dev: &str) -> &[(i64, i64)] {
        self.by_device.get(dev).map_or(&[], Vec::as_slice)
    }
}

/// 取り込み済みの範囲を記録する。同じ `seq_from` が既にあれば、`seq_to` の大きい方を残す
/// （自分のセグメントを統合した大きなファイルを、あとから読んだ場合）。
pub fn record_applied(conn: &Connection, dev: &str, from: i64, to: i64) -> Result<()> {
    if from < 1 || to < from {
        return Err(Error::InvalidOp(format!("seq の範囲が不正: {from}-{to}")));
    }
    conn.execute(
        "INSERT INTO applied_segments(device_id, seq_from, seq_to) VALUES (?1, ?2, ?3)
         ON CONFLICT(device_id, seq_from) DO UPDATE SET seq_to = MAX(seq_to, excluded.seq_to)",
        params![dev, from, to],
    )?;
    Ok(())
}

/// 取り込み済みの範囲をすべて読む。
pub fn applied_ranges(conn: &Connection) -> Result<AppliedRanges> {
    let mut stmt = conn.prepare(
        "SELECT device_id, seq_from, seq_to FROM applied_segments ORDER BY device_id, seq_from",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut out = AppliedRanges::default();
    for row in rows {
        let (dev, from, to) = row?;
        out.insert(&dev, from, to);
    }
    Ok(out)
}

/// `from..=to` が取り込み済みの範囲で全部覆われているか。
pub fn is_range_applied(conn: &Connection, dev: &str, from: i64, to: i64) -> Result<bool> {
    Ok(applied_ranges(conn)?.covers(dev, from, to))
}

/// 端末の Op を、seq 1 から切れ目なく取り込んだ最後の seq（何もなければ 0）。
pub fn contiguous_applied(conn: &Connection, dev: &str) -> Result<i64> {
    Ok(applied_ranges(conn)?.contiguous(dev))
}

/// 取り込み済みの範囲をすべて消す。
pub fn clear_applied(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM applied_segments", [])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// outbox
// ---------------------------------------------------------------------------

/// 最後に採番した自分の seq（まだなければ 0）。
pub fn current_seq(conn: &Connection) -> Result<i64> {
    meta::load_op_seq(conn)
}

/// 未書き出しの Op を、`after_seq` より大きい seq から seq 順に最大 `limit` 件返す。
pub fn pending_ops_after(conn: &Connection, after_seq: i64, limit: usize) -> Result<Vec<Op>> {
    let mut stmt = conn.prepare_cached(
        "SELECT op_json FROM outbox WHERE flushed = 0 AND seq > ?1 ORDER BY seq LIMIT ?2",
    )?;
    let lines = stmt
        .query_map(
            params![after_seq, i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| r.get::<_, String>(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    lines.iter().map(|l| Op::from_json_line(l)).collect()
}

/// 未書き出しの Op の数。
pub fn pending_count(conn: &Connection) -> Result<u64> {
    let n: i64 = conn.query_row("SELECT count(*) FROM outbox WHERE flushed = 0", [], |r| {
        r.get(0)
    })?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// `from..=to` の Op を書き出し済みにする。変えた件数を返す。
pub fn mark_flushed(conn: &Connection, from: i64, to: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE outbox SET flushed = 1 WHERE seq BETWEEN ?1 AND ?2 AND flushed = 0",
        params![from, to],
    )?)
}

/// `seq` までの Op をすべて書き出し済みにする（スナップショットに含めたとき）。変えた件数を返す。
pub fn mark_flushed_through(conn: &Connection, seq: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE outbox SET flushed = 1 WHERE seq <= ?1 AND flushed = 0",
        [seq],
    )?)
}

/// outbox の Op の HLC のミリ秒。行がなければ `None`（自分のセグメントの古さの判定に使う）。
pub fn outbox_hlc_ms(conn: &Connection, seq: i64) -> Result<Option<i64>> {
    let hlc: Option<String> = conn
        .query_row("SELECT hlc FROM outbox WHERE seq = ?1", [seq], |r| r.get(0))
        .optional()?;
    Ok(hlc.and_then(|h| Hlc::parse(&h).ok()).map(|h| h.ms()))
}

/// 復元のときに `outbox_restored` へ移した Op を捨てる。スナップショットに反映済みなので不要になる。
/// 捨てた件数を返す。
pub fn clear_outbox_restored(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM outbox_restored", [])?)
}

// ---------------------------------------------------------------------------
// 状態の書き出しと適用（スナップショットの元）
// ---------------------------------------------------------------------------

/// 1 つのエンティティの状態。`fields` は `field_clocks` に時計がある項目すべて（項目名 → (値, HLC)）。
///
/// 項目名は Op 上の名前。書籍の `field_sources.<項目名>`・`manual_fields.<項目名>` や、
/// `extra_json` にある未知の項目も、ドット付きのキーや元の名前のまま入る。
#[derive(Debug, Clone, PartialEq)]
pub struct EntityState {
    /// エンティティの種類。
    pub ent: Entity,
    /// ID。
    pub id: String,
    /// 項目名 → (値, その項目の HLC)。時計のない行（Op の項目がすべて不正だった行）は空。
    pub fields: BTreeMap<String, (Value, String)>,
}

/// スナップショットの元になる、一貫した状態。
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotBasis {
    /// 自分の端末 ID。
    pub device_id: String,
    /// このスナップショットの HLC（採番し直した値。ファイル名になる）。
    pub hlc: String,
    /// 含まれる自分の Op の seq（現在の seq）。
    pub seq: i64,
    /// 端末ごとの「1 から切れ目なく反映した seq」。自分は `seq`。0 の端末は含めない（自分は常に含める）。
    pub covers: BTreeMap<String, i64>,
    /// 全エンティティの状態。
    pub entities: Vec<EntityState>,
    /// 書籍が参照している書影のハッシュ（完全削除した本のものも含む。重複なし）。同期フォルダへ写す対象。
    pub cover_hashes: Vec<String>,
}

/// 全エンティティの状態を、項目ごとの値と HLC つきで書き出す。
///
/// 値は、項目の型に従った JSON（Bool は真偽値、Json は配列かオブジェクト、NULL は null）。
/// 書籍の取得元・手修正の記録は、項目ごとのキーの値（取得元の文字列／`true`／null）に分けて出す。
/// 行は ID 順。呼び出し側が読み取りトランザクションを張っていれば、一貫した状態になる。
pub fn export_state(conn: &Connection) -> Result<Vec<EntityState>> {
    let mut out = Vec::new();
    for ent in Entity::ALL {
        export_entity_state(conn, ent, &mut out)?;
    }
    Ok(out)
}

fn export_entity_state(conn: &Connection, ent: Entity, out: &mut Vec<EntityState>) -> Result<()> {
    let specs = col_specs(ent);
    let sql = format!("{} ORDER BY id", select_sql(ent, &specs));
    let mut rows_stmt = conn.prepare(&sql)?;
    let mut clocks_stmt = conn.prepare(
        "SELECT entity_id, field, hlc FROM field_clocks WHERE entity = ?1
         ORDER BY entity_id, field",
    )?;
    let mut rows = rows_stmt.query([])?;
    let mut clocks = clocks_stmt.query([ent.as_str()])?;
    // 行は id 順、時計は (entity_id, field) 順なので、並べて読み進める。
    let mut pending_clock: Option<(String, String, String)> = None;
    let mut clocks_done = false;
    let next_clock = |clocks: &mut rusqlite::Rows<'_>| -> Result<Option<(String, String, String)>> {
        Ok(match clocks.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?)),
            None => None,
        })
    };

    while let Some(row) = rows.next()? {
        let obj = row_object(row, &specs)?;
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut fields: BTreeMap<String, (Value, String)> = BTreeMap::new();
        loop {
            if pending_clock.is_none() && !clocks_done {
                pending_clock = next_clock(&mut clocks)?;
                clocks_done = pending_clock.is_none();
            }
            let Some((clock_id, _, _)) = &pending_clock else {
                break;
            };
            match clock_id.as_str().cmp(id.as_str()) {
                std::cmp::Ordering::Less => {
                    // 行のない時計（通常は起こらない）は捨てる。
                    tracing::warn!(ent = ent.as_str(), id = %clock_id, "行のない項目時計があった");
                    pending_clock = None;
                }
                std::cmp::Ordering::Equal => {
                    let (_, field, hlc) = pending_clock.take().expect("直前に Some を確かめた");
                    let value = field_value(ent, &obj, &field);
                    fields.insert(field, (value, hlc));
                }
                std::cmp::Ordering::Greater => break,
            }
        }
        out.push(EntityState { ent, id, fields });
    }
    Ok(())
}

/// 行（`row_object` の形）から、Op 上の項目名の値を取り出す。
fn field_value(ent: Entity, obj: &serde_json::Map<String, Value>, field: &str) -> Value {
    match classify_key(ent, field) {
        OpKey::Column(def) => obj.get(def.name).cloned().unwrap_or(Value::Null),
        OpKey::Source(_) => {
            let name = field.strip_prefix(FIELD_SOURCES_PREFIX).unwrap_or(field);
            obj.get("field_sources")
                .and_then(|m| m.get(name))
                .cloned()
                .unwrap_or(Value::Null)
        }
        OpKey::Manual(_) => {
            let name = field.strip_prefix(MANUAL_FIELDS_PREFIX).unwrap_or(field);
            let on = obj
                .get("manual_fields")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(name)));
            if on { Value::Bool(true) } else { Value::Null }
        }
        OpKey::BadProvenance | OpKey::Unknown => obj
            .get("extra")
            .and_then(|m| m.get(field))
            .cloned()
            .unwrap_or(Value::Null),
    }
}

/// 1 つのエンティティの状態を適用する。同じ HLC の項目をまとめた Op を作って `apply_op` で適用する
/// （項目ごとの LWW なので、どんな状態に重ねても正しい）。項目が空の行は、行だけ作る。
///
/// `observe_hlc` は呼ばない。呼び出し側が、状態に含まれる HLC の最大値を観測すること。
pub fn apply_entity_state(tx: &Transaction<'_>, state: &EntityState) -> Result<ApplyOutcome> {
    let mut groups: BTreeMap<&str, Fields> = BTreeMap::new();
    for (name, (value, hlc)) in &state.fields {
        groups
            .entry(hlc.as_str())
            .or_default()
            .insert(name.clone(), value.clone());
    }
    let mut total = ApplyOutcome::default();
    if groups.is_empty() {
        // 行だけ作る。HLC は使われない。
        let hlc = Hlc::new(0, 0, "d-snapshot")?.to_string();
        let op = state_op(state, hlc, Fields::new());
        apply_op(tx, &op)?;
        return Ok(total);
    }
    for (hlc, set) in groups {
        let op = state_op(state, hlc.to_owned(), set);
        let outcome = apply_op(tx, &op)?;
        total.applied += outcome.applied;
        total.stale += outcome.stale;
        total.invalid += outcome.invalid;
    }
    Ok(total)
}

fn state_op(state: &EntityState, hlc: String, set: Fields) -> Op {
    // dev と seq は適用に使われない。HLC の端末 ID を入れておく（診断のため）。
    let dev = Hlc::parse(&hlc)
        .map(|h| h.device().to_owned())
        .unwrap_or_default();
    Op {
        v: OP_VERSION,
        dev,
        seq: 0,
        hlc,
        ent: state.ent,
        id: state.id.clone(),
        set,
    }
}

/// スナップショットの元になる状態を、1 つのトランザクションで読む。
///
/// 1. スナップショットの HLC を採番して保存する（同じ端末のスナップショットの名前が重ならず、増えていく）。
/// 2. 自分の seq、端末ごとの「1 から切れ目なく反映した seq」、全エンティティの状態、書影のハッシュを読む。
///
/// 書き込みトランザクション（即時）を使う。ファイルを書く前に呼び、書いている間は接続を返しておくこと。
pub fn export_snapshot_basis(conn: &mut Connection, now_ms: i64) -> Result<SnapshotBasis> {
    let tx = crate::db::write_tx(conn)?;
    let dev = meta::device_id(&tx)?;
    let state = meta::load_hlc_state(&tx)?.tick(now_ms);
    meta::save_hlc_state(&tx, state)?;
    let hlc = Hlc::from_state(state, dev.clone())?.to_string();
    let seq = meta::load_op_seq(&tx)?;

    let applied = applied_ranges(&tx)?;
    let mut covers = BTreeMap::new();
    for other in applied.devices() {
        let upto = applied.contiguous(other);
        if other != dev && upto > 0 {
            covers.insert(other.to_owned(), upto);
        }
    }
    covers.insert(dev.clone(), seq);

    let entities = export_state(&tx)?;
    let cover_hashes = all_cover_hashes(&tx)?;
    tx.commit()?;
    Ok(SnapshotBasis {
        device_id: dev,
        hlc,
        seq,
        covers,
        entities,
        cover_hashes,
    })
}

// ---------------------------------------------------------------------------
// 手元のデータの有無と書影
// ---------------------------------------------------------------------------

/// 手元に同期対象のエンティティが 1 つでもあるか（ゴミ箱の本・完全削除した行も含む）。
pub fn has_local_data(conn: &Connection) -> Result<bool> {
    for ent in Entity::ALL {
        let exists: bool = conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {})", ent.table()),
            [],
            |r| r.get(0),
        )?;
        if exists {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 手元のデータの件数（同期の設定で、置き換えてよいかを利用者に確かめるときの表示用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DataSummary {
    /// 完全削除していない本（ゴミ箱の本を含む）。
    pub books: u64,
    /// ゴミ箱の本。
    pub trashed_books: u64,
    /// シリーズ。
    pub series: u64,
    /// 保管場所。
    pub locations: u64,
    /// タグ。
    pub tags: u64,
    /// スマート本棚。
    pub shelves: u64,
    /// 未書き出しの Op。
    pub pending_ops: u64,
}

/// 手元のデータの件数を数える（削除済みのシリーズ・保管場所・タグ・本棚は数えない）。
pub fn data_summary(conn: &Connection) -> Result<DataSummary> {
    let count = |sql: &str| -> Result<u64> {
        let n: i64 = conn.query_row(sql, [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    };
    Ok(DataSummary {
        books: count("SELECT count(*) FROM books WHERE purged = 0")?,
        trashed_books: count("SELECT count(*) FROM books WHERE purged = 0 AND deleted = 1")?,
        series: count("SELECT count(*) FROM series WHERE deleted = 0")?,
        locations: count("SELECT count(*) FROM locations WHERE deleted = 0")?,
        tags: count("SELECT count(*) FROM tags WHERE deleted = 0")?,
        shelves: count("SELECT count(*) FROM shelves WHERE deleted = 0")?,
        pending_ops: pending_count(conn)?,
    })
}

/// 書籍が参照している書影のハッシュ（`cover_hash`）。完全削除した本の分も含めて、重複と空は除く。
///
/// 同期フォルダへ書影を写すときに使う。完全削除（`purged`）は項目ごとの LWW なので、別の端末の新しい変更で
/// 取り消されることがある。そのとき書影が同期フォルダにないと、届かないままになってしまう。
pub fn all_cover_hashes(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT cover_hash FROM books
         WHERE cover_hash IS NOT NULL AND cover_hash != '' ORDER BY cover_hash",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 書籍が参照している書影のハッシュ（`cover_hash`）。完全削除していない本の分で、重複と空は除く。
/// 届いていない書影を数える・書影を整理するときに使う。
pub fn referenced_cover_hashes(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT cover_hash FROM books
         WHERE purged = 0 AND cover_hash IS NOT NULL AND cover_hash != ''
         ORDER BY cover_hash",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applied_ranges_merge_and_cover() {
        let mut r = AppliedRanges::default();
        r.insert("d-a", 1, 10);
        r.insert("d-a", 21, 30);
        assert_eq!(r.contiguous("d-a"), 10);
        assert!(r.covers("d-a", 1, 10));
        assert!(r.covers("d-a", 3, 5));
        assert!(!r.covers("d-a", 5, 25));
        assert!(!r.covers("d-a", 11, 20));
        r.insert("d-a", 11, 20);
        assert_eq!(r.ranges("d-a"), &[(1, 30)]);
        assert_eq!(r.contiguous("d-a"), 30);
        assert!(r.covers("d-a", 5, 25));
        assert_eq!(r.contiguous("d-none"), 0);
        // 1 から始まっていない範囲は「切れ目なく」にならない
        r.insert("d-b", 2, 9);
        assert_eq!(r.contiguous("d-b"), 0);
    }
}
