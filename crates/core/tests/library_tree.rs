//! 保管場所（ツリー）・タグ・シリーズの操作のテスト。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod common;
mod lib_common;

use bibcontainer_core::library::{self, FieldChange, UpdateOptions};
use bibcontainer_core::schema::Entity;
use bibcontainer_core::{Error, dump_canonical, get_book_detail, pending_ops, search_books};
use serde_json::json;

use lib_common::*;

fn loc(fx: &mut Fx, name: &str, parent: Option<&str>) -> String {
    library::create_location(&mut fx.conn, &env(fx.clock), name, parent).unwrap()
}

fn tree(fx: &Fx) -> Vec<(String, u32, String)> {
    library::list_locations(&fx.conn)
        .unwrap()
        .into_iter()
        .map(|l| (l.name, l.depth, l.path))
        .collect()
}

fn names(fx: &Fx) -> Vec<String> {
    tree(fx).into_iter().map(|(n, _, _)| n).collect()
}

// ---------- 保管場所 ----------

#[test]
fn locations_form_a_tree_with_paths_and_depth() {
    let mut fx = Fx::new();
    let home = loc(&mut fx, "自宅", None);
    let office = loc(&mut fx, "研究室", None);
    let study = loc(&mut fx, "書斎の本棚", Some(&home));
    loc(&mut fx, "3段目", Some(&study));
    loc(&mut fx, "1段目", Some(&study));
    loc(&mut fx, "玄関", Some(&home));
    loc(&mut fx, "机", Some(&office));

    // 深さ優先。同じ親の下は作った順（並び順）
    assert_eq!(
        tree(&fx),
        [
            ("自宅".to_owned(), 0, "自宅".to_owned()),
            ("書斎の本棚".to_owned(), 1, "自宅 > 書斎の本棚".to_owned()),
            (
                "3段目".to_owned(),
                2,
                "自宅 > 書斎の本棚 > 3段目".to_owned()
            ),
            (
                "1段目".to_owned(),
                2,
                "自宅 > 書斎の本棚 > 1段目".to_owned()
            ),
            ("玄関".to_owned(), 1, "自宅 > 玄関".to_owned()),
            ("研究室".to_owned(), 0, "研究室".to_owned()),
            ("机".to_owned(), 1, "研究室 > 机".to_owned()),
        ]
    );
    let list = library::list_locations(&fx.conn).unwrap();
    assert_eq!(list[1].parent_id.as_deref(), Some(home.as_str()));
    assert_eq!(list[0].sort_order, Some(1.0));
    assert_eq!(list[5].sort_order, Some(2.0), "ルートの 2 番目");

    // 作成は 1 つの Op に全項目
    let op = ops_for(&fx.conn, Entity::Location, &study).pop().unwrap();
    assert_eq!(op.set["name"], json!("書斎の本棚"));
    assert_eq!(op.set["parent_id"], json!(home));
    assert_eq!(op.set["deleted"], json!(false));
    assert!(op.set["created_at"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn location_names_are_validated_and_parents_must_exist() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    assert!(matches!(
        library::create_location(&mut fx.conn, &e, "  ", None),
        Err(Error::Validation { .. })
    ));
    let missing = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
    assert!(matches!(
        library::create_location(&mut fx.conn, &e, "x", Some(missing)),
        Err(Error::NotFound { .. })
    ));
    let a = loc(&mut fx, "A", None);
    library::delete_location(&mut fx.conn, &e, &a).unwrap();
    assert!(
        matches!(
            library::create_location(&mut fx.conn, &e, "x", Some(&a)),
            Err(Error::NotFound { .. })
        ),
        "削除した保管場所の下には作れない"
    );
    assert_eq!(outbox_len(&fx.conn), 2, "失敗した操作は記録されない");
}

#[test]
fn rename_and_move_locations_and_reject_cycles() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let home = loc(&mut fx, "自宅", None);
    let a = loc(&mut fx, "A", Some(&home));
    let b = loc(&mut fx, "B", Some(&a));
    let c = loc(&mut fx, "C", Some(&b));
    let other = loc(&mut fx, "別宅", None);

    library::rename_location(&mut fx.conn, &e, &a, "  棚 A ").unwrap();
    assert_eq!(
        library::list_locations(&fx.conn).unwrap()[1].path,
        "自宅 > 棚 A"
    );
    // 同じ名前なら Op を書かない
    let ops = outbox_len(&fx.conn);
    library::rename_location(&mut fx.conn, &e, &a, "棚 A").unwrap();
    assert_eq!(outbox_len(&fx.conn), ops);
    assert!(matches!(
        library::rename_location(&mut fx.conn, &e, &a, ""),
        Err(Error::Validation { .. })
    ));

    // 循環は拒否する（自分自身、子孫の下）
    for target in [&a, &b, &c] {
        let r = library::move_location(&mut fx.conn, &e, &a, Some(target));
        assert!(matches!(r, Err(Error::Conflict(_))), "{target}: {r:?}");
    }
    assert_eq!(outbox_len(&fx.conn), ops, "拒否した移動は記録されない");

    // 別の親の下へ。子孫もついていく。親の末尾に並ぶ
    library::move_location(&mut fx.conn, &e, &a, Some(&other)).unwrap();
    assert_eq!(
        tree(&fx)[..4]
            .iter()
            .map(|t| t.2.clone())
            .collect::<Vec<_>>(),
        ["自宅", "別宅", "別宅 > 棚 A", "別宅 > 棚 A > B"]
    );
    // ルートへ
    library::move_location(&mut fx.conn, &e, &c, None).unwrap();
    assert_eq!(names(&fx), ["自宅", "別宅", "棚 A", "B", "C"]);
    // 今の親と同じなら何もしない
    let ops = outbox_len(&fx.conn);
    library::move_location(&mut fx.conn, &e, &c, None).unwrap();
    assert_eq!(outbox_len(&fx.conn), ops);
    // 削除した保管場所の下へは移せない
    library::delete_location(&mut fx.conn, &e, &home).unwrap();
    assert!(matches!(
        library::move_location(&mut fx.conn, &e, &c, Some(&home)),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn reorder_locations_writes_only_the_changed_sort_orders() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let root = loc(&mut fx, "R", None);
    let ids: Vec<String> = ["a", "b", "c", "d"]
        .iter()
        .map(|n| loc(&mut fx, n, Some(&root)))
        .collect();
    assert_eq!(names(&fx), ["R", "a", "b", "c", "d"]);

    let ops = outbox_len(&fx.conn);
    library::reorder_locations(
        &mut fx.conn,
        &e,
        Some(&root),
        &[
            ids[3].clone(),
            ids[1].clone(),
            ids[0].clone(),
            ids[2].clone(),
        ],
    )
    .unwrap();
    assert_eq!(names(&fx), ["R", "d", "b", "a", "c"]);
    // d: 4→1, b: 2→2（同じ）, a: 1→3, c: 3→4
    assert_eq!(outbox_len(&fx.conn), ops + 3);

    // 一部だけ挙げたとき、残りは今の順で後ろに続く
    library::reorder_locations(&mut fx.conn, &e, Some(&root), &[ids[2].clone()]).unwrap();
    assert_eq!(names(&fx), ["R", "c", "d", "b", "a"]);
    // 同じ並びなら何も書かない
    let ops = outbox_len(&fx.conn);
    library::reorder_locations(&mut fx.conn, &e, Some(&root), &[ids[2].clone()]).unwrap();
    assert_eq!(outbox_len(&fx.conn), ops);
    // 別の親の保管場所を混ぜるとエラー
    let r = library::reorder_locations(&mut fx.conn, &e, None, &[ids[0].clone()]);
    assert!(matches!(r, Err(Error::Validation { .. })), "{r:?}");
}

#[test]
fn a_location_can_be_deleted_only_when_it_has_no_children_and_no_books() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let home = loc(&mut fx, "自宅", None);
    let shelf = loc(&mut fx, "棚", Some(&home));

    let r = library::delete_location(&mut fx.conn, &e, &home);
    assert!(
        matches!(r, Err(Error::Conflict(m)) if m.contains("子の保管場所")),
        "子がある"
    );

    let book = add(
        &mut fx,
        &[("title", json!("T")), ("location_id", json!(shelf))],
    );
    let r = library::delete_location(&mut fx.conn, &e, &shelf);
    assert!(
        matches!(r, Err(Error::Conflict(m)) if m.contains("本が 1 冊")),
        "本がある"
    );
    // ゴミ箱の本があるときも削除できない
    library::trash_books(&mut fx.conn, &e, &[book.clone()]).unwrap();
    assert!(matches!(
        library::delete_location(&mut fx.conn, &e, &shelf),
        Err(Error::Conflict(_))
    ));
    // 完全削除した本は数えない
    library::purge_books(&mut fx.conn, &e, &[book]).unwrap();
    library::delete_location(&mut fx.conn, &e, &shelf).unwrap();
    assert_eq!(names(&fx), ["自宅"]);
    library::delete_location(&mut fx.conn, &e, &home).unwrap();
    assert!(tree(&fx).is_empty());
    assert!(matches!(
        library::delete_location(&mut fx.conn, &e, &home),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn a_tree_with_a_cycle_or_a_missing_parent_neither_hangs_nor_disappears() {
    let mut fx = Fx::new();
    // 2 台が同時に親を入れ替えて、循環ができた
    let l1 = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
    let l2 = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d02";
    let orphan = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d03";
    let ghost = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d04";
    for (id, name, parent, ms) in [
        (l1, "L1", Some(l2), 1_000),
        (l2, "L2", Some(l1), 1_000),
        (orphan, "孤児", Some(ghost), 1_000),
    ] {
        common::apply(
            &mut fx.conn,
            &common::remote_op(
                common::DEV_A,
                ms,
                0,
                Entity::Location,
                id,
                json!({"name": name, "parent_id": parent, "sort_order": 1.0}),
            ),
        );
    }
    let list = library::list_locations(&fx.conn).unwrap();
    assert_eq!(
        list.len(),
        3,
        "循環の保管場所も、親が見つからない保管場所も一覧に出る"
    );
    assert!(list.iter().all(|l| !l.path.is_empty()));
    let o = list.iter().find(|l| l.id == orphan).unwrap();
    assert_eq!(o.path, "孤児");
    assert_eq!(o.depth, 0);
    // 循環の中でパスが止まる
    let loc1 = list.iter().find(|l| l.id == l1).unwrap();
    assert!(loc1.path.contains("L1"));
    // 循環の下へは移せない
    let e = env(fx.clock);
    let r = library::move_location(&mut fx.conn, &e, orphan, Some(l1));
    assert!(r.is_ok(), "L1 は孤児の子孫ではないので移せる");
    let r = library::move_location(&mut fx.conn, &e, l1, Some(orphan));
    assert!(
        matches!(r, Err(Error::Conflict(_))),
        "孤児 → L1 → L2 → L1 … 孤児の子孫になる移動"
    );
}

// ---------- タグ ----------

#[test]
fn tag_list_counts_books_that_are_not_trashed() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[("title", json!("A")), ("tags", json!(["小説", "研究/SiC"]))],
    );
    add(&mut fx, &[("title", json!("B")), ("tags", json!(["小説"]))]);
    let c = add(
        &mut fx,
        &[
            ("title", json!("C")),
            ("tags", json!(["小説", "ゴミ箱だけ"])),
        ],
    );
    library::set_tag_color(&mut fx.conn, &e, "研究/SiC", Some("#3366ff")).unwrap();
    library::set_tag_color(&mut fx.conn, &e, "本のないタグ", Some("red")).unwrap();
    library::trash_books(&mut fx.conn, &e, &[c]).unwrap();

    let tags = library::list_tags(&fx.conn).unwrap();
    let summary: Vec<(&str, i64, Option<&str>)> = tags
        .iter()
        .map(|t| (t.name.as_str(), t.count, t.color.as_deref()))
        .collect();
    assert_eq!(
        summary,
        [
            ("小説", 2, None),
            ("本のないタグ", 0, Some("red")),
            ("研究/SiC", 1, Some("#3366ff")),
        ]
    );
    assert!(
        tags[0].id.is_none(),
        "色を設定していないタグに tag エンティティはない"
    );
    assert!(tags[2].id.is_some());
    assert!(!a.is_empty());
}

#[test]
fn renaming_a_tag_rewrites_every_book_and_the_tag_entity_in_one_transaction() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[("title", json!("A")), ("tags", json!(["旧", "x"]))],
    );
    let b = add(
        &mut fx,
        &[("title", json!("B")), ("tags", json!(["y", "旧", "z"]))],
    );
    let c = add(
        &mut fx,
        &[("title", json!("C")), ("tags", json!(["旧", "新"]))],
    );
    let other = add(
        &mut fx,
        &[("title", json!("D")), ("tags", json!(["旧子/孫"]))],
    );
    let trashed = add(&mut fx, &[("title", json!("T")), ("tags", json!(["旧"]))]);
    library::trash_books(&mut fx.conn, &e, &[trashed.clone()]).unwrap();
    let tag_entity = library::set_tag_color(&mut fx.conn, &e, "旧", Some("blue")).unwrap();

    let changed = library::rename_tag(&mut fx.conn, &e, "旧", "新").unwrap();
    assert_eq!(
        changed,
        [a.clone(), b.clone(), c.clone(), trashed.clone()],
        "ゴミ箱の本も"
    );
    assert_eq!(get_book_detail(&fx.conn, &a).unwrap().tags, ["新", "x"]);
    assert_eq!(
        get_book_detail(&fx.conn, &b).unwrap().tags,
        ["y", "新", "z"],
        "位置は保つ"
    );
    assert_eq!(
        get_book_detail(&fx.conn, &c).unwrap().tags,
        ["新"],
        "重複は 1 つにまとめる"
    );
    assert_eq!(
        get_book_detail(&fx.conn, &other).unwrap().tags,
        ["旧子/孫"],
        "名前が一致するものだけ"
    );
    // 旧名の tag エンティティは、変更先に同名のエンティティが無いので改名される
    let tags = library::list_tags(&fx.conn).unwrap();
    let renamed = tags.iter().find(|t| t.name == "新").unwrap();
    assert_eq!(renamed.id.as_deref(), Some(tag_entity.as_str()));
    assert_eq!(renamed.color.as_deref(), Some("blue"));
    assert_eq!(renamed.count, 3);
    assert!(tags.iter().all(|t| t.name != "旧"));
    // 検索の派生データも更新されている
    assert_eq!(
        search_books(&fx.conn, "旧").unwrap(),
        [other.clone()],
        "旧子/孫だけ"
    );
    // 1 つのトランザクション: 本 4 冊 + tag 1 件の Op が連続して記録される
    let ops = pending_ops(&fx.conn).unwrap();
    let tail = &ops[ops.len() - 5..];
    assert_eq!(tail.iter().filter(|o| o.ent == Entity::Book).count(), 4);
    assert_eq!(tail.iter().filter(|o| o.ent == Entity::Tag).count(), 1);
}

#[test]
fn renaming_a_tag_renames_its_descendants_like_a_folder() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[
            ("title", json!("A")),
            ("tags", json!(["研究", "研究/SiC", "x"])),
        ],
    );
    let b = add(
        &mut fx,
        &[("title", json!("B")), ("tags", json!(["研究/SiC/GaN"]))],
    );
    // `/` で区切られていない名前と、途中に含むだけの名前は対象外
    let c = add(
        &mut fx,
        &[
            ("title", json!("C")),
            ("tags", json!(["研究室", "旧研究/SiC"])),
        ],
    );
    let root = library::set_tag_color(&mut fx.conn, &e, "研究", Some("red")).unwrap();
    let child = library::set_tag_color(&mut fx.conn, &e, "研究/SiC", Some("blue")).unwrap();
    let other = library::set_tag_color(&mut fx.conn, &e, "研究室", Some("green")).unwrap();

    let changed = library::rename_tag(&mut fx.conn, &e, "研究", "研究2").unwrap();
    assert_eq!(changed, [a.clone(), b.clone()]);
    assert_eq!(
        get_book_detail(&fx.conn, &a).unwrap().tags,
        ["研究2", "研究2/SiC", "x"]
    );
    assert_eq!(
        get_book_detail(&fx.conn, &b).unwrap().tags,
        ["研究2/SiC/GaN"]
    );
    assert_eq!(
        get_book_detail(&fx.conn, &c).unwrap().tags,
        ["研究室", "旧研究/SiC"]
    );

    // tag エンティティ（色）も、同じ ID のまま名前だけが変わる
    let tags = library::list_tags(&fx.conn).unwrap();
    let find = |name: &str| tags.iter().find(|t| t.name == name);
    assert_eq!(find("研究2").unwrap().id.as_deref(), Some(root.as_str()));
    assert_eq!(find("研究2").unwrap().color.as_deref(), Some("red"));
    assert_eq!(
        find("研究2/SiC").unwrap().id.as_deref(),
        Some(child.as_str())
    );
    assert_eq!(find("研究2/SiC").unwrap().color.as_deref(), Some("blue"));
    assert_eq!(find("研究室").unwrap().id.as_deref(), Some(other.as_str()));
    assert!(find("研究").is_none() && find("研究/SiC").is_none());

    // 検索の派生データも更新されている
    assert_eq!(search_books(&fx.conn, "研究2").unwrap(), [a, b]);
}

