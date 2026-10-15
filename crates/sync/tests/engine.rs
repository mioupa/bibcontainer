//! 同期エンジンの個別の機能のテスト。

mod common;

use std::path::Path;

use bibcontainer_core::backup::{self, BackupKind};
use bibcontainer_core::schema::Entity;
use bibcontainer_core::sync_state;
use bibcontainer_core::{Clock, db, meta};
use bibcontainer_sync::engine::SyncOptions;
use bibcontainer_sync::layout::{cover_remote_path, segment_path, snapshot_path};
use bibcontainer_sync::{
    FolderKind, SetupMode, SyncEngine, SyncError, SyncState, TransportErrorKind,
};
use serde_json::json;

use common::*;

fn start(backend: &Backend) -> (Dev, Dev) {
    let a = Dev::new("A", backend);
    let b = Dev::new("B", backend);
    a.engine().setup(SetupMode::Create).unwrap();
    b.engine().setup(SetupMode::Join).unwrap();
    (a, b)
}

fn each_backend(test: impl Fn(&Backend)) {
    for backend in [Backend::memory(), Backend::local()] {
        test(&backend);
    }
}

fn files_under(backend: &Backend, prefix: &str) -> Vec<String> {
    backend
        .paths()
        .into_iter()
        .filter(|p| p.starts_with(prefix))
        .collect()
}

// ---------------------------------------------------------------------------
// 設定（新規作成・参加・統合・解除）と判定
// ---------------------------------------------------------------------------

#[test]
fn inspect_reports_the_kind_of_folder() {
    each_backend(|backend| {
        let a = Dev::new("A", backend);
        let empty = a.engine().inspect("/lib").unwrap();
        assert_eq!(empty.kind, FolderKind::Empty);
        assert_eq!(empty.library_id, None);
        assert!(!empty.local_has_data);
        assert_eq!(empty.library_folder, "/lib");

        a.add_book(BOOK_1, "手元の本");
        assert!(a.engine().inspect("/lib").unwrap().local_has_data);
        // 調べただけでは、何も作らない
        assert!(backend.paths().is_empty(), "{}", backend.name());

        a.engine().setup(SetupMode::Create).unwrap();
        let b = Dev::new("B", backend);
        let found = b.engine().inspect("/lib").unwrap();
        assert_eq!(found.kind, FolderKind::Library);
        assert!(found.library_id.is_some());
        assert_eq!(found.devices.len(), 1);
        assert_eq!(found.devices[0].name, "A");
        assert!(!found.devices[0].is_self);
        assert_eq!(found.local_library_id, None);
        // 自分から見れば isSelf
        let own = a.engine().inspect("/lib").unwrap();
        assert!(own.devices[0].is_self);
        assert_eq!(own.local_library_id, own.library_id);
        assert_eq!(own.kind, FolderKind::Library);
    });
}

#[test]
fn inspect_detects_update_required_not_writable_and_other_library() {
    let backend = Backend::memory();
    let (a, b) = start(&backend);
    // 新しいアプリが必要なライブラリ
    let path = "library.json";
    let mut lib: serde_json::Value =
        serde_json::from_slice(&backend.read_raw(path).unwrap()).unwrap();
    lib["minAppVersion"] = json!("9.0.0");
    backend.write_raw(path, lib.to_string().as_bytes());
    let found = b.engine().inspect("/lib").unwrap();
    assert_eq!(found.kind, FolderKind::UpdateRequired);
    let err = b.engine().import().unwrap_err();
    assert!(matches!(err, SyncError::UpdateRequired { .. }), "{err}");
    assert_eq!(err.state(), SyncState::UpdateRequired);
    a.add_book(BOOK_3, "更新が必要なとき、書き出しもしない");
    assert!(matches!(
        a.engine().export().unwrap_err(),
        SyncError::UpdateRequired { .. }
    ));
    assert_eq!(a.pending(), 1);
    let c = Dev::new("C", &backend);
    assert!(matches!(
        c.engine().setup(SetupMode::Join).unwrap_err(),
        SyncError::UpdateRequired { .. }
    ));
    // 新しい形式の版も同じ
    lib["minAppVersion"] = json!("0.1.0");
    lib["formatVersion"] = json!(2);
    backend.write_raw(path, lib.to_string().as_bytes());
    assert_eq!(
        b.engine().inspect("/lib").unwrap().kind,
        FolderKind::UpdateRequired
    );
    lib["formatVersion"] = json!(1);
    backend.write_raw(path, lib.to_string().as_bytes());
    assert_eq!(
        b.engine().inspect("/lib").unwrap().kind,
        FolderKind::Library
    );

    // 書き込めない
    let Backend::Memory(folder) = &backend else {
        unreachable!()
    };
    let t = folder.transport("B");
    t.set_fail_writes(Some(TransportErrorKind::PermissionDenied));
    assert_eq!(
        b.engine().inspect("/lib").unwrap().kind,
        FolderKind::NotWritable
    );
    t.set_fail_writes(None);

    // 空のフォルダも、書き込めなければ notWritable
    let empty_backend = Backend::memory();
    let d = Dev::new("D", &empty_backend);
    let Backend::Memory(ef) = &empty_backend else {
        unreachable!()
    };
    ef.transport("D")
        .set_fail_writes(Some(TransportErrorKind::StorageFull));
    assert_eq!(
        d.engine().inspect("/x").unwrap().kind,
        FolderKind::NotWritable
    );
}

