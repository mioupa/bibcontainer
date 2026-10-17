//! 書籍の作成・更新・ゴミ箱・完全削除、由来（origin）の規則、一覧の行と詳細のテスト。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod common;
mod lib_common;

use std::collections::BTreeSet;

use bibcontainer_core::library::{
    self, FieldChange, UpdateOptions, find_by_isbn, pending_book_ids, purchase_store_suggestions,
};
use bibcontainer_core::schema::Entity;
use bibcontainer_core::{Error, dump_canonical, get_book_detail, get_rows, list_rows, pending_ops};
use serde_json::{Value, json};

use lib_common::*;

const ISBN: &str = "9784003101117";

fn provider(field: &str, value: Value, name: &str) -> FieldChange {
    FieldChange::provider(field, value, name)
}

fn update(
    fx: &mut Fx,
    id: &str,
    changes: &[FieldChange],
    override_manual: bool,
) -> library::ChangeReport {
    library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        id,
        changes,
        UpdateOptions {
            override_manual,
            ..UpdateOptions::default()
        },
    )
    .unwrap()
}

fn books_count(fx: &Fx) -> i64 {
    fx.conn
        .query_row("SELECT count(*) FROM books", [], |r| r.get(0))
        .unwrap()
}

fn assert_validation<T: std::fmt::Debug>(r: Result<T, Error>, field: &str) {
    match r {
        Err(Error::Validation { field: f, .. }) => assert_eq!(f, field),
        other => panic!("{field} の検証エラーのはず: {other:?}"),
    }
}

// ---------- 作成 ----------

#[test]
fn create_writes_one_op_with_every_field_defaults_and_provenance() {
    let mut fx = Fx::new();
    let id = library::create_book(
        &mut fx.conn,
        &env(fx.clock),
        &[
            FieldChange::barcode("isbn13", json!(ISBN)),
            FieldChange::barcode("list_price", json!(700)),
            FieldChange::barcode("c_code", json!("0193")),
            FieldChange::provider("title", json!("吾輩は猫である"), "openbd"),
            FieldChange::provider("publisher", json!("岩波書店"), "openbd"),
            FieldChange::manual("subtitle", json!("副題")),
            FieldChange::manual("memo", json!("メモ")),
            FieldChange::manual("title_kana", json!(null)),
        ],
    )
    .unwrap();

    // ID は UUIDv7
    assert_eq!(uuid::Uuid::try_parse(&id).unwrap().get_version_num(), 7);

    let ops = ops_for(&fx.conn, Entity::Book, &id);
    assert_eq!(ops.len(), 1, "作成は 1 つの Op");
    let set = &ops[0].set;
    for def in Entity::Book.fields() {
        assert!(set.contains_key(def.name), "{} が set にない", def.name);
    }
    // 未指定は既定値
    assert_eq!(set["ownership"], json!("owned"));
    assert_eq!(set["read_status"], json!("unread"));
    assert_eq!(set["meta_status"], json!("ok"));
    assert_eq!(set["deleted"], json!(false));
    assert_eq!(set["purged"], json!(false));
    assert_eq!(set["authors"], json!([]));
    assert_eq!(set["tags"], json!([]));
    assert_eq!(set["rating"], json!(null));
    // created_at は ISO 8601 UTC ミリ秒（時計は 2026-10-14T08:30:00.000Z）
    assert_eq!(set["created_at"], json!("2026-10-14T08:30:00.000Z"));
    // 由来の記録（書誌の項目だけ）
    assert_eq!(set["field_sources.isbn13"], json!("barcode"));
    assert_eq!(set["field_sources.list_price"], json!("barcode"));
    assert_eq!(set["field_sources.c_code"], json!("barcode"));
    assert_eq!(set["field_sources.title"], json!("openbd"));
    assert_eq!(set["field_sources.subtitle"], json!("manual"));
    assert_eq!(set["manual_fields.subtitle"], json!(true));
    assert!(!set.contains_key("manual_fields.title"));
    assert!(
        !set.contains_key("field_sources.memo"),
        "メモは書誌の項目でない"
    );
    assert!(!set.contains_key("manual_fields.memo"));
    assert!(
        !set.contains_key("field_sources.title_kana"),
        "値が空の項目には記録しない"
    );
    assert_eq!(set["memo"], json!("メモ"));

    // 保存された状態
    assert_eq!(
        sources(&fx.conn, &id),
        json!({"c_code": "barcode", "isbn13": "barcode", "list_price": "barcode",
               "publisher": "openbd", "subtitle": "manual", "title": "openbd"})
    );
    assert_eq!(manual_fields(&fx.conn, &id), json!(["subtitle"]));
    assert_eq!(column(&fx.conn, &id, "list_price"), json!(700));
}

#[test]
fn create_fills_the_finished_date_for_a_book_registered_as_read() {
    let mut fx = Fx::new();
    let a = add(
        &mut fx,
        &[("title", json!("A")), ("read_status", json!("read"))],
    );
    assert_eq!(column(&fx.conn, &a, "read_finished_on"), json!(TODAY));
    let b = add(
        &mut fx,
        &[
            ("title", json!("B")),
            ("read_status", json!("read")),
            ("read_finished_on", json!("2020-05")),
        ],
    );
    assert_eq!(column(&fx.conn, &b, "read_finished_on"), json!("2020-05"));
    let c = add(&mut fx, &[("title", json!("C"))]);
    assert_eq!(column(&fx.conn, &c, "read_finished_on"), json!(null));
}

#[test]
fn create_with_only_an_isbn_and_pending_status_is_allowed() {
    let mut fx = Fx::new();
    let id = library::create_book(
        &mut fx.conn,
        &env(fx.clock),
        &[
            FieldChange::barcode("isbn13", json!(ISBN)),
            FieldChange::barcode("list_price", json!(700)),
            FieldChange::manual("meta_status", json!("pending")),
        ],
    )
    .unwrap();
    assert_eq!(column(&fx.conn, &id, "title"), json!(""));
    assert_eq!(pending_book_ids(&fx.conn).unwrap(), [id]);
}