#[test]
fn renaming_a_parent_works_even_when_only_children_exist() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[("title", json!("A")), ("tags", json!(["親/子1", "親/子2"]))],
    );
    let changed = library::rename_tag(&mut fx.conn, &e, "親", "新親").unwrap();
    assert_eq!(changed, [a.clone()]);
    assert_eq!(
        get_book_detail(&fx.conn, &a).unwrap().tags,
        ["新親/子1", "新親/子2"]
    );
    assert!(matches!(
        library::rename_tag(&mut fx.conn, &e, "親", "別"),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn renaming_a_parent_merges_children_into_existing_tags() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[("title", json!("A")), ("tags", json!(["a/x", "b/x"]))],
    );
    let old_child = library::set_tag_color(&mut fx.conn, &e, "a/x", Some("red")).unwrap();
    let new_child = library::set_tag_color(&mut fx.conn, &e, "b/x", Some("green")).unwrap();
    library::rename_tag(&mut fx.conn, &e, "a", "b").unwrap();
    assert_eq!(get_book_detail(&fx.conn, &a).unwrap().tags, ["b/x"]);
    let tags = library::list_tags(&fx.conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(
        tags[0].id.as_deref(),
        Some(new_child.as_str()),
        "変更先が残る"
    );
    let deleted: i64 = fx
        .conn
        .query_row(
            "SELECT deleted FROM tags WHERE id = ?1",
            [&old_child],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(deleted, 1);
}

#[test]
fn renaming_a_tag_into_its_own_subtree_moves_everything_once() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[
            ("title", json!("A")),
            ("tags", json!(["a", "a/sub", "a/other"])),
        ],
    );
    library::set_tag_color(&mut fx.conn, &e, "a/sub", Some("red")).unwrap();
    library::rename_tag(&mut fx.conn, &e, "a", "a/sub").unwrap();
    assert_eq!(
        get_book_detail(&fx.conn, &a).unwrap().tags,
        ["a/sub", "a/sub/sub", "a/sub/other"],
        "もとの名前に二重には適用しない"
    );
    let tags = library::list_tags(&fx.conn).unwrap();
    let red = tags
        .iter()
        .find(|t| t.color.as_deref() == Some("red"))
        .unwrap();
    assert_eq!(red.name, "a/sub/sub");
}

