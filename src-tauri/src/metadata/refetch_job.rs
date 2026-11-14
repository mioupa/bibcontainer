//! 一括再取得の作業（0027）。選んだ本を、裏で 10 冊ずつ取り直し、途中でやめられる。
//!
//! 流れ:
//! 1. `start_refetch` が作業の枠（同時に 1 つ）を取り、`running` の状態を知らせて、すぐ戻る。作業は裏の task で進む。
//! 2. 作業は `refetch_lock` を持って動く（取得待ちの自動再取得は `try_lock` で見送る。自動再取得が走っていれば、その終わりを待つ）。
//! 3. 本は、前回の取得が古い順に並べ替える（0027 §3。`sort_ids_by_cache_age`）。
//! 4. 10 冊ずつ取得（`fetch_stage`）→ 書き込み（`apply_stage`）→ `refetch-progress` を知らせる。最初の 1 冊だけは単独で取って、
//!    通信できるかを確かめる。
//! 5. 止まる条件: 取得元が 401・403・429 を返した（その本は書かず、ほかの本は書いてから止まる）／まとまりの取得がすべて通信エラー
//!    （そのまとまりは書かずに止まる）／やめた／想定外のエラー。
//!
//! やめる（`cancel_refetch`）: 取得中ならその取得を捨てて（書き込まない）すぐ止まる。書き込みの途中なら、そのまとまりを書き終えて止まる。
//! 状態は `MetadataState::job`（今の作業の取り消しの合図と、今の・最後の状態）にある。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, PoisonError};
use std::time::Instant;

use serde::Serialize;
use tokio::sync::watch;
use ts_rs::TS;

use super::cache::parse_time;
use super::refetch::{CHUNK, RefetchItem, RefetchOutcome};
use super::state::{Active, MetadataState};
use crate::error::AppError;

/// 再取得の作業の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RefetchJobState {
    Running,
    Finished,
    Cancelled,
    Stopped,
}

/// 再取得の作業が途中で止まった理由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RefetchStopReason {
    Cancelled,
    Offline,
    Limited {
        provider: String,
        status: u16,
        message: String,
    },
    NoActiveProviders,
    Error {
        message: String,
    },
}

/// 再取得の作業の進み具合（`refetch-progress` イベントと `get_refetch_job` の戻り値）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefetchJobStatus {
    // 1 から。起動ごとに数え直す
    pub job_id: u32,
    pub state: RefetchJobState,
    pub override_manual: bool,
    // 重複を除いた冊数
    pub total: u32,
    // 済んだ冊数（updated + unchanged + offline + skipped + failed）
    pub done: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub offline: u32,
    pub skipped: u32,
    pub failed: u32,
    pub covers_saved: u32,
    // 手で直したので書かなかった項目の数（AT-16）
    pub kept_manual_fields: u32,
    // total - done（止まったときは未処理の冊数）
    pub remaining: u32,
    // state が cancelled・stopped のとき
    pub stop_reason: Option<RefetchStopReason>,
    pub elapsed_ms: u32,
}

impl RefetchJobStatus {
    fn new(job_id: u32, override_manual: bool, total: u32) -> Self {
        RefetchJobStatus {
            job_id,
            state: RefetchJobState::Running,
            override_manual,
            total,
            done: 0,
            updated: 0,
            unchanged: 0,
            offline: 0,
            skipped: 0,
            failed: 0,
            covers_saved: 0,
            kept_manual_fields: 0,
            remaining: total,
            stop_reason: None,
            elapsed_ms: 0,
        }
    }

    /// まとまりの結果を数える。`count_offline` が偽なら、通信できなかった本は未処理のままにする（数えない）。
    fn add(&mut self, items: &[RefetchItem], covers_saved: u32, count_offline: bool) {
        for item in items {
            match item.outcome {
                RefetchOutcome::Updated => self.updated += 1,
                RefetchOutcome::Unchanged => self.unchanged += 1,
                RefetchOutcome::Offline if count_offline => self.offline += 1,
                RefetchOutcome::Offline => continue,
                RefetchOutcome::Skipped => self.skipped += 1,
                RefetchOutcome::Error => self.failed += 1,
            }
            self.kept_manual_fields += u32::try_from(item.skipped_manual.len()).unwrap_or(u32::MAX);
        }
        self.covers_saved += covers_saved;
        self.sync_counts();
    }

