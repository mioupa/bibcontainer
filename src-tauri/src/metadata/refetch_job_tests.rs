//! 一括再取得の作業（0027）の結合テスト。AT-46（裏で進む・進み具合）、AT-47（やめる・同時に 1 つ）、
//! AT-48（制限・オフラインで止まる、古い順に取る）。偽の HTTP と一時 DB を使う。

use std::time::{Duration, Instant};

use bibcontainer_core::library::{FieldChange, Origin, UpdateOptions};
use bibcontainer_metadata::ProviderId;
use bibcontainer_metadata::http::TransportError;

use super::refetch_job::{RefetchJobState, RefetchJobStatus, RefetchStopReason};
use super::register::NewBookDraft;
use super::testing::*;
use crate::error::AppError;

/// 書誌を持たない本（ISBN だけ）を作る。
fn bare_book(rig: &Rig, isbn: &str, status: &str) -> String {
    rig.block(rig.state.create_books_from_drafts(vec![NewBookDraft {
        changes: vec![
            FieldChange::new("meta_status", status, Origin::Barcode),
            FieldChange::barcode("isbn13", isbn),
        ],
        series_name: None,
    }]))
    .unwrap()
    .remove(0)
}

/// 同じ ISBN の取得待ちの本を `n` 冊作る。
fn pending_books(rig: &Rig, n: usize, isbn: &str) -> Vec<String> {
    (0..n).map(|_| bare_book(rig, isbn, "pending")).collect()
}

/// 条件を満たすまで待つ（10 ミリ秒ごとに確かめる）。
fn wait_until(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < timeout, "待ちきれませんでした: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn job(rig: &Rig) -> Option<RefetchJobStatus> {
    rig.state.get_refetch_job()
}

fn wait_finished(rig: &Rig) -> RefetchJobStatus {
    wait_until("作業の終わり", Duration::from_secs(60), || {
        job(rig).is_some_and(|j| j.state != RefetchJobState::Running)
    });
    job(rig).unwrap()
}

fn progress_len(rig: &Rig) -> usize {
    rig.progress.lock().unwrap().len()
}

fn title_of(rig: &Rig, id: &str) -> String {
    rig.book(id).title
}

// ---------------------------------------------------------------------------
// AT-46: 裏で進む。進み具合が届く
// ---------------------------------------------------------------------------

#[test]
fn at46_start_returns_at_once_and_progress_is_reported_per_chunk() {
    let rig = Rig::new();
    let mut ids = pending_books(&rig, 25, KOKORO);
    // 手で直した項目は上書きしない（数える）
    rig.update(
        &ids[12],
        vec![FieldChange::manual("title", "手で直した書名")],
        UpdateOptions::default(),
    );
    // 重複は 1 冊として数える
    ids.push(ids[0].clone());

    let status = rig
        .block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    assert_eq!(status.state, RefetchJobState::Running);
    assert_eq!((status.job_id, status.total, status.done), (1, 25, 0));
    assert_eq!(status.remaining, 25);
    assert!(!status.override_manual);
    assert_eq!(
        rig.progress.lock().unwrap()[0].state,
        RefetchJobState::Running
    );

    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Finished);
    assert_eq!(last.stop_reason, None);
    assert_eq!(
        (last.total, last.done, last.remaining),
        (25, 25, 0),
        "{last:?}"
    );
    assert_eq!(
        (
            last.updated,
            last.unchanged,
            last.offline,
            last.skipped,
            last.failed
        ),
        (25, 0, 0, 0, 0)
    );
    assert_eq!(last.kept_manual_fields, 1, "手で直した書名だけ");

    // 開始 → まとまりごと（1・10・10 冊）→ 終了。最後のまとまり（4 冊）は終了の状態で知らせる
    let events = rig.progress.lock().unwrap().clone();
    let dones: Vec<u32> = events.iter().map(|e| e.done).collect();
    assert_eq!(dones, vec![0, 1, 11, 21, 25]);
    let states: Vec<RefetchJobState> = events.iter().map(|e| e.state).collect();
    assert_eq!(
        states[..4],
        [RefetchJobState::Running; 4],
        "終わるまでは running"
    );
    assert_eq!(states[4], RefetchJobState::Finished);
    assert!(
        events
            .windows(2)
            .all(|w| w[0].elapsed_ms <= w[1].elapsed_ms)
    );
    assert!(events.iter().all(|e| e.job_id == 1));
    assert_eq!(job(&rig), Some(last));

    // 手で直した書名は残り、ほかの本は取得した書名になった
    assert_eq!(title_of(&rig, &ids[12]), "手で直した書名");
    assert_eq!(title_of(&rig, &ids[0]), "こころ");
    // まとまりごとに library-changed
    assert!(rig.changed_count() >= 4);

    // 終わったあとは、次の作業を始められる（作業の番号は進む）
    let second = rig
        .block(rig.state.start_refetch(vec![ids[0].clone()], false))
        .unwrap();
    assert_eq!(second.job_id, 2);
    assert_eq!(wait_finished(&rig).state, RefetchJobState::Finished);
}