#[test]
fn renaming_into_an_existing_tag_merges_the_two_entities() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    add(&mut fx, &[("title", json!("A")), ("tags", json!(["a"]))]);
    let old_entity = library::set_tag_color(&mut fx.conn, &e, "a", Some("red")).unwrap();
    let new_entity = library::set_tag_color(&mut fx.conn, &e, "b", Some("green")).unwrap();
    library::rename_tag(&mut fx.conn, &e, "a", "b").unwrap();
    let tags = library::list_tags(&fx.conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "b");
    assert_eq!(
        tags[0].id.as_deref(),
        Some(new_entity.as_str()),
        "変更先の方が残る"
    );
    assert_eq!(tags[0].color.as_deref(), Some("green"));
    let deleted: i64 = fx
        .conn
        .query_row(
            "SELECT deleted FROM tags WHERE id = ?1",
            [&old_entity],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(deleted, 1);
}

#[test]
fn duplicate_tag_entities_resolve_to_the_smallest_id() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    // 2 台が別々の ID で同じ名前のタグを作った
    let (low, high) = (common::TAG_1, common::TAG_2);
    for (id, color, ms) in [(high, "red", 1_000), (low, "blue", 2_000)] {
        common::apply(
            &mut fx.conn,
            &common::remote_op(
                common::DEV_B,
                ms,
                0,
                Entity::Tag,
                id,
                json!({"name": "重複", "color": color, "deleted": false}),
            ),
        );
    }
    add(&mut fx, &[("title", json!("A")), ("tags", json!(["重複"]))]);
    let tags = library::list_tags(&fx.conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].id.as_deref(), Some(low), "ID が最小のもの");
    assert_eq!(tags[0].color.as_deref(), Some("blue"));

    // 色の設定も、ID が最小のものに書く
    let id = library::set_tag_color(&mut fx.conn, &e, "重複", Some("black")).unwrap();
    assert_eq!(id, low);
    assert_eq!(
        library::list_tags(&fx.conn).unwrap()[0].color.as_deref(),
        Some("black")
    );
    let color: String = fx
        .conn
        .query_row("SELECT color FROM tags WHERE id = ?1", [high], |r| r.get(0))
        .unwrap();
    assert_eq!(color, "red", "ほかのエンティティは触らない");

    // 名前の変更は、同じ名前のエンティティすべてに及ぶ（先頭を改名し、残りは削除）
    library::rename_tag(&mut fx.conn, &e, "重複", "統合").unwrap();
    let tags = library::list_tags(&fx.conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "統合");
    assert_eq!(tags[0].id.as_deref(), Some(low));
    assert_eq!(tags[0].color.as_deref(), Some("black"));
}

