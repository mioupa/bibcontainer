//! Op の適用（apply_op）のテスト。

mod common;

use bibcontainer_core::{ApplyOutcome, Entity, Error, Op, apply_op, store::field_clock, write_tx};
use serde_json::{Value, json};

use common::*;

fn applied(n: usize) -> ApplyOutcome {
    ApplyOutcome {
        applied: n,
        stale: 0,
        invalid: 0,
    }
}

fn book(conn: &rusqlite::Connection) -> Value {
    dump_row(&dump(conn), "books", BOOK_1)
}

fn updated_at(conn: &rusqlite::Connection, table: &str, id: &str) -> String {
    conn.query_row(
        &format!("SELECT updated_at FROM {table} WHERE id = ?1"),
        [id],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn applying_the_same_op_twice_gives_the_same_state() {
    let op = book_op(
        DEV_A,
        1_000,
        json!({"title": "坊っちゃん", "rating": 4, "authors": [{"name": "夏目漱石"}], "deleted": false}),
    );
    let mut conn = mem_db();
    assert_eq!(apply(&mut conn, &op), applied(4));
    let once = dump(&conn);

    let second = apply(&mut conn, &op);
    assert_eq!(
        second,
        ApplyOutcome {
            applied: 0,
            stale: 4,
            invalid: 0
        }
    );
    assert!(!second.changed());
    assert_eq!(dump(&conn), once);

    let row = book(&conn);
    assert_eq!(row["title"], json!("坊っちゃん"));
    assert_eq!(row["rating"], json!(4));
    assert_eq!(row["authors"], json!([{"name": "夏目漱石"}]));
    assert_eq!(row["deleted"], json!(false));
}

#[test]
fn an_older_hlc_never_overwrites_a_newer_value() {
    let old = book_op(DEV_A, 1_000, json!({"title": "旧", "rating": 1}));
    let new = book_op(DEV_B, 2_000, json!({"title": "新"}));

    let mut a = mem_db();
    apply_all(&mut a, [&old, &new]);
    let mut b = mem_db();
    apply_all(&mut b, [&new, &old]);

    assert_eq!(dump(&a), dump(&b));
    let row = book(&a);
    assert_eq!(row["title"], json!("新"));
    assert_eq!(
        row["rating"],
        json!(1),
        "新しい Op が触れていない項目は古い Op の値が残る"
    );

    // 後から古い Op が来たときの件数
    let mut c = mem_db();
    apply(&mut c, &new);
    assert_eq!(
        apply(&mut c, &old),
        ApplyOutcome {
            applied: 1,
            stale: 1,
            invalid: 0
        }
    );
}

#[test]
fn ties_in_ms_and_counter_are_broken_by_the_device_id_in_either_order() {
    let from_a = remote_op(DEV_A, 5_000, 3, Entity::Book, BOOK_1, json!({"title": "A"}));
    let from_b = remote_op(DEV_B, 5_000, 3, Entity::Book, BOOK_1, json!({"title": "B"}));
    let mut x = mem_db();
    apply_all(&mut x, [&from_a, &from_b]);
    let mut y = mem_db();
    apply_all(&mut y, [&from_b, &from_a]);
    assert_eq!(dump(&x), dump(&y));
    assert_eq!(book(&x)["title"], json!("B"));

    // カウンタが大きい方が、ms が同じなら勝つ（'9' < '10' のような桁の罠がない）
    let c9 = remote_op(DEV_A, 5_000, 9, Entity::Book, BOOK_1, json!({"title": "9"}));
    let c10 = remote_op(
        DEV_A,
        5_000,
        10,
        Entity::Book,
        BOOK_1,
        json!({"title": "10"}),
    );
    let mut z = mem_db();
    apply_all(&mut z, [&c10, &c9]);
    assert_eq!(book(&z)["title"], json!("10"));
}

#[test]
fn a_partial_update_may_arrive_before_the_create_op() {
    let create = book_op(
        DEV_A,
        1_000,
        json!({"title": "作成時の書名", "memo": "作成時のメモ", "created_at": "2026-10-14T00:00:00.000Z", "ownership": "owned"}),
    );
    let update = book_op(
        DEV_B,
        2_000,
        json!({"title": "後から直した書名", "rating": 3}),
    );

    let mut update_first = mem_db();
    // 作成 Op より先に部分更新が来ても、NOT NULL 列の既定値のおかげで行ができる
    assert_eq!(apply(&mut update_first, &update), applied(2));
    let early = book(&update_first);
    assert_eq!(early["title"], json!("後から直した書名"));
    assert_eq!(early["memo"], json!(null));
    assert_eq!(early["ownership"], json!("owned"), "既定値");
    assert_eq!(early["read_status"], json!("unread"));
    assert_eq!(early["authors"], json!([]));
    assert_eq!(early["created_at"], json!(""));
    apply(&mut update_first, &create);

    let mut create_first = mem_db();
    apply_all(&mut create_first, [&create, &update]);

    assert_eq!(dump(&update_first), dump(&create_first));
    let row = book(&update_first);
    assert_eq!(row["title"], json!("後から直した書名"), "新しい更新が勝つ");
    assert_eq!(
        row["memo"],
        json!("作成時のメモ"),
        "作成 Op のうち更新されていない項目は残る"
    );
    assert_eq!(row["created_at"], json!("2026-10-14T00:00:00.000Z"));
    assert_eq!(row["rating"], json!(3));
}

#[test]
fn unknown_fields_are_kept_in_extra_json_and_follow_lww() {
    let v1 = book_op(
        DEV_A,
        1_000,
        json!({"title": "x", "future_a": 1, "future_b": {"k": [1, 2]}}),
    );
    let v2 = book_op(DEV_B, 2_000, json!({"future_a": 2}));
    let v3 = book_op(DEV_A, 3_000, json!({"future_b": null}));

    let mut forward = mem_db();
    apply_all(&mut forward, [&v1, &v2, &v3]);
    let mut backward = mem_db();
    apply_all(&mut backward, [&v3, &v2, &v1]);
    let mut shuffled = mem_db();
    apply_all(&mut shuffled, [&v2, &v1, &v3, &v1, &v2]);

    assert_eq!(dump(&forward), dump(&backward));
    assert_eq!(dump(&forward), dump(&shuffled));
    let row = book(&forward);
    assert_eq!(row["extra"], json!({"future_a": 2, "future_b": null}));
    assert_eq!(row["title"], json!("x"));

    // 古い値は新しい値を上書きしない
    let stale = book_op(DEV_C, 1_500, json!({"future_a": 99}));
    assert_eq!(
        apply(&mut forward, &stale),
        ApplyOutcome {
            applied: 0,
            stale: 1,
            invalid: 0
        }
    );
    assert_eq!(book(&forward)["extra"]["future_a"], json!(2));

    // 未知の項目にも時計がある（Op 上の名前で記録される）
    let clock = field_clock(&forward, Entity::Book, BOOK_1, "future_a")
        .unwrap()
        .unwrap();
    assert_eq!(clock, v2.hlc);
    // 列名（authors_json など）や予約名は Op 上は未知の項目として extra_json に入る
    let odd = book_op(DEV_A, 4_000, json!({"authors_json": [], "updated_at": "x"}));
    apply(&mut forward, &odd);
    let row = book(&forward);
    assert_eq!(row["authors"], json!([]));
    assert_eq!(row["extra"]["authors_json"], json!([]));
    assert_eq!(row["extra"]["updated_at"], json!("x"));
}

#[test]
fn a_delete_survives_later_edits_and_can_be_restored_with_a_newer_clock() {
    // AT-S4: A が X を削除し、B が（それより後に）X の評価を変更 → X は削除されたまま
    let delete = book_op(DEV_A, 1_000, json!({"deleted": true}));
    let edit = book_op(DEV_B, 2_000, json!({"rating": 5}));

    let mut a = mem_db();
    apply_all(&mut a, [&delete, &edit]);
    let mut b = mem_db();
    apply_all(&mut b, [&edit, &delete]);
    assert_eq!(dump(&a), dump(&b));
    for conn in [&a, &b] {
        let row = book(conn);
        assert_eq!(row["deleted"], json!(true));
        assert_eq!(row["rating"], json!(5), "評価の変更自体は残る");
    }

    // 復元は deleted=false を新しい時刻で書き込む
    let restore = book_op(DEV_B, 3_000, json!({"deleted": false}));
    apply(&mut a, &restore);
    apply(&mut b, &restore);
    assert_eq!(book(&a)["deleted"], json!(false));
    assert_eq!(dump(&a), dump(&b));

    // 復元より古い削除が遅れて届いても、復元は取り消されない
    apply(&mut a, &delete);
    assert_eq!(book(&a)["deleted"], json!(false));
}

#[test]
fn null_writes_the_column_default_for_not_null_columns_and_null_otherwise() {
    let mut conn = mem_db();
    apply(
        &mut conn,
        &book_op(
            DEV_A,
            1_000,
            json!({"title": "x", "authors": [{"name": "a"}], "tags": ["t"], "ownership": "wanted",
                   "deleted": true, "field_sources": {"title": "openbd"}, "rating": 3, "memo": "m"}),
        ),
    );
    let outcome = apply(
        &mut conn,
        &book_op(
            DEV_A,
            2_000,
            json!({"title": null, "authors": null, "tags": null, "ownership": null,
                   "deleted": null, "field_sources": null, "rating": null, "memo": null}),
        ),
    );
    assert_eq!(outcome, applied(8));
    let row = book(&conn);
    assert_eq!(row["title"], json!(""));
    assert_eq!(row["authors"], json!([]));
    assert_eq!(row["tags"], json!([]));
    assert_eq!(row["ownership"], json!("owned"));
    assert_eq!(row["deleted"], json!(false));
    assert_eq!(row["field_sources"], json!({}));
    assert_eq!(row["rating"], json!(null));
    assert_eq!(row["memo"], json!(null));
}

#[test]
fn a_value_of_the_wrong_type_is_ignored_and_does_not_advance_the_clock() {
    let mut conn = mem_db();
    let bad = book_op(
        DEV_B,
        9_000,
        json!({"rating": "five", "deleted": 1, "authors": "x", "volume_sort": "1", "title": "良い値"}),
    );
    assert_eq!(
        apply(&mut conn, &bad),
        ApplyOutcome {
            applied: 1,
            stale: 0,
            invalid: 4
        }
    );
    let row = book(&conn);
    assert_eq!(row["title"], json!("良い値"));
    assert_eq!(row["rating"], json!(null));
    assert_eq!(row["deleted"], json!(false));
    assert_eq!(
        field_clock(&conn, Entity::Book, BOOK_1, "rating").unwrap(),
        None
    );

    // 無視した Op の時計は残らないので、より古い正常な Op がまだ書ける（順序によらず同じ結果）
    let good = book_op(DEV_A, 1_000, json!({"rating": 3}));
    let mut other = mem_db();
    apply_all(&mut other, [&good, &bad]);
    apply(&mut conn, &good);
    assert_eq!(dump(&conn), dump(&other));
    assert_eq!(book(&conn)["rating"], json!(3));
}

#[test]
fn malformed_ops_are_errors() {
    let mut conn = mem_db();
    let mut op = book_op(DEV_A, 1_000, json!({"title": "x"}));

    op.hlc = "not-an-hlc".into();
    let tx = write_tx(&mut conn).unwrap();
    assert!(matches!(apply_op(&tx, &op), Err(Error::InvalidHlc(_))));
    drop(tx);

    let mut op = book_op(DEV_A, 1_000, json!({"title": "x"}));
    op.id = String::new();
    let tx = write_tx(&mut conn).unwrap();
    assert!(matches!(apply_op(&tx, &op), Err(Error::InvalidOp(_))));
    drop(tx);

    assert_eq!(count(&conn, "books"), 0);
}

#[test]
fn an_op_with_an_empty_set_only_creates_the_row() {
    let mut conn = mem_db();
    let op = book_op(DEV_A, 1_000, json!({}));
    assert_eq!(apply(&mut conn, &op), ApplyOutcome::default());
    assert_eq!(count(&conn, "books"), 1);
    assert_eq!(count(&conn, "field_clocks"), 0);
    assert_eq!(
        updated_at(&conn, "books", BOOK_1),
        "",
        "何も書き換わらなければ updated_at も動かない"
    );
}

#[test]
fn op_names_map_to_columns_and_types_are_stored_natively() {
    let mut conn = mem_db();
    apply(
        &mut conn,
        &book_op(
            DEV_A,
            1_000,
            json!({"authors": [{"name": "a"}], "tags": ["x", "y"], "field_sources": {"title": "openbd"},
                   "manual_fields": ["title"], "volume_sort": 2, "list_price": 700, "deleted": true, "purged": false}),
        ),
    );
    let (authors, tags, sources, manual, volume_sort, list_price, deleted, purged): (
        String,
        String,
        String,
        String,
        f64,
        i64,
        i64,
        i64,
    ) = conn
        .query_row(
            "SELECT authors_json, tags_json, field_sources_json, manual_fields_json,
                    volume_sort, list_price, deleted, purged FROM books WHERE id = ?1",
            [BOOK_1],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(authors, r#"[{"name":"a"}]"#);
    assert_eq!(tags, r#"["x","y"]"#);
    assert_eq!(sources, r#"{"title":"openbd"}"#);
    assert_eq!(manual, r#"["title"]"#);
    assert_eq!(volume_sort, 2.0);
    assert_eq!(list_price, 700);
    assert_eq!((deleted, purged), (1, 0));
    // 型は SQLite 上の実際の型でも確かめる
    let types: (String, String, String) = conn
        .query_row(
            "SELECT typeof(volume_sort), typeof(deleted), typeof(authors_json) FROM books WHERE id = ?1",
            [BOOK_1],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(types, ("real".into(), "integer".into(), "text".into()));
}

#[test]
fn every_entity_type_can_be_applied() {
    let mut conn = mem_db();
    let ops = [
        remote_op(
            DEV_A,
            1_000,
            0,
            Entity::Series,
            SERIES_1,
            json!({"name": "シリーズ", "name_kana": "しりーず", "memo": "m", "created_at": "c"}),
        ),
        remote_op(
            DEV_A,
            1_001,
            0,
            Entity::Location,
            TAG_1,
            json!({"name": "書斎", "parent_id": BOOK_2, "sort_order": 1.5, "deleted": false}),
        ),
        remote_op(
            DEV_A,
            1_002,
            0,
            Entity::Tag,
            TAG_1,
            json!({"name": "小説", "color": "#ff0000"}),
        ),
        remote_op(
            DEV_A,
            1_003,
            0,
            Entity::Shelf,
            TAG_2,
            json!({"name": "未読", "query": {"read_status": "unread"}, "sort_order": 2}),
        ),
    ];
    apply_all(&mut conn, &ops);

    let d = dump(&conn);
    assert_eq!(
        dump_row(&d, "series", SERIES_1)["name_kana"],
        json!("しりーず")
    );
    assert_eq!(dump_row(&d, "locations", TAG_1)["sort_order"], json!(1.5));
    assert_eq!(dump_row(&d, "locations", TAG_1)["parent_id"], json!(BOOK_2));
    assert_eq!(dump_row(&d, "tags", TAG_1)["color"], json!("#ff0000"));
    let shelf = dump_row(&d, "shelves", TAG_2);
    assert_eq!(shelf["query"], json!({"read_status": "unread"}));
    assert_eq!(shelf["sort_order"], json!(2.0));
    let query_json: String = conn
        .query_row(
            "SELECT query_json FROM shelves WHERE id = ?1",
            [TAG_2],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(query_json, r#"{"read_status":"unread"}"#);

    // 時計はエンティティごとに別。同じ ID でもエンティティが違えば干渉しない
    assert!(
        field_clock(&conn, Entity::Location, TAG_1, "name")
            .unwrap()
            .is_some()
    );
    assert!(
        field_clock(&conn, Entity::Tag, TAG_1, "name")
            .unwrap()
            .is_some()
    );
    assert_eq!(dump_row(&d, "tags", TAG_1)["name"], json!("小説"));
    assert_eq!(dump_row(&d, "locations", TAG_1)["name"], json!("書斎"));
}

#[test]
fn tags_with_the_same_name_and_different_ids_can_coexist() {
    let mut conn = mem_db();
    apply(
        &mut conn,
        &remote_op(DEV_A, 1_000, 0, Entity::Tag, TAG_1, json!({"name": "SiC"})),
    );
    apply(
        &mut conn,
        &remote_op(DEV_B, 1_000, 0, Entity::Tag, TAG_2, json!({"name": "SiC"})),
    );
    assert_eq!(count(&conn, "tags"), 2);
}

#[test]
fn updated_at_is_the_max_hlc_time_of_the_applied_changes_and_is_not_in_the_dump() {
    let mut conn = mem_db();
    // 2026-10-14T08:30:00.123Z
    let ms = 1_791_966_600_123;
    apply(&mut conn, &book_op(DEV_A, ms, json!({"title": "x"})));
    assert_eq!(
        updated_at(&conn, "books", BOOK_1),
        "2026-10-14T08:30:00.123Z"
    );

    // 古い HLC の別項目が後から届いても戻らない
    apply(
        &mut conn,
        &book_op(DEV_B, ms - 5_000, json!({"memo": "古い"})),
    );
    assert_eq!(
        updated_at(&conn, "books", BOOK_1),
        "2026-10-14T08:30:00.123Z"
    );
    // 新しい HLC の Op が来れば進む（値が同じでも項目は書き換わったものとして扱う）
    apply(
        &mut conn,
        &book_op(DEV_B, ms + 5_000, json!({"title": "x"})),
    );
    assert_eq!(
        updated_at(&conn, "books", BOOK_1),
        "2026-10-14T08:30:05.123Z"
    );
    // 古い HLC で負けた Op では動かない
    apply(&mut conn, &book_op(DEV_B, ms - 1, json!({"title": "y"})));
    assert_eq!(
        updated_at(&conn, "books", BOOK_1),
        "2026-10-14T08:30:05.123Z"
    );

    assert!(book(&conn).get("updated_at").is_none());
    assert!(book(&conn).get("extra_json").is_none(), "extra として出す");
    assert!(book(&conn).get("extra").is_some());
}

#[test]
fn ops_parsed_from_json_lines_apply_like_constructed_ones() {
    // 8.3 の例（先頭 2 行）。未知のエンベロープのキーがあっても読める。
    let lines = [
        r#"{"v":1,"dev":"d-7f3a2c","seq":251,"hlc":"001760412345678:00000:d-7f3a2c","ent":"book","id":"0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55","set":{"title":"坊っちゃん","rating":4}}"#,
        r#"{"v":1,"dev":"d-7f3a2c","seq":252,"hlc":"001760412345901:00000:d-7f3a2c","ent":"book","id":"0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55","set":{"deleted":true},"note":"future"}"#,
    ];
    let ops: Vec<Op> = lines
        .iter()
        .map(|l| Op::from_json_line(l).unwrap())
        .collect();
    let mut conn = mem_db();
    apply_all(&mut conn, &ops);
    let row = book(&conn);
    assert_eq!(row["title"], json!("坊っちゃん"));
    assert_eq!(row["deleted"], json!(true));
    assert_eq!(
        updated_at(&conn, "books", BOOK_1),
        "2025-10-14T03:25:45.901Z"
    );
}
