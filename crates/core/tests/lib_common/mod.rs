//! ライブラリ層（`library`、検索、書き出し、バックアップ）のテストの共通ヘルパー。
#![allow(dead_code)]

use bibcontainer_core::library::{self, Env, FieldChange, UpdateOptions};
use bibcontainer_core::{Entity, ManualClock, db, pending_ops};
use rusqlite::Connection;
use serde_json::{Value, json};

/// 2026-10-14T08:30:00Z 付近
pub const T0: i64 = 1_791_966_600_000;
pub const TODAY: &str = "2026-10-14";

/// メモリ上の DB と、固定した時計。
///
/// 時計はテストの間だけ生きていればよいので、リークして `'static` にする
/// （`Env` が `fx` を借りたままにならず、`&mut fx` と一緒に使える）。
pub struct Fx {
    pub conn: Connection,
    pub clock: &'static ManualClock,
}

impl Fx {
    pub fn new() -> Self {
        Fx {
            conn: db::open_in_memory().unwrap(),
            clock: Box::leak(Box::new(ManualClock::new(T0))),
        }
    }
}

/// 「今日」を固定した環境。
pub fn env(clock: &'static ManualClock) -> Env<'static> {
    Env::new(clock).with_today(TODAY)
}

/// 手入力の変更（`json!` の値）。
pub fn manual(pairs: &[(&str, Value)]) -> Vec<FieldChange> {
    pairs
        .iter()
        .map(|(k, v)| FieldChange::manual(*k, v.clone()))
        .collect()
}

/// 書名だけの本を作る。
pub fn add_titled(fx: &mut Fx, title: &str) -> String {
    add(fx, &[("title", json!(title))])
}

/// 手入力の項目で本を作る。
pub fn add(fx: &mut Fx, pairs: &[(&str, Value)]) -> String {
    library::create_book(&mut fx.conn, &env(fx.clock), &manual(pairs)).unwrap()
}

/// 手入力で 1 冊の項目を変更する。
pub fn edit(fx: &mut Fx, id: &str, pairs: &[(&str, Value)]) -> library::ChangeReport {
    library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        id,
        &manual(pairs),
        UpdateOptions::default(),
    )
    .unwrap()
}

/// 書籍の 1 列を読む（SQL の値を JSON に直したもの）。
pub fn column(conn: &Connection, id: &str, column: &str) -> Value {
    conn.query_row(
        &format!("SELECT {column} FROM books WHERE id = ?1"),
        [id],
        |r| {
            Ok(match r.get_ref(0)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(i) => json!(i),
                rusqlite::types::ValueRef::Real(f) => json!(f),
                rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                rusqlite::types::ValueRef::Blob(_) => Value::Null,
            })
        },
    )
    .unwrap()
}

/// 書籍の `field_sources`（JSON）。
pub fn sources(conn: &Connection, id: &str) -> Value {
    serde_json::from_str(column(conn, id, "field_sources_json").as_str().unwrap()).unwrap()
}

/// 書籍の `manual_fields`（JSON の配列）。
pub fn manual_fields(conn: &Connection, id: &str) -> Value {
    serde_json::from_str(column(conn, id, "manual_fields_json").as_str().unwrap()).unwrap()
}

/// 未書き出しの Op の数。
pub fn outbox_len(conn: &Connection) -> usize {
    pending_ops(conn).unwrap().len()
}

/// 直近の Op（最後に記録したもの）。
pub fn last_op(conn: &Connection) -> bibcontainer_core::Op {
    pending_ops(conn).unwrap().pop().expect("Op がある")
}

/// 指定したエンティティについての Op を、古い順に返す。
pub fn ops_for(conn: &Connection, ent: Entity, id: &str) -> Vec<bibcontainer_core::Op> {
    pending_ops(conn)
        .unwrap()
        .into_iter()
        .filter(|o| o.ent == ent && o.id == id)
        .collect()
}