#[test]
fn creating_a_library_with_local_data_writes_a_snapshot_that_joiners_use() {
    each_backend(|backend| {
        let a = Dev::new("A", backend);
        for i in 0..4 {
            let hash = a.make_cover(format!("書影 {i}").as_bytes());
            a.set_book(
                &book_id(i),
                json!({"title": format!("本 {i}"), "cover_hash": hash}),
            );
        }
        a.set(Entity::Location, LOC_1, json!({"name": "書斎"}));
        assert_eq!(a.pending(), 5);
        let report = a.engine().setup(SetupMode::Create).unwrap();
        assert!(report.created && report.snapshot_written);
        assert_eq!(
            a.pending(),
            0,
            "スナップショットに含めた Op は書き出し済みになる"
        );
        assert!(!files_under(backend, "snapshots/").is_empty());
        assert!(
            files_under(backend, "ops/").is_empty(),
            "セグメントには書かない"
        );
        assert!(!sync_state::needs_snapshot(&a.db.lock()).unwrap());

        let b = Dev::new("B", backend);
        let setup = b.engine().setup(SetupMode::Join).unwrap();
        assert_eq!(setup.import.unwrap().snapshots_applied, 1);
        assert_converged(&[&a, &b]);
        assert_eq!(b.count("books"), 4);
        assert_eq!(b.count("locations"), 1);
        assert!(b.has_cover(a.book_col(&book_id(0), "cover_hash").as_str().unwrap()));
        // 新しい Op は普通にセグメントに書かれる
        a.add_book(BOOK_2, "あとの本");
        settle(&[&a, &b]);
        assert_eq!(b.book_title(BOOK_2).as_deref(), Some("あとの本"));
        assert!(!files_under(backend, "ops/").is_empty());
    });
}

#[test]
fn create_refuses_an_existing_library_and_join_refuses_an_empty_folder() {
    each_backend(|backend| {
        let (_a, b) = start(backend);
        let c = Dev::new("C", backend);
        assert!(matches!(
            c.engine().setup(SetupMode::Create).unwrap_err(),
            SyncError::LibraryExists
        ));
        // 既に参加している端末は、新しく作れない
        assert!(matches!(
            b.engine().setup(SetupMode::Create).unwrap_err(),
            SyncError::LibraryExists | SyncError::AlreadyInLibrary { .. }
        ));
        let other = if backend.name() == "memory" {
            Backend::memory()
        } else {
            Backend::local()
        };
        let d = Dev::new("D", &other);
        assert!(matches!(
            d.engine().setup(SetupMode::Join).unwrap_err(),
            SyncError::LibraryMissing
        ));
        // 参加済みの端末は、空のフォルダに新しいライブラリを作れない（先に解除する）
        let other_transport = other.transport("B");
        let b_in_empty = bibcontainer_sync::SyncEngine::new(
            &b.db,
            &*other_transport,
            &b.clock,
            b.covers.path(),
            b.info.clone(),
        );
        assert!(matches!(
            b_in_empty.setup(SetupMode::Create).unwrap_err(),
            SyncError::AlreadyInLibrary { .. }
        ));
    });
}

/// 統合: 手元のデータ＋ライブラリ → 両方残る。既に参加している端末にも、統合した端末のデータが届く。
#[test]
fn merging_keeps_both_sides_and_reaches_devices_already_in_the_library() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        let a_cover = a.make_cover(b"A cover");
        a.set_book(
            &book_id(1),
            json!({"title": "ライブラリの本 1", "cover_hash": a_cover}),
        );
        a.add_book(&book_id(2), "ライブラリの本 2");
        settle(&[&a, &b]);

        // C は手元に別の本（同じ本が別の ID で入っている）を持っている
        let c = Dev::new("C", backend);
        let c_cover = c.make_cover(b"C cover");
        c.set_book(
            &book_id(101),
            json!({"title": "ライブラリの本 1", "cover_hash": c_cover}),
        );
        c.add_book(&book_id(102), "C だけの本");
        c.set(Entity::Tag, TAG_1, json!({"name": "C のタグ"}));
        assert!(c.engine().inspect("/lib").unwrap().local_has_data);
        // そのままの参加は断る
        assert!(matches!(
            c.engine().setup(SetupMode::Join).unwrap_err(),
            SyncError::LocalHasData
        ));
        assert!(matches!(
            c.engine().setup(SetupMode::JoinReplace).unwrap_err(),
            SyncError::LocalHasData
        ));
        assert_eq!(
            sync_state::library_id(&c.db.lock()).unwrap(),
            None,
            "断ったときは何も記録しない"
        );

        let summary = c.engine().local_summary().unwrap();
        assert_eq!(
            (summary.books, summary.tags, summary.pending_ops),
            (2, 1, 3)
        );

        let report = c.engine().setup(SetupMode::JoinMerge).unwrap();
        assert!(report.snapshot_written);
        assert_eq!(c.pending(), 0);
        assert_eq!(c.count("books"), 4, "両方の本が残る");
        // 既に参加している端末にも届く（スナップショットだけが運ぶ）
        let a_report = import(&a);
        assert_eq!(a_report.snapshots_applied, 1, "{}", backend.name());
        import(&b);
        assert_eq!(a.count("books"), 4);
        assert_eq!(b.count("books"), 4);
        assert!(a.has_cover(&c_cover) && b.has_cover(&c_cover));
        assert!(c.has_cover(&a_cover));
        settle(&[&a, &b, &c]);
        assert_converged(&[&a, &b, &c]);
        // 2 回目の取り込みでは、同じスナップショットを読み直さない
        assert_eq!(import(&a).snapshots_applied, 0);
    });
}

#[test]
fn replace_joins_like_a_plain_join_on_a_fresh_database() {
    each_backend(|backend| {
        let (a, _b) = start(backend);
        a.add_book(BOOK_1, "ライブラリの本");
        export(&a);
        // アプリが DB を作り直した（新しい空の DB、新しい端末 ID）あと
        let fresh = Dev::new("A-new", backend);
        fresh.engine().setup(SetupMode::JoinReplace).unwrap();
        assert_eq!(fresh.count("books"), 1);
        assert_ne!(fresh.id(), a.id());
    });
}

#[test]
fn leave_keeps_local_data_and_the_folder_and_rejoining_the_same_library_merges_cleanly() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "本 1");
        settle(&[&a, &b]);
        let before = backend.paths();
        b.engine().leave().unwrap();
        assert_eq!(backend.paths(), before, "同期フォルダの中身はそのまま");
        assert_eq!(b.count("books"), 1, "手元のデータもそのまま");
        {
            let conn = b.db.lock();
            assert_eq!(sync_state::library_id(&conn).unwrap(), None);
            assert!(sync_state::applied_ranges(&conn).unwrap().is_empty());
        }
        assert!(matches!(
            b.engine().import().unwrap_err(),
            SyncError::NotConfigured
        ));
        assert!(
            matches!(b.engine().export().unwrap_err(), SyncError::NotConfigured)
                || b.pending() == 0
        );

        // 離れている間の変更
        b.add_book(BOOK_2, "離れている間の本");
        a.set_book(BOOK_1, json!({"rating": 5}));
        export(&a);
        // 手元にデータがあるので、統合で戻る。同じ ID の本は 2 冊にならない。
        b.engine().setup(SetupMode::JoinMerge).unwrap();
        settle(&[&a, &b]);
        assert_converged(&[&a, &b]);
        assert_eq!(b.count("books"), 2);
        assert_eq!(b.book_col(BOOK_1, "rating"), json!(5));
        assert_eq!(a.book_title(BOOK_2).as_deref(), Some("離れている間の本"));
    });
}

