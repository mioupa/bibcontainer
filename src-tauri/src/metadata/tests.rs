//! 書誌取得の結合テスト（偽の HTTP と一時 DB。ネットワーク・キーチェーンは使わない）。
//! 受け入れ基準（AT-10・AT-11・AT-16）と、設定・キー・書影・シリーズ・再取得の流れを確かめる。

use std::time::{Duration, Instant};

use bibcontainer_core::library::{FieldChange, Origin, UpdateOptions};
use bibcontainer_metadata::cover::cover_path;
use bibcontainer_metadata::http::{HttpResponse, TransportError};
use bibcontainer_metadata::{MetaGroup, MetaSecondTier, MetaSource, ProviderId, ProviderOutcome};
use serde_json::json;

use super::draft::{DraftSeries, MetaStatus};
use super::providers::{ProviderTestVerdict, test_isbns};
use super::refetch::{RefetchOutcome, RefetchSummary};
use super::register::NewBookDraft;
use super::testing::*;
use crate::error::{AppError, ErrorKind};

fn tier(c_code: &str, price: i64) -> MetaSecondTier {
    MetaSecondTier {
        c_code: Some(c_code.into()),
        list_price: Some(price),
    }
}

/// 取得して、その下書きのまま保存する。本の ID を返す。
fn register(rig: &Rig, isbn: &str, second_tier: Option<MetaSecondTier>) -> String {
    let fetched = rig
        .block(rig.state.fetch_metadata(isbn, second_tier))
        .unwrap();
    let draft = NewBookDraft {
        changes: fetched.draft.fields.to_changes(),
        series_name: fetched.draft.series_name.clone(),
    };
    rig.block(rig.state.create_books_from_drafts(vec![draft]))
        .unwrap()
        .remove(0)
}

fn manual_title(rig: &Rig, id: &str, title: &str) {
    rig.update(
        id,
        vec![FieldChange::manual("title", title)],
        UpdateOptions::default(),
    );
}

/// 書誌を持たない本（ISBN だけ）を作る。
fn bare_book(rig: &Rig, isbn: Option<&str>, status: &str) -> String {
    let mut changes = vec![FieldChange::new("meta_status", status, Origin::Barcode)];
    if let Some(isbn) = isbn {
        changes.push(FieldChange::barcode("isbn13", isbn));
    }
    rig.block(rig.state.create_books_from_drafts(vec![NewBookDraft {
        changes,
        series_name: None,
    }]))
    .unwrap()
    .remove(0)
}

fn item<'a>(summary: &'a RefetchSummary, id: &str) -> &'a super::refetch::RefetchItem {
    summary.items.iter().find(|i| i.id == id).unwrap()
}

// ---------------------------------------------------------------------------
// AT-10: 書誌取得
// ---------------------------------------------------------------------------

#[test]
fn at10_draft_has_title_publisher_c_code_and_a_saved_cover() {
    let rig = Rig::new();
    let result = rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    let f = &result.draft.fields;

    assert_eq!(f.title.as_ref().unwrap().value, "おにぎりレシピ101");
    assert_eq!(f.publisher.as_ref().unwrap().value, "ポット出版");
    let c_code = f.c_code.as_ref().unwrap();
    assert_eq!(c_code.value, "0077");
    assert_eq!(c_code.origin, Origin::Provider("openbd".into()));
    assert_eq!(f.isbn13.as_ref().unwrap().origin, Origin::Barcode);
    assert_eq!(f.meta_status.value, MetaStatus::Ok);
    assert!(!result.offline);

    // 書影: 保存されていて、ハッシュは下書きにも入っている
    let hash = result.cover_hash.clone().expect("書影がある");
    assert_eq!(f.cover_hash.as_ref().unwrap().value, hash);
    assert!(cover_path(&rig.state.dirs().covers, &hash).is_file());

    // 通信したのは有効な取得元だけ（Google はキーがないので使わない。NDL の書影 API は廃止した）
    let urls = rig.http.urls();
    assert!(urls.iter().any(|u| u.contains("api.openbd.jp")));
    assert!(
        urls.iter()
            .any(|u| u.contains("ndlsearch.ndl.go.jp/api/sru"))
    );
    assert!(
        urls.iter()
            .any(|u| u.starts_with("https://cover.openbd.jp/"))
    );
    assert!(
        !urls.iter().any(|u| u.contains("googleapis.com")),
        "{urls:?}"
    );
    assert!(!urls.iter().any(|u| u.contains("/thumbnail/")), "{urls:?}");
    let providers: Vec<ProviderId> = result.reports.iter().map(|r| r.provider).collect();
    assert_eq!(providers, vec![ProviderId::Openbd, ProviderId::Ndl]);
}

#[test]
fn isbn_notations_are_accepted_and_bad_isbns_are_rejected_without_a_request() {
    let rig = Rig::new();
    // ISBN-10・ハイフン・全角
    for input in [
        "4780802040",
        "978-4-7808-0204-7",
        "９７８４７８０８０２０４７",
    ] {
        let r = rig.block(rig.state.fetch_metadata(input, None)).unwrap();
        assert_eq!(r.merged.isbn13, ONIGIRI, "{input}");
        assert_eq!(r.draft.fields.isbn13.as_ref().unwrap().value, ONIGIRI);
    }
    rig.http.clear();
    for bad in ["9784780802040", "", "abc", "12345"] {
        let err = rig.block(rig.state.fetch_metadata(bad, None)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation, "{bad:?}");
    }
    assert!(rig.http.urls().is_empty());
}