#[test]
fn invalid_values_are_rejected_and_nothing_is_written() {
    let mut fx = Fx::new();
    let existing = add_titled(&mut fx, "既存");
    let ops_before = outbox_len(&fx.conn);
    let books_before = books_count(&fx);
    let e = env(fx.clock);

    let cases: Vec<(&str, Value)> = vec![
        ("ownership", json!("lost")),
        ("read_status", json!("done")),
        ("wish_priority", json!("urgent")),
        ("purchase_cond", json!("old")),
        ("meta_status", json!("unknown")),
        ("rating", json!(0)),
        ("rating", json!(6)),
        ("rating", json!(2.5)),
        ("published_on", json!("2026-13")),
        ("published_on", json!("2026-02-30")),
        ("read_started_on", json!("2026/10/14")),
        ("read_finished_on", json!("10-14")),
        ("purchased_on", json!(20261014)),
        ("isbn13", json!("9784003101118")),
        ("isbn13", json!("4003101111")),
        ("c_code", json!("193")),
        ("c_code", json!("01930")),
        ("c_code", json!("01a3")),
        ("list_price", json!(-1)),
        ("authors", json!([{"name": ""}])),
        ("tags", json!("小説")),
        ("series_id", json!("not-an-id")),
        ("series_id", json!("0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c01")), // 存在しない
        ("location_id", json!("0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01")), // 存在しない
        ("deleted", json!(true)),
        ("purged", json!(true)),
        ("created_at", json!("2026-01-01T00:00:00.000Z")),
        ("no_such_field", json!(1)),
        ("field_sources", json!({"title": "x"})),
    ];
    for (field, value) in &cases {
        let change = [FieldChange::manual(*field, value.clone())];
        let created = library::create_book(&mut fx.conn, &e, &change);
        assert_validation(created, field);
        let updated = library::update_book(
            &mut fx.conn,
            &e,
            &existing,
            &change,
            UpdateOptions::default(),
        );
        assert_validation(updated, field);
    }
    assert_eq!(outbox_len(&fx.conn), ops_before, "Op は増えない");
    assert_eq!(books_count(&fx), books_before, "本は増えない");

    // 取得元の名前が不正
    for name in ["", "  ", "manual"] {
        let r = library::create_book(
            &mut fx.conn,
            &e,
            &[FieldChange::provider("title", json!("x"), name)],
        );
        assert_validation(r, "title");
    }
}

#[test]
fn values_are_normalized_before_they_are_stored() {
    let mut fx = Fx::new();
    let id = add(
        &mut fx,
        &[
            ("title", json!("  吾輩は猫である  ")),
            ("isbn13", json!("978-4-00-310111-7")),
            ("c_code", json!("０１９３")),
            ("published_on", json!("２００２－１０")),
            ("publisher", json!("   ")),
            (
                "authors",
                json!([{"name": " 夏目漱石 ", "kana": "", "role": "著"}]),
            ),
            ("tags", json!([" 小説 ", "", "小説", "研究/SiC"])),
        ],
    );
    assert_eq!(column(&fx.conn, &id, "title"), json!("吾輩は猫である"));
    assert_eq!(column(&fx.conn, &id, "isbn13"), json!(ISBN));
    assert_eq!(column(&fx.conn, &id, "c_code"), json!("0193"));
    assert_eq!(column(&fx.conn, &id, "published_on"), json!("2002-10"));
    assert_eq!(column(&fx.conn, &id, "publisher"), json!(null));
    let d = get_book_detail(&fx.conn, &id).unwrap();
    assert_eq!(d.authors.len(), 1);
    assert_eq!(d.authors[0].name, "夏目漱石");
    assert_eq!(d.authors[0].kana, None);
    assert_eq!(d.authors[0].role.as_deref(), Some("著"));
    assert_eq!(d.tags, ["小説", "研究/SiC"]);
}

#[test]
fn create_books_is_all_or_nothing() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let ok = |t: &str| vec![FieldChange::manual("title", json!(t))];
    let ids = library::create_books(&mut fx.conn, &e, &[ok("A"), ok("B"), ok("C")]).unwrap();
    assert_eq!(ids.len(), 3);
    assert_eq!(books_count(&fx), 3);
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "UUIDv7 は作成順に並ぶ");

    let bad = vec![FieldChange::manual("rating", json!(9))];
    let r = library::create_books(&mut fx.conn, &e, &[ok("D"), bad, ok("E")]);
    assert!(matches!(r, Err(Error::Validation { .. })));
    assert_eq!(books_count(&fx), 3, "1 冊でも失敗したら全体を取り消す");
    assert_eq!(outbox_len(&fx.conn), 3);
}

// ---------- 由来（origin）の規則 ----------

#[test]
fn at16_a_manually_fixed_title_survives_a_refetch_unless_overridden() {
    let mut fx = Fx::new();
    let id = library::create_book(
        &mut fx.conn,
        &env(fx.clock),
        &[
            provider("title", json!("おにぎりレシピ１０１"), "openbd"),
            provider("publisher", json!("ポット出版"), "openbd"),
        ],
    )
    .unwrap();
    assert_eq!(sources(&fx.conn, &id)["title"], json!("openbd"));

    // 手で直す
    let r = update(
        &mut fx,
        &id,
        &[FieldChange::manual("title", json!("おにぎりレシピ101"))],
        false,
    );
    assert_eq!(r.written, ["title"]);
    assert_eq!(column(&fx.conn, &id, "title"), json!("おにぎりレシピ101"));
    assert_eq!(sources(&fx.conn, &id)["title"], json!("manual"));
    assert_eq!(manual_fields(&fx.conn, &id), json!(["title"]));
    let manual_op = last_op(&fx.conn);
    assert_eq!(manual_op.set["title"], json!("おにぎりレシピ101"));
    assert_eq!(manual_op.set["field_sources.title"], json!("manual"));
    assert_eq!(manual_op.set["manual_fields.title"], json!(true));

    // 再取得（openBD の値）では書名を上書きしない。ほかの項目は更新される
    let ops_before = outbox_len(&fx.conn);
    let r = update(
        &mut fx,
        &id,
        &[
            provider("title", json!("おにぎりレシピ１０１（再取得）"), "openbd"),
            provider("publisher", json!("ポット出版 改"), "openbd"),
        ],
        false,
    );
    assert_eq!(r.skipped_manual, ["title"]);
    assert_eq!(r.written, ["publisher"]);
    assert_eq!(column(&fx.conn, &id, "title"), json!("おにぎりレシピ101"));
    assert_eq!(column(&fx.conn, &id, "publisher"), json!("ポット出版 改"));
    assert_eq!(sources(&fx.conn, &id)["title"], json!("manual"));
    assert_eq!(manual_fields(&fx.conn, &id), json!(["title"]));
    assert_eq!(outbox_len(&fx.conn), ops_before + 1);
    let refetch_op = last_op(&fx.conn);
    assert!(
        !refetch_op.set.contains_key("title"),
        "書名は Op にも入らない"
    );
    assert!(!refetch_op.set.keys().any(|k| k.ends_with(".title")));

    // 書名だけの再取得は、何も書かない（Op も増えない）
    let ops_before = outbox_len(&fx.conn);
    let r = update(
        &mut fx,
        &id,
        &[provider("title", json!("NDL の書名"), "ndl")],
        false,
    );
    assert_eq!(r.skipped_manual, ["title"]);
    assert!(r.written.is_empty());
    assert_eq!(outbox_len(&fx.conn), ops_before);

    // 上書きを指定したときだけ書き、手修正の記録を外す
    let r = update(
        &mut fx,
        &id,
        &[provider("title", json!("NDL の書名"), "ndl")],
        true,
    );
    assert!(r.skipped_manual.is_empty());
    assert_eq!(r.written, ["title"]);
    assert_eq!(column(&fx.conn, &id, "title"), json!("NDL の書名"));
    assert_eq!(sources(&fx.conn, &id)["title"], json!("ndl"));
    assert_eq!(manual_fields(&fx.conn, &id), json!([]));
    let over = last_op(&fx.conn);
    assert_eq!(over.set["field_sources.title"], json!("ndl"));
    assert_eq!(over.set["manual_fields.title"], json!(null));

    // 手修正の記録が外れたので、次の再取得は書ける
    let r = update(
        &mut fx,
        &id,
        &[provider("title", json!("Google の書名"), "google")],
        false,
    );
    assert_eq!(r.written, ["title"]);
    assert_eq!(column(&fx.conn, &id, "title"), json!("Google の書名"));
}