#[test]
fn setting_up_the_same_library_again_keeps_progress() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..3 {
            a.add_book(&book_id(i), "本");
            export(&a);
        }
        import(&b);
        // フォルダの場所が変わった（同じライブラリを選び直した）
        let report = b.engine().setup(SetupMode::Join).unwrap();
        let import = report.import.unwrap();
        assert_eq!(import.files_applied, 0, "取り込み済みの範囲を引き継ぐ");
        assert_eq!(b.count("books"), 3);
    });
}

// ---------------------------------------------------------------------------
// 取り込み・書き出しの細部
// ---------------------------------------------------------------------------

#[test]
fn export_does_nothing_when_nothing_is_pending() {
    each_backend(|backend| {
        let (a, _b) = start(backend);
        let before = backend.paths();
        let report = export(&a);
        assert_eq!(report, bibcontainer_sync::ExportReport::default());
        assert_eq!(backend.paths(), before);
        a.add_book(BOOK_1, "本");
        let report = export(&a);
        assert_eq!((report.ops_exported, report.segments_written), (1, 1));
        assert!(report.exported_at_ms.is_some());
        assert_eq!(report.pending_after, 0);
        assert!(backend.paths().contains(&segment_path(&a.id(), 1, 1)));
        // 端末の情報も更新される
        let devices = b_devices(&a);
        assert_eq!(devices.len(), 2);
        assert!(
            devices
                .iter()
                .any(|d| d.device_id == a.id() && d.last_export_at.is_some())
        );
    });
}

fn b_devices(d: &Dev) -> Vec<bibcontainer_sync::SyncDevice> {
    d.engine().devices().unwrap()
}

#[test]
fn segments_hold_at_most_a_thousand_ops_and_split_when_larger() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        {
            let mut conn = a.db.lock();
            let tx = db::write_tx(&mut conn).unwrap();
            for i in 0..2500usize {
                bibcontainer_core::record_change(
                    &tx,
                    &a.clock,
                    Entity::Book,
                    &book_id(i),
                    [("title".to_owned(), json!(format!("本 {i}")))],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let report = export(&a);
        assert_eq!((report.ops_exported, report.segments_written), (2500, 3));
        let id = a.id();
        let names: Vec<String> = files_under(backend, &format!("ops/{id}/"));
        assert_eq!(names.len(), 3);
        assert!(backend.paths().contains(&segment_path(&id, 1, 1000)));
        assert!(backend.paths().contains(&segment_path(&id, 2001, 2500)));
        import(&b);
        assert_eq!(b.count("books"), 2500);
        assert_converged(&[&a, &b]);
    });
}

#[test]
fn a_crash_between_writing_and_flushing_is_repaired_by_the_next_export() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "本 1");
        a.add_book(BOOK_2, "本 2");
        export(&a);
        // 書いたあと、flushed にする前に落ちたことにする
        a.db.lock()
            .execute("UPDATE outbox SET flushed = 0", [])
            .unwrap();
        let report = export(&a);
        assert_eq!(
            report.segments_written, 0,
            "同じ内容のファイルがあるので書き直さない"
        );
        assert_eq!(report.ops_exported, 2);
        assert_eq!(a.pending(), 0);
        import(&b);
        assert_eq!(b.count("books"), 2);

        // 内容が違うファイルがあれば、同じ端末 ID を別の端末が使っているとみなす
        a.add_book(BOOK_3, "本 3");
        let path = segment_path(&a.id(), 3, 3);
        backend.write_raw(
            &path,
            &bibcontainer_sync::format::encode_segment(&[]).unwrap(),
        );
        let err = a.engine().export().unwrap_err();
        assert!(matches!(err, SyncError::DuplicateDevice { .. }), "{err}");
        assert_eq!(a.pending(), 1, "止めたので、未書き出しのまま");
    });
}

#[test]
fn a_device_with_the_same_id_is_detected() {
    each_backend(|backend| {
        let (a, _b) = start(backend);
        a.add_book(BOOK_1, "A の本");
        a.add_book(BOOK_2, "A の本 2");
        export(&a);

        // A のデータフォルダを丸ごと別の PC に写した、ではなく、同じ ID で別の履歴を持つ端末
        let twin = Dev::new("A-twin", backend);
        meta::set(&twin.db.lock(), meta::KEY_DEVICE_ID, &a.id()).unwrap();
        let err = twin.engine().setup(SetupMode::Join).unwrap_err();
        assert!(matches!(err, SyncError::DuplicateDevice { .. }), "{err}");
        // 何も書き込んでいない（A の端末情報を上書きしていない）
        let devices = a.engine().devices().unwrap();
        assert!(devices.iter().all(|d| d.name != "A-twin"));

        // 設定済みで、seq が追いついてしまった双子は、書き出しで止まる
        {
            let conn = twin.db.lock();
            let tx = conn.unchecked_transaction().unwrap();
            sync_state::join_library(
                &tx,
                &sync_state::library_id(&a.db.lock()).unwrap().unwrap(),
                false,
            )
            .unwrap();
            tx.commit().unwrap();
        }
        for i in 0..3 {
            twin.add_book(&book_id(50 + i), &format!("双子の本 {i}"));
        }
        let err = twin.engine().export().unwrap_err();
        assert!(matches!(err, SyncError::DuplicateDevice { .. }), "{err}");
        assert_eq!(err.state(), SyncState::Error);
        assert_eq!(twin.pending(), 3);
        // A のフォルダは壊れていない
        assert_eq!(files_under(backend, &format!("ops/{}/", a.id())).len(), 1);
    });
}