#[test]
fn start_with_no_usable_network_provider_stops_at_once() {
    let rig = Rig::new();
    let ids = pending_books(&rig, 3, KOKORO);
    let mut s = rig.block(rig.state.get_provider_settings()).unwrap();
    for id in [
        ProviderId::Openbd,
        ProviderId::Ndl,
        ProviderId::Google,
        ProviderId::OpenLibrary,
    ] {
        s.enabled.insert(id, false);
    }
    rig.block(rig.state.set_provider_settings(s)).unwrap();

    let status = rig.block(rig.state.start_refetch(ids, false)).unwrap();
    assert_eq!(status.state, RefetchJobState::Stopped);
    assert_eq!(
        status.stop_reason,
        Some(RefetchStopReason::NoActiveProviders)
    );
    assert_eq!((status.total, status.done, status.remaining), (3, 0, 3));
    assert_eq!(job(&rig), Some(status.clone()));
    assert_eq!(*rig.progress.lock().unwrap(), vec![status]);
    assert!(rig.http.urls().is_empty());
    // 枠は空いている
    assert!(
        rig.block(rig.state.start_refetch(Vec::new(), false))
            .is_ok()
    );
}

#[test]
fn an_empty_job_finishes_at_once() {
    let rig = Rig::new();
    let status = rig
        .block(rig.state.start_refetch(Vec::new(), false))
        .unwrap();
    assert_eq!(status.total, 0);
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Finished);
    assert_eq!((last.done, last.remaining), (0, 0));
}

// ---------------------------------------------------------------------------
// AT-47: やめる。同時に 1 つ。登録の取得は待たない
// ---------------------------------------------------------------------------

#[test]
fn at47_cancel_during_a_slow_fetch_stops_at_once_and_writes_nothing_for_that_chunk() {
    let rig = Rig::new();
    let ids = pending_books(&rig, 25, KOKORO);
    // 最初のまとまりが終わったあとの通信は、すべて 5 秒かかる
    {
        let progress = std::sync::Arc::clone(&rig.progress);
        rig.http.set_delay(move |_| {
            if progress.lock().unwrap().len() >= 2 {
                Duration::from_secs(5)
            } else {
                Duration::ZERO
            }
        });
    }
    rig.block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    wait_until("最初のまとまり", Duration::from_secs(20), || {
        progress_len(&rig) >= 2
    });
    // 2 つ目のまとまりの取得が始まるのを待つ
    std::thread::sleep(Duration::from_millis(150));
    let ops = rig.outbox_len();

    // 動いている間は、もう 1 つ始められない。取得待ちの自動再取得は見送る
    let err = rig
        .block(rig.state.start_refetch(vec![ids[0].clone()], false))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "{err:?}");
    assert_eq!(rig.block(rig.state.run_pending_refetch()).unwrap(), None);

    let cancelled_at = Instant::now();
    rig.state.cancel_refetch();
    let last = wait_finished(&rig);
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(2),
        "すぐ止まる: {:?}",
        cancelled_at.elapsed()
    );
    assert_eq!(last.state, RefetchJobState::Cancelled);
    assert_eq!(last.stop_reason, Some(RefetchStopReason::Cancelled));
    assert_eq!((last.total, last.done, last.remaining), (25, 1, 24));
    assert_eq!(rig.progress.lock().unwrap().last(), Some(&last));

    // 最初の 1 冊は書いてあり、取得中だったまとまりは書いていない
    assert_eq!(rig.outbox_len(), ops);
    assert_eq!(title_of(&rig, &ids[0]), "こころ");
    for id in &ids[1..] {
        assert_eq!(rig.book(id).meta_status, "pending");
    }

    // 枠が空いて、続きを取り直せる
    rig.http.set_delay(|_| Duration::ZERO);
    rig.block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Finished);
    assert_eq!(last.done, 25);
    assert!(ids.iter().all(|id| rig.book(id).meta_status == "ok"));
}