#[test]
fn tag_color_can_be_set_cleared_and_validated() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let id = library::set_tag_color(&mut fx.conn, &e, " 色付き ", Some(" #ff0000 ")).unwrap();
    let t = &library::list_tags(&fx.conn).unwrap()[0];
    assert_eq!(
        (t.name.as_str(), t.color.as_deref()),
        ("色付き", Some("#ff0000"))
    );
    // 同じ色なら Op を書かない
    let ops = outbox_len(&fx.conn);
    assert_eq!(
        library::set_tag_color(&mut fx.conn, &e, "色付き", Some("#ff0000")).unwrap(),
        id
    );
    assert_eq!(outbox_len(&fx.conn), ops);
    // 色を外す
    library::set_tag_color(&mut fx.conn, &e, "色付き", None).unwrap();
    assert_eq!(library::list_tags(&fx.conn).unwrap()[0].color, None);
    assert!(matches!(
        library::set_tag_color(&mut fx.conn, &e, "", Some("red")),
        Err(Error::Validation { .. })
    ));
    assert!(matches!(
        library::set_tag_color(&mut fx.conn, &e, "x", Some("  ")),
        Err(Error::Validation { .. })
    ));
}

#[test]
fn rename_tag_edge_cases() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    add(&mut fx, &[("title", json!("A")), ("tags", json!(["x"]))]);
    assert!(
        library::rename_tag(&mut fx.conn, &e, "x", "x")
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        library::rename_tag(&mut fx.conn, &e, "無い", "y"),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        library::rename_tag(&mut fx.conn, &e, "x", "  "),
        Err(Error::Validation { .. })
    ));
}