#[test]
fn the_import_limit_leaves_the_rest_for_the_next_round() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..5 {
            a.add_book(&book_id(i), "本");
            export(&a);
        }
        let engine = b.engine_with(SyncOptions {
            max_files_per_import: 2,
            ..SyncOptions::default()
        });
        let first = engine.import().unwrap();
        assert_eq!((first.files_applied, first.remaining_files), (2, 3));
        assert_eq!(b.count("books"), 2);
        let second = engine.import().unwrap();
        assert_eq!((second.files_applied, second.remaining_files), (2, 1));
        let all = engine.import_all(10).unwrap();
        assert_eq!((all.files_applied, all.remaining_files), (1, 0));
        assert_eq!(b.count("books"), 5);
        assert_converged(&[&a, &b]);
    });
}

#[test]
fn conflict_copies_are_never_read() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "本 1");
        export(&a);
        let seg = segment_path(&a.id(), 1, 1);
        // 元のファイルより先に、内容の違うコピーだけが届いた、という状況
        let copy_path = format!("ops/{}/0000000002-0000000002 (1).jsonl", a.id());
        let op = foreign_op(
            &a.id(),
            2,
            T0 + 5,
            Entity::Book,
            BOOK_2,
            json!({"title": "コピーの本"}),
        );
        backend.write_raw(
            &copy_path,
            &bibcontainer_sync::format::encode_segment(&[op]).unwrap(),
        );
        backend.write_raw(&format!("ops/{}/desktop.ini", a.id()), b"[.ShellClassInfo]");
        backend.write_raw(&format!("ops/{}/.DS_Store", a.id()), b"x");
        let report = import(&b);
        assert_eq!(report.conflict_copies, 1, "隠しファイルは数えない");
        assert_eq!(b.book_title(BOOK_2), None);
        assert_eq!(b.book_title(BOOK_1).as_deref(), Some("本 1"));
        let _ = seg;
        let _ = a;
    });
}

#[test]
fn clock_skew_is_reported_but_ops_are_still_applied_and_observed() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        // A の時計が 10 分進んでいる
        a.clock.set(T0 + 10 * MIN);
        a.add_book(BOOK_1, "進んだ時計の本");
        export(&a);
        let report = import(&b);
        let skew = report.clock_skew.expect("5 分以上先なので警告する");
        assert_eq!(skew.device_id, a.id());
        assert_eq!(skew.device_name.as_deref(), Some("A"));
        assert!(skew.ahead_ms >= 10 * MIN, "{}", skew.ahead_ms);
        assert_eq!(
            b.book_title(BOOK_1).as_deref(),
            Some("進んだ時計の本"),
            "適用はする"
        );
        // 観測したので、B の次の変更は ClockBehind にならない
        b.set_book(BOOK_1, json!({"title": "B が直した"}));
        settle(&[&a, &b]);
        assert_eq!(a.book_title(BOOK_1).as_deref(), Some("B が直した"));
    });
}

#[test]
fn a_few_minutes_of_clock_difference_is_not_a_warning() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.clock.set(T0 + 4 * MIN);
        a.add_book(BOOK_2, "少しだけ進んだ時計の本");
        export(&a);
        assert!(import(&b).clock_skew.is_none());
        assert_eq!(
            b.book_title(BOOK_2).as_deref(),
            Some("少しだけ進んだ時計の本")
        );
    });
}

#[test]
fn unknown_ents_and_versions_are_skipped_but_known_ops_apply() {
    each_backend(|backend| {
        let (_a, b) = start(backend);
        let x = "d-123456789abc";
        let good = foreign_op(
            x,
            1,
            T0 + 1,
            Entity::Book,
            BOOK_1,
            json!({"title": "知っている Op"}),
        );
        let future_ent = format!(
            r#"{{"v":1,"dev":"{x}","seq":2,"hlc":"{:015}:00000:{x}","ent":"collection","id":"c1","set":{{"name":"新しい種類"}}}}"#,
            T0 + 2
        );
        let future_v = format!(
            r#"{{"v":2,"dev":"{x}","seq":3,"hlc":"{:015}:00000:{x}","ent":"book","id":"{BOOK_2}","set":{{"title":{{"new":"shape"}}}}}}"#,
            T0 + 3
        );
        let lines = vec![good.to_json_line().unwrap(), future_ent, future_v];
        let path = segment_path(x, 1, 3);
        backend.write_raw(&path, &sealed(&lines));
        let report = import(&b);
        assert_eq!(report.files_applied, 1);
        assert_eq!(report.unknown_skipped, 1);
        assert_eq!(b.book_title(BOOK_1).as_deref(), Some("知っている Op"));
        assert_eq!(b.book_title(BOOK_2), None);
        // 範囲は記録しない（アプリを更新したあとに、全部適用し直せるように）
        assert!(
            !sync_state::applied_ranges(&b.db.lock())
                .unwrap()
                .covers(x, 1, 3)
        );
        assert_eq!(import(&b).files_applied, 1, "次の回にも読み直す");
    });
}

#[test]
fn unknown_fields_and_provenance_travel_through_snapshots() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        let x = "d-123456789abc";
        let ops = vec![
            foreign_op(
                x,
                1,
                T0 + 1,
                Entity::Book,
                BOOK_1,
                json!({"title": "書名", "zz_future": {"a": [1, 2.5]}, "field_sources.title": "openbd",
                       "manual_fields.memo": true, "memo": "手で書いた", "volume_sort": 1.0}),
            ),
            foreign_op(
                x,
                2,
                T0 + 2,
                Entity::Tag,
                TAG_1,
                json!({"name": "タグ", "zz_tag": "x"}),
            ),
            foreign_op(
                x,
                3,
                T0 + 3,
                Entity::Shelf,
                SHELF_1,
                json!({"name": "本棚", "query": {"q": "a"}}),
            ),
        ];
        put_segment(backend, x, 1, 3, &ops);
        import(&a);
        import(&b);
        a.engine().write_snapshot().unwrap();
        let c = Dev::new("C", backend);
        c.engine().setup(SetupMode::Join).unwrap();
        assert_converged(&[&a, &b, &c]);
        let dump = c.dump();
        let book = dump_row(&dump, "books", BOOK_1);
        assert_eq!(book["extra"]["zz_future"], json!({"a": [1, 2.5]}));
        assert_eq!(book["field_sources"], json!({"title": "openbd"}));
        assert_eq!(book["manual_fields"], json!(["memo"]));
    });
}