#[test]
fn cancel_without_a_job_does_nothing() {
    let rig = Rig::new();
    rig.state.cancel_refetch();
    assert_eq!(job(&rig), None);
    assert!(rig.progress.lock().unwrap().is_empty());
}

#[test]
fn at47_registration_fetch_and_pending_refetch_do_not_wait_for_the_job() {
    let rig = Rig::new();
    // 同じ ISBN の本なので、まとまりごとに NDL へ 1 回（1 秒に 1 回）。9 まとまり（81 冊）で 8 秒以上かかる
    // （CI の遅い機械でも、登録の取得が終わるまで作業が動いているように、余裕を持たせる）
    let ids = pending_books(&rig, 81, KOKORO);
    rig.block(rig.state.start_refetch(ids, false)).unwrap();
    wait_until("作業の開始", Duration::from_secs(20), || {
        progress_len(&rig) >= 2
    });

    // 登録の取得は、作業が動いている間に終わる
    let started = Instant::now();
    let fetched = rig
        .block(rig.state.fetch_metadata(ONE_PIECE, None))
        .unwrap();
    assert!(!fetched.draft.fields.to_changes().is_empty());
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        job(&rig).unwrap().state,
        RefetchJobState::Running,
        "まだ作業の途中"
    );
    assert_eq!(rig.block(rig.state.run_pending_refetch()).unwrap(), None);

    let last = wait_finished(&rig);
    assert_eq!((last.state, last.done), (RefetchJobState::Finished, 81));
}

#[test]
fn a_job_waits_for_the_pending_refetch_that_holds_the_lock() {
    let rig = Rig::new();
    let ids = pending_books(&rig, 2, KOKORO);
    rig.block(async {
        let guard = rig.state.refetch_lock.lock().await;
        rig.state.start_refetch(ids.clone(), false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(job(&rig).unwrap().done, 0, "ロックを待つ");
        assert_eq!(job(&rig).unwrap().state, RefetchJobState::Running);
        drop(guard);
    });
    let last = wait_finished(&rig);
    assert_eq!((last.state, last.done), (RefetchJobState::Finished, 2));
}

#[test]
fn cancel_while_waiting_for_the_lock_ends_the_job() {
    let rig = Rig::new();
    let ids = pending_books(&rig, 2, KOKORO);
    rig.block(async {
        let _guard = rig.state.refetch_lock.lock().await;
        rig.state.start_refetch(ids.clone(), false).await.unwrap();
        rig.state.cancel_refetch();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Cancelled);
    assert_eq!((last.done, last.remaining), (0, 2));
}

// ---------------------------------------------------------------------------
// AT-48: 制限・オフラインで止まる。古い順に取る
// ---------------------------------------------------------------------------

fn ndl_refuses(rig: &Rig, isbn: &'static str, status: u16) {
    let base = online_handler();
    rig.http.set_handler(Box::new(move |url| {
        if url.contains("ndlsearch.ndl.go.jp/api/sru") && url.contains(&format!("isbn%3D{isbn}")) {
            return response(status, "text/plain", b"slow down".to_vec());
        }
        base(url)
    }));
}

/// 15 冊（0 番目と 1〜4・7〜14 番目が『こころ』、5・6 番目が『雪国』）。
fn mixed_books(rig: &Rig) -> Vec<String> {
    (0..15)
        .map(|i| {
            bare_book(
                rig,
                if i == 5 || i == 6 { YUKIGUNI } else { KOKORO },
                "pending",
            )
        })
        .collect()
}

#[test]
fn at48_a_refusal_stops_the_job_and_leaves_the_refused_books_unwritten() {
    let rig = Rig::new();
    let ids = mixed_books(&rig);
    ndl_refuses(&rig, YUKIGUNI, 429);

    rig.block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Stopped, "{last:?}");
    match &last.stop_reason {
        Some(RefetchStopReason::Limited {
            provider, status, ..
        }) => assert_eq!((provider.as_str(), *status), ("ndl", 429)),
        other => panic!("{other:?}"),
    }
    // 1 冊目と、2 つ目のまとまり（1〜10 番目）のうち制限を受けた 2 冊を除く 8 冊が済んだ
    assert_eq!(
        (last.total, last.done, last.updated),
        (15, 9, 9),
        "{last:?}"
    );
    assert_eq!(last.remaining, 6);
    assert_eq!(rig.progress.lock().unwrap().last(), Some(&last));
    for (i, id) in ids.iter().enumerate() {
        let expected = if i == 5 || i == 6 || i > 10 {
            "pending"
        } else {
            "ok"
        };
        assert_eq!(rig.book(id).meta_status, expected, "{i}");
    }

    // もう一度: 取れなかった本（キャッシュの行がない）から進む
    rig.http.set_online();
    rig.http.clear();
    rig.block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Finished, "{last:?}");
    assert_eq!((last.done, last.remaining), (15, 0));
    let first_ndl = rig
        .http
        .urls()
        .into_iter()
        .find(|u| u.contains("ndlsearch.ndl.go.jp/api/sru"))
        .unwrap();
    assert!(first_ndl.contains(YUKIGUNI), "{first_ndl}");
    assert!(ids.iter().all(|id| rig.book(id).meta_status == "ok"));
}