#[test]
fn barcode_and_import_origins_are_protected_by_manual_edits_too() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("list_price", json!(800))]);
    let r = update(
        &mut fx,
        &id,
        &[FieldChange::barcode("list_price", json!(700))],
        false,
    );
    assert_eq!(r.skipped_manual, ["list_price"]);
    assert_eq!(column(&fx.conn, &id, "list_price"), json!(800));
    let r = update(
        &mut fx,
        &id,
        &[FieldChange::import("list_price", json!(650))],
        false,
    );
    assert_eq!(r.skipped_manual, ["list_price"]);

    let r = update(
        &mut fx,
        &id,
        &[FieldChange::barcode("list_price", json!(700))],
        true,
    );
    assert_eq!(r.written, ["list_price"]);
    assert_eq!(column(&fx.conn, &id, "list_price"), json!(700));
    assert_eq!(sources(&fx.conn, &id)["list_price"], json!("barcode"));
    assert_eq!(manual_fields(&fx.conn, &id), json!([]));
}

#[test]
fn provider_writes_record_sources_and_skip_unchanged_values_and_nulls() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("memo", json!("m"))]);
    let r = update(
        &mut fx,
        &id,
        &[
            provider("title", json!("T"), "openbd"),
            provider("pages", json!(300), "ndl"),
        ],
        false,
    );
    assert_eq!(r.written, ["pages", "title"]);
    assert_eq!(
        sources(&fx.conn, &id),
        json!({"pages": "ndl", "title": "openbd"})
    );
    assert_eq!(manual_fields(&fx.conn, &id), json!([]));

    // 同じ値の再取得では、取得元を書き換えず Op も作らない
    let ops_before = outbox_len(&fx.conn);
    let r = update(
        &mut fx,
        &id,
        &[provider("title", json!("T"), "google")],
        false,
    );
    assert!(r.written.is_empty());
    assert_eq!(outbox_len(&fx.conn), ops_before);
    assert_eq!(sources(&fx.conn, &id)["title"], json!("openbd"));

    // 値が違えば、新しい取得元に変わる
    update(
        &mut fx,
        &id,
        &[provider("title", json!("T2"), "google")],
        false,
    );
    assert_eq!(sources(&fx.conn, &id)["title"], json!("google"));

    // 取得できなかった項目（null）は、既にある値を消さない
    let ops_before = outbox_len(&fx.conn);
    let r = update(
        &mut fx,
        &id,
        &[provider("pages", json!(null), "ndl")],
        false,
    );
    assert!(r.written.is_empty());
    assert_eq!(column(&fx.conn, &id, "pages"), json!(300));
    assert_eq!(outbox_len(&fx.conn), ops_before);

    // 空の値（null・空文字・空の配列）も「取得できなかった」ので、既にある値を消さない
    let id3 = add(
        &mut fx,
        &[
            ("title", json!("既存の書名")),
            ("authors", json!([{"name": "既存の著者"}])),
        ],
    );
    let ops_before = outbox_len(&fx.conn);
    let r = update(
        &mut fx,
        &id3,
        &[
            provider("title", json!(null), "openbd"),
            provider("authors", json!(null), "openbd"),
            provider("publisher", json!(""), "openbd"),
        ],
        true,
    );
    assert!(r.written.is_empty() && r.skipped_manual.is_empty());
    update(
        &mut fx,
        &id3,
        &[
            provider("title", json!("  "), "ndl"),
            provider("authors", json!([]), "ndl"),
        ],
        true,
    );
    assert_eq!(column(&fx.conn, &id3, "title"), json!("既存の書名"));
    assert_eq!(get_book_detail(&fx.conn, &id3).unwrap().authors.len(), 1);
    assert_eq!(outbox_len(&fx.conn), ops_before);
    // 作成でも、空の値には取得元を記録しない
    let id4 = library::create_book(
        &mut fx.conn,
        &env(fx.clock),
        &[
            provider("title", json!(""), "openbd"),
            provider("authors", json!([]), "openbd"),
            provider("publisher", json!("P"), "openbd"),
        ],
    )
    .unwrap();
    assert_eq!(sources(&fx.conn, &id4), json!({"publisher": "openbd"}));

    // 手入力の null は消す（手で空にした）。以後の再取得では埋めない
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("pages", json!(null))],
        false,
    );
    assert_eq!(column(&fx.conn, &id, "pages"), json!(null));
    assert_eq!(manual_fields(&fx.conn, &id), json!(["pages"]));
    let r = update(&mut fx, &id, &[provider("pages", json!(320), "ndl")], false);
    assert_eq!(r.skipped_manual, ["pages"]);
}

/// `clear_stale` つきの更新。
fn update_clearing(
    fx: &mut Fx,
    id: &str,
    changes: &[FieldChange],
    override_manual: bool,
) -> library::ChangeReport {
    library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        id,
        changes,
        UpdateOptions {
            override_manual,
            clear_stale: true,
        },
    )
    .unwrap()
}

