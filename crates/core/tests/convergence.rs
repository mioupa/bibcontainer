//! 収束の性質テスト（フェーズ 2 の AT-S7 の土台）。
//!
//! 同じ Op の集合を、どんな順序・重複で適用しても、正規化したダンプが一致すること。

mod common;

use std::collections::HashSet;

use bibcontainer_core::{Entity, Op, dump_canonical};
use proptest::prelude::*;
use serde_json::{Value, json};

use common::*;

const DEVICES: [&str; 3] = [DEV_A, DEV_B, DEV_C];

/// 手動でまとめた作品の ID（`work_id` の値）。
const WORK_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
const WORK_2: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d02";

const BOOK_FIELDS: &[&str] = &[
    "title",
    "rating",
    "deleted",
    "authors",
    "volume_sort",
    "work_id",
    "memo",
    "created_at",
    "zz_unknown_1",
    "zz_unknown_2",
    // 項目ごとの取得元・手修正の記録（項目名が不正なものも混ぜる。不正なものは無視される）
    "field_sources.title",
    "field_sources.memo",
    "field_sources.rating",
    "manual_fields.title",
    "manual_fields.memo",
    "manual_fields.authors",
    "field_sources.zz_unknown_1",
    "manual_fields.zz_unknown_1",
    // 検索用の派生データに関わる項目
    "title_kana",
    "publisher",
    "series_id",
    "tags",
    "purged",
];
const TAG_FIELDS: &[&str] = &["name", "color", "deleted", "zz_unknown_1"];
const SERIES_FIELDS: &[&str] = &["name", "memo", "deleted"];

fn entity_ids(ent: Entity) -> &'static [&'static str] {
    match ent {
        Entity::Book => &[BOOK_1, BOOK_2],
        Entity::Tag => &[TAG_1, TAG_2],
        _ => &[SERIES_1, BOOK_2],
    }
}

/// 型が合う値（null を含む）と、型の合わない値を混ぜて出す。
fn value_for(field: &'static str) -> BoxedStrategy<Value> {
    let typed: BoxedStrategy<Value> = match field {
        f if f.starts_with("field_sources.") => prop_oneof![
            Just(json!("manual")),
            Just(json!("openbd")),
            Just(json!("ndl"))
        ]
        .boxed(),
        // false は不正（true か null だけが有効）。any_value 側の true と合わせて混ぜる。
        f if f.starts_with("manual_fields.") => {
            prop_oneof![8 => Just(json!(true)), 1 => Just(json!(false))].boxed()
        }
        "series_id" => {
            prop_oneof![Just(json!(SERIES_1)), Just(json!(BOOK_2)), Just(json!("x"))].boxed()
        }
        "tags" => prop_oneof![
            Just(json!([])),
            Just(json!(["a"])),
            Just(json!(["a", "研究/SiC"])),
            Just(json!(["ｂ"]))
        ]
        .boxed(),
        "purged" => any::<bool>().prop_map(|b| json!(b)).boxed(),
        "title" | "memo" | "name" | "color" | "created_at" | "title_kana" | "publisher" => {
            prop_oneof![
                Just(json!("")),
                Just(json!("a")),
                Just(json!("b")),
                Just(json!("日本語"))
            ]
            .boxed()
        }
        "rating" => (0i64..=5).prop_map(|n| json!(n)).boxed(),
        "deleted" => any::<bool>().prop_map(|b| json!(b)).boxed(),
        "authors" => prop_oneof![
            Just(json!([])),
            Just(json!([{"name": "a"}])),
            Just(json!([{"name": "b", "role": "著"}])),
            Just(json!({"k": 1})),
        ]
        .boxed(),
        "volume_sort" => prop_oneof![Just(json!(1)), Just(json!(1.5)), Just(json!(10.0))].boxed(),
        // 作品の手動の指定: 作品から外す（none）、手動の作品の ID。null は下の 1 => で混ぜる。
        "work_id" => prop_oneof![
            Just(json!("none")),
            Just(json!(WORK_1)),
            Just(json!(WORK_2))
        ]
        .boxed(),
        _ => prop_oneof![
            Just(json!(1)),
            Just(json!("x")),
            Just(json!({"a": [1, 2]})),
            Just(json!(false))
        ]
        .boxed(),
    };
    let any_value = prop_oneof![
        Just(json!("wrong")),
        Just(json!(12)),
        Just(json!(1.25)),
        Just(json!(true)),
        Just(json!([1, 2])),
        Just(json!({"x": null})),
    ];
    prop_oneof![
        8 => typed,
        1 => Just(Value::Null),
        2 => any_value,
    ]
    .boxed()
}