#[test]
fn raw_responses_are_cached_in_the_db_between_fetches() {
    let rig = Rig::new();
    rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    assert_eq!(rig.http.count("api.openbd.jp"), 1);
    rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    assert_eq!(rig.http.count("api.openbd.jp"), 1, "2 回目はキャッシュから");
    assert_eq!(rig.http.count("ndlsearch.ndl.go.jp/api/sru"), 1);

    let rows: Vec<(String, String)> = rig
        .block(rig.db.run(|conn, _| {
            let mut stmt = conn
                .prepare("SELECT provider, status FROM provider_cache ORDER BY provider")
                .map_err(bibcontainer_core::Error::from)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(bibcontainer_core::Error::from)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(bibcontainer_core::Error::from)?;
            Ok(rows)
        }))
        .unwrap();
    assert_eq!(
        rows,
        vec![("ndl".into(), "ok".into()), ("openbd".into(), "ok".into())]
    );
}

// ---------------------------------------------------------------------------
// AT-11: オフライン
// ---------------------------------------------------------------------------

#[test]
fn at11_offline_scan_is_saved_as_pending_and_completed_when_back_online() {
    let rig = Rig::new();
    rig.http.set_offline();

    // 取得できない。ISBN と 2段目だけの下書きが「取得待ち」で返る
    let fetched = rig
        .block(rig.state.fetch_metadata(KOKORO, Some(tier("0193", 700))))
        .unwrap();
    assert!(fetched.offline);
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Pending);
    assert!(fetched.draft.fields.title.is_none());
    assert_eq!(fetched.draft.fields.c_code.as_ref().unwrap().value, "0193");
    assert_eq!(fetched.cover_hash, None);

    // そのまま保存できる
    let id = register(&rig, KOKORO, Some(tier("0193", 700)));
    let book = rig.book(&id);
    assert_eq!(book.meta_status, "pending");
    assert_eq!(book.isbn13.as_deref(), Some(KOKORO));
    assert_eq!(book.c_code.as_deref(), Some("0193"));
    assert_eq!(book.list_price, Some(700));
    assert_eq!(book.field_sources["c_code"], "barcode");
    assert_eq!(
        rig.changed.lock().unwrap().last().unwrap().0,
        vec![id.clone()]
    );
    let pending = rig
        .block(
            rig.db
                .run(|conn, _| Ok(bibcontainer_core::library::pending_book_ids(conn)?)),
        )
        .unwrap();
    assert_eq!(pending, vec![id.clone()]);

    // まだオフライン: 何も書かず、取得待ちのまま次回に回す
    let ops = rig.outbox_len();
    let changes = rig.changed_count();
    let summary = rig.block(rig.state.run_pending_refetch()).unwrap().unwrap();
    assert_eq!(
        (summary.requested, summary.offline, summary.updated),
        (1, 1, 0)
    );
    assert_eq!(item(&summary, &id).outcome, RefetchOutcome::Offline);
    assert_eq!(rig.outbox_len(), ops, "Op を書かない");
    assert_eq!(rig.changed_count(), changes, "通知も出さない");
    assert_eq!(rig.book(&id).meta_status, "pending");
    // 通信エラーはキャッシュしない
    let cached: i64 = rig
        .block(rig.db.run(|conn, _| {
            Ok(conn
                .query_row("SELECT count(*) FROM provider_cache", [], |r| r.get(0))
                .map_err(bibcontainer_core::Error::from)?)
        }))
        .unwrap();
    assert_eq!(cached, 0);

    // オンラインに戻る: 自動で補完される。2段目の値は残る
    rig.http.set_online();
    let summary = rig.block(rig.state.run_pending_refetch()).unwrap().unwrap();
    assert_eq!(
        (summary.requested, summary.updated, summary.offline),
        (1, 1, 0)
    );
    let book = rig.book(&id);
    assert_eq!(book.meta_status, "ok");
    assert_eq!(book.title, "こころ");
    assert!(book.publisher.is_some());
    assert_eq!(book.c_code.as_deref(), Some("0193"));
    assert_eq!(
        book.list_price,
        Some(700),
        "2段目の価格は取得元の価格に置き換わらない"
    );
    assert_eq!(book.field_sources["list_price"], "barcode");
    assert_eq!(book.field_sources["title"], "openbd");
    assert_eq!(
        rig.changed.lock().unwrap().last().unwrap().0,
        vec![id.clone()]
    );
    // 取得待ちはなくなる
    let summary = rig.block(rig.state.run_pending_refetch()).unwrap().unwrap();
    assert_eq!(summary.requested, 0);
}

#[test]
fn offline_pending_books_stop_after_the_first_probe() {
    let rig = Rig::new();
    rig.http.set_offline();
    let ids: Vec<String> = (0..4)
        .map(|_| bare_book(&rig, Some(KOKORO), "pending"))
        .collect();
    rig.http.clear();
    let summary = rig.block(rig.state.run_pending_refetch()).unwrap().unwrap();
    assert_eq!((summary.requested, summary.offline), (4, 4));
    assert!(
        ids.iter()
            .all(|id| item(&summary, id).outcome == RefetchOutcome::Offline)
    );
    // 試したのは最初の 1 冊だけ（openBD と NDL に 1 回ずつ）
    assert_eq!(rig.http.urls().len(), 2, "{:?}", rig.http.urls());
}

#[test]
fn a_second_pending_run_is_skipped_while_one_is_running() {
    let rig = Rig::new();
    bare_book(&rig, Some(KOKORO), "pending");
    let result = rig.block(async {
        let _running = rig.state.refetch_lock.lock().await;
        rig.state.run_pending_refetch().await
    });
    assert_eq!(result.unwrap(), None, "走っている間は見送る");
    // 終われば走る
    let summary = rig.block(rig.state.run_pending_refetch()).unwrap().unwrap();
    assert_eq!(summary.updated, 1);
}