#[test]
fn clear_stale_removes_only_a_value_that_the_same_provider_no_longer_returns() {
    let mut fx = Fx::new();
    let id = add_titled(&mut fx, "T");
    update(
        &mut fx,
        &id,
        &[
            provider("volume", json!("v. 2 : pbk."), "ndl"),
            provider("pages", json!(300), "ndl"),
            provider("publisher", json!("P"), "openbd"),
            provider("authors", json!([{"name": "A"}]), "ndl"),
            FieldChange::barcode("list_price", json!(700)),
        ],
        false,
    );
    assert_eq!(column(&fx.conn, &id, "volume_sort"), json!(2.0));
    assert_eq!(sources(&fx.conn, &id)["volume_sort"], json!("ndl"));

    // `clear_stale` がなければ、空の値は何もしない（これまでどおり）
    let ops = outbox_len(&fx.conn);
    let r = update(
        &mut fx,
        &id,
        &[
            provider("volume", json!(null), "ndl"),
            provider("pages", json!(null), "ndl"),
        ],
        false,
    );
    assert!(r.written.is_empty());
    assert_eq!(outbox_len(&fx.conn), ops);

    // 別の取得元の空の値、バーコード由来の項目、手修正の項目は消さない
    let manual_id = add(&mut fx, &[("volume", json!("3")), ("pages", json!(100))]);
    let r = update_clearing(
        &mut fx,
        &id,
        &[
            provider("pages", json!(null), "google"),
            provider("publisher", json!(""), "ndl"),
            provider("list_price", json!(null), "openbd"),
        ],
        false,
    );
    assert!(r.written.is_empty(), "{r:?}");
    assert_eq!(column(&fx.conn, &id, "pages"), json!(300));
    assert_eq!(column(&fx.conn, &id, "list_price"), json!(700));
    for override_manual in [false, true] {
        let ops = outbox_len(&fx.conn);
        let r = update_clearing(
            &mut fx,
            &manual_id,
            &[
                provider("volume", json!(null), "ndl"),
                provider("pages", json!(null), "ndl"),
            ],
            override_manual,
        );
        assert!(r.written.is_empty(), "{r:?}");
        assert!(r.skipped_manual.is_empty(), "{r:?}");
        assert_eq!(column(&fx.conn, &manual_id, "volume"), json!("3"));
        assert_eq!(column(&fx.conn, &manual_id, "volume_sort"), json!(3.0));
        assert_eq!(column(&fx.conn, &manual_id, "pages"), json!(100));
        assert_eq!(
            sources(&fx.conn, &manual_id)["volume"],
            json!("manual"),
            "override_manual = {override_manual}"
        );
        assert_eq!(outbox_len(&fx.conn), ops);
    }

    // 同じ取得元の空の値は、値と取得元の記録を消す。巻次なら、並べ替え用の数値も消える。Op は 1 つ
    let ops = outbox_len(&fx.conn);
    let r = update_clearing(
        &mut fx,
        &id,
        &[
            provider("volume", json!(null), "ndl"),
            provider("authors", json!([]), "ndl"),
            provider("pages", json!(null), "ndl"),
            // 同じ更新の、空でない値は、これまでどおり書く
            provider("publisher", json!("P2"), "openbd"),
        ],
        false,
    );
    assert_eq!(
        r.written,
        ["authors", "pages", "publisher", "volume", "volume_sort"]
    );
    assert_eq!(outbox_len(&fx.conn), ops + 1);
    assert_eq!(column(&fx.conn, &id, "volume"), json!(null));
    assert_eq!(column(&fx.conn, &id, "volume_sort"), json!(null));
    assert_eq!(column(&fx.conn, &id, "pages"), json!(null));
    assert!(get_book_detail(&fx.conn, &id).unwrap().authors.is_empty());
    assert_eq!(column(&fx.conn, &id, "publisher"), json!("P2"));
    assert_eq!(
        sources(&fx.conn, &id),
        json!({"title": "manual", "publisher": "openbd", "list_price": "barcode"})
    );

    // 値がもうなければ、何も書かない（記録も増えない）
    let ops = outbox_len(&fx.conn);
    let r = update_clearing(
        &mut fx,
        &id,
        &[
            provider("volume", json!(null), "ndl"),
            provider("pages", json!(null), "ndl"),
        ],
        false,
    );
    assert!(r.written.is_empty());
    assert_eq!(outbox_len(&fx.conn), ops);
}

#[test]
fn clear_stale_clears_the_sort_value_with_the_volume_even_if_another_source_wrote_it() {
    let mut fx = Fx::new();
    let id = add_titled(&mut fx, "T");
    update(
        &mut fx,
        &id,
        &[provider("volume", json!("巻二"), "ndl")],
        false,
    );
    // 数値だけ別の取得元が書いた状態（取得元の記録が巻次と違う）
    update(
        &mut fx,
        &id,
        &[provider("volume_sort", json!(2.5), "other")],
        false,
    );
    assert_eq!(column(&fx.conn, &id, "volume_sort"), json!(2.5));
    assert_eq!(sources(&fx.conn, &id)["volume_sort"], json!("other"));
    let r = update_clearing(
        &mut fx,
        &id,
        &[provider("volume", json!(null), "ndl")],
        false,
    );
    assert_eq!(r.written, ["volume", "volume_sort"]);
    assert_eq!(column(&fx.conn, &id, "volume_sort"), json!(null));
    assert!(sources(&fx.conn, &id).get("volume_sort").is_none());
}

#[test]
fn only_bibliographic_fields_get_provenance_records() {
    let mut fx = Fx::new();
    let id = add_titled(&mut fx, "T");
    update(
        &mut fx,
        &id,
        &[
            FieldChange::manual("rating", json!(4)),
            FieldChange::manual("memo", json!("m")),
            FieldChange::manual("tags", json!(["a"])),
            FieldChange::manual("purchase_store", json!("紀伊國屋")),
            // 書誌の項目以外は、取得元の変更でも値だけを書く（手修正の保護もない）
            FieldChange::provider("review", json!("r"), "openbd"),
        ],
        false,
    );
    let op = last_op(&fx.conn);
    assert!(
        !op.set
            .keys()
            .any(|k| k.starts_with("field_sources.") || k.starts_with("manual_fields.")),
        "{:?}",
        op.set.keys().collect::<Vec<_>>()
    );
    assert_eq!(sources(&fx.conn, &id), json!({"title": "manual"}));
    assert_eq!(manual_fields(&fx.conn, &id), json!(["title"]));
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("rating", json!(5))],
        false,
    );
    let r = update(
        &mut fx,
        &id,
        &[FieldChange::provider("rating", json!(1), "x")],
        false,
    );
    assert_eq!(r.written, ["rating"]);
}

#[test]
fn unchanged_manual_values_are_not_recorded() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("title", json!("T")), ("rating", json!(3))]);
    let ops_before = outbox_len(&fx.conn);
    // フォームが変えていない項目も送ってきても、何も記録しない
    let r = update(
        &mut fx,
        &id,
        &[
            FieldChange::manual("title", json!("T")),
            FieldChange::manual("rating", json!(3)),
            FieldChange::manual("memo", json!(null)),
        ],
        false,
    );
    assert!(r.written.is_empty());
    assert_eq!(outbox_len(&fx.conn), ops_before);
    // 数値は値で比べる
    let id2 = add(&mut fx, &[("volume_sort", json!(2.0))]);
    let ops_before = outbox_len(&fx.conn);
    update(
        &mut fx,
        &id2,
        &[FieldChange::manual("volume_sort", json!(2))],
        false,
    );
    assert_eq!(outbox_len(&fx.conn), ops_before);
}