#[test]
fn modify_tags_adds_and_removes_on_many_books_at_once() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let a = add(
        &mut fx,
        &[("title", json!("A")), ("tags", json!(["x", "y"]))],
    );
    let b = add(&mut fx, &[("title", json!("B"))]);
    let c = add(
        &mut fx,
        &[("title", json!("C")), ("tags", json!(["z", "新"]))],
    );
    let ops = outbox_len(&fx.conn);
    let changed = library::modify_tags(
        &mut fx.conn,
        &e,
        &[a.clone(), b.clone(), c.clone(), a.clone()],
        &["新".to_owned(), " 追加 ".to_owned()],
        &["y".to_owned(), "無い".to_owned()],
    )
    .unwrap();
    assert_eq!(changed, [a.clone(), b.clone(), c.clone()]);
    assert_eq!(
        get_book_detail(&fx.conn, &a).unwrap().tags,
        ["x", "新", "追加"]
    );
    assert_eq!(get_book_detail(&fx.conn, &b).unwrap().tags, ["新", "追加"]);
    assert_eq!(
        get_book_detail(&fx.conn, &c).unwrap().tags,
        ["z", "新", "追加"]
    );
    assert_eq!(outbox_len(&fx.conn), ops + 3);
    // 変わらないときは何も書かない
    let changed = library::modify_tags(&mut fx.conn, &e, &[a], &["新".to_owned()], &[]).unwrap();
    assert!(changed.is_empty());
    assert_eq!(outbox_len(&fx.conn), ops + 3);
    // 失敗したら全体を取り消す
    let r = library::modify_tags(
        &mut fx.conn,
        &e,
        &[b.clone(), "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned()],
        &[],
        &["新".to_owned()],
    );
    assert!(matches!(r, Err(Error::NotFound { .. })));
    assert_eq!(get_book_detail(&fx.conn, &b).unwrap().tags, ["新", "追加"]);
}