#[test]
fn trigger_runs_in_the_background_but_not_twice_in_a_short_time() {
    let rig = Rig::new();
    let id = bare_book(&rig, Some(KOKORO), "pending");

    // 前回の試行から間もないときは、何もしない
    *rig.state.last_pass.lock().unwrap() = Some(Instant::now());
    rig.block(async { rig.state.trigger() });
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(rig.book(&id).meta_status, "pending");
    assert!(rig.http.urls().is_empty());

    // 間があけば、裏で取得待ちを取り直す
    *rig.state.last_pass.lock().unwrap() = None;
    rig.block(async { rig.state.trigger() });
    let deadline = Instant::now() + Duration::from_secs(20);
    while rig.book(&id).meta_status == "pending" && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(rig.book(&id).meta_status, "ok");
}

// ---------------------------------------------------------------------------
// AT-16: 手修正の保護
// ---------------------------------------------------------------------------

#[test]
fn at16_refetch_does_not_overwrite_a_manually_edited_title() {
    let rig = Rig::new();
    let id = register(&rig, ONIGIRI, None);
    assert_eq!(rig.book(&id).title, "おにぎりレシピ101");
    manual_title(&rig, &id, "手で直した書名");
    // 取得元の値が変わっていても書名は保たれる。ほかの項目の値を変えておく
    rig.update(
        &id,
        vec![FieldChange::provider("publisher", "古い出版社", "openbd")],
        UpdateOptions::default(),
    );

    let summary = rig
        .block(rig.state.refetch_books(vec![id.clone()], false))
        .unwrap();
    let it = item(&summary, &id);
    assert_eq!(it.outcome, RefetchOutcome::Updated);
    assert_eq!(it.skipped_manual, vec!["title".to_owned()], "{it:?}");
    assert!(it.written.contains(&"publisher".to_owned()));
    let book = rig.book(&id);
    assert_eq!(book.title, "手で直した書名");
    assert!(book.manual_fields.contains(&"title".to_owned()));
    assert_eq!(book.field_sources["title"], "manual");
    assert_eq!(
        book.publisher.as_deref(),
        Some("ポット出版"),
        "手修正でない項目は更新される"
    );
    // キャッシュを使わず取り直した（強制の再取得）
    assert_eq!(rig.http.count("api.openbd.jp"), 2);

    // 上書きを選ぶと、取得元の値に戻り、手修正の記録も外れる
    let summary = rig
        .block(rig.state.refetch_books(vec![id.clone()], true))
        .unwrap();
    assert!(item(&summary, &id).skipped_manual.is_empty());
    let book = rig.book(&id);
    assert_eq!(book.title, "おにぎりレシピ101");
    assert!(!book.manual_fields.contains(&"title".to_owned()));
    assert_eq!(book.field_sources["title"], "openbd");
}

#[test]
fn refetch_keeps_second_tier_values_but_replaces_other_provider_values() {
    let rig = Rig::new();
    // 2段目の価格・Cコードで登録（取得元には 1300 円・0077 とある）
    let id = register(&rig, ONIGIRI, Some(tier("0193", 1200)));
    assert_eq!(rig.book(&id).c_code.as_deref(), Some("0193"));
    // 取得元由来の値は、再取得で最新の値になる
    let other = register(&rig, ONIGIRI, None);
    rig.update(
        &other,
        vec![FieldChange::provider("c_code", "0000", "openbd")],
        UpdateOptions::default(),
    );

    rig.block(
        rig.state
            .refetch_books(vec![id.clone(), other.clone()], false),
    )
    .unwrap();

    let kept = rig.book(&id);
    assert_eq!(kept.c_code.as_deref(), Some("0193"));
    assert_eq!(kept.list_price, Some(1200));
    assert_eq!(kept.field_sources["c_code"], "barcode");
    assert_eq!(kept.field_sources["list_price"], "barcode");
    let replaced = rig.book(&other);
    assert_eq!(replaced.c_code.as_deref(), Some("0077"));
    assert_eq!(replaced.field_sources["c_code"], "openbd");
}

#[test]
fn refetch_fetches_a_cover_only_when_the_book_has_none() {
    let rig = Rig::new();
    let id = bare_book(&rig, Some(ONIGIRI), "pending");
    assert_eq!(rig.book(&id).cover_hash, None);

    let summary = rig
        .block(rig.state.refetch_books(vec![id.clone()], false))
        .unwrap();
    assert_eq!(summary.covers_saved, 1);
    let book = rig.book(&id);
    let hash = book.cover_hash.clone().expect("書影が付く");
    assert!(cover_path(&rig.state.dirs().covers, &hash).is_file());
    assert_eq!(book.field_sources["cover_hash"], "openbd");
    assert_eq!(book.meta_status, "ok");
    assert_eq!(rig.http.count("cover.openbd.jp"), 1);

    // 書影があれば、もう取らない
    let summary = rig
        .block(rig.state.refetch_books(vec![id.clone()], false))
        .unwrap();
    assert_eq!(summary.covers_saved, 0);
    assert_eq!(rig.http.count("cover.openbd.jp"), 1);
}