#[test]
fn updating_missing_or_purged_books_fails() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let missing = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55";
    let r = library::update_book(
        &mut fx.conn,
        &e,
        missing,
        &[FieldChange::manual("title", json!("x"))],
        UpdateOptions::default(),
    );
    assert!(matches!(r, Err(Error::NotFound { .. })), "{r:?}");

    let id = add_titled(&mut fx, "T");
    library::purge_books(&mut fx.conn, &e, &[id.clone()]).unwrap();
    let r = library::update_book(
        &mut fx.conn,
        &e,
        &id,
        &[FieldChange::manual("title", json!("x"))],
        UpdateOptions::default(),
    );
    assert!(matches!(r, Err(Error::Conflict(_))), "{r:?}");
}

#[test]
fn reading_status_and_ownership_rules_fill_dates() {
    let mut fx = Fx::new();
    let id = add_titled(&mut fx, "T");

    // 読了にしたとき、読了日が空なら今日（FR-READ-02）
    let r = update(
        &mut fx,
        &id,
        &[FieldChange::manual("read_status", json!("read"))],
        false,
    );
    assert_eq!(r.written, ["read_finished_on", "read_status"]);
    assert_eq!(column(&fx.conn, &id, "read_finished_on"), json!(TODAY));

    // 読了日を直したあと、読了のまま別の項目を変えても読了日はそのまま
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("read_finished_on", json!("2025-01-02"))],
        false,
    );
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("rating", json!(4))],
        false,
    );
    assert_eq!(
        column(&fx.conn, &id, "read_finished_on"),
        json!("2025-01-02")
    );
    // 読了日を空にしても、状態が変わらなければ自動入力しない
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("read_finished_on", json!(null))],
        false,
    );
    assert_eq!(column(&fx.conn, &id, "read_finished_on"), json!(null));
    // 読書中に戻して、また読了にすると今日が入る
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("read_status", json!("reading"))],
        false,
    );
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("read_status", json!("read"))],
        false,
    );
    assert_eq!(column(&fx.conn, &id, "read_finished_on"), json!(TODAY));
    // 同じ変更で読了日を指定したときは、その日付
    update(
        &mut fx,
        &id,
        &[FieldChange::manual("read_status", json!("paused"))],
        false,
    );
    update(
        &mut fx,
        &id,
        &[
            FieldChange::manual("read_status", json!("read")),
            FieldChange::manual("read_finished_on", json!("2024-12-31")),
        ],
        false,
    );
    assert_eq!(
        column(&fx.conn, &id, "read_finished_on"),
        json!("2024-12-31")
    );

    // 欲しい本を所有に移すとき、購入日が空なら今日（FR-SCAN-18）
    let wish = add(
        &mut fx,
        &[
            ("title", json!("W")),
            ("ownership", json!("wanted")),
            ("wish_priority", json!("high")),
        ],
    );
    assert_eq!(column(&fx.conn, &wish, "purchased_on"), json!(null));
    let r = update(
        &mut fx,
        &wish,
        &[FieldChange::manual("ownership", json!("owned"))],
        false,
    );
    assert_eq!(r.written, ["ownership", "purchased_on"]);
    assert_eq!(column(&fx.conn, &wish, "purchased_on"), json!(TODAY));
    // 手放した本を所有に戻しても購入日は触らない
    update(
        &mut fx,
        &wish,
        &[FieldChange::manual("ownership", json!("released"))],
        false,
    );
    update(
        &mut fx,
        &wish,
        &[FieldChange::manual("purchased_on", json!(null))],
        false,
    );
    update(
        &mut fx,
        &wish,
        &[FieldChange::manual("ownership", json!("owned"))],
        false,
    );
    assert_eq!(column(&fx.conn, &wish, "purchased_on"), json!(null));
    // 購入日を指定して移すときは、その日付
    let wish2 = add(
        &mut fx,
        &[("title", json!("W2")), ("ownership", json!("wanted"))],
    );
    update(
        &mut fx,
        &wish2,
        &[
            FieldChange::manual("ownership", json!("owned")),
            FieldChange::manual("purchased_on", json!("2026-09-01")),
        ],
        false,
    );
    assert_eq!(
        column(&fx.conn, &wish2, "purchased_on"),
        json!("2026-09-01")
    );
}

#[test]
fn today_defaults_to_the_local_date_of_the_clock() {
    let fx = Fx::new();
    let today = library::Env::new(fx.clock).today();
    use chrono::TimeZone;
    let expected = chrono::Local
        .timestamp_millis_opt(T0)
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(today, expected);
    assert_eq!(env(fx.clock).today(), TODAY);
}

// ---------- 一括変更 ----------

#[test]
fn bulk_update_applies_the_same_change_to_every_book_in_one_transaction() {
    let mut fx = Fx::new();
    let ids: Vec<String> = ["A", "B", "C"]
        .iter()
        .map(|t| add_titled(&mut fx, t))
        .collect();
    let shelf = library::create_location(&mut fx.conn, &env(fx.clock), "自宅", None).unwrap();
    let ops_before = outbox_len(&fx.conn);

    let reports = library::update_books(
        &mut fx.conn,
        &env(fx.clock),
        &[
            ids[0].clone(),
            ids[1].clone(),
            ids[0].clone(),
            ids[2].clone(),
        ],
        &[
            FieldChange::manual("location_id", json!(shelf)),
            FieldChange::manual("read_status", json!("read")),
        ],
        UpdateOptions::default(),
    )
    .unwrap();
    assert_eq!(reports.len(), 3, "重複した ID は 1 回だけ");
    assert_eq!(outbox_len(&fx.conn), ops_before + 3);
    for id in &ids {
        assert_eq!(column(&fx.conn, id, "location_id"), json!(shelf));
        assert_eq!(column(&fx.conn, id, "read_status"), json!("read"));
        assert_eq!(column(&fx.conn, id, "read_finished_on"), json!(TODAY));
    }
}

#[test]
fn bulk_update_is_atomic() {
    let mut fx = Fx::new();
    let a = add_titled(&mut fx, "A");
    let b = add_titled(&mut fx, "B");
    let missing = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned();
    let ops_before = outbox_len(&fx.conn);
    let r = library::update_books(
        &mut fx.conn,
        &env(fx.clock),
        &[a.clone(), missing, b.clone()],
        &[FieldChange::manual("rating", json!(5))],
        UpdateOptions::default(),
    );
    assert!(matches!(r, Err(Error::NotFound { .. })), "{r:?}");
    assert_eq!(
        column(&fx.conn, &a, "rating"),
        json!(null),
        "1 冊目も取り消される"
    );
    assert_eq!(outbox_len(&fx.conn), ops_before);

    // 検証に失敗したときも同じ
    let r = library::update_books(
        &mut fx.conn,
        &env(fx.clock),
        &[a.clone(), b.clone()],
        &[FieldChange::manual("rating", json!(7))],
        UpdateOptions::default(),
    );
    assert!(matches!(r, Err(Error::Validation { .. })));
    assert_eq!(outbox_len(&fx.conn), ops_before);
}

