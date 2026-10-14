//! ローカル変更の記録（record_change）のテスト。

mod common;

use bibcontainer_core::{
    Entity, Error, Hlc, ManualClock, Op, apply_op, meta, observe_hlc, open, pending_ops,
    record_change, record_change_committed, store::field_clock, write_tx,
};
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use serde_json::json;
use tempfile::{TempDir, tempdir};

use common::*;

struct Env {
    dir: TempDir,
}

impl Env {
    fn new() -> Self {
        Env {
            dir: tempdir().unwrap(),
        }
    }

    fn open(&self) -> Connection {
        open(
            &self.dir.path().join("bibcontainer.sqlite3"),
            &self.dir.path().join("backups"),
        )
        .unwrap()
        .0
    }
}

/// 2026-10-14T08:30:00.123Z
fn t0() -> i64 {
    Utc.with_ymd_and_hms(2026, 10, 14, 8, 30, 0)
        .unwrap()
        .timestamp_millis()
        + 123
}

fn outbox_rows(conn: &Connection) -> Vec<(i64, String, String, i64)> {
    conn.prepare("SELECT seq, hlc, op_json, flushed FROM outbox ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn set_title(conn: &mut Connection, clock: &ManualClock, id: &str, title: &str) -> Op {
    record_change_committed(conn, clock, Entity::Book, id, kv(json!({ "title": title }))).unwrap()
}

#[test]
fn record_change_creates_the_row_the_clocks_and_the_outbox_entry() {
    let env = Env::new();
    let mut conn = env.open();
    let clock = ManualClock::new(t0());
    let dev = meta::device_id(&conn).unwrap();

    let op = record_change_committed(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({
            "title": "坊っちゃん",
            "rating": 4,
            "authors": [{"name": "夏目漱石", "role": "著"}],
            "created_at": "2026-10-14T08:30:00.123Z",
        })),
    )
    .unwrap();

    // 返り値の Op
    assert_eq!(op.v, 1);
    assert_eq!(op.dev, dev);
    assert_eq!(op.seq, 1);
    assert_eq!(op.hlc, format!("{:015}:00000:{dev}", t0()));
    assert_eq!(op.ent, Entity::Book);
    assert_eq!(op.id, BOOK_1);
    assert_eq!(op.set.len(), 4);

    // 行（updated_at は HLC のミリ秒を ISO 8601 にしたもの）
    let (title, rating, authors, created, updated): (String, i64, String, String, String) = conn
        .query_row(
            "SELECT title, rating, authors_json, created_at, updated_at FROM books WHERE id = ?1",
            [BOOK_1],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(title, "坊っちゃん");
    assert_eq!(rating, 4);
    assert_eq!(authors, r#"[{"name":"夏目漱石","role":"著"}]"#);
    assert_eq!(created, "2026-10-14T08:30:00.123Z");
    assert_eq!(updated, "2026-10-14T08:30:00.123Z");

    // 書いた項目すべてに時計がつき、書いていない項目にはつかない
    assert_eq!(count(&conn, "field_clocks"), 4);
    for field in ["title", "rating", "authors", "created_at"] {
        assert_eq!(
            field_clock(&conn, Entity::Book, BOOK_1, field)
                .unwrap()
                .as_deref(),
            Some(op.hlc.as_str()),
            "{field}"
        );
    }
    assert_eq!(
        field_clock(&conn, Entity::Book, BOOK_1, "memo").unwrap(),
        None
    );

    // outbox: op_json は返り値の Op と一致する
    let rows = outbox_rows(&conn);
    assert_eq!(rows.len(), 1);
    let (seq, hlc, op_json, flushed) = &rows[0];
    assert_eq!((*seq, hlc.as_str(), *flushed), (1, op.hlc.as_str(), 0));
    assert!(!op_json.contains('\n'));
    assert_eq!(Op::from_json_line(op_json).unwrap(), op);
    assert_eq!(*op_json, op.to_json_line().unwrap());
    let v: serde_json::Value = serde_json::from_str(op_json).unwrap();
    for key in ["v", "dev", "seq", "hlc", "ent", "id", "set"] {
        assert!(v.get(key).is_some(), "{key}");
    }
    assert_eq!(pending_ops(&conn).unwrap(), vec![op]);
}

#[test]
fn seq_counts_up_from_one_and_continues_after_reopening() {
    let env = Env::new();
    let clock = ManualClock::new(t0());

    let mut conn = env.open();
    let seqs: Vec<i64> = (0..3)
        .map(|i| set_title(&mut conn, &clock, BOOK_1, &format!("v{i}")).seq)
        .collect();
    assert_eq!(seqs, [1, 2, 3]);
    drop(conn);

    let mut conn = env.open();
    assert_eq!(set_title(&mut conn, &clock, BOOK_1, "v3").seq, 4);

    // outbox の行を消しても seq は再利用されない
    conn.execute("DELETE FROM outbox", []).unwrap();
    assert_eq!(set_title(&mut conn, &clock, BOOK_1, "v4").seq, 5);
    assert_eq!(outbox_rows(&conn).len(), 1);
}

#[test]
fn hlc_increases_with_a_frozen_clock_and_continues_after_reopening_even_if_the_wall_clock_went_back()
 {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();
    let dev = meta::device_id(&conn).unwrap();

    let a = set_title(&mut conn, &clock, BOOK_1, "a");
    let b = set_title(&mut conn, &clock, BOOK_1, "b");
    assert_eq!(a.hlc, format!("{:015}:00000:{dev}", t0()));
    assert_eq!(
        b.hlc,
        format!("{:015}:00001:{dev}", t0()),
        "同じ ms ならカウンタが増える"
    );
    drop(conn);

    // 再起動後、壁時計が 1 時間戻っていても、HLC は続きから増える
    let mut conn = env.open();
    clock.set(t0() - 3_600_000);
    let c = set_title(&mut conn, &clock, BOOK_1, "c");
    assert_eq!(c.hlc, format!("{:015}:00002:{dev}", t0()));
    assert!(a.hlc < b.hlc && b.hlc < c.hlc);

    // 壁時計が進めば、ms が進んでカウンタは 0 に戻る
    clock.set(t0() + 10);
    let d = set_title(&mut conn, &clock, BOOK_1, "d");
    assert_eq!(d.hlc, format!("{:015}:00000:{dev}", t0() + 10));
    assert!(c.hlc < d.hlc);

    // updated_at は戻らない（max）。最後の変更の時刻になる。
    let updated: String = conn
        .query_row(
            "SELECT updated_at FROM books WHERE id = ?1",
            [BOOK_1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(updated, "2026-10-14T08:30:00.133Z");
}

#[test]
fn rolling_back_leaves_no_row_outbox_hlc_or_seq_behind() {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();

    {
        let tx = write_tx(&mut conn).unwrap();
        let op =
            record_change(&tx, &clock, Entity::Book, BOOK_1, kv(json!({"title": "x"}))).unwrap();
        assert_eq!(op.seq, 1);
        // commit しない（ロールバック）
    }

    assert_eq!(count(&conn, "books"), 0);
    assert_eq!(count(&conn, "field_clocks"), 0);
    assert_eq!(count(&conn, "outbox"), 0);
    assert_eq!(meta::get(&conn, meta::KEY_HLC_LAST).unwrap(), None);
    assert_eq!(meta::get(&conn, meta::KEY_OP_SEQ_LAST).unwrap(), None);

    // 次の記録は、何もなかったかのように seq 1・カウンタ 0 から始まる
    let op = set_title(&mut conn, &clock, BOOK_1, "y");
    assert_eq!(op.seq, 1);
    assert!(op.hlc.contains(":00000:"));
}

#[test]
fn several_changes_in_one_transaction_are_committed_or_rolled_back_together() {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();

    let tx = write_tx(&mut conn).unwrap();
    let ops: Vec<Op> = [BOOK_1, BOOK_2]
        .iter()
        .map(|id| {
            record_change(
                &tx,
                &clock,
                Entity::Book,
                id,
                kv(json!({"memo": "まとめて保存"})),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(ops.iter().map(|o| o.seq).collect::<Vec<_>>(), [1, 2]);
    tx.commit().unwrap();
    assert_eq!(count(&conn, "books"), 2);
    assert_eq!(outbox_rows(&conn).len(), 2);

    // 途中で失敗した 1 件は取り消され、それ以前の記録は残ったまま続けられる
    let tx = write_tx(&mut conn).unwrap();
    record_change(
        &tx,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"title": "ok"})),
    )
    .unwrap();
    assert!(
        record_change(
            &tx,
            &clock,
            Entity::Book,
            BOOK_2,
            kv(json!({"rating": "bad"}))
        )
        .is_err()
    );
    let op = record_change(&tx, &clock, Entity::Book, BOOK_2, kv(json!({"rating": 5}))).unwrap();
    assert_eq!(op.seq, 4, "失敗した呼び出しは seq を消費しない");
    tx.commit().unwrap();
    assert_eq!(outbox_rows(&conn).len(), 4);
}

#[test]
fn invalid_changes_are_rejected_without_touching_anything() {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();

    let cases = [
        (
            "未知の項目",
            Entity::Book,
            BOOK_1,
            json!({"no_such_field": 1}),
        ),
        (
            "列名（Op 名ではない）",
            Entity::Book,
            BOOK_1,
            json!({"authors_json": []}),
        ),
        (
            "extra_json は直接触れない",
            Entity::Book,
            BOOK_1,
            json!({"extra_json": {}}),
        ),
        (
            "updated_at は派生値",
            Entity::Book,
            BOOK_1,
            json!({"updated_at": "x"}),
        ),
        (
            "id は項目ではない",
            Entity::Book,
            BOOK_1,
            json!({"id": "x"}),
        ),
        ("整数に文字列", Entity::Book, BOOK_1, json!({"rating": "4"})),
        ("整数に小数", Entity::Book, BOOK_1, json!({"rating": 4.5})),
        ("真偽値に整数", Entity::Book, BOOK_1, json!({"deleted": 1})),
        ("文字列に数値", Entity::Book, BOOK_1, json!({"title": 5})),
        (
            "JSON 項目に文字列",
            Entity::Book,
            BOOK_1,
            json!({"tags": "a,b"}),
        ),
        (
            "実数に文字列",
            Entity::Book,
            BOOK_1,
            json!({"volume_sort": "1"}),
        ),
        (
            "NOT NULL 列に null",
            Entity::Book,
            BOOK_1,
            json!({"title": null}),
        ),
        (
            "NOT NULL 列に null（JSON）",
            Entity::Book,
            BOOK_1,
            json!({"authors": null}),
        ),
        (
            "NOT NULL 列に null（真偽値）",
            Entity::Book,
            BOOK_1,
            json!({"deleted": null}),
        ),
        (
            "別エンティティの項目",
            Entity::Tag,
            TAG_1,
            json!({"rating": 3}),
        ),
        ("空の set", Entity::Book, BOOK_1, json!({})),
        (
            "id が UUID でない",
            Entity::Book,
            "book-1",
            json!({"title": "x"}),
        ),
        ("id が空", Entity::Book, "", json!({"title": "x"})),
    ];
    for (label, ent, id, set) in cases {
        let err = record_change_committed(&mut conn, &clock, ent, id, kv(set)).unwrap_err();
        assert!(matches!(err, Error::InvalidOp(_)), "{label}: {err:?}");
    }
    // 1 つでも悪い項目があれば、良い項目も書かない
    let err = record_change_committed(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"title": "ok", "bogus": 1})),
    )
    .unwrap_err();
    assert!(matches!(err, Error::InvalidOp(_)));

    for table in ["books", "field_clocks", "outbox"] {
        assert_eq!(count(&conn, table), 0, "{table}");
    }
    assert_eq!(meta::get(&conn, meta::KEY_HLC_LAST).unwrap(), None);
    assert_eq!(meta::get(&conn, meta::KEY_OP_SEQ_LAST).unwrap(), None);
}

#[test]
fn nullable_columns_can_be_cleared_with_null() {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();
    set_title(&mut conn, &clock, BOOK_1, "x");
    record_change_committed(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"rating": 5})),
    )
    .unwrap();
    record_change_committed(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"rating": null})),
    )
    .unwrap();
    let rating: Option<i64> = conn
        .query_row("SELECT rating FROM books WHERE id = ?1", [BOOK_1], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rating, None);
}