#[test]
fn refetch_reports_books_it_cannot_refetch_and_continues_with_the_rest() {
    let rig = Rig::new();
    let no_isbn = bare_book(&rig, None, "ok");
    let good = bare_book(&rig, Some(KOKORO), "failed");
    let purged = bare_book(&rig, Some(YUKIGUNI), "ok");
    rig.block(rig.db.run({
        let purged = purged.clone();
        move |conn, _| {
            let clock = bibcontainer_core::SystemClock;
            let env = bibcontainer_core::library::Env::new(&clock);
            bibcontainer_core::library::purge_books(conn, &env, &[purged])?;
            Ok(())
        }
    }))
    .unwrap();

    let ids = vec![
        no_isbn.clone(),
        "no-such-id".to_owned(),
        good.clone(),
        purged.clone(),
        good.clone(), // 重複は 1 回だけ
    ];
    let summary = rig.block(rig.state.refetch_books(ids, false)).unwrap();
    assert_eq!(summary.requested, 4);
    assert_eq!(
        (summary.updated, summary.skipped, summary.failed),
        (1, 3, 0)
    );
    assert_eq!(item(&summary, &no_isbn).outcome, RefetchOutcome::Skipped);
    assert_eq!(
        item(&summary, "no-such-id").outcome,
        RefetchOutcome::Skipped
    );
    assert_eq!(item(&summary, &purged).outcome, RefetchOutcome::Skipped);
    assert!(
        item(&summary, &no_isbn)
            .message
            .as_deref()
            .unwrap()
            .contains("ISBN")
    );
    let it = item(&summary, &good);
    assert_eq!(it.outcome, RefetchOutcome::Updated);
    assert_eq!(it.meta_status, Some(MetaStatus::Ok));
    assert_eq!(
        rig.book(&good).meta_status,
        "ok",
        "failed の本が取れて ok になる"
    );
    // 渡した順
    let order: Vec<&str> = summary.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        order,
        vec![
            no_isbn.as_str(),
            "no-such-id",
            good.as_str(),
            purged.as_str()
        ]
    );
}

#[test]
fn a_refetch_that_finds_nothing_keeps_the_status_of_a_book_that_has_data() {
    let rig = Rig::new();
    let id = register(&rig, ONIGIRI, None);
    // どの取得元にも「該当なし」を返させる
    rig.http.set_handler(Box::new(|url| {
        if url.contains("api.openbd.jp") {
            response(200, "application/json", b"[null]".to_vec())
        } else {
            response(
                200,
                "text/xml;charset=UTF-8",
                fixture("ndl_notfound.xml").into_bytes(),
            )
        }
    }));
    let before = rig.book(&id);
    let summary = rig
        .block(rig.state.refetch_books(vec![id.clone()], false))
        .unwrap();
    assert_eq!(item(&summary, &id).outcome, RefetchOutcome::Unchanged);
    let after = rig.book(&id);
    assert_eq!(after.meta_status, "ok");
    assert_eq!(after.title, before.title);

    // 取得待ちの本なら、「該当なし」は failed になる
    let pending = bare_book(&rig, Some(ONIGIRI), "pending");
    rig.block(rig.state.refetch_books(vec![pending.clone()], false))
        .unwrap();
    assert_eq!(rig.book(&pending).meta_status, "failed");
}

#[test]
fn refetch_with_every_provider_off_changes_nothing_and_says_so() {
    let rig = Rig::new();
    let id = bare_book(&rig, Some(KOKORO), "pending");
    let mut s = rig.block(rig.state.get_provider_settings()).unwrap();
    for enabled in s.enabled.values_mut() {
        *enabled = false;
    }
    rig.block(rig.state.set_provider_settings(s)).unwrap();

    let summary = rig
        .block(rig.state.refetch_books(vec![id.clone()], false))
        .unwrap();
    assert!(summary.no_active_providers);
    assert_eq!(summary.skipped, 1);
    assert_eq!(rig.book(&id).meta_status, "pending");
    assert_eq!(rig.block(rig.state.run_pending_refetch()).unwrap(), None);
    assert!(rig.http.urls().is_empty());

    // 取得しようとすると、取得待ちの下書き
    let fetched = rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Pending);
    assert!(fetched.reports.is_empty());
}

// ---------------------------------------------------------------------------
// 下書きからの作成
// ---------------------------------------------------------------------------

#[test]
fn drafts_create_books_in_one_transaction_with_series_resolved_by_name() {
    let rig = Rig::new();
    let series = |name: &str| {
        Some(DraftSeries {
            name: name.into(),
            origin: Origin::Provider("openbd".into()),
        })
    };
    let drafts = vec![
        NewBookDraft {
            changes: vec![
                FieldChange::manual("title", "ワンピース 1"),
                FieldChange::provider("volume", "巻1", "openbd"),
            ],
            series_name: series("ONE PIECE"),
        },
        NewBookDraft {
            changes: vec![FieldChange::manual("title", "ワンピース 2")],
            // 正規化（NFKC・小文字）で同じ名前は、同じシリーズ
            series_name: series("one piece"),
        },
        NewBookDraft {
            changes: vec![FieldChange::manual("title", "シリーズなし")],
            series_name: series("   "),
        },
    ];
    let ids = rig
        .block(rig.state.create_books_from_drafts(drafts))
        .unwrap();
    assert_eq!(ids.len(), 3);
    assert_eq!(rig.series_names(), vec!["ONE PIECE".to_owned()]);

    let first = rig.book(&ids[0]);
    let second = rig.book(&ids[1]);
    assert!(first.series_id.is_some());
    assert_eq!(first.series_id, second.series_id);
    assert_eq!(first.field_sources["series_id"], "openbd");
    assert_eq!(first.volume.as_deref(), Some("巻1"));
    assert_eq!(
        first.volume_sort,
        Some(1.0),
        "巻次から並べ替え用の数値を作る"
    );
    assert_eq!(
        rig.book(&ids[2]).series_id,
        None,
        "空のシリーズ名は無視する"
    );

    let (changed, others) = rig.changed.lock().unwrap().last().cloned().unwrap();
    assert_eq!(changed, ids);
    assert!(others, "シリーズが増えたので、本以外も変わった");
}

