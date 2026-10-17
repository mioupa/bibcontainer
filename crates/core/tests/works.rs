//! 作品の手動のまとめ（`merge_works`）と、`work_id` 項目の扱いのテスト。

mod common;
mod lib_common;

use bibcontainer_core::library::{self, MergeWorksResult, UpdateOptions, WORK_NONE, merge_works};
use bibcontainer_core::schema::Entity;
use bibcontainer_core::{Error, dump_canonical, get_book_detail, get_rows, list_rows};
use serde_json::{Value, json};

use lib_common::*;

/// 手動の作品の ID（UUIDv7 でなくてよい。書式が正しければ受け付ける）。
const W1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
const W2: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d02";

fn books(fx: &mut Fx, n: usize) -> Vec<String> {
    (0..n).map(|i| add_titled(fx, &format!("本 {i}"))).collect()
}

fn ids(v: &[&String]) -> Vec<String> {
    v.iter().map(|s| (*s).clone()).collect()
}

fn merge(fx: &mut Fx, targets: &[&String]) -> MergeWorksResult {
    merge_works(&mut fx.conn, &env(fx.clock), &ids(targets)).unwrap()
}

fn work_of(fx: &Fx, id: &str) -> Value {
    column(&fx.conn, id, "work_id")
}

/// 指定した本の `work_id` を、`update_books` で直接そろえる（テストの前提を作る）。
fn set_work(fx: &mut Fx, targets: &[&String], work: Value) {
    library::update_books(
        &mut fx.conn,
        &env(fx.clock),
        &ids(targets),
        &manual(&[("work_id", work)]),
        UpdateOptions::default(),
    )
    .unwrap();
}

fn assert_validation<T: std::fmt::Debug>(r: Result<T, Error>, field: &str) {
    match r {
        Err(Error::Validation { field: f, .. }) => assert_eq!(f, field),
        other => panic!("{field} の検証エラーのはず: {other:?}"),
    }
}

/// `work_id` の取得元・手修正の記録（`field_sources`、`manual_fields`）とその時計がないこと。
fn assert_no_provenance(fx: &Fx, id: &str) {
    assert!(sources(&fx.conn, id).get("work_id").is_none());
    assert!(
        !manual_fields(&fx.conn, id)
            .as_array()
            .unwrap()
            .contains(&json!("work_id"))
    );
    let clocks: i64 = fx
        .conn
        .query_row(
            "SELECT count(*) FROM field_clocks WHERE entity_id = ?1 AND field LIKE '%.work_id'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(clocks, 0, "記録の時計もない");
}

// ---------- merge_works ----------

#[test]
fn merging_auto_books_makes_a_new_uuid_v7_and_writes_every_target_once() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 4);
    let before = outbox_len(&fx.conn);

    let r = merge(&mut fx, &[&b[2], &b[0], &b[1]]);
    assert_eq!(
        uuid::Uuid::try_parse(&r.work_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(r.changed, ids(&[&b[2], &b[0], &b[1]]), "渡した順");
    assert_eq!(outbox_len(&fx.conn), before + 3, "1 冊 1 つの Op");
    for id in [&b[0], &b[1], &b[2]] {
        assert_eq!(work_of(&fx, id), json!(r.work_id));
        let op = ops_for(&fx.conn, Entity::Book, id).pop().unwrap();
        assert_eq!(
            serde_json::to_value(&op.set).unwrap(),
            json!({ "work_id": r.work_id }),
            "Op は work_id だけ（取得元・手修正の記録は付けない）"
        );
    }
    assert_eq!(
        work_of(&fx, &b[3]),
        json!(null),
        "選ばなかった本は変わらない"
    );

    // 一覧の行と詳細に出る
    let rows = get_rows(&fx.conn, &ids(&[&b[0], &b[3]])).unwrap();
    assert_eq!(rows[0].work_id.as_deref(), Some(r.work_id.as_str()));
    assert_eq!(rows[1].work_id, None);
    assert_eq!(
        get_book_detail(&fx.conn, &b[1]).unwrap().work_id.as_deref(),
        Some(r.work_id.as_str())
    );
    let listed = list_rows(&fx.conn).unwrap();
    assert_eq!(
        listed
            .iter()
            .filter(|row| row.work_id.as_deref() == Some(r.work_id.as_str()))
            .count(),
        3
    );
    // dump にも出る
    let dump = dump_canonical(&fx.conn).unwrap();
    assert_eq!(
        common::dump_row(&dump, "books", &b[0])["work_id"],
        json!(r.work_id)
    );
    assert_no_provenance(&fx, &b[0]);
}

#[test]
fn merging_into_an_existing_manual_work_keeps_it_and_writes_only_the_others() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 4);
    let w = merge(&mut fx, &[&b[0], &b[1]]).work_id;
    let before = outbox_len(&fx.conn);

    let r = merge(&mut fx, &[&b[2], &b[0]]);
    assert_eq!(r.work_id, w, "既にある作品の ID を使う");
    assert_eq!(
        r.changed,
        ids(&[&b[2]]),
        "書き換えるのは、まだ入っていない本だけ"
    );
    assert_eq!(outbox_len(&fx.conn), before + 1);
    assert_eq!(work_of(&fx, &b[2]), json!(w));
    assert_eq!(work_of(&fx, &b[1]), json!(w));
    assert_eq!(work_of(&fx, &b[3]), json!(null));
}

