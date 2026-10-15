//! 同期エンジンが使う DB 側の操作（`sync_state`）のテスト。

mod common;

use bibcontainer_core::backup::{self, BackupKind};
use bibcontainer_core::sync_state::{
    self, AppliedRanges, EntityState, apply_entity_state, export_state,
};
use bibcontainer_core::{Entity, ManualClock, db, meta, pending_ops, record_change_committed};
use serde_json::{Value, json};
use tempfile::tempdir;

use common::*;

fn rec(conn: &mut rusqlite::Connection, clock: &ManualClock, ent: Entity, id: &str, set: Value) {
    clock.advance(1);
    record_change_committed(conn, clock, ent, id, kv(set)).unwrap();
}

#[test]
fn library_id_join_and_leave() {
    let mut conn = mem_db();
    assert_eq!(sync_state::library_id(&conn).unwrap(), None);
    sync_state::set_library_id(&conn, "lib-1").unwrap();
    assert_eq!(
        sync_state::library_id(&conn).unwrap().as_deref(),
        Some("lib-1")
    );
    sync_state::clear_library_id(&conn).unwrap();
    assert_eq!(sync_state::library_id(&conn).unwrap(), None);
    assert!(sync_state::set_library_id(&conn, "").is_err());

    // 参加すると、ID と印がそろって入る
    {
        let tx = db::write_tx(&mut conn).unwrap();
        sync_state::join_library(&tx, "lib-1", true).unwrap();
        sync_state::record_applied(&tx, DEV_B, 1, 10).unwrap();
        sync_state::mark_snapshot_seen(&tx, DEV_B, "0001-00001").unwrap();
        sync_state::set_last_snapshot(&tx, 5, 123).unwrap();
        tx.commit().unwrap();
    }
    assert!(sync_state::needs_snapshot(&conn).unwrap());
    // 同じライブラリへの再設定は、記録を引き継ぐ（印は指定どおり）
    {
        let tx = db::write_tx(&mut conn).unwrap();
        sync_state::join_library(&tx, "lib-1", false).unwrap();
        tx.commit().unwrap();
    }
    assert!(!sync_state::needs_snapshot(&conn).unwrap());
    assert_eq!(sync_state::contiguous_applied(&conn, DEV_B).unwrap(), 10);
    assert_eq!(
        sync_state::snapshot_seen(&conn)
            .unwrap()
            .get(DEV_B)
            .map(String::as_str),
        Some("0001-00001")
    );
    assert_eq!(sync_state::last_snapshot(&conn).unwrap(), Some((5, 123)));
    // 別のライブラリに切り替えると、前のライブラリの記録は消える
    {
        let tx = db::write_tx(&mut conn).unwrap();
        sync_state::join_library(&tx, "lib-2", false).unwrap();
        tx.commit().unwrap();
    }
    assert!(sync_state::applied_ranges(&conn).unwrap().is_empty());
    assert!(sync_state::snapshot_seen(&conn).unwrap().is_empty());
    assert_eq!(sync_state::last_snapshot(&conn).unwrap(), None);
    // 解除
    {
        let tx = db::write_tx(&mut conn).unwrap();
        sync_state::record_applied(&tx, DEV_B, 1, 3).unwrap();
        sync_state::set_needs_snapshot(&tx, true).unwrap();
        sync_state::leave_library(&tx).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(sync_state::library_id(&conn).unwrap(), None);
    assert!(!sync_state::needs_snapshot(&conn).unwrap());
    assert!(sync_state::applied_ranges(&conn).unwrap().is_empty());
    // 端末 ID などの他の記録は消さない
    assert!(meta::device_id(&conn).is_ok());
}

#[test]
fn snapshot_seen_never_goes_back() {
    let conn = mem_db();
    sync_state::mark_snapshot_seen(&conn, DEV_A, "002").unwrap();
    sync_state::mark_snapshot_seen(&conn, DEV_A, "001").unwrap();
    sync_state::mark_snapshot_seen(&conn, DEV_B, "005").unwrap();
    let seen = sync_state::snapshot_seen(&conn).unwrap();
    assert_eq!(seen[DEV_A], "002");
    assert_eq!(seen[DEV_B], "005");
}

#[test]
fn applied_segments_record_merge_and_cover() {
    let conn = mem_db();
    sync_state::record_applied(&conn, DEV_A, 1, 10).unwrap();
    sync_state::record_applied(&conn, DEV_A, 21, 30).unwrap();
    sync_state::record_applied(&conn, DEV_B, 1, 5).unwrap();
    assert!(sync_state::is_range_applied(&conn, DEV_A, 1, 10).unwrap());
    assert!(sync_state::is_range_applied(&conn, DEV_A, 2, 9).unwrap());
    assert!(!sync_state::is_range_applied(&conn, DEV_A, 5, 25).unwrap());
    assert!(!sync_state::is_range_applied(&conn, DEV_C, 1, 1).unwrap());
    assert_eq!(sync_state::contiguous_applied(&conn, DEV_A).unwrap(), 10);
    // 同じ seq_from を、大きい範囲で記録し直す（統合したセグメント）
    sync_state::record_applied(&conn, DEV_A, 1, 20).unwrap();
    assert_eq!(sync_state::contiguous_applied(&conn, DEV_A).unwrap(), 30);
    // 小さい範囲では縮まない
    sync_state::record_applied(&conn, DEV_A, 1, 3).unwrap();
    assert_eq!(sync_state::contiguous_applied(&conn, DEV_A).unwrap(), 30);
    let all = sync_state::applied_ranges(&conn).unwrap();
    assert_eq!(all.ranges(DEV_A), &[(1, 30)]);
    assert_eq!(all.devices().collect::<Vec<_>>(), vec![DEV_A, DEV_B]);
    assert!(sync_state::record_applied(&conn, DEV_A, 0, 3).is_err());
    assert!(sync_state::record_applied(&conn, DEV_A, 5, 3).is_err());
    sync_state::clear_applied(&conn).unwrap();
    assert_eq!(
        sync_state::applied_ranges(&conn).unwrap(),
        AppliedRanges::default()
    );
}

#[test]
fn outbox_helpers() {
    let mut conn = mem_db();
    let clock = ManualClock::new(1_000_000);
    for i in 0..7 {
        rec(
            &mut conn,
            &clock,
            Entity::Book,
            &format!("0192f3a0-6c1e-7cc3-9b1a-{i:012x}"),
            json!({"title": format!("本 {i}")}),
        );
    }
    assert_eq!(sync_state::current_seq(&conn).unwrap(), 7);
    assert_eq!(sync_state::pending_count(&conn).unwrap(), 7);
    let first = sync_state::pending_ops_after(&conn, 0, 3).unwrap();
    assert_eq!(
        first.iter().map(|o| o.seq).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let next = sync_state::pending_ops_after(&conn, 3, 100).unwrap();
    assert_eq!(
        next.iter().map(|o| o.seq).collect::<Vec<_>>(),
        vec![4, 5, 6, 7]
    );
    assert_eq!(sync_state::mark_flushed(&conn, 1, 3).unwrap(), 3);
    assert_eq!(
        sync_state::mark_flushed(&conn, 1, 3).unwrap(),
        0,
        "二度目は変えない"
    );
    assert_eq!(pending_ops(&conn).unwrap().len(), 4);
    assert_eq!(sync_state::mark_flushed_through(&conn, 5).unwrap(), 2);
    assert_eq!(sync_state::pending_count(&conn).unwrap(), 2);
    assert_eq!(
        sync_state::outbox_hlc_ms(&conn, 4).unwrap(),
        Some(1_000_004)
    );
    assert_eq!(sync_state::outbox_hlc_ms(&conn, 99).unwrap(), None);
    // 書き出し済みにしても、seq のカウンタは進んだまま
    sync_state::mark_flushed_through(&conn, 7).unwrap();
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        "0192f3a0-6c1e-7cc3-9b1a-0000000000ff",
        json!({"title": "8"}),
    );
    assert_eq!(
        sync_state::pending_ops_after(&conn, 0, 10).unwrap()[0].seq,
        8
    );
}

/// 状態の書き出し → 別の DB への適用で、`dump_canonical`（行と項目時計）が一致する。
#[test]
fn exported_state_reproduces_every_entity_and_clock() {
    let mut src = mem_db();
    let ops = [
        remote_op(
            DEV_A,
            1_000,
            0,
            Entity::Book,
            BOOK_1,
            json!({"title": "書名", "rating": 4, "authors": [{"name": "a"}], "volume_sort": 1.0, "deleted": false,
                   "zz_unknown": {"k": [1, 2.5]}, "field_sources.title": "openbd", "manual_fields.memo": true,
                   "memo": "手で書いた", "tags": ["a", "研究/SiC"], "cover_hash": "ab".repeat(32)}),
        ),
        remote_op(
            DEV_B,
            1_500,
            0,
            Entity::Book,
            BOOK_1,
            json!({"title": "新しい書名", "rating": null, "field_sources.title": null}),
        ),
        remote_op(
            DEV_C,
            1_200,
            3,
            Entity::Book,
            BOOK_2,
            json!({"deleted": true, "purged": true}),
        ),
        remote_op(
            DEV_A,
            1_100,
            0,
            Entity::Series,
            SERIES_1,
            json!({"name": "シリーズ", "zz_series": 1}),
        ),
        remote_op(
            DEV_A,
            1_100,
            1,
            Entity::Location,
            "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01",
            json!({"name": "書斎", "sort_order": 2.5}),
        ),
        remote_op(
            DEV_A,
            1_100,
            2,
            Entity::Tag,
            TAG_1,
            json!({"name": "タグ", "color": null}),
        ),
        remote_op(
            DEV_A,
            1_100,
            3,
            Entity::Shelf,
            "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0e01",
            json!({"name": "本棚", "query": {"q": "a"}}),
        ),
        // 項目がすべて不正で、時計のない行になる
        remote_op(
            DEV_A,
            1_100,
            4,
            Entity::Tag,
            TAG_2_FOR_TEST,
            json!({"name": 12}),
        ),
    ];
    apply_all(&mut src, &ops);

    let state = export_state(&src).unwrap();
    // 行ごとの項目の値と HLC が入る
    let book = state
        .iter()
        .find(|e| e.ent == Entity::Book && e.id == BOOK_1)
        .unwrap();
    assert_eq!(
        book.fields["title"],
        (json!("新しい書名"), format!("{:015}:00000:{DEV_B}", 1_500))
    );
    assert_eq!(book.fields["zz_unknown"].0, json!({"k": [1, 2.5]}));
    assert_eq!(
        book.fields["field_sources.title"].0,
        Value::Null,
        "null で消した記録も時計を持つ"
    );
    assert_eq!(book.fields["manual_fields.memo"].0, json!(true));
    assert_eq!(book.fields["volume_sort"].0, json!(1.0));
    let empty = state.iter().find(|e| e.id == TAG_2_FOR_TEST).unwrap();
    assert!(empty.fields.is_empty(), "時計のない行も、行として出す");

    let mut dst = mem_db();
    {
        let tx = db::write_tx(&mut dst).unwrap();
        for e in &state {
            apply_entity_state(&tx, e).unwrap();
        }
        tx.commit().unwrap();
    }
    assert_eq!(dump(&src), dump(&dst));
    // 適用は冪等で、もう一度重ねても変わらず、古い時計の値には負ける
    {
        let tx = db::write_tx(&mut dst).unwrap();
        for e in &state {
            assert!(!apply_entity_state(&tx, e).unwrap().changed());
        }
        tx.commit().unwrap();
    }
    assert_eq!(dump(&src), dump(&dst));
    // 検索用の派生データも、適用で作られる
    assert_eq!(
        bibcontainer_core::search_books(&dst, "新しい書名").unwrap(),
        vec![BOOK_1.to_owned()]
    );
}

const TAG_2_FOR_TEST: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0b02";

#[test]
fn applying_a_state_over_newer_local_values_keeps_the_newer_ones() {
    let mut local = mem_db();
    apply(
        &mut local,
        &remote_op(
            DEV_A,
            5_000,
            0,
            Entity::Book,
            BOOK_1,
            json!({"title": "新しい", "rating": 5}),
        ),
    );
    let old_state = EntityState {
        ent: Entity::Book,
        id: BOOK_1.to_owned(),
        fields: [
            (
                "title".to_owned(),
                (json!("古い"), format!("{:015}:00000:{DEV_B}", 1_000)),
            ),
            (
                "memo".to_owned(),
                (json!("古いメモ"), format!("{:015}:00000:{DEV_B}", 1_000)),
            ),
        ]
        .into_iter()
        .collect(),
    };
    let tx = db::write_tx(&mut local).unwrap();
    let out = apply_entity_state(&tx, &old_state).unwrap();
    tx.commit().unwrap();
    assert_eq!((out.applied, out.stale), (1, 1));
    let d = dump(&local);
    let row = dump_row(&d, "books", BOOK_1);
    assert_eq!(row["title"], json!("新しい"));
    assert_eq!(row["memo"], json!("古いメモ"));
}

#[test]
fn snapshot_basis_is_consistent_and_its_hlc_increases() {
    let mut conn = mem_db();
    let clock = ManualClock::new(10_000);
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        json!({"title": "本", "cover_hash": "ab".repeat(32), "purged": true}),
    );
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_2,
        json!({"title": "本 2", "cover_hash": "cd".repeat(32)}),
    );
    sync_state::record_applied(&conn, DEV_B, 1, 4).unwrap();
    sync_state::record_applied(&conn, DEV_B, 6, 9).unwrap();
    sync_state::record_applied(&conn, DEV_C, 2, 3).unwrap();
    let me = meta::device_id(&conn).unwrap();

    let first = sync_state::export_snapshot_basis(&mut conn, 10_000).unwrap();
    assert_eq!(first.device_id, me);
    assert_eq!(first.seq, 2);
    assert_eq!(first.covers.get(&me), Some(&2));
    assert_eq!(
        first.covers.get(DEV_B),
        Some(&4),
        "切れ目なく反映した所まで"
    );
    assert!(
        !first.covers.contains_key(DEV_C),
        "1 から始まっていない端末は含めない"
    );
    assert_eq!(first.entities.len(), 2);
    assert_eq!(
        first.cover_hashes,
        vec!["ab".repeat(32), "cd".repeat(32)],
        "完全削除した本の書影も写す対象"
    );
    assert_eq!(
        sync_state::referenced_cover_hashes(&conn).unwrap(),
        vec!["cd".repeat(32)],
        "整理・届いていない書影の数え方は、完全削除していない本だけ"
    );
    // 続けて呼んでも HLC は増える（名前が重ならない）
    let second = sync_state::export_snapshot_basis(&mut conn, 10_000).unwrap();
    assert!(second.hlc > first.hlc);
    // 時計が戻っても逆行しない
    let third = sync_state::export_snapshot_basis(&mut conn, 1).unwrap();
    assert!(third.hlc > second.hlc);
    // 変更の記録の HLC とも、逆行しない
    clock.set(1);
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        json!({"memo": "m"}),
    );
    let fourth = sync_state::export_snapshot_basis(&mut conn, 1).unwrap();
    assert!(fourth.hlc > third.hlc);
    assert_eq!(fourth.seq, 3);
}