    fn sync_counts(&mut self) {
        self.done = self.updated + self.unchanged + self.offline + self.skipped + self.failed;
        self.remaining = self.total.saturating_sub(self.done);
    }
}

/// 今の作業と、最後の作業の状態。
#[derive(Default)]
pub(super) struct JobSlot {
    /// 作業の番号の数え方
    counter: u32,
    /// 動いている作業の取り消しの合図（動いていなければ `None`）
    cancel: Option<watch::Sender<bool>>,
    /// 今の作業、または最後に終わった作業の状態
    status: Option<RefetchJobStatus>,
}

/// 作業の終わり方。
enum End {
    Finished,
    Cancelled,
    Stopped(RefetchStopReason),
}

/// 取り消しの合図が来るまで待つ（合図の送り手がなくなったら、待ち続ける）。
async fn cancelled(rx: &mut watch::Receiver<bool>) {
    if rx.wait_for(|v| *v).await.is_err() {
        std::future::pending::<()>().await;
    }
}

impl MetadataState {
    /// 表示中の本などをまとめて再取得する作業を始める。すぐ戻り、作業は裏で進む。
    /// 動いている作業があれば `conflict`。使える取得元がなければ、すぐ `stopped`（`noActiveProviders`）。
    pub async fn start_refetch(
        self: &Arc<Self>,
        ids: Vec<String>,
        override_manual: bool,
    ) -> Result<RefetchJobStatus, AppError> {
        let mut seen = HashSet::new();
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        let total = u32::try_from(ids.len()).unwrap_or(u32::MAX);
        let (tx, rx) = watch::channel(false);
        let job_id = {
            let mut slot = self.job.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.cancel.is_some() {
                return Err(AppError::Conflict("再取得がすでに動いています".to_owned()));
            }
            slot.counter += 1;
            slot.cancel = Some(tx);
            slot.counter
        };
        let active = match self.active().await {
            Ok(active) => active,
            Err(err) => {
                self.job
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .cancel = None;
                return Err(err);
            }
        };
        let mut status = RefetchJobStatus::new(job_id, override_manual, total);
        if !active.has_network_provider() {
            status.state = RefetchJobState::Stopped;
            status.stop_reason = Some(RefetchStopReason::NoActiveProviders);
            self.publish(&status, true);
            return Ok(status);
        }
        self.publish(&status, false);

        let state = Arc::clone(self);
        let initial = status.clone();
        tauri::async_runtime::spawn(async move {
            let runner = Arc::clone(&state);
            let handle = tauri::async_runtime::spawn(async move {
                runner
                    .run_job(initial, ids, active, rx, override_manual)
                    .await;
            });
            if let Err(err) = handle.await {
                state.fail_job(&format!("再取得の作業が中断されました: {err}"));
            }
        });
        Ok(status)
    }

    /// 動いている再取得の作業をやめる。なければ何もしない。
    pub fn cancel_refetch(&self) {
        let slot = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(tx) = &slot.cancel {
            let _ = tx.send(true);
        }
    }

    /// 今の再取得の作業、または最後に終わった作業の状態。
    pub fn get_refetch_job(&self) -> Option<RefetchJobStatus> {
        self.job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .clone()
    }

    /// 状態を保存して、`refetch-progress` で知らせる。`finished` なら、作業の枠を空ける。
    fn publish(&self, status: &RefetchJobStatus, finished: bool) {
        {
            let mut slot = self.job.lock().unwrap_or_else(PoisonError::into_inner);
            slot.status = Some(status.clone());
            if finished {
                slot.cancel = None;
            }
        }
        (self.progress)(status);
    }