#[test]
fn merging_two_manual_works_keeps_the_larger_and_relabels_every_member_of_the_other() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 8);
    // W1: b0 b1 b2、W2: b3 b4（b4 はゴミ箱、b3 は選ばない）
    let w1 = merge(&mut fx, &[&b[0], &b[1], &b[2]]).work_id;
    let w2 = merge(&mut fx, &[&b[3], &b[4]]).work_id;
    library::trash_books(&mut fx.conn, &env(fx.clock), &ids(&[&b[4]])).unwrap();
    let before = outbox_len(&fx.conn);

    let r = merge(&mut fx, &[&b[0], &b[4], &b[5]]);
    assert_eq!(r.work_id, w1, "所属する本が多い（3 冊 対 2 冊）ほうを残す");
    // 対象（渡した順。b0 は既に W1）、続けて W2 の残り（b3）
    assert_eq!(r.changed, ids(&[&b[4], &b[5], &b[3]]));
    assert_eq!(outbox_len(&fx.conn), before + 3);
    for id in &b[..6] {
        assert_eq!(work_of(&fx, id), json!(w1), "{id}");
    }
    assert_ne!(w1, w2);
    assert_eq!(work_of(&fx, &b[6]), json!(null));
    // ゴミ箱の本の作品も書き換わる
    let detail = get_book_detail(&fx.conn, &b[4]).unwrap();
    assert!(detail.deleted);
    assert_eq!(detail.work_id.as_deref(), Some(w1.as_str()));
}

#[test]
fn trashed_members_count_when_choosing_the_larger_work() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 5);
    // W1: 2 冊、W2: 3 冊（うち 1 冊はゴミ箱）。ゴミ箱を数えなければ 2 対 2 で、ID が小さい W1 になる。
    set_work(&mut fx, &[&b[0], &b[1]], json!(W1));
    set_work(&mut fx, &[&b[2], &b[3], &b[4]], json!(W2));
    library::trash_books(&mut fx.conn, &env(fx.clock), &ids(&[&b[4]])).unwrap();

    let r = merge(&mut fx, &[&b[0], &b[2]]);
    assert_eq!(r.work_id, W2);
    assert_eq!(r.changed, ids(&[&b[0], &b[1]]));
}

#[test]
fn a_tie_goes_to_the_smallest_work_id() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 4);
    // 先に作った組に、ID の大きいほうを付ける（作成順に左右されないことを確かめる）
    set_work(&mut fx, &[&b[0], &b[1]], json!(W2));
    set_work(&mut fx, &[&b[2], &b[3]], json!(W1));

    let r = merge(&mut fx, &[&b[0], &b[2]]);
    assert_eq!(r.work_id, W1);
    assert_eq!(r.changed, ids(&[&b[0], &b[1]]));
    for id in &b {
        assert_eq!(work_of(&fx, id), json!(W1));
    }
}

