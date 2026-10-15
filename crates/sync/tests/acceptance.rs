//! フェーズ 2 の受け入れ基準 AT-S1〜S6・S8（AT-S7 は `convergence.rs`）。
//!
//! どれも、メモリ上の同期フォルダとローカルのフォルダの両方で動かす。

mod common;

use bibcontainer_core::Clock;
use bibcontainer_core::schema::Entity;
use bibcontainer_sync::{DeliveryPolicy, SetupMode, SyncError, SyncState};
use serde_json::json;

use common::*;

fn each_backend(test: impl Fn(&Backend)) {
    for backend in [Backend::memory(), Backend::local()] {
        test(&backend);
    }
}

/// A がライブラリを作り、B が参加した状態。
fn start(backend: &Backend) -> (Dev, Dev) {
    let a = Dev::new("A", backend);
    let b = Dev::new("B", backend);
    a.engine().setup(SetupMode::Create).unwrap();
    b.engine().setup(SetupMode::Join).unwrap();
    (a, b)
}

/// AT-S1: 端末 A で 10 冊登録 → 書き出し → B が取り込むと 10 冊（書影も）。
#[test]
fn at_s1_ten_books_registered_on_a_appear_on_b() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        let mut hashes = Vec::new();
        for i in 0..10 {
            let hash = a.make_cover(format!("書影 {i}").as_bytes());
            a.set_book(
                &book_id(i),
                json!({"title": format!("本 {i}"), "cover_hash": hash, "rating": (i % 5) + 1}),
            );
            hashes.push(hash);
        }
        let exported = export(&a);
        assert_eq!(exported.ops_exported, 10, "{}", backend.name());
        assert_eq!(exported.covers_uploaded, 10);
        assert_eq!(a.pending(), 0);

        let report = import(&b);
        assert_eq!(b.count("books"), 10, "{}", backend.name());
        assert_eq!(report.changed_book_ids.len(), 10);
        assert_eq!(report.covers_fetched, 10);
        assert_eq!(report.missing_covers, 0);
        for h in &hashes {
            assert!(b.has_cover(h), "書影が届いた");
        }
        assert_converged(&[&a, &b]);
        // 2 回目の取り込みは、何も読まない
        let again = import(&b);
        assert_eq!((again.files_applied, again.snapshots_applied), (0, 0));
        assert!(!again.changed_anything());
    });
}

/// AT-S2: A は書名、B は評価を、それぞれオフラインで変える → 両方の変更が両端末に残る。
#[test]
fn at_s2_different_fields_changed_offline_both_survive() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "元の書名");
        settle(&[&a, &b]);
        assert_eq!(b.book_title(BOOK_1).as_deref(), Some("元の書名"));

        // ここから、それぞれ同期せずに変更する。
        a.set_book(BOOK_1, json!({"title": "A が変えた書名"}));
        b.set_book(BOOK_1, json!({"rating": 4}));
        settle(&[&a, &b]);

        for d in [&a, &b] {
            assert_eq!(
                d.book_title(BOOK_1).as_deref(),
                Some("A が変えた書名"),
                "{}",
                d.name
            );
            assert_eq!(d.book_col(BOOK_1, "rating"), json!(4), "{}", d.name);
        }
        assert_converged(&[&a, &b]);
    });
}

/// AT-S3: 同じ項目を両方で変える → HLC が新しい方にそろう。
#[test]
fn at_s3_same_field_the_newer_hlc_wins_on_both() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "元");
        settle(&[&a, &b]);

        // B の変更の方が新しい
        a.clock.advance(100);
        b.clock.set(a.clock.now_ms() + 50);
        a.set_book(BOOK_1, json!({"title": "A の書名"}));
        b.set_book(BOOK_1, json!({"title": "B の書名"}));
        settle(&[&a, &b]);
        assert_eq!(a.book_title(BOOK_1).as_deref(), Some("B の書名"));
        assert_eq!(b.book_title(BOOK_1).as_deref(), Some("B の書名"));

        // 今度は A の方が新しい（A の時計を B より先に進める）
        a.clock.set(b.clock.now_ms() + 1000);
        a.set_book(BOOK_1, json!({"title": "A の新しい書名"}));
        b.set_book(BOOK_1, json!({"title": "B の古い書名"}));
        settle(&[&a, &b]);
        assert_eq!(a.book_title(BOOK_1).as_deref(), Some("A の新しい書名"));
        assert_eq!(b.book_title(BOOK_1).as_deref(), Some("A の新しい書名"));
        assert_converged(&[&a, &b]);
    });
}