#[test]
fn recording_a_new_op_after_applying_a_remote_one_needs_the_hlc_to_be_observed() {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();

    // 壁時計が 10 分進んだ他端末の Op を、HLC を観測せずに適用してしまった場合
    let remote = book_op(DEV_B, t0() + 600_000, json!({"title": "リモート"}));
    apply(&mut conn, &remote);

    let tx = write_tx(&mut conn).unwrap();
    let err = record_change(
        &tx,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"title": "ローカル"})),
    )
    .unwrap_err();
    assert!(matches!(err, Error::ClockBehind { .. }), "{err:?}");
    // 失敗した呼び出しは何も残さない
    tx.commit().unwrap();
    assert_eq!(meta::get(&conn, meta::KEY_HLC_LAST).unwrap(), None);
    assert_eq!(meta::get(&conn, meta::KEY_OP_SEQ_LAST).unwrap(), None);
    assert_eq!(count(&conn, "outbox"), 0);
    assert_eq!(
        dump_row(&dump(&conn), "books", BOOK_1)["title"],
        json!("リモート")
    );

    // 観測すれば、ローカルの変更は必ず勝つ
    let tx = write_tx(&mut conn).unwrap();
    observe_hlc(&tx, &Hlc::parse(&remote.hlc).unwrap()).unwrap();
    let op = record_change(
        &tx,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"title": "ローカル"})),
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(op.hlc > remote.hlc);
    assert_eq!(
        dump_row(&dump(&conn), "books", BOOK_1)["title"],
        json!("ローカル")
    );
}