    /// 作業の task が落ちたとき、動いたままにしない（`stopped` と `error` で終える）。
    fn fail_job(&self, message: &str) {
        let status = {
            let slot = self.job.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.cancel.is_none() {
                return;
            }
            slot.status.clone()
        };
        let Some(mut status) = status else { return };
        status.state = RefetchJobState::Stopped;
        status.stop_reason = Some(RefetchStopReason::Error {
            message: message.to_owned(),
        });
        status.sync_counts();
        self.publish(&status, true);
    }

    async fn run_job(
        self: Arc<Self>,
        mut status: RefetchJobStatus,
        ids: Vec<String>,
        active: Arc<Active>,
        mut cancel: watch::Receiver<bool>,
        override_manual: bool,
    ) {
        let started = Instant::now();
        let end = match self
            .run_job_steps(
                &mut status,
                ids,
                &active,
                &mut cancel,
                override_manual,
                started,
            )
            .await
        {
            Ok(end) => end,
            Err(err) => {
                tracing::warn!(error = %err, "再取得の作業が失敗しました");
                End::Stopped(RefetchStopReason::Error {
                    message: err.to_string(),
                })
            }
        };
        match end {
            End::Finished => status.state = RefetchJobState::Finished,
            End::Cancelled => {
                status.state = RefetchJobState::Cancelled;
                status.stop_reason = Some(RefetchStopReason::Cancelled);
            }
            End::Stopped(reason) => {
                status.state = RefetchJobState::Stopped;
                status.stop_reason = Some(reason);
            }
        }
        status.elapsed_ms = elapsed_ms(started);
        status.sync_counts();
        tracing::info!(
            job = status.job_id,
            state = ?status.state,
            total = status.total,
            done = status.done,
            updated = status.updated,
            "書誌の再取得の作業が終わりました"
        );
        self.publish(&status, true);
    }

    async fn run_job_steps(
        &self,
        status: &mut RefetchJobStatus,
        ids: Vec<String>,
        active: &Arc<Active>,
        cancel: &mut watch::Receiver<bool>,
        override_manual: bool,
        started: Instant,
    ) -> Result<End, AppError> {
        // 取得待ちの自動再取得が走っていれば、その終わりを待つ
        let _guard = tokio::select! {
            biased;
            () = cancelled(cancel) => return Ok(End::Cancelled),
            guard = self.refetch_lock.lock() => guard,
        };
        let ordered = self.order_by_cache_age(&ids, active).await?;
        let mut rest: &[String] = &ordered;
        let mut first = true;
        while !rest.is_empty() {
            if *cancel.borrow() {
                return Ok(End::Cancelled);
            }
            // 最初の 1 冊だけを単独で取って、通信できるかを確かめる（0010 §9）
            let size = if first && rest.len() > 1 {
                1
            } else {
                CHUNK.min(rest.len())
            };
            first = false;
            let (chunk, tail) = rest.split_at(size);
            rest = tail;

            // 取得中にやめたら、その取得を捨てる（取得は読むだけなので、DB は変わらない）
            let fetched = tokio::select! {
                biased;
                () = cancelled(cancel) => return Ok(End::Cancelled),
                fetched = self.fetch_stage(active, chunk, override_manual, true) => fetched?,
            };
            let refusal = fetched.refusals.first().map(|(_, r)| r.clone());
            let exclude: HashSet<String> =
                fetched.refusals.iter().map(|(id, _)| id.clone()).collect();
            let all_offline =
                refusal.is_none() && fetched.attempted > 0 && fetched.offline == fetched.attempted;
            // 書き込みの途中でやめても、このまとまりは書き終える
            let (items, covers_saved) =
                self.apply_stage(fetched, override_manual, &exclude).await?;
            status.add(&items, covers_saved, !all_offline);
            status.elapsed_ms = elapsed_ms(started);

            if let Some(r) = refusal {
                return Ok(End::Stopped(RefetchStopReason::Limited {
                    provider: r.provider,
                    status: r.status,
                    message: r.message,
                }));
            }
            if all_offline {
                return Ok(End::Stopped(RefetchStopReason::Offline));
            }
            if *cancel.borrow() {
                return Ok(End::Cancelled);
            }
            if !rest.is_empty() {
                self.publish(status, false);
            }
        }
        Ok(End::Finished)
    }