/// AT-S4: A が X を削除し、B がその後に X の評価を変える → X は削除されたまま。
#[test]
fn at_s4_deleted_stays_deleted_when_the_other_edits_later() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        a.add_book(BOOK_1, "消す本");
        settle(&[&a, &b]);

        a.set_book(BOOK_1, json!({"deleted": true}));
        b.clock.set(a.clock.now_ms() + 10_000);
        b.set_book(BOOK_1, json!({"rating": 5}));
        settle(&[&a, &b]);

        for d in [&a, &b] {
            assert_eq!(d.book_col(BOOK_1, "deleted"), json!(1), "{}", d.name);
            assert_eq!(
                d.book_col(BOOK_1, "rating"),
                json!(5),
                "評価の変更も残る: {}",
                d.name
            );
        }
        assert_converged(&[&a, &b]);
    });
}

/// AT-S5: 終端レコードのないセグメントは読まずに飛ばし、完成後に取り込む。件数・ハッシュの不一致も。
#[test]
fn at_s5_incomplete_segments_are_skipped_then_applied_when_complete() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..3 {
            a.add_book(&book_id(i), &format!("本 {i}"));
        }
        export(&a);
        let path = backend
            .paths()
            .into_iter()
            .find(|p| p.starts_with("ops/") && p.ends_with(".jsonl"))
            .expect("セグメントがある");
        let full = backend.read_raw(&path).unwrap();
        let text = String::from_utf8(full.clone()).unwrap();

        let broken: Vec<(&str, Vec<u8>)> = vec![
            (
                "終端レコードがない",
                text.rsplit_once("{\"end\"").unwrap().0.as_bytes().to_vec(),
            ),
            ("途中で切れている", full[..full.len() / 2].to_vec()),
            (
                "件数が合わない",
                text.replace("\"count\":3", "\"count\":4").into_bytes(),
            ),
            (
                "ハッシュが合わない",
                text.replacen("本 0", "本 9", 1).into_bytes(),
            ),
        ];
        for (what, bytes) in broken {
            backend.write_raw(&path, &bytes);
            let report = b.engine().import().unwrap();
            assert_eq!(report.skipped_incomplete, 1, "{what}（{}）", backend.name());
            assert_eq!(report.files_applied, 0, "{what}");
            assert_eq!(b.count("books"), 0, "{what}: 読んではいけない");
        }
        // 完成した
        backend.write_raw(&path, &full);
        let report = import(&b);
        assert_eq!(report.files_applied, 1);
        assert_eq!(report.skipped_incomplete, 0);
        assert_eq!(b.count("books"), 3);
        assert_converged(&[&a, &b]);
    });
}

/// AT-S5（メモリ上の同期フォルダ）: 途中まで見える・遅れて見える・衝突コピー。
#[test]
fn at_s5_memory_transport_partial_hidden_and_conflict_copy() {
    let backend = Backend::memory();
    let (a, b) = start(&backend);
    let Backend::Memory(folder) = &backend else {
        unreachable!()
    };
    let tb = folder.transport("B");
    tb.set_policy(DeliveryPolicy::Partial { percent: 60 });
    for i in 0..4 {
        a.add_book(&book_id(i), &format!("本 {i}"));
    }
    export(&a);
    let report = b.engine().import().unwrap();
    assert_eq!((report.files_applied, report.skipped_incomplete), (0, 1));
    assert_eq!(b.count("books"), 0);

    tb.set_policy(DeliveryPolicy::Immediate);
    tb.deliver_all();
    // 衝突コピーは読まない
    let seg = backend
        .paths()
        .into_iter()
        .find(|p| p.starts_with("ops/") && p.ends_with(".jsonl"))
        .unwrap();
    let copy = backend.make_conflict_copy(&seg).unwrap();
    assert!(copy.contains("(1)"));
    let report = import(&b);
    assert_eq!(report.files_applied, 1);
    assert_eq!(report.conflict_copies, 1);
    assert_eq!(b.count("books"), 4);

    // 遅れて見える: 見えるようになるまで何も起きない
    tb.set_policy(DeliveryPolicy::Hidden);
    a.add_book(&book_id(10), "遅れる本");
    export(&a);
    let report = import(&b);
    assert_eq!(report.files_applied, 0);
    assert_eq!(b.count("books"), 4);
    tb.deliver_all();
    import(&b);
    assert_eq!(b.count("books"), 5);
    assert_converged(&[&a, &b]);
}