// ---------- シリーズ ----------

#[test]
fn find_or_create_series_matches_normalized_names() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let first = library::find_or_create_series(&mut fx.conn, &e, " 岩波文庫 ").unwrap();
    assert_eq!(
        library::find_or_create_series(&mut fx.conn, &e, "岩波文庫").unwrap(),
        first
    );
    let latin = library::find_or_create_series(&mut fx.conn, &e, "ＯＮＥ ＰＩＥＣＥ").unwrap();
    assert_ne!(latin, first);
    assert_eq!(
        library::find_or_create_series(&mut fx.conn, &e, "one piece").unwrap(),
        latin
    );
    let kana = library::find_or_create_series(&mut fx.conn, &e, "ワンピース").unwrap();
    assert_eq!(
        library::find_or_create_series(&mut fx.conn, &e, "わんぴーす").unwrap(),
        kana
    );
    assert_eq!(library::list_series(&fx.conn).unwrap().len(), 3);
    assert!(matches!(
        library::find_or_create_series(&mut fx.conn, &e, "  "),
        Err(Error::Validation { .. })
    ));

    // 作成は 1 つの Op（全項目）
    let op = ops_for(&fx.conn, Entity::Series, &first).pop().unwrap();
    assert_eq!(op.set["name"], json!("岩波文庫"));
    assert_eq!(op.set["deleted"], json!(false));
    assert_eq!(op.set["name_kana"], json!(null));
    assert!(op.set.contains_key("created_at"));
}