#[test]
fn one_bad_draft_cancels_everything_including_the_series() {
    let rig = Rig::new();
    let drafts = vec![
        NewBookDraft {
            changes: vec![FieldChange::manual("title", "良い本")],
            series_name: Some(DraftSeries {
                name: "新しいシリーズ".into(),
                origin: Origin::Manual,
            }),
        },
        NewBookDraft {
            changes: vec![FieldChange::manual("rating", 9)],
            series_name: None,
        },
    ];
    let err = rig
        .block(rig.state.create_books_from_drafts(drafts))
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(rig.series_names().is_empty(), "シリーズも作られない");
    let books: i64 = rig
        .block(rig.db.run(|conn, _| {
            Ok(conn
                .query_row("SELECT count(*) FROM books", [], |r| r.get(0))
                .map_err(bibcontainer_core::Error::from)?)
        }))
        .unwrap();
    assert_eq!(books, 0);
    assert_eq!(rig.changed_count(), 0, "失敗したら通知しない");
}

#[test]
fn manual_registration_without_an_isbn_goes_through_the_same_path() {
    let rig = Rig::new();
    let ids = rig
        .block(rig.state.create_books_from_drafts(vec![NewBookDraft {
            changes: vec![
                FieldChange::manual("title", "ISBN のない本"),
                FieldChange::manual("publisher", "自費出版"),
            ],
            series_name: None,
        }]))
        .unwrap();
    let book = rig.book(&ids[0]);
    assert_eq!(book.isbn13, None);
    assert_eq!(book.title, "ISBN のない本");
    assert_eq!(book.meta_status, "ok");
    assert!(book.manual_fields.contains(&"title".to_owned()));
    assert!(
        rig.block(rig.state.create_books_from_drafts(vec![]))
            .unwrap()
            .is_empty()
    );
    assert_eq!(rig.changed.lock().unwrap().len(), 1, "空の依頼は通知しない");
}

// ---------------------------------------------------------------------------
// 設定・キー・取得元の一覧・接続テスト
// ---------------------------------------------------------------------------

#[test]
fn providers_list_shows_enabled_key_and_active_and_no_ndl_cover_provider() {
    let rig = Rig::new();
    let list = rig.block(rig.state.list_providers()).unwrap();
    let ids: Vec<ProviderId> = list.iter().map(|p| p.info.id).collect();
    assert_eq!(
        ids,
        vec![ProviderId::Openbd, ProviderId::Ndl, ProviderId::Google]
    );
    assert!(list.iter().all(|p| !p.info.credit.is_empty()));
    let status = |id: ProviderId| list.iter().find(|p| p.info.id == id).unwrap().clone();
    // 既定ではどれも有効。Google はキーがないので使わない
    assert!(status(ProviderId::Openbd).active);
    assert!(status(ProviderId::Ndl).active);
    let google = status(ProviderId::Google);
    assert!(google.enabled && !google.has_key && !google.active);

    // IPC の形: 能力情報と同じ階層に、enabled・hasKey・active がある
    let json = serde_json::to_value(&google).unwrap();
    assert_eq!(json["id"], "google");
    assert_eq!(json["displayName"], "Google Books");
    assert_eq!(json["auth"]["kind"], "required");
    assert_eq!(json["enabled"], true);
    assert_eq!(json["hasKey"], false);
    assert_eq!(json["active"], false);
    assert!(json["credit"].as_str().unwrap().contains("Google"));
}

#[test]
fn api_key_is_stored_in_the_keychain_under_the_provider_id_and_enables_google() {
    let rig = Rig::new();
    assert!(
        !rig.block(rig.state.has_api_key(ProviderId::Google))
            .unwrap()
    );

    rig.block(
        rig.state
            .set_api_key(ProviderId::Google, "  AIzaSy-test-key  "),
    )
    .unwrap();
    // 保存先は OS のキーチェーン（テストではメモリ）。アカウント名は取得元 ID、前後の空白は除く
    use super::keychain::KeyStore;
    assert_eq!(
        rig.keys.get("google").unwrap().unwrap().expose(),
        "AIzaSy-test-key"
    );
    assert!(
        rig.block(rig.state.has_api_key(ProviderId::Google))
            .unwrap()
    );
    let google = rig
        .block(rig.state.list_providers())
        .unwrap()
        .into_iter()
        .find(|p| p.info.id == ProviderId::Google)
        .unwrap();
    assert!(google.has_key && google.active);

    // キーを入れると Google にも問い合わせ、キーを URL につけて送る
    let result = rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    assert!(
        result
            .reports
            .iter()
            .any(|r| r.provider == ProviderId::Google)
    );
    assert!(
        rig.http
            .urls()
            .iter()
            .any(|u| u.contains("googleapis.com") && u.contains("key=AIzaSy-test-key"))
    );
    // DB（設定）にキーは入らない
    let stored = rig
        .block(
            rig.db
                .run(|conn, _| Ok(bibcontainer_core::settings::all(conn)?)),
        )
        .unwrap();
    assert!(!format!("{stored:?}").contains("AIzaSy"));

    // 削除するとまた使わなくなる
    rig.block(rig.state.delete_api_key(ProviderId::Google))
        .unwrap();
    assert!(rig.keys.get("google").unwrap().is_none());
    assert!(
        !rig.block(rig.state.has_api_key(ProviderId::Google))
            .unwrap()
    );
    rig.http.clear();
    rig.block(rig.state.fetch_metadata(KOKORO, None)).unwrap();
    assert_eq!(rig.http.count("googleapis.com"), 0);
    // なくても削除はエラーにならない
    rig.block(rig.state.delete_api_key(ProviderId::Google))
        .unwrap();
}

#[test]
fn keys_are_validated_and_only_for_providers_that_use_them() {
    let rig = Rig::new();
    for bad in ["", "   ", "has space", "line\nbreak"] {
        let err = rig
            .block(rig.state.set_api_key(ProviderId::Google, bad))
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation, "{bad:?}");
        assert!(
            !err.to_string().contains("space"),
            "キーをメッセージに出さない"
        );
    }
    use super::keychain::KeyStore;
    assert!(rig.keys.get("google").unwrap().is_none());
    // キーを使わない取得元
    let err = rig
        .block(rig.state.set_api_key(ProviderId::Openbd, "x"))
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(rig.keys.get("openbd").unwrap().is_none());
    assert!(
        !rig.block(rig.state.has_api_key(ProviderId::Openbd))
            .unwrap()
    );
}