fn dump_row(dump: &serde_json::Value, table: &str, id: &str) -> serde_json::Value {
    dump[table]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap()
        .clone()
}

// ---------------------------------------------------------------------------
// スナップショット
// ---------------------------------------------------------------------------

#[test]
fn snapshot_is_written_after_enough_ops_or_days_and_only_two_are_kept() {
    each_backend(|backend| {
        let (a, _b) = start(backend);
        let options = SyncOptions {
            snapshot_every_ops: 5,
            snapshot_every_days: 7,
            ..SyncOptions::default()
        };
        let engine = a.engine_with(options.clone());
        let snaps = |b: &Backend| files_under(b, &format!("snapshots/{}/", a.id())).len();
        for i in 0..4 {
            a.add_book(&book_id(i), "本");
        }
        let r = engine.export().unwrap();
        assert!(!r.snapshot_written);
        assert_eq!(snaps(backend), 0);
        a.add_book(&book_id(4), "本");
        assert!(engine.export().unwrap().snapshot_written, "5 件たまった");
        assert_eq!(snaps(backend), 1);
        // 日数: 8 日たって新しい Op がある
        a.clock.advance(8 * DAY);
        backend.set_now_ms(a.clock.now_ms());
        a.add_book(&book_id(5), "本");
        assert!(
            engine.export().unwrap().snapshot_written,
            "7 日以上たって新しい Op がある"
        );
        assert_eq!(snaps(backend), 2);
        // 新しい Op がなければ、日数がたっても書かない
        a.clock.advance(8 * DAY);
        assert!(!engine.export().unwrap().snapshot_written);
        // 3 つ目を書くと、古いものが消えて 2 つだけ残る
        for i in 6..11 {
            a.add_book(&book_id(i), "本");
        }
        assert!(engine.export().unwrap().snapshot_written);
        assert_eq!(snaps(backend), 2);
        let names = files_under(backend, &format!("snapshots/{}/", a.id()));
        assert!(
            names.iter().all(|n| !n.contains(':')),
            "Windows で使えない文字を含まない: {names:?}"
        );
    });
}

#[test]
fn an_incomplete_newest_snapshot_falls_back_to_the_previous_one() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..3 {
            a.add_book(&book_id(i), "本");
        }
        settle(&[&a, &b]);
        let first = a.engine().write_snapshot().unwrap();
        a.add_book(&book_id(3), "S2 のあとの本");
        export(&a);
        let second = a.engine().write_snapshot().unwrap();
        assert_ne!(first.name, second.name);
        let id = a.id();
        let second_path = snapshot_path(&id, &second.name);
        let full = backend.read_raw(&second_path).unwrap();
        backend.write_raw(&second_path, &full[..full.len() / 2]);

        let c = Dev::new("C", backend);
        let report = c.engine().setup(SetupMode::Join).unwrap().import.unwrap();
        assert_eq!(report.snapshots_applied, 1, "古い方を使う");
        assert!(report.skipped_incomplete >= 1);
        assert_eq!(c.count("books"), 4, "残りはセグメントから");
        // 完成したら、あらためて確かめる（内容は同じなので何も変わらない）
        backend.write_raw(&second_path, &full);
        let again = import(&c);
        assert_eq!(again.skipped_incomplete, 0);
        settle(&[&a, &b, &c]);
        assert_converged(&[&a, &b, &c]);
    });
}

#[test]
fn unusable_snapshots_are_reported_and_do_not_stop_the_import() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "本");
        export(&a);
        let x = "d-123456789abc";
        // 新しい形式（v2）のスナップショット
        let header = json!({"v": 2, "kind": "snapshot", "libraryId": "x", "dev": x,
                            "hlc": format!("{:015}:00000:{x}", T0), "covers": {}});
        let gz = gzip_for_test(&sealed(&[header.to_string()]));
        backend.write_raw(&format!("snapshots/{x}/{:015}-00000.jsonl.gz", T0), &gz);
        let report = import(&b);
        assert_eq!(b.count("books"), 1, "セグメントは取り込める");
        assert!(
            report.errors.iter().any(|e| e.contains("使えません")),
            "{:?}",
            report.errors
        );
    });
}

