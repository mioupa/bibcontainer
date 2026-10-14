//! 統合テストの共通ヘルパー。
#![allow(dead_code)]

use bibcontainer_core::{
    ApplyOutcome, Entity, Hlc, Op, apply_op, db, dump_canonical, schema::Fields,
};
use rusqlite::Connection;
use serde_json::{Value, json};

pub const BOOK_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55";
pub const BOOK_2: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a56";
pub const TAG_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0b01";
pub const TAG_2: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0b02";
pub const SERIES_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c01";

pub const DEV_A: &str = "d-aaaaaaaaaaaa";
pub const DEV_B: &str = "d-bbbbbbbbbbbb";
pub const DEV_C: &str = "d-cccccccccccc";

/// `json!({...})` の連想配列を `record_change` に渡せる形にする。
pub fn kv(v: Value) -> Vec<(String, Value)> {
    match v {
        Value::Object(m) => m.into_iter().collect(),
        other => panic!("オブジェクトを渡すこと: {other}"),
    }
}

/// 他端末から届いた Op を作る（seq は適用に影響しない）。
pub fn remote_op(dev: &str, ms: i64, counter: u32, ent: Entity, id: &str, set: Value) -> Op {
    let set: Fields = kv(set).into_iter().collect();
    Op {
        v: 1,
        dev: dev.to_owned(),
        seq: 1,
        hlc: Hlc::new(ms, counter, dev).unwrap().to_string(),
        ent,
        id: id.to_owned(),
        set,
    }
}

pub fn book_op(dev: &str, ms: i64, set: Value) -> Op {
    remote_op(dev, ms, 0, Entity::Book, BOOK_1, set)
}

pub fn mem_db() -> Connection {
    db::open_in_memory().unwrap()
}

/// 1 つのトランザクションで適用して commit する。
pub fn apply(conn: &mut Connection, op: &Op) -> ApplyOutcome {
    let tx = db::write_tx(conn).unwrap();
    let outcome = apply_op(&tx, op).unwrap();
    tx.commit().unwrap();
    outcome
}

pub fn apply_all<'a>(conn: &mut Connection, ops: impl IntoIterator<Item = &'a Op>) {
    for op in ops {
        apply(conn, op);
    }
}

pub fn dump(conn: &Connection) -> Value {
    dump_canonical(conn).unwrap()
}

/// dump から、指定した ID の行を取り出す。
pub fn dump_row(dump: &Value, table: &str, id: &str) -> Value {
    dump[table]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap_or_else(|| panic!("{table} に {id} がない"))
        .clone()
}

pub fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