// ---------- ゴミ箱・完全削除 ----------

#[test]
fn trash_restore_and_purge() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = library::create_book(
        &mut fx.conn,
        &e,
        &[
            provider("title", json!("消える本"), "openbd"),
            FieldChange::manual("publisher", json!("出版社")),
            FieldChange::manual("tags", json!(["x"])),
            FieldChange::manual("memo", json!("メモ")),
            FieldChange::manual("rating", json!(4)),
            FieldChange::manual("ownership", json!("released")),
        ],
    )
    .unwrap();
    let b = add_titled(&mut fx, "残る本");
    let created_at = column(&fx.conn, &a, "created_at");

    // ゴミ箱へ
    let ops_before = outbox_len(&fx.conn);
    assert_eq!(
        library::trash_books(&mut fx.conn, &e, &[a.clone()]).unwrap(),
        [a.clone()]
    );
    assert_eq!(column(&fx.conn, &a, "deleted"), json!(1));
    assert_eq!(outbox_len(&fx.conn), ops_before + 1);
    // 既にゴミ箱にある本は何もしない
    assert!(
        library::trash_books(&mut fx.conn, &e, &[a.clone()])
            .unwrap()
            .is_empty()
    );
    assert_eq!(outbox_len(&fx.conn), ops_before + 1);
    // ほかの項目を編集しても、ゴミ箱のまま（deleted は別の項目）
    update(
        &mut fx,
        &a,
        &[FieldChange::manual("memo", json!("編集"))],
        false,
    );
    assert_eq!(column(&fx.conn, &a, "deleted"), json!(1));
    // 復元
    assert_eq!(
        library::restore_books(&mut fx.conn, &e, &[a.clone()]).unwrap(),
        [a.clone()]
    );
    assert_eq!(column(&fx.conn, &a, "deleted"), json!(0));
    assert!(
        library::restore_books(&mut fx.conn, &e, &[a.clone()])
            .unwrap()
            .is_empty()
    );

    // 完全削除
    let changed = library::purge_books(&mut fx.conn, &e, &[a.clone(), b.clone()]).unwrap();
    assert_eq!(changed, [a.clone(), b.clone()]);
    let d = get_book_detail(&fx.conn, &a).unwrap();
    assert!(d.purged && d.deleted, "purged と deleted が真");
    assert_eq!(d.title, "");
    assert_eq!(d.publisher, None);
    assert_eq!(d.memo, None);
    assert_eq!(d.rating, None);
    assert!(d.tags.is_empty() && d.authors.is_empty());
    assert_eq!(d.ownership, "owned", "既定値に戻る");
    assert_eq!(
        d.created_at,
        created_at.as_str().unwrap(),
        "created_at は残す"
    );
    assert!(d.field_sources.is_empty(), "取得元の記録も消える");
    assert!(d.manual_fields.is_empty());
    let op = last_op(&fx.conn);
    assert_eq!(op.id, b);
    let purge_op = ops_for(&fx.conn, Entity::Book, &a).pop().unwrap();
    assert_eq!(purge_op.set["purged"], json!(true));
    assert_eq!(purge_op.set["deleted"], json!(true));
    assert_eq!(purge_op.set["title"], json!(""));
    assert_eq!(purge_op.set["publisher"], json!(null));
    assert_eq!(purge_op.set["field_sources.title"], json!(null));
    assert_eq!(purge_op.set["manual_fields.publisher"], json!(null));
    assert!(!purge_op.set.contains_key("created_at"));

    // 完全削除した本は一覧に出ないが、行は残る
    let rows = list_rows(&fx.conn).unwrap();
    assert!(rows.is_empty());
    assert_eq!(books_count(&fx), 2);
    // もう一度は何もしない。戻せない。ゴミ箱にも入れない（飛ばす）
    assert!(
        library::purge_books(&mut fx.conn, &e, &[a.clone()])
            .unwrap()
            .is_empty()
    );
    assert!(
        library::trash_books(&mut fx.conn, &e, &[a.clone()])
            .unwrap()
            .is_empty()
    );
    let r = library::restore_books(&mut fx.conn, &e, &[a.clone()]);
    assert!(matches!(r, Err(Error::Conflict(_))), "{r:?}");
    // 存在しない ID は NotFound
    let r = library::trash_books(
        &mut fx.conn,
        &e,
        &["0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned()],
    );
    assert!(matches!(r, Err(Error::NotFound { .. })));
}

#[test]
fn ops_recorded_locally_replay_to_another_database_with_the_same_result() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let shelf = library::create_location(&mut fx.conn, &e, "自宅", None).unwrap();
    let series = library::find_or_create_series(&mut fx.conn, &e, "岩波文庫").unwrap();
    let a = library::create_book(
        &mut fx.conn,
        &e,
        &[
            provider("title", json!("A"), "openbd"),
            FieldChange::barcode("isbn13", json!(ISBN)),
            FieldChange::manual("series_id", json!(series)),
            FieldChange::manual("location_id", json!(shelf)),
            FieldChange::manual("tags", json!(["x", "y"])),
        ],
    )
    .unwrap();
    update(
        &mut fx,
        &a,
        &[FieldChange::manual("title", json!("A 改"))],
        false,
    );
    update(
        &mut fx,
        &a,
        &[provider("title", json!("無視される"), "ndl")],
        false,
    );
    update(
        &mut fx,
        &a,
        &[provider("title", json!("上書き"), "ndl")],
        true,
    );
    update(
        &mut fx,
        &a,
        &[FieldChange::manual("read_status", json!("read"))],
        false,
    );
    let b = add_titled(&mut fx, "B");
    library::trash_books(&mut fx.conn, &e, &[b.clone()]).unwrap();
    library::purge_books(&mut fx.conn, &e, &[b.clone()]).unwrap();
    library::rename_tag(&mut fx.conn, &e, "x", "z").unwrap();

    let mut other = common::mem_db();
    for op in pending_ops(&fx.conn).unwrap() {
        common::apply(&mut other, &op);
    }
    assert_eq!(
        dump_canonical(&fx.conn).unwrap(),
        dump_canonical(&other).unwrap()
    );
    // 派生データも同じ検索結果になる
    for q in ["上書き", "岩波", "z", "9784003101117"] {
        assert_eq!(
            bibcontainer_core::search_books(&fx.conn, q).unwrap(),
            bibcontainer_core::search_books(&other, q).unwrap(),
            "{q}"
        );
    }
    assert_eq!(list_rows(&fx.conn).unwrap(), list_rows(&other).unwrap());
}

// ---------- 重複チェック・候補 ----------