    /// 本を、前回の取得が古い順に並べる（0027 §3）。
    async fn order_by_cache_age(
        &self,
        ids: &[String],
        active: &Arc<Active>,
    ) -> Result<Vec<String>, AppError> {
        let providers: Vec<&'static str> = active
            .active_providers
            .iter()
            .filter(|id| active.info(**id).is_some_and(|i| i.needs_network))
            .map(|id| id.as_str())
            .collect();
        let owned = ids.to_vec();
        let providers_owned = providers.clone();
        let rows = self
            .db
            .run(move |conn, _| load_age_rows(conn, &owned, &providers_owned))
            .await?;
        Ok(sort_ids_by_cache_age(ids, &rows, &providers))
    }
}

fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// 並べ替えのために読む、本とキャッシュの行（本 × 取得元）。キャッシュの行がなければ `provider` は `None`。
#[derive(Debug, Clone)]
pub(super) struct AgeRow {
    pub id: String,
    pub isbn13: Option<String>,
    pub purged: bool,
    pub provider: Option<String>,
    pub fetched_at: Option<String>,
}

/// 本と、使う取得元のキャッシュの行を読む（本 500 冊ごとに 1 回の問い合わせ）。
fn load_age_rows(
    conn: &rusqlite::Connection,
    ids: &[String],
    providers: &[&str],
) -> Result<Vec<AgeRow>, AppError> {
    let mut out = Vec::new();
    for chunk in ids.chunks(500) {
        let id_marks = vec!["?"; chunk.len()].join(",");
        let provider_marks = vec!["?"; providers.len().max(1)].join(",");
        let sql = format!(
            "SELECT b.id, b.isbn13, b.purged, p.provider, p.fetched_at
             FROM books b
             LEFT JOIN provider_cache p
               ON p.isbn13 = b.isbn13 AND p.provider IN ({provider_marks})
             WHERE b.id IN ({id_marks})"
        );
        let mut params: Vec<&str> = if providers.is_empty() {
            vec![""]
        } else {
            providers.to_vec()
        };
        params.extend(chunk.iter().map(String::as_str));
        let mut stmt = conn.prepare(&sql).map_err(bibcontainer_core::Error::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params), |r| {
                Ok(AgeRow {
                    id: r.get(0)?,
                    isbn13: r.get(1)?,
                    purged: r.get::<_, i64>(2)? != 0,
                    provider: r.get(3)?,
                    fetched_at: r.get(4)?,
                })
            })
            .map_err(bibcontainer_core::Error::from)?;
        for row in rows {
            out.push(row.map_err(bibcontainer_core::Error::from)?);
        }
    }
    Ok(out)
}