fn arb_op() -> impl Strategy<Value = Op> {
    let ent = prop_oneof![Just(Entity::Book), Just(Entity::Tag), Just(Entity::Series)];
    (ent, 0usize..2, 1i64..=6, 0u32..=3, 0usize..3).prop_flat_map(
        |(ent, id_idx, ms, counter, dev_idx)| {
            let pool = match ent {
                Entity::Book => BOOK_FIELDS,
                Entity::Tag => TAG_FIELDS,
                _ => SERIES_FIELDS,
            };
            let entry = prop::sample::select(pool)
                .prop_flat_map(|f| value_for(f).prop_map(move |v| (f, v)));
            prop::collection::vec(entry, 1..=4).prop_map(move |entries| {
                let set: Value = Value::Object(
                    entries
                        .into_iter()
                        .map(|(f, v)| (f.to_owned(), v))
                        .collect(),
                );
                remote_op(
                    DEVICES[dev_idx],
                    ms,
                    counter,
                    ent,
                    entity_ids(ent)[id_idx],
                    set,
                )
            })
        },
    )
}

/// 同じ HLC を持つ別々の Op は現実には存在しない（端末 ID とカウンタで一意）ので、取り除く。
fn dedupe_by_hlc(ops: Vec<Op>) -> Vec<Op> {
    let mut seen = HashSet::new();
    ops.into_iter()
        .filter(|o| seen.insert(o.hlc.clone()))
        .collect()
}

fn apply_in_one_db(ops: &[Op]) -> Value {
    let mut conn = mem_db();
    apply_all(&mut conn, ops);
    dump_canonical(&conn).unwrap()
}

