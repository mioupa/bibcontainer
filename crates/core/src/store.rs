//! Op の適用と、ローカルの変更の記録。

use std::collections::BTreeMap;

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::hlc::{Clock, Hlc, ms_to_iso8601};
use crate::meta;
use crate::op::{OP_VERSION, Op};
use crate::schema::{Entity, Fields, validate_local_change};

/// `apply_op` の結果（項目ごとの件数）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// 値と時計を書き込んだ項目の数。
    pub applied: usize,
    /// 既存の時計が同じか新しいために、何もしなかった項目の数。
    pub stale: usize,
    /// 値の型が項目の型に合わないために、無視した項目の数。
    pub invalid: usize,
}

impl ApplyOutcome {
    /// 1 項目でも書き換わったか。
    pub fn changed(&self) -> bool {
        self.applied > 0
    }
}

/// Op を適用する。ローカルの変更とリモートから届いた変更で共通の関数。
///
/// 1. 行がなければ `INSERT OR IGNORE` で id だけの行を作る（NOT NULL 列には既定値がある）。
/// 2. `set` の各項目について、`field_clocks` の HLC が無いか Op の HLC の方が新しい場合だけ、
///    値を書き込んで時計を更新する。同じか古ければ何もしない。
/// 3. 知らない項目名は、`extra_json` の該当キーに入れる（時計の扱いは通常の項目と同じ）。
/// 4. NOT NULL 列への null は、列の既定値を書く。
///
/// 冪等で、適用の順序に依存しない。削除も `deleted` 項目の LWW なので、ほかの項目の更新で
/// 取り消されることはない。項目の値の型が合わない場合は、その項目だけを無視する
/// （時計も進めない）ので、どの順序で適用しても結果が変わらない。
///
/// `updated_at`（端末ローカルの派生値）は、項目が 1 つでも書き換わったときに
/// `max(既存値, Op の HLC のミリ秒)` にする。
///
/// 途中でエラーになると一部だけ書き込まれていることがあるので、呼び出し側はトランザクションを
/// ロールバックすること。
pub fn apply_op(tx: &Transaction<'_>, op: &Op) -> Result<ApplyOutcome> {
    let hlc = Hlc::parse(&op.hlc)?;
    if op.id.is_empty() {
        return Err(Error::InvalidOp("id が空".to_owned()));
    }
    let table = op.ent.table();
    let ent_name = op.ent.as_str();

    tx.execute(
        &format!("INSERT OR IGNORE INTO {table}(id) VALUES (?1)"),
        [&op.id],
    )?;

    let mut outcome = ApplyOutcome::default();
    let mut assignments: Vec<(&'static str, SqlValue)> = Vec::new();
    let mut extras: Vec<(&str, &Value)> = Vec::new();

    for (name, value) in &op.set {
        let column_value = match op.ent.field(name) {
            Some(def) => match def.to_sql(value) {
                Some(v) => Some((def.column, v)),
                None => {
                    tracing::warn!(
                        ent = ent_name, id = %op.id, field = %name, hlc = %op.hlc,
                        "値の型が項目の型に合わないので無視した"
                    );
                    outcome.invalid += 1;
                    continue;
                }
            },
            None => None,
        };
        if !advance_clock(tx, ent_name, &op.id, name, &op.hlc)? {
            outcome.stale += 1;
            continue;
        }
        outcome.applied += 1;
        match column_value {
            Some(assignment) => assignments.push(assignment),
            None => extras.push((name, value)),
        }
    }

    if outcome.applied > 0 {
        let mut params: Vec<SqlValue> = Vec::new();
        let mut sets: Vec<String> = Vec::new();
        for (column, value) in assignments {
            params.push(value);
            sets.push(format!("\"{column}\" = ?{}", params.len()));
        }
        if !extras.is_empty() {
            let merged = merge_extras(tx, table, &op.id, &extras)?;
            params.push(SqlValue::Text(merged));
            sets.push(format!("extra_json = ?{}", params.len()));
        }
        params.push(SqlValue::Text(ms_to_iso8601(hlc.ms())?));
        sets.push(format!("updated_at = MAX(updated_at, ?{})", params.len()));
        params.push(SqlValue::Text(op.id.clone()));
        let sql = format!(
            "UPDATE {table} SET {} WHERE id = ?{}",
            sets.join(", "),
            params.len()
        );
        tx.execute(&sql, params_from_iter(params))?;
    }
    Ok(outcome)
}

/// 項目の時計を `hlc` に進める。既存の時計の方が新しい（または同じ）なら何もせず `false`。
fn advance_clock(
    tx: &Transaction<'_>,
    ent: &str,
    id: &str,
    field: &str,
    hlc: &str,
) -> Result<bool> {
    let changed = tx
        .prepare_cached(
            "INSERT INTO field_clocks(entity, entity_id, field, hlc) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(entity, entity_id, field) DO UPDATE SET hlc = excluded.hlc
             WHERE excluded.hlc > field_clocks.hlc",
        )?
        .execute(params![ent, id, field, hlc])?;
    Ok(changed > 0)
}

/// 既存の `extra_json` に、未知の項目の値を入れた JSON 文字列を返す。
///
/// キーを辞書順にそろえて保存するので、適用の順序によらず同じ文字列になる。
fn merge_extras(
    tx: &Transaction<'_>,
    table: &str,
    id: &str,
    extras: &[(&str, &Value)],
) -> Result<String> {
    let current: String = tx.query_row(
        &format!("SELECT extra_json FROM {table} WHERE id = ?1"),
        [id],
        |r| r.get(0),
    )?;
    let mut map: BTreeMap<String, Value> = serde_json::from_str(&current)?;
    for (key, value) in extras {
        map.insert((*key).to_owned(), (*value).clone());
    }
    Ok(serde_json::to_string(&map)?)
}

/// 他端末の HLC を観測して、`local_meta` の前回値を `max(自分, 受け取った値)` に進める。
///
/// 他端末の Op を取り込むときに呼ぶ。これを呼んでおけば、その後にローカルで採番する HLC は
/// 取り込んだどの Op よりも新しくなる。
pub fn observe_hlc(tx: &Transaction<'_>, remote: &Hlc) -> Result<()> {
    let current = meta::load_hlc_state(tx)?;
    let next = current.observe(remote);
    if next != current {
        meta::save_hlc_state(tx, next)?;
    }
    Ok(())
}

/// ローカルの変更を記録する。呼び出し側がトランザクションを持つので、複数冊の「まとめて保存」を
/// 1 つのトランザクションにできる。
///
/// 1. 未知の項目・型の不一致・NOT NULL 列への null・空の `set` はエラー（何も変更しない）。
/// 2. `local_meta` から端末 ID を読み、次の HLC と seq を採番して保存する。
/// 3. Op を組み立てて `apply_op` で適用し、`outbox` に追加する。
///
/// DB の更新と outbox への追記は、渡されたトランザクションの中で行う（要件 0.5、8.3）。
/// 途中で失敗した場合は、この呼び出しの変更だけを取り消す（セーブポイント）。
pub fn record_change(
    tx: &Transaction<'_>,
    clock: &dyn Clock,
    ent: Entity,
    id: &str,
    set: impl IntoIterator<Item = (String, Value)>,
) -> Result<Op> {
    let set: Fields = set.into_iter().collect();
    validate_local_change(ent, id, &set)?;

    tx.execute_batch("SAVEPOINT record_change")?;
    match record_validated(tx, clock, ent, id, set) {
        Ok(op) => {
            tx.execute_batch("RELEASE record_change")?;
            Ok(op)
        }
        Err(e) => {
            // 取り消しに失敗しても、元のエラーを返す。
            let _ = tx.execute_batch("ROLLBACK TO record_change; RELEASE record_change");
            Err(e)
        }
    }
}

fn record_validated(
    tx: &Transaction<'_>,
    clock: &dyn Clock,
    ent: Entity,
    id: &str,
    set: Fields,
) -> Result<Op> {
    let dev = meta::device_id(tx)?;

    let state = meta::load_hlc_state(tx)?.tick(clock.now_ms());
    let hlc = Hlc::from_state(state, dev.clone())?;
    meta::save_hlc_state(tx, state)?;

    // seq は outbox の行ではなくカウンタで採番する（行を消しても再利用しない）。
    let seq = meta::load_op_seq(tx)? + 1;
    meta::save_op_seq(tx, seq)?;

    let op = Op {
        v: OP_VERSION,
        dev,
        seq,
        hlc: hlc.to_string(),
        ent,
        id: id.to_owned(),
        set,
    };

    let outcome = apply_op(tx, &op)?;
    if outcome.applied != op.set.len() {
        return Err(Error::ClockBehind {
            hlc: op.hlc,
            entity: ent.as_str(),
            id: op.id,
        });
    }

    tx.execute(
        "INSERT INTO outbox(seq, hlc, op_json, flushed) VALUES (?1, ?2, ?3, 0)",
        params![op.seq, op.hlc, op.to_json_line()?],
    )?;
    Ok(op)
}

/// `record_change` を 1 件だけ、自前のトランザクションで実行して commit する。
pub fn record_change_committed(
    conn: &mut Connection,
    clock: &dyn Clock,
    ent: Entity,
    id: &str,
    set: impl IntoIterator<Item = (String, Value)>,
) -> Result<Op> {
    let tx = crate::db::write_tx(conn)?;
    let op = record_change(&tx, clock, ent, id, set)?;
    tx.commit()?;
    Ok(op)
}

/// 未書き出し（`flushed = 0`）の Op を seq 順に返す。
pub fn pending_ops(conn: &Connection) -> Result<Vec<Op>> {
    let mut stmt = conn.prepare("SELECT op_json FROM outbox WHERE flushed = 0 ORDER BY seq")?;
    let lines = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    lines.iter().map(|l| Op::from_json_line(l)).collect()
}

/// `field_clocks` から項目の時計を引く（テストや診断用）。
pub fn field_clock(
    conn: &Connection,
    ent: Entity,
    id: &str,
    field: &str,
) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT hlc FROM field_clocks WHERE entity = ?1 AND entity_id = ?2 AND field = ?3",
            params![ent.as_str(), id, field],
            |r| r.get(0),
        )
        .optional()?)
}