#[test]
fn duplicate_series_resolve_to_the_smallest_id_and_deleted_ones_are_not_matched() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let (low, high) = (
        "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c01",
        "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c09",
    );
    for (id, ms) in [(high, 1_000), (low, 2_000)] {
        common::apply(
            &mut fx.conn,
            &common::remote_op(
                common::DEV_B,
                ms,
                0,
                Entity::Series,
                id,
                json!({"name": "重複シリーズ"}),
            ),
        );
    }
    assert_eq!(
        library::find_or_create_series(&mut fx.conn, &e, "重複シリーズ").unwrap(),
        low
    );
    // 削除したシリーズは探さない（新しく作る）
    common::apply(
        &mut fx.conn,
        &common::remote_op(
            common::DEV_B,
            3_000,
            0,
            Entity::Series,
            low,
            json!({"deleted": true}),
        ),
    );
    common::apply(
        &mut fx.conn,
        &common::remote_op(
            common::DEV_B,
            3_000,
            1,
            Entity::Series,
            high,
            json!({"deleted": true}),
        ),
    );
    let fresh = library::find_or_create_series(&mut fx.conn, &e, "重複シリーズ").unwrap();
    assert!(fresh != low && fresh != high);
}

#[test]
fn series_can_be_renamed_and_listed_with_book_counts() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let b_series = library::find_or_create_series(&mut fx.conn, &e, "ブルー").unwrap();
    let a_series = library::find_or_create_series(&mut fx.conn, &e, "あか").unwrap();
    let book =
        |fx: &mut Fx, t: &str, s: &str| add(fx, &[("title", json!(t)), ("series_id", json!(s))]);
    let one = book(&mut fx, "1", &a_series);
    let two = book(&mut fx, "2", &a_series);
    let trash = book(&mut fx, "3", &a_series);
    book(&mut fx, "4", &b_series);
    library::trash_books(&mut fx.conn, &e, &[trash.clone()]).unwrap();
    assert_eq!(
        library::book_ids_in_series(&fx.conn, &a_series).unwrap(),
        [one, two, trash],
        "ゴミ箱の本も含む（一覧の行を取り直すため）"
    );

    let list = library::list_series(&fx.conn).unwrap();
    assert_eq!(
        list.iter()
            .map(|s| (s.name.as_str(), s.book_count))
            .collect::<Vec<_>>(),
        [("あか", 2), ("ブルー", 1)],
        "ヨミ順（正規化）。ゴミ箱の本は数えない"
    );
    library::rename_series(&mut fx.conn, &e, &a_series, "赤").unwrap();
    assert_eq!(
        library::list_series(&fx.conn)
            .unwrap()
            .iter()
            .find(|s| s.id == a_series)
            .unwrap()
            .name,
        "赤"
    );
    // 同名なら Op を書かない
    let ops = outbox_len(&fx.conn);
    library::rename_series(&mut fx.conn, &e, &a_series, "赤").unwrap();
    assert_eq!(outbox_len(&fx.conn), ops);
    assert!(matches!(
        library::rename_series(&mut fx.conn, &e, &a_series, " "),
        Err(Error::Validation { .. })
    ));
    assert!(matches!(
        library::rename_series(
            &mut fx.conn,
            &e,
            "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c77",
            "x"
        ),
        Err(Error::NotFound { .. })
    ));
    // 本のシリーズ ID は検証される
    let fifth = book(&mut fx, "5", &b_series);
    let r = library::update_book(
        &mut fx.conn,
        &e,
        &fifth,
        &[FieldChange::manual(
            "series_id",
            json!("0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c77"),
        )],
        UpdateOptions::default(),
    );
    assert!(matches!(r, Err(Error::Validation { .. })));
}

#[test]
fn tree_operations_replay_to_another_database_with_the_same_result() {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let home = loc(&mut fx, "自宅", None);
    let a = loc(&mut fx, "A", Some(&home));
    let b = loc(&mut fx, "B", Some(&home));
    library::reorder_locations(&mut fx.conn, &e, Some(&home), &[b.clone(), a.clone()]).unwrap();
    library::move_location(&mut fx.conn, &e, &a, None).unwrap();
    library::rename_location(&mut fx.conn, &e, &b, "B2").unwrap();
    library::delete_location(&mut fx.conn, &e, &b).unwrap();
    library::set_tag_color(&mut fx.conn, &e, "t", Some("red")).unwrap();
    library::find_or_create_series(&mut fx.conn, &e, "S").unwrap();

    let mut other = common::mem_db();
    for op in pending_ops(&fx.conn).unwrap() {
        common::apply(&mut other, &op);
    }
    assert_eq!(
        dump_canonical(&fx.conn).unwrap(),
        dump_canonical(&other).unwrap()
    );
}