#[test]
fn settings_change_which_providers_are_used_and_the_priorities_and_the_user_agent() {
    let rig = Rig::new();
    let mut s = rig.block(rig.state.get_provider_settings()).unwrap();
    assert_eq!(s.contact_url, "https://github.com/mioupa/bibcontainer");
    assert_eq!(
        rig.user_agents.lock().unwrap().last(),
        None,
        "最初の取得まで道具は作らない"
    );

    // NDL を先にし、openBD を無効にする。連絡先も変える
    s.enabled.insert(ProviderId::Openbd, false);
    s.priorities
        .set(MetaGroup::Title, vec![MetaSource::Ndl, MetaSource::Openbd]);
    s.contact_url = "https://example.org/contact".into();
    let saved = rig.block(rig.state.set_provider_settings(s)).unwrap();
    assert_eq!(rig.block(rig.state.get_provider_settings()).unwrap(), saved);
    assert_eq!(
        rig.user_agents.lock().unwrap().last().unwrap(),
        "bibcontainer/0.0.0-test (+https://example.org/contact)"
    );

    let result = rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    assert_eq!(
        rig.http.count("api.openbd.jp"),
        0,
        "無効にした取得元には問い合わせない"
    );
    assert_eq!(result.reports.len(), 1);
    assert_eq!(
        result.draft.fields.title.as_ref().unwrap().origin,
        Origin::Provider("ndl".into())
    );
    assert!(
        result.draft.fields.c_code.is_none(),
        "Cコードは openBD にしかない"
    );

    // 不正な連絡先は保存されない
    let mut bad = saved.clone();
    bad.contact_url = "mailto:me@example.org".into();
    let err = rig.block(rig.state.set_provider_settings(bad)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert_eq!(rig.block(rig.state.get_provider_settings()).unwrap(), saved);
}

#[test]
fn test_provider_fetches_a_known_isbn_without_the_cache_even_if_the_provider_is_off() {
    let rig = Rig::new();
    let found = rig
        .block(rig.state.test_provider(ProviderId::Openbd))
        .unwrap();
    assert_eq!(found.provider, ProviderId::Openbd);
    assert_eq!(found.isbn13, "9784780802047");
    assert_eq!(found.tried, vec!["9784780802047"], "取れたら、そこで止める");
    assert_eq!(found.outcome, ProviderOutcome::Found);
    assert_eq!(found.verdict, ProviderTestVerdict::Found);
    assert_eq!(found.title.as_deref(), Some("おにぎりレシピ101"));
    assert_eq!(found.c_code.as_deref(), Some("0077"));
    assert!(found.has_cover);
    assert_eq!(rig.http.count("api.openbd.jp"), 1);

    // もう 1 回: キャッシュがあっても通信する
    rig.block(rig.state.test_provider(ProviderId::Openbd))
        .unwrap();
    assert_eq!(rig.http.count("api.openbd.jp"), 2);

    // 無効にしていても試せる
    let mut s = rig.block(rig.state.get_provider_settings()).unwrap();
    s.enabled.insert(ProviderId::Ndl, false);
    rig.block(rig.state.set_provider_settings(s)).unwrap();
    let ndl = rig.block(rig.state.test_provider(ProviderId::Ndl)).unwrap();
    assert_eq!(ndl.outcome, ProviderOutcome::Found);
    assert!(ndl.title.is_some());

    // Google はキーがなければ試さない。キーがあれば試せる
    let err = rig
        .block(rig.state.test_provider(ProviderId::Google))
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert_eq!(rig.http.count("googleapis.com"), 0);
    rig.block(rig.state.set_api_key(ProviderId::Google, "AIza-key"))
        .unwrap();
    let google = rig
        .block(rig.state.test_provider(ProviderId::Google))
        .unwrap();
    assert_eq!(google.outcome, ProviderOutcome::Found);
    assert_eq!(google.verdict, ProviderTestVerdict::Found);
    assert_eq!(
        google.isbn13, KOKORO,
        "Google は、和書の定番（こころ）から試す"
    );
    assert_eq!(google.title.as_deref(), Some("こころ"));

    // つながらないときは、エラーではなく結果として返す
    rig.http.set_offline();
    let down = rig
        .block(rig.state.test_provider(ProviderId::Openbd))
        .unwrap();
    assert!(matches!(down.outcome, ProviderOutcome::NetworkError { .. }));
    assert_eq!(down.verdict, ProviderTestVerdict::NetworkError);
    assert_eq!(down.title, None);
    assert_eq!(down.tried.len(), 1, "通信できなければ、次の本は試さない");
}

#[test]
fn test_isbns_are_valid_distinct_and_at_most_three_per_provider() {
    use bibcontainer_metadata::isbn::to_isbn13;
    for id in [ProviderId::Openbd, ProviderId::Ndl, ProviderId::Google] {
        let isbns = test_isbns(id);
        assert!((1..=3).contains(&isbns.len()), "{id:?}");
        for isbn in isbns {
            assert_eq!(
                to_isbn13(isbn).as_deref(),
                Some(*isbn),
                "{id:?}: {isbn} はチェックデジットが正しい ISBN-13"
            );
        }
        let mut sorted = isbns.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), isbns.len(), "{id:?}: 重複がない");
    }
    // Google は、和書と洋書の両方を試す（978-4 は日本の ISBN）
    let google = test_isbns(ProviderId::Google);
    assert!(google.iter().any(|i| i.starts_with("9784")));
    assert!(google.iter().any(|i| !i.starts_with("9784")));
    // openBD・NDL は和書だけの取得元なので、和書だけ
    for id in [ProviderId::Openbd, ProviderId::Ndl] {
        assert!(test_isbns(id).iter().all(|i| i.starts_with("9784")));
    }
}