#[test]
fn observed_hlc_survives_a_restart() {
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut conn = env.open();
    let remote = Hlc::new(t0() + 5_000, 7, DEV_B).unwrap();

    let tx = write_tx(&mut conn).unwrap();
    observe_hlc(&tx, &remote).unwrap();
    tx.commit().unwrap();
    drop(conn);

    let mut conn = env.open();
    let op = set_title(&mut conn, &clock, BOOK_1, "x");
    assert!(op.hlc > remote.to_string());

    // 観測した値が自分の値より古ければ、前回値は変わらない
    let before = meta::get(&conn, meta::KEY_HLC_LAST).unwrap();
    let tx = write_tx(&mut conn).unwrap();
    observe_hlc(&tx, &Hlc::new(1, 0, DEV_B).unwrap()).unwrap();
    tx.commit().unwrap();
    assert_eq!(meta::get(&conn, meta::KEY_HLC_LAST).unwrap(), before);
}

#[test]
fn recorded_ops_can_be_applied_to_another_database_with_the_same_result() {
    // ローカルで記録した Op を、別の端末の DB に適用すると同じ状態になる（同期の最小の往復）。
    let env = Env::new();
    let clock = ManualClock::new(t0());
    let mut local = env.open();
    set_title(&mut local, &clock, BOOK_1, "坊っちゃん");
    record_change_committed(
        &mut local,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"rating": 4, "deleted": true, "tags": ["小説"]})),
    )
    .unwrap();

    let mut other = mem_db();
    for op in pending_ops(&local).unwrap() {
        let tx = write_tx(&mut other).unwrap();
        apply_op(&tx, &op).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(dump(&local), dump(&other));
}