#[test]
fn find_by_isbn_reports_ownership_location_path_and_trash() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let home = library::create_location(&mut fx.conn, &e, "自宅", None).unwrap();
    let study = library::create_location(&mut fx.conn, &e, "書斎の本棚", Some(&home)).unwrap();
    let tier3 = library::create_location(&mut fx.conn, &e, "3段目", Some(&study)).unwrap();

    let owned = add(
        &mut fx,
        &[
            ("title", json!("所有本")),
            ("isbn13", json!(ISBN)),
            ("location_id", json!(tier3)),
        ],
    );
    let wanted = add(
        &mut fx,
        &[
            ("title", json!("欲しい本")),
            ("isbn13", json!(ISBN)),
            ("ownership", json!("wanted")),
        ],
    );
    let trashed = add(
        &mut fx,
        &[("title", json!("ゴミ箱")), ("isbn13", json!(ISBN))],
    );
    library::trash_books(&mut fx.conn, &e, &[trashed.clone()]).unwrap();
    add(
        &mut fx,
        &[
            ("title", json!("別の本")),
            ("isbn13", json!("9784873119786")),
        ],
    );

    let hits = find_by_isbn(&fx.conn, ISBN).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.id.clone()).collect::<Vec<_>>(),
        [owned.clone(), wanted.clone(), trashed.clone()],
        "ゴミ箱でない本が先、所有・欲しい本の順"
    );
    assert_eq!(hits[0].title, "所有本");
    assert_eq!(hits[0].ownership, "owned");
    assert_eq!(hits[0].location_id.as_deref(), Some(tier3.as_str()));
    assert_eq!(
        hits[0].location_path.as_deref(),
        Some("自宅 > 書斎の本棚 > 3段目")
    );
    assert!(!hits[0].deleted);
    assert_eq!(hits[1].ownership, "wanted");
    assert_eq!(hits[1].location_path, None);
    assert!(hits[2].deleted);

    // ハイフン・ISBN-10 でも探せる。見つからなければ空
    assert_eq!(
        find_by_isbn(&fx.conn, "978-4-00-310111-7").unwrap().len(),
        3
    );
    assert_eq!(find_by_isbn(&fx.conn, "4003101111").unwrap().len(), 3);
    assert!(find_by_isbn(&fx.conn, "9780000000002").unwrap().is_empty());

    // 完全削除した本は出ない
    library::purge_books(&mut fx.conn, &e, &[trashed]).unwrap();
    assert_eq!(find_by_isbn(&fx.conn, ISBN).unwrap().len(), 2);
}

#[test]
fn purchase_store_suggestions_are_ordered_by_use() {
    let mut fx = Fx::new();
    for (i, store) in [
        "紀伊國屋",
        "Amazon",
        "紀伊國屋",
        "ブックオフ",
        "Amazon",
        "紀伊國屋",
    ]
    .iter()
    .enumerate()
    {
        add(
            &mut fx,
            &[
                ("title", json!(format!("本{i}"))),
                ("purchase_store", json!(store)),
            ],
        );
    }
    add_titled(&mut fx, "購入店なし");
    let s = purchase_store_suggestions(&fx.conn).unwrap();
    assert_eq!(
        s.iter()
            .map(|x| (x.name.as_str(), x.count))
            .collect::<Vec<_>>(),
        [("紀伊國屋", 3), ("Amazon", 2), ("ブックオフ", 1)]
    );
}

#[test]
fn pending_book_ids_lists_only_pending_books_that_are_not_trashed() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let p1 = add(
        &mut fx,
        &[("isbn13", json!(ISBN)), ("meta_status", json!("pending"))],
    );
    let p2 = add(
        &mut fx,
        &[
            ("isbn13", json!("9784873119786")),
            ("meta_status", json!("pending")),
        ],
    );
    add(
        &mut fx,
        &[("title", json!("完了")), ("meta_status", json!("ok"))],
    );
    add(
        &mut fx,
        &[("title", json!("一部")), ("meta_status", json!("partial"))],
    );
    let p3 = add(&mut fx, &[("meta_status", json!("pending"))]);
    assert_eq!(
        pending_book_ids(&fx.conn).unwrap(),
        [p1.clone(), p2.clone(), p3.clone()]
    );
    library::trash_books(&mut fx.conn, &e, &[p2.clone()]).unwrap();
    assert_eq!(
        pending_book_ids(&fx.conn).unwrap(),
        [p1.clone(), p3.clone()]
    );
    // 取得できたら外れる
    update(
        &mut fx,
        &p1,
        &[FieldChange::manual("meta_status", json!("ok"))],
        false,
    );
    assert_eq!(pending_book_ids(&fx.conn).unwrap(), [p3]);
}

// ---------- 一覧の行と詳細 ----------

#[test]
fn list_rows_have_the_documented_shape_and_sort_keys() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let series = library::find_or_create_series(&mut fx.conn, &e, "岩波文庫").unwrap();
    let a = add(
        &mut fx,
        &[
            ("title", json!("吾輩は猫である")),
            ("title_kana", json!("ワガハイハネコデアル")),
            ("isbn13", json!(ISBN)),
            ("series_id", json!(series)),
            ("volume", json!("上")),
            ("volume_sort", json!(1)),
            (
                "authors",
                json!([{"name": "夏目漱石", "kana": "ナツメソウセキ", "role": "著"}, {"name": "別人"}]),
            ),
            ("publisher", json!("岩波書店")),
            ("published_on", json!("1990-01")),
            ("c_code", json!("0193")),
            ("list_price", json!(700)),
            ("pages", json!(500)),
            ("tags", json!(["小説"])),
            ("rating", json!(5)),
            ("purchase_store", json!("紀伊國屋")),
            ("purchase_price", json!(770)),
            ("read_status", json!("read")),
        ],
    );
    let b = add(
        &mut fx,
        &[
            ("title", json!("Ｚｅｔａ 入門")),
            ("authors", json!([{"name": "山田太郎"}])),
        ],
    );
    let c = add_titled(&mut fx, "ｱｲｳ");
    library::trash_books(&mut fx.conn, &e, &[b.clone()]).unwrap();
    let purged = add_titled(&mut fx, "消す");
    library::purge_books(&mut fx.conn, &e, &[purged]).unwrap();

    let rows = list_rows(&fx.conn).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        [a.clone(), b.clone(), c.clone()],
        "完全削除は除き、ゴミ箱は含める。登録順"
    );

    // IPC の形（0005 §4）と一致するキー
    let json_row = serde_json::to_value(&rows[0]).unwrap();
    let keys: BTreeSet<&str> = json_row
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let spec: BTreeSet<&str> = [
        "id",
        "isbn13",
        "title",
        "titleKana",
        "subtitle",
        "volume",
        "volumeSort",
        "seriesId",
        "seriesName",
        "workId",
        "authors",
        "publisher",
        "publishedOn",
        "cCode",
        "ndc",
        "listPrice",
        "pages",
        "ownership",
        "wishPriority",
        "locationId",
        "tags",
        "readStatus",
        "readFinishedOn",
        "rating",
        "purchasedOn",
        "purchasePrice",
        "purchaseStore",
        "coverHash",
        "metaStatus",
        "deleted",
        "createdAt",
        "sortTitle",
        "sortAuthors",
    ]
    .into_iter()
    .collect();
    assert_eq!(keys, spec);

    let r = &rows[0];
    assert_eq!(r.title, "吾輩は猫である");
    assert_eq!(r.series_name.as_deref(), Some("岩波文庫"));
    assert_eq!(r.work_id, None, "作品の手動の指定は、既定では自動（null）");
    assert_eq!(r.volume_sort, Some(1.0));
    assert_eq!(r.authors.len(), 2);
    assert_eq!(r.authors[0].kana.as_deref(), Some("ナツメソウセキ"));
    assert_eq!(r.c_code.as_deref(), Some("0193"));
    assert_eq!(r.list_price, Some(700));
    assert_eq!(r.tags, ["小説"]);
    assert_eq!(r.read_status, "read");
    assert_eq!(r.read_finished_on.as_deref(), Some(TODAY));
    assert_eq!(r.meta_status, "ok");
    assert!(!r.deleted);
    assert_eq!(json_row["publishedOn"], json!("1990-01"));
    assert_eq!(json_row["authors"][0]["role"], json!("著"));
    // 並べ替えキー: ヨミがあればヨミ（正規化）、著者は先頭の著者のヨミ
    assert_eq!(r.sort_title, "わがはいはねこである");
    assert_eq!(r.sort_authors, "なつめそうせき");
    // ヨミが無ければ書名を正規化したもの。著者の名前だけならその名前
    assert_eq!(rows[1].sort_title, "zeta 入門");
    assert_eq!(rows[1].sort_authors, "山田太郎");
    assert!(rows[1].deleted);
    assert_eq!(rows[2].sort_title, "あいう");
    assert_eq!(rows[2].sort_authors, "", "著者なし");
}