/// Google を、決めた応答で動かす（キーは入れる）。ほかの取得元は、ふだんのフィクスチャのまま。
fn rig_with_google(
    google: impl Fn(&str) -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
) -> Rig {
    let rig = Rig::new();
    rig.block(rig.state.set_api_key(ProviderId::Google, "AIza-key"))
        .unwrap();
    let base = online_handler();
    rig.http.set_handler(Box::new(move |url| {
        if url.contains("googleapis.com") {
            google(url)
        } else {
            base(url)
        }
    }));
    rig
}

/// ISBN に合う 1 冊だけを返す Google の応答（`industryIdentifiers` が合わないと該当なしになる）。
fn google_body_for(isbn: &str, title: &str) -> String {
    json!({
        "kind": "books#volumes",
        "totalItems": 1,
        "items": [{
            "volumeInfo": {
                "title": title,
                "industryIdentifiers": [{ "type": "ISBN_13", "identifier": isbn }],
            }
        }]
    })
    .to_string()
}

fn google_json(status: u16, body: impl Into<String>) -> Result<HttpResponse, TransportError> {
    response(
        status,
        "application/json; charset=UTF-8",
        body.into().into_bytes(),
    )
}

#[test]
fn test_provider_tries_the_next_book_when_one_is_not_found() {
    // 1 冊目（こころ）は Google にない。2 冊目（吾輩は猫である）で取れる
    let isbns = test_isbns(ProviderId::Google);
    let second = isbns[1];
    let rig = rig_with_google(move |url| {
        if url.contains(&format!("isbn:{second}")) {
            google_json(200, google_body_for(second, "吾輩は猫である"))
        } else {
            google_json(200, fixture("google_synthetic_notfound.json"))
        }
    });
    let r = rig
        .block(rig.state.test_provider(ProviderId::Google))
        .unwrap();
    assert_eq!(r.verdict, ProviderTestVerdict::Found);
    assert_eq!(r.isbn13, second);
    assert_eq!(r.tried, vec![isbns[0], isbns[1]]);
    assert_eq!(r.title.as_deref(), Some("吾輩は猫である"));
    assert_eq!(
        rig.http.count("googleapis.com"),
        2,
        "取れたら、そこで止める"
    );
}

#[test]
fn test_provider_reports_success_when_the_key_works_but_no_test_book_is_found() {
    // 200 で、3 冊とも該当なし。通信できていて、キーも有効（無効なら 400・403）なので、失敗ではない
    let rig = rig_with_google(|_| google_json(200, fixture("google_synthetic_notfound.json")));
    let r = rig
        .block(rig.state.test_provider(ProviderId::Google))
        .unwrap();
    assert_eq!(r.outcome, ProviderOutcome::NotFound);
    assert_eq!(r.verdict, ProviderTestVerdict::NotFound);
    assert_eq!(r.tried, test_isbns(ProviderId::Google));
    assert_eq!(r.isbn13, *test_isbns(ProviderId::Google).last().unwrap());
    assert_eq!(r.title, None);
    assert!(!r.has_cover);
    assert_eq!(rig.http.count("googleapis.com"), 3, "最大 3 件で止める");
}

#[test]
fn test_provider_tells_an_invalid_key_from_a_quota_problem() {
    let zero = fixture("google_quota_exceeded.json");
    let daily = zero.replace(
        "\"quota_limit_value\": \"0\"",
        "\"quota_limit_value\": \"1000\"",
    );
    let invalid_key = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID"}]}}"#;
    let disabled = r#"{"error":{"code":403,"message":"Books API has not been used in project 123 before or it is disabled.","status":"PERMISSION_DENIED"}}"#;
    let unauthenticated = r#"{"error":{"code":401,"message":"Request had invalid authentication credentials.","status":"UNAUTHENTICATED"}}"#;
    let teapot = r#"{"error":{"code":418,"message":"I am a teapot"}}"#;

    let cases: Vec<(u16, String, ProviderTestVerdict)> = vec![
        (400, invalid_key.to_owned(), ProviderTestVerdict::InvalidKey),
        (403, disabled.to_owned(), ProviderTestVerdict::InvalidKey),
        (
            401,
            unauthenticated.to_owned(),
            ProviderTestVerdict::InvalidKey,
        ),
        (429, daily, ProviderTestVerdict::QuotaExceeded),
        (429, zero, ProviderTestVerdict::QuotaZero),
        (418, teapot.to_owned(), ProviderTestVerdict::HttpError),
    ];
    for (status, body, expected) in cases {
        let rig = rig_with_google(move |_| google_json(status, body.clone()));
        let r = rig
            .block(rig.state.test_provider(ProviderId::Google))
            .unwrap();
        assert_eq!(r.verdict, expected, "{status}");
        assert!(
            matches!(&r.outcome, ProviderOutcome::HttpError { status: s, .. } if *s == status),
            "{status}: {:?}",
            r.outcome
        );
        assert_eq!(r.title, None);
        assert_eq!(
            rig.http.count("googleapis.com"),
            1,
            "{status}: エラーなら、次の本は試さない"
        );
    }
}