#[test]
fn purged_books_are_skipped_and_not_counted() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 6);
    set_work(&mut fx, &[&b[0], &b[1]], json!(W1));
    set_work(&mut fx, &[&b[2], &b[3], &b[4]], json!(W2));
    library::purge_books(&mut fx.conn, &env(fx.clock), &ids(&[&b[4], &b[5]])).unwrap();
    // 完全削除した本は、ふつう work_id が空になる。数えられないことを確かめるため、あえて W2 を戻す。
    fx.conn
        .execute("UPDATE books SET work_id = ?1 WHERE id = ?2", [W2, &b[4]])
        .unwrap();
    let before = outbox_len(&fx.conn);

    // 完全削除した本は、渡しても対象にならない
    let r = merge(&mut fx, &[&b[0], &b[2], &b[4], &b[5]]);
    assert_eq!(
        r.work_id, W1,
        "完全削除した本は数えない（2 対 2 で ID が小さい W1。数えると W2）"
    );
    assert_eq!(r.changed, ids(&[&b[2], &b[3]]));
    assert_eq!(outbox_len(&fx.conn), before + 2);
    assert_eq!(work_of(&fx, &b[4]), json!(W2), "完全削除した本は書かない");
    assert_eq!(work_of(&fx, &b[5]), json!(null));

    // 完全削除した本を除くと 2 冊に満たないなら、検証エラー
    assert_validation(
        merge_works(&mut fx.conn, &env(fx.clock), &ids(&[&b[0], &b[4], &b[5]])),
        "work_id",
    );
}

#[test]
fn books_set_apart_with_none_join_the_chosen_work_and_do_not_count_as_a_work() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 4);

    // none と自動だけ: 新しい ID
    set_work(&mut fx, &[&b[0]], json!(WORK_NONE));
    let r = merge(&mut fx, &[&b[0], &b[1]]);
    assert_eq!(r.changed, ids(&[&b[0], &b[1]]));
    assert_ne!(r.work_id, WORK_NONE);
    assert_eq!(work_of(&fx, &b[0]), json!(r.work_id));

    // none と、既にある作品: その作品に入る
    set_work(&mut fx, &[&b[2]], json!(WORK_NONE));
    let again = merge(&mut fx, &[&b[2], &b[1]]);
    assert_eq!(again.work_id, r.work_id);
    assert_eq!(again.changed, ids(&[&b[2]]));
    assert_eq!(work_of(&fx, &b[2]), json!(r.work_id));
}

#[test]
fn fewer_than_two_targets_is_a_validation_error_and_writes_nothing() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 2);
    library::purge_books(&mut fx.conn, &env(fx.clock), &ids(&[&b[1]])).unwrap();
    let before = outbox_len(&fx.conn);

    for target in [
        vec![],
        ids(&[&b[0]]),
        ids(&[&b[0], &b[0]]),
        ids(&[&b[0], &b[1]]),
    ] {
        assert_validation(
            merge_works(&mut fx.conn, &env(fx.clock), &target),
            "work_id",
        );
    }
    assert_eq!(outbox_len(&fx.conn), before);
    assert_eq!(work_of(&fx, &b[0]), json!(null));
}

#[test]
fn an_unknown_id_is_not_found_and_nothing_is_written() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 2);
    let w = merge(&mut fx, &[&b[0], &b[1]]).work_id;
    let c = add_titled(&mut fx, "まとめていない本");
    let before = outbox_len(&fx.conn);

    let unknown = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0aff".to_owned();
    let r = merge_works(
        &mut fx.conn,
        &env(fx.clock),
        &[c.clone(), b[0].clone(), unknown.clone()],
    );
    match r {
        Err(Error::NotFound { what, id }) => {
            assert_eq!(what, "書籍");
            assert_eq!(id, unknown);
        }
        other => panic!("NotFound のはず: {other:?}"),
    }
    assert_eq!(outbox_len(&fx.conn), before);
    assert_eq!(work_of(&fx, &c), json!(null));
    assert_eq!(work_of(&fx, &b[0]), json!(w));
}

#[test]
fn merging_books_that_already_share_a_work_changes_nothing() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 3);
    let w = merge(&mut fx, &[&b[0], &b[1], &b[2]]).work_id;
    let before = outbox_len(&fx.conn);

    let r = merge(&mut fx, &[&b[1], &b[0]]);
    assert_eq!(r.work_id, w);
    assert!(r.changed.is_empty());
    assert_eq!(outbox_len(&fx.conn), before, "Op を書かない");
}

#[test]
fn merge_result_serializes_in_camel_case() {
    let r = MergeWorksResult {
        work_id: W1.to_owned(),
        changed: vec!["a".to_owned()],
    };
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        json!({ "workId": W1, "changed": ["a"] })
    );
}

// ---------- update_books で work_id を変える（外す・自動に戻す） ----------