fn gzip_for_test(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

// ---------------------------------------------------------------------------
// 復元後の再開
// ---------------------------------------------------------------------------

#[test]
fn restore_then_resume_writes_a_snapshot_with_a_new_device_id_and_reimports_the_rest() {
    each_backend(|backend| {
        let a = Dev::new_on_disk("A", backend);
        let b = Dev::new("B", backend);
        a.engine().setup(SetupMode::Create).unwrap();
        b.engine().setup(SetupMode::Join).unwrap();
        for i in 0..3 {
            a.add_book(&book_id(i), &format!("本 {i}"));
        }
        settle(&[&a, &b]);

        // バックアップの時点で、書き出していない変更がある
        a.add_book(&book_id(3), "バックアップ時点の未書き出し");
        let dir = a.dir.as_ref().unwrap().path().to_path_buf();
        let info = backup::create_backup(
            &a.db.lock(),
            &dir.join("manual"),
            BackupKind::Manual,
            &a.clock,
        )
        .unwrap();
        // そのあとの変更（書き出されて B にも届く）と、B の変更
        a.add_book(&book_id(4), "バックアップのあとの本");
        a.set_book(&book_id(0), json!({"rating": 5}));
        settle(&[&a, &b]);
        b.add_book(&book_id(5), "B の本");
        settle(&[&a, &b]);
        let old_id = a.id();

        // 復元（接続をすべて閉じてから）
        a.db.replace(rusqlite::Connection::open_in_memory().unwrap());
        let db_path = dir.join("library.sqlite3");
        let report = backup::restore_backup(
            Path::new(&info.path),
            &db_path,
            &dir.join("safety"),
            &a.clock,
        )
        .unwrap();
        assert_eq!(report.moved_unflushed_ops, 1);
        let (conn, _) = db::open(&db_path, &dir.join("backups")).unwrap();
        a.db.replace(conn);
        assert_ne!(a.id(), old_id, "新しい端末 ID");
        assert!(sync_state::needs_snapshot(&a.db.lock()).unwrap());
        assert_eq!(a.count("books"), 4, "復元した DB には本 4 と 5 がない");
        assert_eq!(a.count("outbox_restored"), 1);

        // 同期を再開する
        let resumed = a.engine().sync(50).unwrap();
        assert!(resumed.export.snapshot_written);
        assert!(!sync_state::needs_snapshot(&a.db.lock()).unwrap());
        assert_eq!(
            a.count("outbox_restored"),
            0,
            "スナップショットに含まれるので捨てる"
        );
        assert_eq!(a.pending(), 0);
        // 古い端末 ID のセグメントと B の変更を取り込み直して、失った変更が戻る
        assert_eq!(a.count("books"), 6);
        assert_eq!(a.book_col(&book_id(0), "rating"), json!(5));
        assert!(!files_under(backend, &format!("snapshots/{}/", a.id())).is_empty());

        settle(&[&a, &b]);
        assert_converged(&[&a, &b]);
        assert_eq!(
            b.book_title(&book_id(3)).as_deref(),
            Some("バックアップ時点の未書き出し")
        );

        // 復元後の変更は、新しい端末 ID の seq 1 から書かれ、B に届く
        a.add_book(&book_id(6), "復元後の本");
        settle(&[&a, &b]);
        assert!(backend.paths().contains(&segment_path(&a.id(), 1, 1)));
        assert_eq!(b.book_title(&book_id(6)).as_deref(), Some("復元後の本"));
        assert_converged(&[&a, &b]);
    });
}

// ---------------------------------------------------------------------------
// 手入れ（統合・整理・掃除）
// ---------------------------------------------------------------------------

#[test]
fn old_segments_of_my_own_are_consolidated_and_readers_do_not_reapply() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..5 {
            a.add_book(&book_id(i), "本");
            export(&a);
            if i == 2 {
                import(&b);
            }
        }
        a.clock.advance(40 * DAY);
        backend.set_now_ms(a.clock.now_ms());
        a.add_book(&book_id(5), "新しい本");
        export(&a);
        let id = a.id();
        assert_eq!(files_under(backend, &format!("ops/{id}/")).len(), 6);

        let report = a.engine().consolidate_own_segments().unwrap();
        assert_eq!(
            (report.files_written, report.files_removed),
            (1, 5),
            "{}",
            backend.name()
        );
        let after = files_under(backend, &format!("ops/{id}/"));
        assert_eq!(after.len(), 2, "{after:?}");
        assert!(after.contains(&segment_path(&id, 1, 5)));
        assert!(after.contains(&segment_path(&id, 6, 6)));
        // もう一度呼んでも、何も変わらない
        assert_eq!(
            a.engine().consolidate_own_segments().unwrap(),
            bibcontainer_sync::ConsolidationReport::default()
        );

        import(&b);
        assert_eq!(b.count("books"), 6);
        assert_converged(&[&a, &b]);
        // 新しい端末も読める
        let c = Dev::new("C", backend);
        c.engine().setup(SetupMode::Join).unwrap();
        assert_converged(&[&a, &b, &c]);
        // 取り込み済みの範囲で覆われているので、読み直さない
        assert_eq!(import(&c).files_applied, 0);
        assert_eq!(import(&b).files_applied, 0);
    });
}

#[test]
fn unused_covers_are_removed_only_when_old_and_unreferenced() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        let used = a.make_cover(b"used cover");
        let old_orphan = a.make_cover(b"old orphan");
        let young_orphan = a.make_cover(b"young orphan");
        a.set_book(BOOK_1, json!({"title": "本", "cover_hash": used}));
        export(&a);
        // 孤立した書影を、同期フォルダにも置く
        for h in [&old_orphan, &young_orphan] {
            backend.write_raw(&cover_remote_path(h), format!("remote {h}").as_bytes());
        }
        let long_ago = T0 - 40 * DAY;
        backend.set_mtime(&cover_remote_path(&old_orphan), long_ago);
        backend.set_mtime(&cover_remote_path(&young_orphan), T0 - DAY);
        // 手元の書影の更新時刻も、時計（T0）を基準に決める（実際の「今」に左右されないように）
        for (hash, ms) in [(&old_orphan, long_ago), (&young_orphan, T0 - DAY)] {
            let path = bibcontainer_sync::layout::cover_local_path(a.covers.path(), hash);
            let f = std::fs::File::options().write(true).open(&path).unwrap();
            f.set_modified(
                std::time::UNIX_EPOCH
                    + std::time::Duration::from_millis(u64::try_from(ms).unwrap()),
            )
            .unwrap();
        }

        let report = a.engine().gc_covers().unwrap();
        assert_eq!(report.local_removed, 1, "{}", backend.name());
        assert_eq!(report.remote_removed, 1);
        assert!(report.kept_recent >= 1);
        assert!(a.has_cover(&used) && a.has_cover(&young_orphan) && !a.has_cover(&old_orphan));
        assert!(backend.read_raw(&cover_remote_path(&used)).is_some());
        assert!(
            backend
                .read_raw(&cover_remote_path(&young_orphan))
                .is_some()
        );
        assert!(backend.read_raw(&cover_remote_path(&old_orphan)).is_none());
        // 同期が設定されていなければ、手元だけ整理する
        b.engine().leave().unwrap();
        let _ = b.engine().gc_covers().unwrap();
    });
}

#[test]
fn stale_tmp_files_are_removed_and_fresh_ones_kept() {
    each_backend(|backend| {
        let (a, _b) = start(backend);
        a.add_book(BOOK_1, "本");
        export(&a);
        let id = a.id();
        let h = "ab".repeat(32);
        let dirs = [
            format!("ops/{id}"),
            format!("snapshots/{id}"),
            "devices".to_owned(),
            format!("covers/{}", &h[..2]),
        ];
        for d in &dirs {
            for (name, age) in [("old", 2 * DAY), ("fresh", 0)] {
                let p = format!("{d}/.tmp-{name}");
                backend.write_raw(&p, b"x");
                backend.set_mtime(&p, T0 - age);
            }
        }
        // 他の端末のフォルダは触らない
        backend.write_raw("ops/d-someoneelse/.tmp-old", b"x");
        backend.set_mtime("ops/d-someoneelse/.tmp-old", T0 - 2 * DAY);
        let removed = a.engine().cleanup_tmp().unwrap();
        assert_eq!(removed, 4, "{}", backend.name());
        for d in &dirs {
            assert!(backend.read_raw(&format!("{d}/.tmp-old")).is_none(), "{d}");
            assert!(
                backend.read_raw(&format!("{d}/.tmp-fresh")).is_some(),
                "{d}"
            );
        }
        assert!(backend.read_raw("ops/d-someoneelse/.tmp-old").is_some());
    });
}