#[test]
fn get_rows_returns_requested_rows_in_order_and_skips_missing_ones() {
    let mut fx = Fx::new();
    let ids: Vec<String> = (0..5)
        .map(|i| add_titled(&mut fx, &format!("本{i}")))
        .collect();
    let purged = ids[4].clone();
    library::purge_books(&mut fx.conn, &env(fx.clock), &[purged.clone()]).unwrap();

    let want = vec![
        ids[3].clone(),
        ids[0].clone(),
        "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned(),
        ids[3].clone(),
        purged,
    ];
    let rows = get_rows(&fx.conn, &want).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.title.clone()).collect::<Vec<_>>(),
        ["本3", "本0"],
        "指定した順、重複と存在しない ID・完全削除した ID は飛ばす"
    );
    assert!(get_rows(&fx.conn, &[]).unwrap().is_empty());

    // 件数が多くても取れる（500 件ずつに分けて問い合わせる）
    let many: Vec<String> = (0..1200)
        .map(|i| format!("id-{i}"))
        .chain(ids[..4].iter().cloned())
        .collect();
    assert_eq!(get_rows(&fx.conn, &many).unwrap().len(), 4);
}

#[test]
fn book_detail_has_every_field_provenance_and_extra() {
    let mut fx = Fx::new();
    let id = library::create_book(
        &mut fx.conn,
        &env(fx.clock),
        &[
            provider("title", json!("T"), "openbd"),
            FieldChange::manual("publisher", json!("P")),
            FieldChange::manual("description", json!("説明\n2 行目")),
            FieldChange::manual("review", json!("感想")),
            FieldChange::manual("purchase_cond", json!("used")),
            FieldChange::manual("wish_priority", json!("low")),
            FieldChange::manual("ref_price_incl", json!(880)),
            FieldChange::manual("size_text", json!("15cm")),
        ],
    )
    .unwrap();
    // 新しい版の端末から届いた未知の項目
    common::apply(
        &mut fx.conn,
        &common::remote_op(
            common::DEV_B,
            1_000,
            0,
            Entity::Book,
            &id,
            json!({"zz_future": {"a": 1}}),
        ),
    );

    let d = get_book_detail(&fx.conn, &id).unwrap();
    assert_eq!(d.title, "T");
    assert_eq!(
        d.field_sources.get("title").map(String::as_str),
        Some("openbd")
    );
    assert_eq!(
        d.field_sources.get("publisher").map(String::as_str),
        Some("manual")
    );
    // 手入力した書誌の項目（書名は取得元の値）。感想・購入状態などは対象外
    assert_eq!(
        d.manual_fields,
        ["description", "publisher", "ref_price_incl", "size_text"]
    );
    assert_eq!(d.extra, json!({"zz_future": {"a": 1}}));
    assert_eq!(d.description.as_deref(), Some("説明\n2 行目"));
    assert_eq!(d.purchase_cond.as_deref(), Some("used"));
    assert_eq!(d.wish_priority.as_deref(), Some("low"));
    assert_eq!(d.ref_price_incl, Some(880));
    assert!(!d.deleted && !d.purged);

    // 同期する全項目がキーとして出ている（IPC は camelCase）
    let json_detail = serde_json::to_value(&d).unwrap();
    let keys: BTreeSet<String> = json_detail.as_object().unwrap().keys().cloned().collect();
    for def in Entity::Book.fields() {
        let camel = snake_to_camel(def.name);
        assert!(keys.contains(&camel), "{camel} が BookDetail にない");
    }
    for extra in ["id", "fieldSources", "manualFields", "extra"] {
        assert!(keys.contains(extra), "{extra}");
    }
    assert_eq!(json_detail["fieldSources"]["title"], json!("openbd"));
    assert_eq!(json_detail["manualFields"], json!(d.manual_fields));

    let missing = get_book_detail(&fx.conn, "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55");
    assert!(matches!(missing, Err(Error::NotFound { .. })));
}

fn snake_to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn unknown_enum_values_from_other_devices_do_not_reach_the_screen() {
    let mut fx = Fx::new();
    let id = add_titled(&mut fx, "T");
    common::apply(
        &mut fx.conn,
        &common::remote_op(
            common::DEV_B,
            9_000_000_000_000,
            0,
            Entity::Book,
            &id,
            json!({"ownership": "stolen", "read_status": "skimmed", "meta_status": "??",
                   "wish_priority": "urgent", "authors": [{"name": ""}, "x", {"name": "ok"}]}),
        ),
    );
    let row = &list_rows(&fx.conn).unwrap()[0];
    assert_eq!(row.ownership, "owned");
    assert_eq!(row.read_status, "unread");
    assert_eq!(row.meta_status, "ok");
    assert_eq!(row.wish_priority, None);
    assert_eq!(row.authors.len(), 1);
    assert_eq!(row.authors[0].name, "ok");
}