#[test]
fn update_books_sets_none_a_valid_id_or_clears_the_work_without_provenance() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 3);
    let before = outbox_len(&fx.conn);

    // none（作品から外す）
    let reports = library::update_books(
        &mut fx.conn,
        &env(fx.clock),
        &ids(&[&b[0], &b[1]]),
        &manual(&[("work_id", json!("none"))]),
        UpdateOptions::default(),
    )
    .unwrap();
    assert!(reports.iter().all(|r| r.written == ["work_id"]));
    assert_eq!(outbox_len(&fx.conn), before + 2);
    let op = last_op(&fx.conn);
    assert_eq!(
        serde_json::to_value(&op.set).unwrap(),
        json!({ "work_id": "none" }),
        "取得元・手修正の記録は付かない"
    );
    assert_eq!(work_of(&fx, &b[0]), json!("none"));
    for id in &b {
        assert_no_provenance(&fx, id);
    }

    // 同じ値は、記録しない
    let r = edit(&mut fx, &b[0], &[("work_id", json!("none"))]);
    assert!(r.written.is_empty());
    assert_eq!(outbox_len(&fx.conn), before + 2);

    // 書式が正しい ID
    let r = edit(&mut fx, &b[2], &[("work_id", json!(W1))]);
    assert_eq!(r.written, ["work_id"]);
    assert_eq!(work_of(&fx, &b[2]), json!(W1));

    // 空文字と null は、自動に戻す
    edit(&mut fx, &b[0], &[("work_id", json!(""))]);
    assert_eq!(work_of(&fx, &b[0]), json!(null));
    edit(&mut fx, &b[2], &[("work_id", json!(null))]);
    assert_eq!(work_of(&fx, &b[2]), json!(null));
    let op = last_op(&fx.conn);
    assert_eq!(
        serde_json::to_value(&op.set).unwrap(),
        json!({ "work_id": null })
    );
    // null の Op の後も、dump では null
    let dump = dump_canonical(&fx.conn).unwrap();
    assert_eq!(
        common::dump_row(&dump, "books", &b[2])["work_id"],
        json!(null)
    );
}

#[test]
fn update_books_rejects_a_malformed_work_id_and_writes_nothing() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 2);
    let before = outbox_len(&fx.conn);
    for bad in [json!("abc"), json!("None"), json!(1), json!(true)] {
        let r = library::update_books(
            &mut fx.conn,
            &env(fx.clock),
            &ids(&[&b[0], &b[1]]),
            &manual(&[("work_id", bad)]),
            UpdateOptions::default(),
        );
        assert_validation(r, "work_id");
    }
    assert_eq!(outbox_len(&fx.conn), before);
}

#[test]
fn work_id_is_not_a_tracked_or_searchable_field() {
    use bibcontainer_core::{search_books, values};
    assert!(!values::TRACKED_FIELDS.contains(&"work_id"));

    let mut fx = Fx::new();
    let b = books(&mut fx, 2);
    let w = merge(&mut fx, &[&b[0], &b[1]]).work_id;
    // 作品の ID では検索に当たらない
    assert!(search_books(&fx.conn, &w).unwrap().is_empty());
}

// ---------- 完全削除・作成 ----------

#[test]
fn purging_a_book_resets_its_work_to_null() {
    let mut fx = Fx::new();
    let b = books(&mut fx, 3);
    let w = merge(&mut fx, &[&b[0], &b[1]]).work_id;
    set_work(&mut fx, &[&b[2]], json!(WORK_NONE));

    library::purge_books(&mut fx.conn, &env(fx.clock), &ids(&[&b[0], &b[2]])).unwrap();
    assert_eq!(work_of(&fx, &b[0]), json!(null));
    assert_eq!(work_of(&fx, &b[2]), json!(null));
    let op = last_op(&fx.conn);
    assert_eq!(
        op.set["work_id"],
        json!(null),
        "完全削除の Op は work_id を空にする"
    );
    // 残った本は、そのまま
    assert_eq!(work_of(&fx, &b[1]), json!(w));
}

#[test]
fn a_new_book_starts_with_an_automatic_work_and_can_be_created_with_one() {
    let mut fx = Fx::new();
    let a = add_titled(&mut fx, "自動");
    assert_eq!(work_of(&fx, &a), json!(null));
    let op = ops_for(&fx.conn, Entity::Book, &a).pop().unwrap();
    assert_eq!(op.set["work_id"], json!(null));

    let b = add(&mut fx, &[("title", json!("手動")), ("work_id", json!(W1))]);
    assert_eq!(work_of(&fx, &b), json!(W1));
}