// ---------------------------------------------------------------------------
// 書影
// ---------------------------------------------------------------------------

#[test]
fn covers_are_verified_by_hash_and_missing_ones_are_counted_until_they_arrive() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        let hash = a.make_cover(b"the cover");
        a.set_book(BOOK_1, json!({"title": "本", "cover_hash": hash}));
        // 書影の本体が届く前に、Op だけが届いた
        let cover_path = cover_remote_path(&hash);
        let full = b"the cover".to_vec();
        {
            // 書き出しで書影も写るので、いったん消して「まだ届いていない」状態にする
            export(&a);
            backend.remove_raw(&cover_path);
        }
        let report = import(&b);
        assert_eq!(report.missing_covers, 1);
        assert_eq!(b.engine().local_counters().unwrap().missing_covers, 1);
        assert!(!b.has_cover(&hash));
        // 途中のファイル（内容がハッシュと合わない）は使わない
        backend.write_raw(&cover_path, &full[..4]);
        let report = import(&b);
        assert_eq!((report.covers_fetched, report.missing_covers), (0, 1));
        assert!(!b.has_cover(&hash));
        // 届いた
        backend.write_raw(&cover_path, &full);
        let report = import(&b);
        assert_eq!((report.covers_fetched, report.missing_covers), (1, 0));
        assert!(b.has_cover(&hash));
        assert_eq!(b.engine().local_counters().unwrap().missing_covers, 0);
    });
}

#[test]
fn covers_referenced_by_exported_ops_are_uploaded_before_the_segment() {
    let backend = Backend::memory();
    let (a, _b) = start(&backend);
    let hash = a.make_cover(b"cover body");
    a.set_book(BOOK_1, json!({"title": "本", "cover_hash": hash}));
    // 手元にない書影を参照する Op があっても、書き出しは止まらない
    a.set_book(
        BOOK_2,
        json!({"title": "書影のない本", "cover_hash": "ab".repeat(32)}),
    );
    let report = export(&a);
    assert_eq!(report.covers_uploaded, 1);
    assert_eq!(report.ops_exported, 2);
    assert!(backend.read_raw(&cover_remote_path(&hash)).is_some());
    // 同じ書影を 2 度写さない
    a.set_book(BOOK_3, json!({"title": "同じ書影", "cover_hash": hash}));
    assert_eq!(export(&a).covers_uploaded, 0);
}

// ---------------------------------------------------------------------------
// 同期フォルダが使えないとき
// ---------------------------------------------------------------------------

#[test]
fn an_unavailable_or_full_folder_reports_errors_and_never_blocks_local_work() {
    let backend = Backend::memory();
    let (a, b) = start(&backend);
    let Backend::Memory(folder) = &backend else {
        unreachable!()
    };
    let ta = folder.transport("A");

    ta.set_unavailable(true);
    let err = a.engine().import().unwrap_err();
    assert!(
        matches!(&err, SyncError::Transport(e) if e.kind == TransportErrorKind::Unavailable),
        "{err}"
    );
    assert_eq!(err.state(), SyncState::Error);
    assert!(err.to_string().contains("同期フォルダ"));
    a.add_book(BOOK_1, "オフラインで登録した本");
    assert!(a.engine().export().is_err());
    assert_eq!(a.pending(), 1);
    ta.set_unavailable(false);

    ta.set_fail_writes(Some(TransportErrorKind::StorageFull));
    let err = a.engine().export().unwrap_err();
    assert!(
        matches!(&err, SyncError::Transport(e) if e.kind == TransportErrorKind::StorageFull),
        "{err}"
    );
    assert_eq!(a.pending(), 1, "書けなかった Op は未書き出しのまま");
    // 取り込みは書き込みなしでできる
    assert!(a.engine().import().is_ok());
    ta.set_fail_writes(None);

    settle(&[&a, &b]);
    assert_eq!(
        b.book_title(BOOK_1).as_deref(),
        Some("オフラインで登録した本")
    );
}

#[test]
fn a_missing_library_folder_on_disk_is_unavailable_not_silently_created() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp
        .path()
        .join("no-such-drive")
        .join("bibcontainer-library");
    let d = Dev::new("D", &Backend::memory());
    let transport = bibcontainer_sync::LocalFolder::new(&missing);
    let engine = SyncEngine::new(&d.db, &transport, &d.clock, d.covers.path(), d.info.clone());
    let err = engine.setup(SetupMode::Create).unwrap_err();
    assert!(
        matches!(&err, SyncError::Transport(e) if e.kind == TransportErrorKind::Unavailable),
        "{err}"
    );
    assert!(!missing.exists());
    assert!(!tmp.path().join("no-such-drive").exists());
    assert_eq!(engine.inspect("x").unwrap_err().state(), SyncState::Error);
}

#[test]
fn local_folder_resolves_the_library_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let selected = tmp.path().join("マイドライブ");
    std::fs::create_dir_all(&selected).unwrap();
    let nested = bibcontainer_sync::LocalFolder::resolve_root(&selected);
    assert_eq!(nested, selected.join("bibcontainer-library"));
    // 作ってみて、選び直す
    let d = Dev::new("D", &Backend::memory());
    let t = bibcontainer_sync::LocalFolder::new(&nested);
    let engine = SyncEngine::new(&d.db, &t, &d.clock, d.covers.path(), d.info.clone());
    assert_eq!(engine.inspect("x").unwrap().kind, FolderKind::Empty);
    assert!(!nested.exists(), "調べただけでは作らない");
    engine.setup(SetupMode::Create).unwrap();
    assert!(nested.join("library.json").is_file());
    // 親のフォルダを選んでも、bibcontainer-library そのものを選んでも、同じ場所になる
    assert_eq!(
        bibcontainer_sync::LocalFolder::resolve_root(&selected),
        nested
    );
    assert_eq!(
        bibcontainer_sync::LocalFolder::resolve_root(&nested),
        nested
    );
}