/// 検索用の派生データ（索引の行、FTS の中身、タグの索引）を比べやすい形にしたもの。
fn derived_snapshot(conn: &rusqlite::Connection) -> Value {
    let rows: Vec<Value> = conn
        .prepare(
            "SELECT i.book_id, i.sort_title, i.sort_authors, f.title, f.title_kana, f.authors,
                    f.publisher, f.series, f.tags, f.memo, f.isbn
             FROM book_idx i JOIN books_fts f ON f.rowid = i.idx ORDER BY i.book_id",
        )
        .unwrap()
        .query_map([], |r| {
            Ok(Value::Array(
                (0..11)
                    .map(|i| json!(r.get::<_, String>(i).unwrap()))
                    .collect(),
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let tags: Vec<Value> = conn
        .prepare("SELECT book_id, tag FROM book_tags ORDER BY book_id, tag")
        .unwrap()
        .query_map([], |r| {
            Ok(json!([r.get::<_, String>(0)?, r.get::<_, String>(1)?]))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let fts_rows: i64 = conn
        .query_row("SELECT count(*) FROM books_fts", [], |r| r.get(0))
        .unwrap();
    json!({"rows": rows, "tags": tags, "fts_rows": fts_rows})
}

/// 適用して、(ダンプ, 派生データ) を返す。途中の更新で作った派生データは、
/// 最後に全体を作り直したものと一致しなければならない。
fn apply_with_derived(ops: &[Op]) -> (Value, Value) {
    let mut conn = mem_db();
    apply_all(&mut conn, ops);
    let incremental = derived_snapshot(&conn);
    bibcontainer_core::derived::rebuild(&mut conn).unwrap();
    assert_eq!(
        incremental,
        derived_snapshot(&conn),
        "差分の更新と全体の作り直しが一致する"
    );
    (dump_canonical(&conn).unwrap(), incremental)
}

/// (重複なしの Op 集合, 重複を混ぜた並べ替え A, 重複を混ぜた並べ替え B)
fn arb_scenario() -> impl Strategy<Value = (Vec<Op>, Vec<Op>, Vec<Op>)> {
    (prop::collection::vec(arb_op(), 1..40), 0usize..20).prop_flat_map(|(ops, dup_count)| {
        let ops = dedupe_by_hlc(ops);
        let mut with_dups = ops.clone();
        with_dups.extend(ops.iter().cycle().take(dup_count).cloned());
        (
            Just(ops),
            Just(with_dups.clone()).prop_shuffle(),
            Just(with_dups).prop_shuffle(),
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// 2 つの異なるランダムな順序（重複を含む）で別々の新規 DB に適用すると、同じ状態になる。
    /// HLC 順に 1 回ずつ適用した場合とも一致する。
    #[test]
    fn any_order_and_any_duplication_converges((ops, order_a, order_b) in arb_scenario()) {
        let mut by_hlc = ops;
        by_hlc.sort_by(|x, y| x.hlc.cmp(&y.hlc));

        let (da, sa) = apply_with_derived(&order_a);
        let (db, sb) = apply_with_derived(&order_b);
        let (dl, sl) = apply_with_derived(&by_hlc);
        prop_assert_eq!(&da, &db);
        prop_assert_eq!(&da, &dl);
        // 派生データも、順序・重複によらず同じになる
        prop_assert_eq!(&sa, &sb);
        prop_assert_eq!(&sa, &sl);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// 適用の途中でトランザクションを区切っても結果は同じ（区切り方に依存しない）。
    #[test]
    fn transaction_boundaries_do_not_matter(
        ops in prop::collection::vec(arb_op(), 1..30).prop_map(dedupe_by_hlc),
        chunk in 1usize..8,
    ) {
        use bibcontainer_core::{apply_op, write_tx};
        let one_by_one = apply_in_one_db(&ops);

        let mut conn = mem_db();
        for part in ops.chunks(chunk) {
            let tx = write_tx(&mut conn).unwrap();
            for op in part {
                apply_op(&tx, op).unwrap();
            }
            tx.commit().unwrap();
        }
        prop_assert_eq!(one_by_one, dump_canonical(&conn).unwrap());
    }
}

/// 小さな具体例（衝突、削除、未知の項目、別エンティティ、取得元・手修正の記録）を、全通りの順序で適用する。
#[test]
fn every_permutation_of_a_small_scenario_converges() {
    let ops = [
        remote_op(
            DEV_A,
            1_000,
            0,
            Entity::Book,
            BOOK_1,
            json!({"title": "A の書名", "rating": 2, "authors": [{"name": "a"}],
                   "field_sources.title": "openbd", "manual_fields.title": true}),
        ),
        remote_op(
            DEV_B,
            1_000,
            0,
            Entity::Book,
            BOOK_1,
            // 端末 B は別の項目（出版社）に手修正を記録した（別のキーなので、どちらの記録も残る）
            json!({"title": "B の書名", "memo": "B", "field_sources.title": "manual",
                   "field_sources.publisher": "manual", "manual_fields.publisher": true}),
        ),
        remote_op(
            DEV_A,
            2_000,
            0,
            Entity::Book,
            BOOK_1,
            // 項目名が不正なキーは無視される
            json!({"deleted": true, "zz_unknown_1": 1, "field_sources.nope": "x",
                   "manual_fields.nope": true}),
        ),
        remote_op(
            DEV_C,
            3_000,
            1,
            Entity::Book,
            BOOK_1,
            json!({"rating": 5, "zz_unknown_1": null, "zz_unknown_2": {"k": 1},
                   "manual_fields.title": null}),
        ),
        remote_op(
            DEV_C,
            3_000,
            2,
            Entity::Tag,
            TAG_1,
            json!({"name": "タグ", "color": null}),
        ),
    ];
    let expected = apply_in_one_db(&ops);

    let mut indices: Vec<usize> = (0..ops.len()).collect();
    let mut checked = 0;
    let factorial: usize = (1..=ops.len()).product();
    permute(&mut indices, 0, &mut |order| {
        let shuffled: Vec<Op> = order.iter().map(|&i| ops[i].clone()).collect();
        assert_eq!(apply_in_one_db(&shuffled), expected, "順序 {order:?}");
        checked += 1;
    });
    assert_eq!(checked, factorial);

    // 期待する最終状態
    let row = dump_row(&expected, "books", BOOK_1);
    assert_eq!(
        row["title"],
        json!("B の書名"),
        "同じ ms・カウンタなら端末 ID が大きい方"
    );
    assert_eq!(row["rating"], json!(5));
    assert_eq!(row["deleted"], json!(true));
    assert_eq!(row["memo"], json!("B"));
    assert_eq!(
        row["extra"],
        json!({"zz_unknown_1": null, "zz_unknown_2": {"k": 1}})
    );
    // 別々の端末が別の項目に付けた記録は、どちらも残る
    // title の取得元は端末 B（同じ ms・カウンタなら端末 ID が大きい方）、手修正は端末 C が外した
    assert_eq!(
        row["field_sources"],
        json!({"publisher": "manual", "title": "manual"})
    );
    assert_eq!(row["manual_fields"], json!(["publisher"]));
}

fn permute(items: &mut Vec<usize>, k: usize, visit: &mut impl FnMut(&[usize])) {
    if k == items.len() {
        visit(items);
        return;
    }
    for i in k..items.len() {
        items.swap(k, i);
        permute(items, k + 1, visit);
        items.swap(k, i);
    }
}
