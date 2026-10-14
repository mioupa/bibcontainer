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

const BOOK_FIELDS: &[&str] = &[
    "title",
    "rating",
    "deleted",
    "authors",
    "volume_sort",
    "memo",
    "created_at",
    "zz_unknown_1",
    "zz_unknown_2",
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
        "title" | "memo" | "name" | "color" | "created_at" => prop_oneof![
            Just(json!("")),
            Just(json!("a")),
            Just(json!("b")),
            Just(json!("日本語"))
        ]
        .boxed(),
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

        let da = apply_in_one_db(&order_a);
        let db = apply_in_one_db(&order_b);
        let dl = apply_in_one_db(&by_hlc);
        prop_assert_eq!(&da, &db);
        prop_assert_eq!(&da, &dl);
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

/// 小さな具体例（衝突、削除、未知の項目、別エンティティ）を、全 120 通りの順序で適用する。
#[test]
fn every_permutation_of_a_small_scenario_converges() {
    let ops = [
        remote_op(
            DEV_A,
            1_000,
            0,
            Entity::Book,
            BOOK_1,
            json!({"title": "A の書名", "rating": 2, "authors": [{"name": "a"}]}),
        ),
        remote_op(
            DEV_B,
            1_000,
            0,
            Entity::Book,
            BOOK_1,
            json!({"title": "B の書名", "memo": "B"}),
        ),
        remote_op(
            DEV_A,
            2_000,
            0,
            Entity::Book,
            BOOK_1,
            json!({"deleted": true, "zz_unknown_1": 1}),
        ),
        remote_op(
            DEV_C,
            3_000,
            1,
            Entity::Book,
            BOOK_1,
            json!({"rating": 5, "zz_unknown_1": null, "zz_unknown_2": {"k": 1}}),
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
    permute(&mut indices, 0, &mut |order| {
        let shuffled: Vec<Op> = order.iter().map(|&i| ops[i].clone()).collect();
        assert_eq!(apply_in_one_db(&shuffled), expected, "順序 {order:?}");
        checked += 1;
    });
    assert_eq!(checked, 120);

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
