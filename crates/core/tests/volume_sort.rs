//! 巻次（`volume`）から並べ替え用の数値（`volume_sort`）を自動で作る規則のテスト（0006 §2）。

mod common;
mod lib_common;

use bibcontainer_core::library::{self, FieldChange, UpdateOptions};
use bibcontainer_core::{get_book_detail, pending_ops};
use serde_json::json;

use lib_common::*;

fn detail(fx: &Fx, id: &str) -> bibcontainer_core::BookDetail {
    get_book_detail(&fx.conn, id).unwrap()
}

#[test]
fn creating_a_book_with_a_volume_derives_the_sort_value_in_the_same_op() {
    let mut fx = Fx::new();
    let before = pending_ops(&fx.conn).unwrap().len();
    let id = add(
        &mut fx,
        &[("title", json!("T")), ("volume", json!("第12巻"))],
    );
    let d = detail(&fx, &id);
    assert_eq!(d.volume_sort, Some(12.0));
    // 巻次と同じ由来（手入力）で、手修正の記録も付く。Op は 1 つ
    assert_eq!(d.field_sources["volume_sort"], "manual");
    assert!(d.manual_fields.contains(&"volume_sort".to_owned()));
    assert_eq!(pending_ops(&fx.conn).unwrap().len(), before + 1);

    // 取得元の由来なら、取得元の由来で入る
    let id = library::create_book(
        &mut fx.conn,
        &env(fx.clock),
        &[
            FieldChange::manual("title", json!("T")),
            FieldChange::provider("volume", json!("下"), "ndl"),
        ],
    )
    .unwrap();
    let d = detail(&fx, &id);
    assert_eq!(d.volume_sort, Some(3.0));
    assert_eq!(d.field_sources["volume_sort"], "ndl");
    assert!(!d.manual_fields.contains(&"volume_sort".to_owned()));
}

#[test]
fn an_explicit_sort_value_or_an_unreadable_volume_is_left_alone() {
    let mut fx = Fx::new();
    // 明示した値（null を含む）が優先
    let id = add(
        &mut fx,
        &[("volume", json!("3")), ("volume_sort", json!(30))],
    );
    assert_eq!(detail(&fx, &id).volume_sort, Some(30.0));
    let id = add(
        &mut fx,
        &[("volume", json!("3")), ("volume_sort", json!(null))],
    );
    assert_eq!(detail(&fx, &id).volume_sort, None);
    // 読み取れない巻次では、入れない
    let id = add(&mut fx, &[("volume", json!("別巻"))]);
    let d = detail(&fx, &id);
    assert_eq!(d.volume_sort, None);
    assert!(!d.field_sources.contains_key("volume_sort"));
    // 巻次がなければ何もしない
    let id = add_titled(&mut fx, "巻次なし");
    assert_eq!(detail(&fx, &id).volume_sort, None);
}

#[test]
fn editing_the_volume_updates_the_sort_value_in_the_same_op() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("title", json!("T")), ("volume", json!("1"))]);
    assert_eq!(detail(&fx, &id).volume_sort, Some(1.0));
    let before = pending_ops(&fx.conn).unwrap().len();

    let report = edit(&mut fx, &id, &[("volume", json!("第十二巻"))]);
    assert_eq!(report.written, ["volume", "volume_sort"]);
    assert_eq!(detail(&fx, &id).volume_sort, Some(12.0));
    assert_eq!(pending_ops(&fx.conn).unwrap().len(), before + 1, "同じ Op");

    // 読み取れない巻次にすると、古い数値を残さない
    edit(&mut fx, &id, &[("volume", json!("別巻"))]);
    assert_eq!(detail(&fx, &id).volume_sort, None);
    assert_eq!(detail(&fx, &id).volume.as_deref(), Some("別巻"));

    // 巻次を消すと、数値も消える
    edit(&mut fx, &id, &[("volume", json!("2"))]);
    assert_eq!(detail(&fx, &id).volume_sort, Some(2.0));
    edit(&mut fx, &id, &[("volume", json!(null))]);
    assert_eq!(detail(&fx, &id).volume_sort, None);

    // 同じ巻次の保存は、何も書かない
    edit(&mut fx, &id, &[("volume", json!("5"))]);
    let ops = pending_ops(&fx.conn).unwrap().len();
    let report = edit(&mut fx, &id, &[("volume", json!("5"))]);
    assert!(report.written.is_empty());
    assert_eq!(pending_ops(&fx.conn).unwrap().len(), ops);
}

#[test]
fn an_explicit_sort_value_in_an_update_wins() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("volume", json!("1"))]);
    let report = edit(
        &mut fx,
        &id,
        &[("volume", json!("2")), ("volume_sort", json!(2.5))],
    );
    assert_eq!(report.written, ["volume", "volume_sort"]);
    assert_eq!(detail(&fx, &id).volume_sort, Some(2.5));
}

#[test]
fn a_provider_does_not_override_a_manually_edited_volume_or_its_sort_value() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("volume", json!("3"))]); // 巻次も数値も手修正の記録つき
    let provider = [FieldChange::provider("volume", json!("1"), "openbd")];
    let report = library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        &id,
        &provider,
        UpdateOptions::default(),
    )
    .unwrap();
    assert_eq!(report.skipped_manual, ["volume"]);
    assert!(
        report.written.is_empty(),
        "巻次が書かれないので、数値も作らない"
    );
    assert_eq!(detail(&fx, &id).volume_sort, Some(3.0));

    // 取り込み時に巻次だけが手修正で、数値には記録がない古い本でも、巻次が書かれなければ数値は変わらない
    let id2 = add_titled(&mut fx, "古い本");
    edit(&mut fx, &id2, &[("volume", json!("3"))]);
    // 数値の手修正の記録だけ外した状態にする（古い版が作った本の再現）
    library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        &id2,
        &[FieldChange::provider("volume_sort", json!(3), "openbd")],
        UpdateOptions {
            override_manual: true,
        },
    )
    .unwrap();
    assert!(
        !detail(&fx, &id2)
            .manual_fields
            .contains(&"volume_sort".to_owned())
    );
    let report = library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        &id2,
        &provider,
        UpdateOptions::default(),
    )
    .unwrap();
    assert_eq!(report.skipped_manual, ["volume"]);
    assert_eq!(detail(&fx, &id2).volume_sort, Some(3.0));

    // overrideManual なら、巻次も数値も取得元の値になる
    let report = library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        &id,
        &provider,
        UpdateOptions {
            override_manual: true,
        },
    )
    .unwrap();
    assert_eq!(report.written, ["volume", "volume_sort"]);
    let d = detail(&fx, &id);
    assert_eq!(d.volume_sort, Some(1.0));
    assert_eq!(d.field_sources["volume_sort"], "openbd");
}

#[test]
fn the_op_carries_the_derived_value_so_other_devices_do_not_recompute_it() {
    let mut fx = Fx::new();
    let id = add(&mut fx, &[("title", json!("T")), ("volume", json!("巻七"))]);
    edit(&mut fx, &id, &[("volume", json!("八"))]);
    let op = last_op(&fx.conn);
    assert_eq!(op.set["volume"], json!("八"));
    assert_eq!(op.set["volume_sort"], json!(8.0));
}