/// 本を、前回の取得が古い順に並べる。本ごとの値は、使う取得元のキャッシュの `fetched_at` のうちいちばん古いもの。
/// 行のない取得元があれば、取ったことがないとみなして先頭。ISBN のない本・見つからない本・完全削除した本も先頭
/// （すぐ終わる）。同じ値なら渡された順。
pub(super) fn sort_ids_by_cache_age(
    ids: &[String],
    rows: &[AgeRow],
    providers: &[&str],
) -> Vec<String> {
    let mut by_book: HashMap<&str, Vec<&AgeRow>> = HashMap::new();
    for row in rows {
        by_book.entry(row.id.as_str()).or_default().push(row);
    }
    // None（先頭に来る）か、いちばん古い取得日時（UNIX 秒）
    let key = |id: &String| -> Option<i64> {
        let rows = by_book.get(id.as_str())?;
        let first = rows.first()?;
        if first.purged || first.isbn13.as_deref().is_none_or(str::is_empty) {
            return None;
        }
        let mut oldest: Option<i64> = None;
        for provider in providers {
            let time = rows
                .iter()
                .filter(|r| r.provider.as_deref() == Some(*provider))
                .filter_map(|r| r.fetched_at.as_deref().and_then(parse_time))
                .max()?;
            oldest = Some(oldest.map_or(time, |o| o.min(time)));
        }
        oldest
    };
    let mut keyed: Vec<(Option<i64>, &String)> = ids.iter().map(|id| (key(id), id)).collect();
    keyed.sort_by_key(|(k, _)| *k);
    keyed.into_iter().map(|(_, id)| id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, isbn: Option<&str>, provider: Option<&str>, at: Option<&str>) -> AgeRow {
        AgeRow {
            id: id.to_owned(),
            isbn13: isbn.map(str::to_owned),
            purged: false,
            provider: provider.map(str::to_owned),
            fetched_at: at.map(str::to_owned),
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn books_are_ordered_by_the_oldest_cache_row_with_missing_rows_first() {
        let providers = ["ndl", "openbd"];
        let rows = vec![
            // a: ndl が新しく、openbd が古い → 古いほう（2026-01-01）
            row("a", Some("1"), Some("ndl"), Some("2026-03-01T00:00:00Z")),
            row("a", Some("1"), Some("openbd"), Some("2026-01-01T00:00:00Z")),
            // b: 両方 2026-02-01
            row("b", Some("2"), Some("ndl"), Some("2026-02-01T00:00:00Z")),
            row("b", Some("2"), Some("openbd"), Some("2026-02-01T00:00:00Z")),
            // c: openbd の行がない → 取ったことがない（先頭）
            row("c", Some("3"), Some("ndl"), Some("2025-01-01T00:00:00Z")),
            // d: 行が 1 つもない（先頭）
            row("d", Some("4"), None, None),
            // e: ISBN がない（先頭）
            row("e", None, None, None),
            // f: b と同じ値（渡された順）
            row("f", Some("6"), Some("ndl"), Some("2026-02-01T00:00:00Z")),
            row("f", Some("6"), Some("openbd"), Some("2026-02-01T00:00:00Z")),
            // g: 完全削除（先頭）
            AgeRow {
                purged: true,
                ..row("g", Some("7"), Some("ndl"), Some("2026-05-01T00:00:00Z"))
            },
        ];
        // h は DB にない（先頭）
        let order = sort_ids_by_cache_age(
            &ids(&["b", "a", "c", "d", "e", "f", "g", "h"]),
            &rows,
            &providers,
        );
        assert_eq!(order, ids(&["c", "d", "e", "g", "h", "a", "b", "f"]));
    }

    #[test]
    fn ties_keep_the_given_order() {
        let providers = ["ndl"];
        let rows = vec![
            row("x", Some("1"), Some("ndl"), Some("2026-02-01T00:00:00Z")),
            row("y", Some("2"), Some("ndl"), Some("2026-02-01T00:00:00Z")),
            row("z", Some("3"), Some("ndl"), Some("2026-02-01T00:00:00Z")),
        ];
        assert_eq!(
            sort_ids_by_cache_age(&ids(&["z", "x", "y"]), &rows, &providers),
            ids(&["z", "x", "y"])
        );
    }

    #[test]
    fn status_counts_follow_the_items() {
        let mut s = RefetchJobStatus::new(1, false, 10);
        let mut manual = RefetchItem::new("a", RefetchOutcome::Updated);
        manual.skipped_manual = vec!["title".into(), "pages".into()];
        let items = vec![
            manual,
            RefetchItem::new("b", RefetchOutcome::Unchanged),
            RefetchItem::new("c", RefetchOutcome::Offline),
            RefetchItem::new("d", RefetchOutcome::Skipped),
            RefetchItem::new("e", RefetchOutcome::Error),
        ];
        s.add(&items, 1, true);
        assert_eq!((s.done, s.remaining, s.kept_manual_fields), (5, 5, 2));
        assert_eq!(s.offline, 1);
        let mut s = RefetchJobStatus::new(1, false, 10);
        s.add(&items, 0, false);
        assert_eq!(
            (s.done, s.offline),
            (4, 0),
            "通信できなかった本は未処理のまま"
        );
    }
}