/// AT-S6: 新しい端末 C が参加 → スナップショット＋残りのセグメントで、書影も含めて全データが再現される。
#[test]
fn at_s6_new_device_joins_with_snapshot_and_remaining_segments() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..6 {
            let hash = a.make_cover(format!("A の書影 {i}").as_bytes());
            a.set_book(
                &book_id(i),
                json!({"title": format!("A の本 {i}"), "cover_hash": hash}),
            );
        }
        a.set(
            Entity::Tag,
            TAG_1,
            json!({"name": "小説", "color": "#ff0000"}),
        );
        a.set(Entity::Series, SERIES_1, json!({"name": "シリーズ"}));
        settle(&[&a, &b]);
        for i in 10..13 {
            let hash = b.make_cover(format!("B の書影 {i}").as_bytes());
            b.set_book(
                &book_id(i),
                json!({"title": format!("B の本 {i}"), "cover_hash": hash}),
            );
        }
        b.set_book(&book_id(0), json!({"deleted": true, "memo": "メモ"}));
        settle(&[&a, &b]);

        // A がスナップショットを書く。その後の変更はセグメントだけにある。
        let snap = a.engine().write_snapshot().unwrap();
        assert!(snap.entities >= 9);
        let hash = b.make_cover(b"snapshot after");
        b.set_book(
            &book_id(20),
            json!({"title": "スナップショットのあとの本", "cover_hash": hash}),
        );
        a.set_book(&book_id(1), json!({"rating": 3}));
        settle(&[&a, &b]);

        let c = Dev::new("C", backend);
        let setup = c.engine().setup(SetupMode::Join).unwrap();
        let import = setup.import.unwrap();
        assert!(import.snapshots_applied >= 1, "{}", backend.name());
        assert_eq!(import.missing_covers, 0);
        assert_converged(&[&a, &b, &c]);
        assert_eq!(c.count("books"), 10);
        for d in [&a, &b] {
            for h in
                c.db.lock()
                    .prepare("SELECT cover_hash FROM books WHERE cover_hash IS NOT NULL")
                    .unwrap()
                    .query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .flatten()
            {
                assert!(c.has_cover(&h), "C に書影 {h} が届いた（{}）", d.name);
            }
        }
        // C が参加した後の変更も、全員に行き渡る
        c.set_book(&book_id(30), json!({"title": "C の本"}));
        settle(&[&a, &b, &c]);
        assert_converged(&[&a, &b, &c]);
        assert_eq!(a.count("books"), 11);
    });
}

/// AT-S6（スナップショットがないとき）: セグメントだけからも全データが再現される。
#[test]
fn at_s6_join_without_any_snapshot_uses_segments() {
    each_backend(|backend| {
        let (a, b) = start(backend);
        for i in 0..5 {
            a.add_book(&book_id(i), &format!("本 {i}"));
        }
        settle(&[&a, &b]);
        let c = Dev::new("C", backend);
        c.engine().setup(SetupMode::Join).unwrap();
        assert_converged(&[&a, &b, &c]);
        assert_eq!(c.count("books"), 5);
    });
}

/// AT-S8: 別のライブラリのフォルダ → ライブラリ ID の不一致で同期しない。
#[test]
fn at_s8_other_librarys_folder_is_refused() {
    each_backend(|backend| {
        let other_backend = if backend.name() == "memory" {
            Backend::memory()
        } else {
            Backend::local()
        };
        let (a, _b) = start(backend);
        a.add_book(BOOK_1, "A の本");
        export(&a);

        // D は別のライブラリ（別のフォルダ）に参加している
        let d = Dev::new("D", &other_backend);
        d.engine().setup(SetupMode::Create).unwrap();
        d.add_book(BOOK_2, "D の本");
        d.engine().sync(10).unwrap();
        d.add_book(BOOK_3, "D のあとの本"); // 未書き出しの Op がある

        // D のエンジンに、A のライブラリのフォルダを渡す（誤って指定した）
        let wrong_transport = backend.transport("D");
        let paths_before = backend.paths();
        let engine = bibcontainer_sync::SyncEngine::new(
            &d.db,
            &*wrong_transport,
            &d.clock,
            d.covers.path(),
            d.info.clone(),
        );
        let err = engine.import().unwrap_err();
        assert!(matches!(err, SyncError::LibraryMismatch { .. }), "{err}");
        assert_eq!(err.state(), SyncState::Mismatch);
        assert!(matches!(
            engine.export().unwrap_err(),
            SyncError::LibraryMismatch { .. }
        ));
        assert!(matches!(
            engine.sync(3).unwrap_err(),
            SyncError::LibraryMismatch { .. }
        ));
        assert_eq!(d.count("books"), 2, "取り込んでいない");
        assert_eq!(d.pending(), 1, "書き出していない");
        assert_eq!(backend.paths(), paths_before, "書き込んでいない");
        // 設定でも断る（先に解除が必要）
        assert!(matches!(
            engine.setup(SetupMode::JoinMerge).unwrap_err(),
            SyncError::LibraryMismatch { .. }
        ));
        let inspected = engine.inspect("folder").unwrap();
        assert_eq!(inspected.kind, bibcontainer_sync::FolderKind::OtherLibrary);
    });
}