#[test]
fn local_data_detection_and_summary() {
    let mut conn = mem_db();
    assert!(!sync_state::has_local_data(&conn).unwrap());
    assert_eq!(
        sync_state::data_summary(&conn).unwrap(),
        sync_state::DataSummary::default()
    );
    let clock = ManualClock::new(10_000);
    rec(&mut conn, &clock, Entity::Tag, TAG_1, json!({"name": "t"}));
    assert!(sync_state::has_local_data(&conn).unwrap());
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        json!({"title": "a"}),
    );
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_2,
        json!({"title": "b", "deleted": true}),
    );
    rec(
        &mut conn,
        &clock,
        Entity::Series,
        SERIES_1,
        json!({"name": "s", "deleted": true}),
    );
    let s = sync_state::data_summary(&conn).unwrap();
    assert_eq!(
        (
            s.books,
            s.trashed_books,
            s.series,
            s.tags,
            s.locations,
            s.shelves
        ),
        (2, 1, 0, 1, 0, 0)
    );
    assert_eq!(s.pending_ops, 4);
    // 他端末の Op だけでも「データがある」
    let mut other = mem_db();
    apply(
        &mut other,
        &remote_op(
            DEV_A,
            1,
            0,
            Entity::Shelf,
            "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0e01",
            json!({"name": "x"}),
        ),
    );
    assert!(sync_state::has_local_data(&other).unwrap());
}