#[test]
fn status_values_come_from_the_local_database() {
    each_backend(|backend| {
        let (a, _b) = start(backend);
        let counters = a.engine().local_counters().unwrap();
        assert_eq!(counters.pending_ops, 0);
        assert_eq!(counters.device_id, a.id());
        assert!(counters.library_id.is_some());
        a.add_book(BOOK_1, "本");
        assert_eq!(a.engine().local_counters().unwrap().pending_ops, 1);
        let devices = a.engine().devices().unwrap();
        assert_eq!(devices.len(), 2);
        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"A") && names.contains(&"B"));
        assert_eq!(devices.iter().filter(|d| d.is_self).count(), 1);
        assert!(
            devices
                .iter()
                .all(|d| d.os == "test" && d.app_version == "0.1.0")
        );
        // 端末名の変更は、書き直した情報に反映される
        let mut renamed = a.info.clone();
        renamed.name = "新しい名前".to_owned();
        let engine = SyncEngine::new(&a.db, &*a.transport, &a.clock, a.covers.path(), renamed);
        engine.write_device_file(None).unwrap();
        assert!(
            a.engine()
                .devices()
                .unwrap()
                .iter()
                .any(|d| d.name == "新しい名前" && d.is_self)
        );
        let _ = a.clock.now_ms();
    });
}

/// バックアップの時点で未書き出しだった変更が、そのあと書き出されないまま復元された場合、
/// 復元した端末のスナップショットが、ほかの端末にその変更を届ける（`outbox_restored` の Op）。
#[test]
fn unflushed_changes_in_a_restored_backup_reach_other_devices_through_the_snapshot() {
    each_backend(|backend| {
        let a = Dev::new_on_disk("A", backend);
        let b = Dev::new("B", backend);
        a.engine().setup(SetupMode::Create).unwrap();
        b.engine().setup(SetupMode::Join).unwrap();
        a.add_book(&book_id(0), "共有済みの本");
        settle(&[&a, &b]);

        // 書き出していない変更を持ったままバックアップし、書き出す前に壊れて、復元する
        a.add_book(&book_id(1), "未書き出しのまま復元した本");
        a.set_book(&book_id(0), json!({"rating": 4}));
        let dir = a.dir.as_ref().unwrap().path().to_path_buf();
        let info = backup::create_backup(
            &a.db.lock(),
            &dir.join("manual"),
            BackupKind::Manual,
            &a.clock,
        )
        .unwrap();
        a.db.replace(rusqlite::Connection::open_in_memory().unwrap());
        let db_path = dir.join("library.sqlite3");
        let report = backup::restore_backup(
            Path::new(&info.path),
            &db_path,
            &dir.join("safety"),
            &a.clock,
        )
        .unwrap();
        assert_eq!(report.moved_unflushed_ops, 2);
        let (conn, _) = db::open(&db_path, &dir.join("backups")).unwrap();
        a.db.replace(conn);

        settle(&[&a, &b]);
        assert_eq!(
            b.book_title(&book_id(1)).as_deref(),
            Some("未書き出しのまま復元した本"),
            "{}",
            backend.name()
        );
        assert_eq!(b.book_col(&book_id(0), "rating"), json!(4));
        assert_converged(&[&a, &b]);
    });
}

/// 別のライブラリに移るとき（解除して、統合で参加）、前のライブラリの内容も新しいライブラリの端末に届く。
#[test]
fn data_received_from_a_previous_library_reaches_the_new_librarys_devices_on_merge() {
    each_backend(|backend| {
        let other = if backend.name() == "memory" {
            Backend::memory()
        } else {
            Backend::local()
        };
        // 前のライブラリ: P が本を作り、D は参加して受け取るだけ（自分の Op は 1 つもない）
        let p = Dev::new("P", &other);
        let d = Dev::new("D", &other);
        p.engine().setup(SetupMode::Create).unwrap();
        d.engine().setup(SetupMode::Join).unwrap();
        p.add_book(&book_id(1), "前のライブラリの本");
        settle(&[&p, &d]);
        assert_eq!(d.count("books"), 1);
        assert_eq!(d.pending(), 0);

        // 新しいライブラリ: A と B
        let (a, b) = start(backend);
        a.add_book(&book_id(2), "新しいライブラリの本");
        settle(&[&a, &b]);
        // D は前のライブラリを離れ、新しいライブラリに、手元のデータごと統合する
        d.engine().leave().unwrap();
        let transport = backend.transport("D");
        let engine = SyncEngine::new(
            &d.db,
            &*transport,
            &d.clock,
            d.covers.path(),
            d.info.clone(),
        );
        engine.setup(SetupMode::JoinMerge).unwrap();
        for dev in [&a, &b] {
            import(dev);
            assert_eq!(
                dev.book_title(&book_id(1)).as_deref(),
                Some("前のライブラリの本"),
                "{}",
                dev.name
            );
        }
        assert_eq!(d.count("books"), 2);
    });
}

/// 完全削除（`purged`）は項目ごとの LWW なので、別の端末の新しい変更で取り消されることがある。
/// そのとき書影が届くよう、スナップショットは完全削除した本の書影も同期フォルダへ写す（性質テストで見つけた穴）。
#[test]
fn covers_of_purged_books_are_uploaded_because_purged_can_be_reverted() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        let hash = b.make_cover(b"cover of a purged book");
        b.set_book(
            BOOK_1,
            json!({"title": "完全削除した本", "cover_hash": hash, "purged": true}),
        );
        b.engine().write_snapshot().unwrap();
        assert!(
            backend.read_raw(&cover_remote_path(&hash)).is_some(),
            "{}",
            backend.name()
        );
        import(&a);
        assert!(
            !a.has_cover(&hash),
            "完全削除した本の書影は、手元には取らない"
        );
        // A が、あとから取り消す
        a.set_book(BOOK_1, json!({"purged": false}));
        settle(&[&a, &b]);
        assert!(a.has_cover(&hash));
        assert_converged(&[&a, &b]);
    });
}