#[test]
fn test_provider_gives_the_http_status_as_is_for_providers_without_a_key() {
    // 401・403 を「API キーが無効」と読むのは、キーを使う取得元（Google）だけ
    let rig = Rig::new();
    rig.http.set_handler(Box::new(|_| {
        response(403, "text/plain", b"forbidden".to_vec())
    }));
    let r = rig
        .block(rig.state.test_provider(ProviderId::Openbd))
        .unwrap();
    assert!(matches!(
        r.outcome,
        ProviderOutcome::HttpError { status: 403, .. }
    ));
    assert_eq!(r.verdict, ProviderTestVerdict::HttpError);

    // NDL: 該当なしが続けば、3 冊で止めて、通信とも成功として返す
    rig.http.set_handler(Box::new(|url| {
        if url.contains("ndlsearch") {
            response(
                200,
                "text/xml;charset=UTF-8",
                fixture("ndl_notfound.xml").into_bytes(),
            )
        } else {
            response(404, "text/plain", Vec::new())
        }
    }));
    let r = rig.block(rig.state.test_provider(ProviderId::Ndl)).unwrap();
    assert_eq!(r.verdict, ProviderTestVerdict::NotFound);
    assert_eq!(r.tried.len(), 3);
}

// ---------------------------------------------------------------------------
// 書影の取り込み
// ---------------------------------------------------------------------------

#[test]
fn covers_can_be_imported_from_bytes_a_file_and_a_url() {
    let rig = Rig::new();
    let png = fixture_bytes("google_noimage_575x750.png");
    let covers = rig.state.dirs().covers.clone();

    // バイト列
    let hash = rig
        .block(rig.state.import_cover_bytes(png.clone()))
        .unwrap();
    assert_eq!(hash.len(), 64);
    assert!(cover_path(&covers, &hash).is_file());
    // 同じ画像は同じハッシュ（1 つだけ保存される）
    assert_eq!(
        rig.block(rig.state.import_cover_bytes(png.clone()))
            .unwrap(),
        hash
    );

    // ファイル
    let file = rig.dir.path().join("picked.png");
    std::fs::write(&file, &png).unwrap();
    assert_eq!(
        rig.block(rig.state.import_cover_file(file.to_str().unwrap()))
            .unwrap(),
        hash
    );

    // URL（画像）
    let from_url = rig
        .block(rig.state.import_cover_url("https://img.example/cover.jpg"))
        .unwrap();
    assert_eq!(from_url.len(), 64);
    assert!(cover_path(&covers, &from_url).is_file());
}

#[test]
fn bad_cover_imports_are_rejected_with_clear_error_kinds() {
    let rig = Rig::new();
    let kind = |r: Result<String, AppError>| r.unwrap_err().kind();

    assert_eq!(
        kind(rig.block(rig.state.import_cover_bytes(vec![]))),
        ErrorKind::Validation
    );
    assert_eq!(
        kind(rig.block(rig.state.import_cover_bytes(b"<html>no</html>".to_vec()))),
        ErrorKind::Validation
    );
    // ファイル: 無い・フォルダ・空のパス・画像でない
    assert_eq!(
        kind(
            rig.block(
                rig.state
                    .import_cover_file(rig.dir.path().join("none.png").to_str().unwrap())
            )
        ),
        ErrorKind::Io
    );
    assert_eq!(
        kind(
            rig.block(
                rig.state
                    .import_cover_file(rig.dir.path().to_str().unwrap())
            )
        ),
        ErrorKind::Validation
    );
    assert_eq!(
        kind(rig.block(rig.state.import_cover_file("  "))),
        ErrorKind::Validation
    );
    let text = rig.dir.path().join("memo.txt");
    std::fs::write(&text, "hello").unwrap();
    assert_eq!(
        kind(rig.block(rig.state.import_cover_file(text.to_str().unwrap()))),
        ErrorKind::Validation
    );
    // URL: http(s) でない・画像でない応答・見つからない
    assert_eq!(
        kind(rig.block(rig.state.import_cover_url("ftp://img.example/cover.jpg"))),
        ErrorKind::Validation
    );
    assert_eq!(
        kind(rig.block(rig.state.import_cover_url("https://img.example/page.html"))),
        ErrorKind::Validation
    );
    assert_eq!(
        kind(
            rig.block(
                rig.state
                    .import_cover_url("https://img.example/missing.jpg")
            )
        ),
        ErrorKind::Network
    );
    // 取り込みに失敗したあとも、書影のフォルダに壊れたものを残さない
    let covers = rig.state.dirs().covers.clone();
    assert!(!covers.exists() || walk(&covers).is_empty());
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn the_cover_scheme_serves_a_cover_saved_by_a_fetch() {
    let rig = Rig::new();
    let result = rig.block(rig.state.fetch_metadata(ONIGIRI, None)).unwrap();
    let hash = result.cover_hash.unwrap();
    let reply =
        super::cover_protocol::respond(rig.state.dirs(), &format!("/{hash}"), Some("size=160"));
    assert_eq!(reply.status, 200);
    assert_eq!(&reply.body[..2], &[0xFF, 0xD8]);
    let original = super::cover_protocol::respond(rig.state.dirs(), &format!("/{hash}"), None);
    assert_eq!(
        original.body,
        std::fs::read(cover_path(&rig.state.dirs().covers, &hash)).unwrap()
    );
}

#[test]
fn draft_values_round_trip_to_field_changes_that_the_core_accepts() {
    // 下書き → FieldChange → 作成 → 読み出し（ISBN・巻次・著者・取得元の記録まで）
    let rig = Rig::new();
    let id = register(&rig, ONE_PIECE, None);
    let book = rig.book(&id);
    assert_eq!(book.isbn13.as_deref(), Some(ONE_PIECE));
    assert_eq!(book.title, "One piece");
    assert_eq!(book.volume_sort, Some(1.0));
    assert!(!book.authors.is_empty());
    assert!(book.series_id.is_some());
    assert_eq!(book.field_sources["isbn13"], "barcode");
    assert_eq!(
        book.field_sources["volume"].as_str(),
        book.field_sources["volume_sort"].as_str()
    );
    assert_eq!(json!(book.meta_status), json!("ok"));
}