#[test]
fn at48_google_daily_quota_stops_the_job_with_its_message() {
    let rig = Rig::new();
    rig.block(rig.state.set_api_key(ProviderId::Google, "AIza-key"))
        .unwrap();
    let ids = pending_books(&rig, 3, KOKORO);
    let base = online_handler();
    rig.http.set_handler(Box::new(move |url| {
        if url.contains("volumes?q=isbn:") {
            return response(
                429,
                "application/json",
                fixture("google_quota_exceeded.json").into_bytes(),
            );
        }
        base(url)
    }));
    rig.block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Stopped);
    match &last.stop_reason {
        Some(RefetchStopReason::Limited {
            provider,
            status,
            message,
        }) => {
            assert_eq!((provider.as_str(), *status), ("google", 429));
            assert!(message.contains("per day"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    // Google の値が順位の低い取得元の値に置き換わらないよう、書かない
    assert_eq!((last.done, last.remaining), (0, 3));
    assert!(ids.iter().all(|id| rig.book(id).meta_status == "pending"));
}

#[test]
fn at48_going_offline_stops_the_job_without_writing_that_chunk() {
    let rig = Rig::new();
    let ids = pending_books(&rig, 25, KOKORO);
    // 最初のまとまりのあとは、通信できない
    {
        let progress = std::sync::Arc::clone(&rig.progress);
        let base = online_handler();
        rig.http.set_handler(Box::new(move |url| {
            if progress.lock().unwrap().len() >= 2 {
                Err(TransportError::Connect("offline".into()))
            } else {
                base(url)
            }
        }));
    }
    rig.block(rig.state.start_refetch(ids.clone(), false))
        .unwrap();
    let last = wait_finished(&rig);
    assert_eq!(last.state, RefetchJobState::Stopped, "{last:?}");
    assert_eq!(last.stop_reason, Some(RefetchStopReason::Offline));
    assert_eq!(
        (last.done, last.remaining, last.offline, last.updated),
        (1, 24, 0, 1),
        "通信できなかった本は未処理のまま"
    );
    assert_eq!(rig.book(&ids[0]).meta_status, "ok");
    for id in &ids[1..] {
        assert_eq!(rig.book(id).meta_status, "pending");
    }
    assert_eq!(rig.progress.lock().unwrap().last(), Some(&last));
}

#[test]
fn the_pending_auto_refetch_still_writes_what_it_got_on_a_refusal() {
    let rig = Rig::new();
    let kokoro = bare_book(&rig, KOKORO, "pending");
    let yukiguni = bare_book(&rig, YUKIGUNI, "pending");
    let before = title_of(&rig, &yukiguni);
    ndl_refuses(&rig, YUKIGUNI, 429);

    let summary = rig.block(rig.state.run_pending_refetch()).unwrap().unwrap();
    assert_eq!((summary.requested, summary.updated), (2, 2), "{summary:?}");
    assert_eq!(title_of(&rig, &kokoro), "こころ");
    // NDL が制限を返しても、ほかの取得元の値は書く
    assert_ne!(title_of(&rig, &yukiguni), before);
}
