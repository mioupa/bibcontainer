//! 取得元・手修正の記録（`field_sources.<項目名>`、`manual_fields.<項目名>`）の Op のテスト。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod common;

use bibcontainer_core::store::field_clock;
use bibcontainer_core::{ApplyOutcome, Entity, Error, ManualClock, record_change_committed};
use serde_json::json;

use common::*;

fn row(conn: &rusqlite::Connection) -> serde_json::Value {
    dump_row(&dump(conn), "books", BOOK_1)
}

#[test]
fn provenance_keys_update_the_stored_forms_and_keep_clocks_with_dotted_names() {
    let mut conn = mem_db();
    let op = book_op(
        DEV_A,
        1_000,
        json!({"title": "坊っちゃん", "field_sources.title": "openbd",
               "manual_fields.title": true, "manual_fields.memo": true,
               "field_sources.list_price": "barcode"}),
    );
    assert_eq!(
        apply(&mut conn, &op),
        ApplyOutcome {
            applied: 5,
            stale: 0,
            invalid: 0
        }
    );
    let r = row(&conn);
    assert_eq!(
        r["field_sources"],
        json!({"list_price": "barcode", "title": "openbd"})
    );
    assert_eq!(r["manual_fields"], json!(["memo", "title"]), "辞書順の配列");
    for key in [
        "field_sources.title",
        "field_sources.list_price",
        "manual_fields.title",
        "manual_fields.memo",
    ] {
        assert_eq!(
            field_clock(&conn, Entity::Book, BOOK_1, key).unwrap(),
            Some(op.hlc.clone()),
            "{key} の時計"
        );
    }
    // 項目全体のキーに時計は無い
    assert_eq!(
        field_clock(&conn, Entity::Book, BOOK_1, "field_sources").unwrap(),
        None
    );
}

#[test]
fn null_removes_an_entry_and_a_newer_op_wins_in_any_order() {
    let set = book_op(
        DEV_A,
        1_000,
        json!({"field_sources.title": "openbd", "manual_fields.title": true}),
    );
    let clear = book_op(
        DEV_B,
        2_000,
        json!({"field_sources.title": null, "manual_fields.title": null}),
    );
    let again = book_op(
        DEV_A,
        3_000,
        json!({"field_sources.title": "ndl", "manual_fields.title": true}),
    );

    let mut a = mem_db();
    apply_all(&mut a, [&set, &clear]);
    assert_eq!(row(&a)["field_sources"], json!({}));
    assert_eq!(row(&a)["manual_fields"], json!([]));

    let mut b = mem_db();
    apply_all(&mut b, [&clear, &set]);
    assert_eq!(dump(&a), dump(&b), "古い Op が後から届いても同じ");

    apply_all(&mut a, [&again]);
    apply_all(&mut b, [&again, &set, &clear]);
    assert_eq!(dump(&a), dump(&b));
    assert_eq!(row(&a)["field_sources"], json!({"title": "ndl"}));
    assert_eq!(row(&a)["manual_fields"], json!(["title"]));
}

#[test]
fn records_for_different_fields_from_different_devices_both_survive() {
    // 端末 A が書名、端末 B が出版社を手で直した（FR-SYNC-05）
    let a_op = book_op(
        DEV_A,
        1_000,
        json!({"title": "A の書名", "field_sources.title": "manual", "manual_fields.title": true}),
    );
    let b_op = book_op(
        DEV_B,
        1_000,
        json!({"publisher": "B 社", "field_sources.publisher": "manual",
               "manual_fields.publisher": true}),
    );
    let mut x = mem_db();
    apply_all(&mut x, [&a_op, &b_op]);
    let mut y = mem_db();
    apply_all(&mut y, [&b_op, &a_op]);
    assert_eq!(dump(&x), dump(&y));
    let r = row(&x);
    assert_eq!(
        r["field_sources"],
        json!({"publisher": "manual", "title": "manual"})
    );
    assert_eq!(r["manual_fields"], json!(["publisher", "title"]));
}

#[test]
fn invalid_provenance_keys_are_ignored_without_advancing_clocks() {
    let mut conn = mem_db();
    let op = book_op(
        DEV_A,
        1_000,
        json!({"memo": "x",
               "field_sources.nope": "x",        // 項目名が不正
               "manual_fields.nope": true,       // 項目名が不正
               "field_sources.title": 1,         // 型が不正
               "manual_fields.title": false,     // true か null だけが有効
               "manual_fields.memo": "yes"}),
    );
    assert_eq!(
        apply(&mut conn, &op),
        ApplyOutcome {
            applied: 1,
            stale: 0,
            invalid: 5
        }
    );
    let r = row(&conn);
    assert_eq!(r["memo"], json!("x"));
    assert_eq!(r["field_sources"], json!({}));
    assert_eq!(r["manual_fields"], json!([]));
    assert_eq!(r["extra"], json!({}), "不正なキーは extra にも入らない");
    assert_eq!(
        count(&conn, "field_clocks"),
        1,
        "memo の時計だけ（不正なものは時計も進めない）"
    );

    // 後から正しい値が来れば、古い HLC でも書ける
    let valid = book_op(DEV_B, 500, json!({"field_sources.title": "ndl"}));
    apply(&mut conn, &valid);
    assert_eq!(row(&conn)["field_sources"], json!({"title": "ndl"}));
}

#[test]
fn the_whole_object_keys_are_not_special_any_more() {
    let mut conn = mem_db();
    apply(
        &mut conn,
        &book_op(
            DEV_A,
            1_000,
            json!({"field_sources": {"title": "openbd"}, "manual_fields": ["title"]}),
        ),
    );
    let r = row(&conn);
    assert_eq!(r["field_sources"], json!({}));
    assert_eq!(r["manual_fields"], json!([]));
    assert_eq!(
        r["extra"],
        json!({"field_sources": {"title": "openbd"}, "manual_fields": ["title"]})
    );
}

#[test]
fn provenance_keys_on_other_entities_are_plain_unknown_fields() {
    let mut conn = mem_db();
    apply(
        &mut conn,
        &remote_op(
            DEV_A,
            1_000,
            0,
            Entity::Series,
            SERIES_1,
            json!({"name": "S", "field_sources.name": "x"}),
        ),
    );
    let r = dump_row(&dump(&conn), "series", SERIES_1);
    assert_eq!(r["extra"], json!({"field_sources.name": "x"}));
}

#[test]
fn local_changes_validate_provenance_keys_and_record_them_in_the_outbox() {
    let mut conn = mem_db();
    let clock = ManualClock::new(10_000);
    let op = record_change_committed(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        kv(json!({"title": "t", "field_sources.title": "manual", "manual_fields.title": true})),
    )
    .unwrap();
    assert_eq!(op.set["field_sources.title"], json!("manual"));
    assert_eq!(
        row(&conn)["field_sources"],
        json!({"title": "manual"}),
        "ローカルの変更も同じ経路で反映される"
    );

    let before = count(&conn, "outbox");
    for bad in [
        json!({"field_sources.nope": "x"}),
        json!({"manual_fields.nope": true}),
        json!({"field_sources.title": 3}),
        json!({"field_sources.title": ""}),
        json!({"manual_fields.title": false}),
        json!({"field_sources": {"title": "x"}}),
        json!({"manual_fields": ["title"]}),
    ] {
        let err = record_change_committed(&mut conn, &clock, Entity::Book, BOOK_1, kv(bad.clone()))
            .unwrap_err();
        assert!(matches!(err, Error::InvalidOp(_)), "{bad}: {err:?}");
    }
    assert_eq!(count(&conn, "outbox"), before, "何も記録されない");
}