/// 復元で「スナップショットが必要」の印が立ち、退避した Op は `clear_outbox_restored` で捨てられる。
#[test]
fn restore_sets_the_needs_snapshot_mark() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("library.sqlite3");
    let backups = dir.path().join("backups");
    let clock = ManualClock::new(1_791_966_600_000);
    let (mut conn, _) = db::open(&db_path, &backups).unwrap();
    rec(
        &mut conn,
        &clock,
        Entity::Book,
        BOOK_1,
        json!({"title": "未書き出し"}),
    );
    sync_state::set_library_id(&conn, "lib-1").unwrap();
    assert!(!sync_state::needs_snapshot(&conn).unwrap());
    let info = backup::create_backup(
        &conn,
        &dir.path().join("manual"),
        BackupKind::Manual,
        &clock,
    )
    .unwrap();
    drop(conn);

    backup::restore_backup(
        std::path::Path::new(&info.path),
        &db_path,
        &dir.path().join("safety"),
        &clock,
    )
    .unwrap();
    let (conn, _) = db::open(&db_path, &backups).unwrap();
    assert!(sync_state::needs_snapshot(&conn).unwrap());
    assert_eq!(
        sync_state::library_id(&conn).unwrap().as_deref(),
        Some("lib-1"),
        "ライブラリへの所属は引き継ぐ"
    );
    assert_eq!(sync_state::current_seq(&conn).unwrap(), 0);
    assert_eq!(sync_state::pending_count(&conn).unwrap(), 0);
    let moved: i64 = conn
        .query_row("SELECT count(*) FROM outbox_restored", [], |r| r.get(0))
        .unwrap();
    assert_eq!(moved, 1);
    assert_eq!(sync_state::clear_outbox_restored(&conn).unwrap(), 1);
}
