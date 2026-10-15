//! 書誌の再取得（FR-META-13）と、取得待ちの本の自動再取得（FR-SCAN-16、AT-11）。
//!
//! 流れ（1 冊ごと）:
//! 1. 本の ISBN と、2段目由来（`field_sources` が `barcode`）の Cコード・本体価格を読む。
//! 2. その ISBN で取得する（2段目由来の値は最優先の値として統合に渡すので、取得元の値に置き換わらない）。
//! 3. 書影がまだなければ取得する。
//! 4. `update_book` を、取得元の由来（`Provider`）で呼ぶ。手で直した項目は、`override_manual` が偽なら上書きされない
//!    （コアが守る。FR-META-07、AT-16）。取得状態（`meta_status`）も更新する。
//!    取得元が値を返さなくなった項目の古い値は消す（`stale_clears`）: 今の値の由来がその取得元で、今回その取得元が
//!    見つかった（`found`）のに、統合の結果にその項目の値がないとき。手修正・バーコード由来の項目と、取得元が失敗した
//!    （見つからない・通信エラー・無効）項目は、そのまま残す。
//!
//! 全取得元が通信エラー（オフライン）のときは何も書かない。取得待ちの本は取得待ちのまま、次回に回す（AT-11）。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bibcontainer_core::library::{
    ChangeReport, Env, FieldChange, Origin, UpdateOptions, find_or_create_series_in,
    pending_book_ids, update_book_in,
};
use bibcontainer_core::rows::find_book_detail;
use bibcontainer_core::{SystemClock, write_tx};
use bibcontainer_metadata::{MetaFetchResult, MetaSecondTier, ProviderOutcome};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::draft::{
    BookDraft, DraftSeries, MetaStatus, build_draft, decide_status, status_after_refetch,
};
use super::state::{Active, MetadataState};
use crate::error::AppError;

/// 取得待ちの本を自動で再取得する間隔（0005 §9）。
pub const PENDING_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// 起動してから最初の試行までの待ち時間（起動処理が落ち着くのを待つ）。
const STARTUP_DELAY: Duration = Duration::from_secs(5);
/// `trigger` による試行の最短の間隔（ウィンドウが前面に来るたびに通信しないため）。
pub const MIN_TRIGGER_GAP: Duration = Duration::from_secs(30);
/// 一度に取得する本の数（NDL・Google・Open Library は 1 件ずつ 1 秒あけるので、進捗が保存される単位）。
const CHUNK: usize = 20;

/// 1 冊の再取得の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RefetchOutcome {
    /// 書き換えた項目がある
    Updated,
    /// 取得できたが、変わった項目はない
    Unchanged,
    /// 通信できず、何も変えなかった（取得待ちの本は取得待ちのまま）
    Offline,
    /// 対象にならなかった（ISBN がない、完全削除した、見つからない、有効な取得元がない）
    Skipped,
    /// 保存に失敗した
    Error,
}

/// 1 冊ぶんの結果。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefetchItem {
    pub id: String,
    pub outcome: RefetchOutcome,
    /// 再取得のあとの取得状態（書き換えていなければ null）
    pub meta_status: Option<MetaStatus>,
    /// 書き換えた項目名
    pub written: Vec<String>,
    /// 手で直した項目のために、書かなかった項目名（AT-16）
    pub skipped_manual: Vec<String>,
    /// 理由（`skipped`・`error`・`offline` のとき）
    pub message: Option<String>,
}

impl RefetchItem {
    fn new(id: &str, outcome: RefetchOutcome) -> Self {
        RefetchItem {
            id: id.to_owned(),
            outcome,
            meta_status: None,
            written: Vec::new(),
            skipped_manual: Vec::new(),
            message: None,
        }
    }

    fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// 再取得の結果の要約。
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefetchSummary {
    /// 渡された本の数（重複を除く）
    pub requested: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub offline: u32,
    pub skipped: u32,
    pub failed: u32,
    /// 新しく書影を保存した本の数
    pub covers_saved: u32,
    /// 使える取得元が 1 つもなかった（全部無効、または API キーがない）
    pub no_active_providers: bool,
    /// 本ごとの結果（渡した順）
    pub items: Vec<RefetchItem>,
}

impl RefetchSummary {
    fn push(&mut self, item: RefetchItem) {
        match item.outcome {
            RefetchOutcome::Updated => self.updated += 1,
            RefetchOutcome::Unchanged => self.unchanged += 1,
            RefetchOutcome::Offline => self.offline += 1,
            RefetchOutcome::Skipped => self.skipped += 1,
            RefetchOutcome::Error => self.failed += 1,
        }
        self.items.push(item);
    }
}

/// なぜ対象にならないか。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Eligibility {
    Yes,
    Missing,
    Purged,
    NoIsbn,
}

/// 再取得の前に読む、本の状態。
#[derive(Debug, Clone)]
struct Target {
    id: String,
    eligibility: Eligibility,
    isbn13: Option<String>,
    status: MetaStatus,
    has_cover: bool,
    manual: BTreeSet<String>,
    /// 項目名 → 今の値の由来（`openbd`・`ndl`・`barcode`・`manual` など）
    field_sources: BTreeMap<String, String>,
    /// 2段目（バーコード）由来の Cコード・本体価格
    second_tier: Option<MetaSecondTier>,
}

fn load_targets(conn: &Connection, ids: &[String]) -> Result<Vec<Target>, AppError> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(d) = find_book_detail(conn, id)? else {
            out.push(Target::ineligible(id, Eligibility::Missing));
            continue;
        };
        let eligibility = if d.purged {
            Eligibility::Purged
        } else if d.isbn13.as_deref().is_none_or(str::is_empty) {
            Eligibility::NoIsbn
        } else {
            Eligibility::Yes
        };
        let from_barcode =
            |field: &str| d.field_sources.get(field).map(String::as_str) == Some("barcode");
        let tier = MetaSecondTier {
            c_code: d.c_code.clone().filter(|_| from_barcode("c_code")),
            list_price: d.list_price.filter(|_| from_barcode("list_price")),
        };
        out.push(Target {
            id: id.clone(),
            eligibility,
            isbn13: d.isbn13.clone(),
            status: MetaStatus::from_db(&d.meta_status),
            has_cover: d.cover_hash.is_some(),
            manual: d.manual_fields.iter().cloned().collect(),
            field_sources: d.field_sources.clone(),
            second_tier: (tier.c_code.is_some() || tier.list_price.is_some()).then_some(tier),
        });
    }
    Ok(out)
}

impl Target {
    fn ineligible(id: &str, eligibility: Eligibility) -> Self {
        Target {
            id: id.to_owned(),
            eligibility,
            isbn13: None,
            status: MetaStatus::Ok,
            has_cover: true,
            manual: BTreeSet::new(),
            field_sources: BTreeMap::new(),
            second_tier: None,
        }
    }
}

/// 書き込む内容。
#[derive(Debug, Clone, PartialEq)]
struct Plan {
    changes: Vec<FieldChange>,
    series: Option<DraftSeries>,
    /// 再取得のあとの取得状態（今の状態と同じなら、書き込まない）
    status: MetaStatus,
}

/// 取得元が値を返さなくなっても消してよい項目（下書きの項目名）。書名（なくしてはいけない）・書影（再取得では、
/// 書影のない本だけ取る）・ISBN・取得状態は含めない。巻次を消すと、並べ替え用の数値（`volume_sort`）も、コアが一緒に消す。
const CLEARABLE_FIELDS: &[&str] = &[
    "title_kana",
    "subtitle",
    "authors",
    "publisher",
    "published_on",
    "c_code",
    "ndc",
    "list_price",
    "ref_price_incl",
    "pages",
    "size_text",
    "language",
    "description",
    "volume",
    "series_id",
];

/// 取得元が値を返さなくなった項目を消す変更を決める（0010 §9）。古い取得元の値（たとえば NDL が返していた
/// 巻次 `: pbk.`）を、解析を直したあとの再取得で残さないため。消すのは、次のすべてを満たす項目だけ:
///
/// - 今の値の由来（`field_sources`）が取得元（`openbd`・`ndl`・`google`・`openlibrary`）。手修正（`manual`）・バーコード・
///   取り込みの値は消さない。
/// - その取得元が、今回の取得で見つかった（`found`）。見つからない・通信エラー・解析エラー・無効（問い合わせていない）
///   のときは、その取得元が今も値を返すかわからないので残す。
/// - 統合の結果（`draft`）に、その項目の値がない（ほかの取得元が値を返したなら、その値を書く通常の更新になる）。
///
/// 変更は、値が null で、由来は消す項目の今の取得元。コアが `UpdateOptions::clear_stale` のときだけ、同じ取得元の
/// 値として消す。
fn stale_clears(
    field_sources: &BTreeMap<String, String>,
    result: &MetaFetchResult,
    draft: &BookDraft,
) -> Vec<FieldChange> {
    let found: BTreeSet<&str> = result
        .reports
        .iter()
        .filter(|r| r.outcome == ProviderOutcome::Found)
        .map(|r| r.provider.as_str())
        .collect();
    let mut present: BTreeSet<String> = draft
        .fields
        .to_changes()
        .into_iter()
        .map(|c| c.field)
        .collect();
    if draft.series_name.is_some() {
        present.insert("series_id".to_owned());
    }
    CLEARABLE_FIELDS
        .iter()
        .filter(|field| !present.contains(**field))
        .filter_map(|field| {
            let source = field_sources.get(*field)?;
            found
                .contains(source.as_str())
                .then(|| FieldChange::new(*field, Value::Null, Origin::Provider(source.clone())))
        })
        .collect()
}

/// 下書きから、再取得で書く変更を決める。
///
/// - ISBN は書かない（本の ISBN で取得したもの）。
/// - 手で直した項目をコアが守る（`override_manual` が偽のとき）。そのうえで、組になる項目は一緒に扱う:
///   巻次が手修正済みなら、巻次の数値（`volume_sort`）も書かない。シリーズ（`series_id`）が手修正済みなら、
///   シリーズも探さない（作らない）。
/// - 取得状態は、今の状態と今回の判断から決める（`status_after_refetch`）。
/// - `stale` は、取得元が値を返さなくなった項目を消す変更（`stale_clears`）。手修正の項目は、念のためここでも外す。
fn plan(
    draft: &BookDraft,
    current_status: MetaStatus,
    manual: &BTreeSet<String>,
    override_manual: bool,
    stale: Vec<FieldChange>,
) -> Plan {
    let protected = |field: &str| manual.contains(field) && !override_manual;
    let mut changes: Vec<FieldChange> = draft
        .fields
        .to_changes()
        .into_iter()
        .filter(|c| c.field != "isbn13" && c.field != "meta_status")
        .collect();
    if protected("volume") {
        changes.retain(|c| c.field != "volume_sort");
    }
    changes.extend(stale.into_iter().filter(|c| !manual.contains(&c.field)));
    let status = status_after_refetch(current_status, draft.fields.meta_status.value);
    if status != current_status {
        changes.push(FieldChange::new(
            "meta_status",
            status.as_str(),
            draft.fields.meta_status.origin.clone(),
        ));
    }
    Plan {
        changes,
        series: draft
            .series_name
            .clone()
            .filter(|_| !protected("series_id")),
        status,
    }
}

/// 1 冊を書き込む（本ごとに 1 つのトランザクション。失敗しても、ほかの本には響かない）。
fn apply_one(
    conn: &mut Connection,
    env: &Env<'_>,
    id: &str,
    plan: &Plan,
    opts: UpdateOptions,
) -> Result<(ChangeReport, bool), AppError> {
    let tx = write_tx(conn)?;
    let mut changes = plan.changes.clone();
    let mut touched_series = false;
    if let Some(series) = &plan.series
        && !series.name.trim().is_empty()
    {
        let series_id = find_or_create_series_in(&tx, env, &series.name)?;
        changes.push(FieldChange::new(
            "series_id",
            series_id,
            series.origin.clone(),
        ));
        touched_series = true;
    }
    let report = update_book_in(&tx, env, id, &changes, opts)?;
    tx.commit().map_err(bibcontainer_core::Error::from)?;
    Ok((report, touched_series))
}

/// 取得して、書き込む内容まで決めた 1 冊。
struct Prepared {
    id: String,
    plan: Plan,
    cover_saved: bool,
}

impl MetadataState {
    /// 選んだ本の書誌を再取得する（FR-META-13）。キャッシュは使わず、取り直す。
    ///
    /// `override_manual` が偽なら、手で直した項目は上書きしない（AT-16）。真なら、取得元の値で上書きして、
    /// その項目の手修正の記録を外す。2段目（バーコード）由来の Cコード・本体価格は、どちらの場合も保つ。
    pub async fn refetch_books(
        &self,
        ids: Vec<String>,
        override_manual: bool,
    ) -> Result<RefetchSummary, AppError> {
        let _guard = self.refetch_lock.lock().await;
        self.refetch_locked(ids, override_manual, true).await
    }

    /// 取得待ち（`pending`）の本を 1 回、再取得する。キャッシュは使う（該当なしは 1 日、通信エラーは保存されない）。
    /// すでに再取得が走っていれば、何もせず `None` を返す（同時に 2 つ走らせない）。
    pub async fn run_pending_refetch(&self) -> Result<Option<RefetchSummary>, AppError> {
        let Ok(_guard) = self.refetch_lock.try_lock() else {
            tracing::debug!("再取得が走っているので、取得待ちの再取得は見送ります");
            return Ok(None);
        };
        let ids = self.db.run(|conn, _| Ok(pending_book_ids(conn)?)).await?;
        if ids.is_empty() {
            return Ok(Some(RefetchSummary::default()));
        }
        let active = self.active().await?;
        if !active.has_network_provider() {
            tracing::debug!(
                pending = ids.len(),
                "使える取得元がないので、取得待ちの再取得は見送ります"
            );
            return Ok(None);
        }
        self.refetch_locked(ids, false, false).await.map(Some)
    }

    async fn refetch_locked(
        &self,
        ids: Vec<String>,
        override_manual: bool,
        force_refresh: bool,
    ) -> Result<RefetchSummary, AppError> {
        let mut seen = BTreeSet::new();
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        let mut summary = RefetchSummary {
            requested: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            ..RefetchSummary::default()
        };
        if ids.is_empty() {
            return Ok(summary);
        }
        let active = self.active().await?;
        if !active.has_network_provider() {
            summary.no_active_providers = true;
            for id in &ids {
                summary
                    .push(RefetchItem::new(id, RefetchOutcome::Skipped).with_message(
                        "使える書誌の取得元がありません（設定で有効にしてください）",
                    ));
            }
            return Ok(summary);
        }

        // 最初の 1 冊で通信できるかを確かめる。できなければ（オフライン）、残りは試さない
        // （NDL・Google・Open Library は 1 件ずつ、リトライしながら取るので、全冊を試すと長くかかる）。
        let mut rest: &[String] = &ids;
        let mut offline = false;
        while !rest.is_empty() {
            let size = if summary.items.is_empty() && rest.len() > 1 {
                1
            } else {
                CHUNK.min(rest.len())
            };
            let (chunk, tail) = rest.split_at(size);
            rest = tail;
            if offline {
                for id in chunk {
                    summary.push(
                        RefetchItem::new(id, RefetchOutcome::Offline)
                            .with_message("通信できないため、取得は見送りました"),
                    );
                }
                continue;
            }
            let attempted_before = summary.items.len();
            let (items, covers_saved) = self
                .refetch_chunk(&active, chunk, override_manual, force_refresh)
                .await?;
            summary.covers_saved += covers_saved;
            let attempted: Vec<&RefetchItem> = items
                .iter()
                .filter(|i| i.outcome != RefetchOutcome::Skipped)
                .collect();
            offline = !attempted.is_empty()
                && attempted
                    .iter()
                    .all(|i| i.outcome == RefetchOutcome::Offline);
            for item in items {
                summary.push(item);
            }
            tracing::debug!(
                done = summary.items.len() - attempted_before,
                "再取得を進めました"
            );
        }
        tracing::info!(
            requested = summary.requested,
            updated = summary.updated,
            unchanged = summary.unchanged,
            offline = summary.offline,
            skipped = summary.skipped,
            failed = summary.failed,
            "書誌を再取得しました"
        );
        Ok(summary)
    }

    /// 1 まとまりの本を再取得して、書き込む。結果の一覧（渡した順）と、新しく保存した書影の数を返す。
    async fn refetch_chunk(
        &self,
        active: &Arc<Active>,
        ids: &[String],
        override_manual: bool,
        force_refresh: bool,
    ) -> Result<(Vec<RefetchItem>, u32), AppError> {
        let owned: Vec<String> = ids.to_vec();
        let targets = self
            .db
            .run(move |conn, _| load_targets(conn, &owned))
            .await?;

        let eligible: Vec<&Target> = targets
            .iter()
            .filter(|t| t.eligibility == Eligibility::Yes)
            .collect();
        let requests: Vec<_> = eligible
            .iter()
            .map(|t| {
                active.request(
                    t.isbn13.as_deref().unwrap_or_default(),
                    t.second_tier.clone(),
                    force_refresh,
                )
            })
            .collect();
        let results = active.service.fetch_many(&requests).await;

        let mut fetched = results.into_iter();
        let mut prepared: Vec<Prepared> = Vec::new();
        let mut items: Vec<RefetchItem> = Vec::with_capacity(targets.len());
        for target in &targets {
            match target.eligibility {
                Eligibility::Yes => {}
                Eligibility::Missing => {
                    items.push(
                        RefetchItem::new(&target.id, RefetchOutcome::Skipped)
                            .with_message("本が見つかりません"),
                    );
                    continue;
                }
                Eligibility::Purged => {
                    items.push(
                        RefetchItem::new(&target.id, RefetchOutcome::Skipped)
                            .with_message("完全削除した本です"),
                    );
                    continue;
                }
                Eligibility::NoIsbn => {
                    items.push(
                        RefetchItem::new(&target.id, RefetchOutcome::Skipped)
                            .with_message("ISBN がないため、取得できません"),
                    );
                    continue;
                }
            }
            let Some(result) = fetched.next() else {
                return Err(AppError::Internal(
                    "取得の結果が依頼の数と合いません".to_owned(),
                ));
            };
            if result.offline {
                items.push(
                    RefetchItem::new(&target.id, RefetchOutcome::Offline)
                        .with_message("通信できませんでした。あとで取り直します"),
                );
                continue;
            }
            let cover = if target.has_cover {
                super::fetch::CoverOutcome::default()
            } else {
                self.fetch_cover(active, &result.merged).await
            };
            let decided = decide_status(&result, cover.offline);
            let draft = build_draft(&result.merged, None, cover.saved(), decided);
            let stale = stale_clears(&target.field_sources, &result, &draft);
            prepared.push(Prepared {
                id: target.id.clone(),
                plan: plan(
                    &draft,
                    target.status,
                    &target.manual,
                    override_manual,
                    stale,
                ),
                cover_saved: cover.hash.is_some(),
            });
        }

        let opts = UpdateOptions {
            override_manual,
            clear_stale: true,
        };
        let to_apply: Vec<(String, Plan)> = prepared
            .iter()
            .map(|p| (p.id.clone(), p.plan.clone()))
            .collect();
        let applied = self
            .db
            .run(move |conn, _| {
                let clock = SystemClock;
                let env = Env::new(&clock);
                Ok(to_apply
                    .into_iter()
                    .map(|(id, plan)| {
                        let result = apply_one(conn, &env, &id, &plan, opts);
                        (id, result)
                    })
                    .collect::<Vec<_>>())
            })
            .await?;

        let mut changed_books = Vec::new();
        let mut series_touched = false;
        let mut covers_saved = 0;
        let mut by_id: std::collections::HashMap<String, RefetchItem> =
            std::collections::HashMap::new();
        for (p, (id, result)) in prepared.iter().zip(applied) {
            let item = match result {
                Ok((report, touched)) => {
                    series_touched |= touched;
                    let updated = !report.written.is_empty();
                    if updated {
                        changed_books.push(id.clone());
                        if p.cover_saved && report.written.iter().any(|f| f == "cover_hash") {
                            covers_saved += 1;
                        }
                    }
                    RefetchItem {
                        id: id.clone(),
                        outcome: if updated {
                            RefetchOutcome::Updated
                        } else {
                            RefetchOutcome::Unchanged
                        },
                        meta_status: Some(p.plan.status),
                        written: report.written,
                        skipped_manual: report.skipped_manual,
                        message: None,
                    }
                }
                Err(err) => {
                    tracing::warn!(book = %id, error = %err, "再取得の結果を保存できませんでした");
                    RefetchItem::new(&id, RefetchOutcome::Error).with_message(err.to_string())
                }
            };
            by_id.insert(id, item);
        }
        // 渡した順にそろえる（対象にならなかった本・オフラインの本は上で入れてある）
        let mut ordered = Vec::with_capacity(ids.len());
        let mut early: std::collections::HashMap<String, RefetchItem> =
            items.into_iter().map(|i| (i.id.clone(), i)).collect();
        for id in ids {
            if let Some(item) = by_id.remove(id).or_else(|| early.remove(id)) {
                ordered.push(item);
            }
        }

        if !changed_books.is_empty() {
            (self.notifier)(changed_books, series_touched);
        }
        Ok((ordered, covers_saved))
    }

    /// 取得待ちの再取得を 1 回行い、結果をログに残す（失敗しても続ける）。
    async fn pending_pass(&self) {
        *self
            .last_pass
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
        match self.run_pending_refetch().await {
            Ok(Some(s)) if s.requested > 0 => tracing::info!(
                requested = s.requested,
                updated = s.updated,
                offline = s.offline,
                "取得待ちの本を再取得しました"
            ),
            Ok(_) => {}
            Err(err) => tracing::warn!(error = %err, "取得待ちの本の再取得に失敗しました"),
        }
    }

    /// 取得待ちの本を、今すぐ試す（ウィンドウが前面に来たときに呼ぶ）。
    ///
    /// 呼び出しはすぐ戻る（処理は裏で行う）。前回の試行から `MIN_TRIGGER_GAP` たっていなければ何もしない。
    /// すでに再取得が走っていれば、そちらに任せる。
    pub fn trigger(self: &Arc<Self>) {
        let recent = self
            .last_pass
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|t| t.elapsed() < MIN_TRIGGER_GAP);
        if recent {
            tracing::debug!("前回の試行から間もないので、取得待ちの再取得は見送ります");
            return;
        }
        let state = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            state.pending_pass().await;
        });
    }

    /// 取得待ちの本を `PENDING_INTERVAL`（5 分）ごとに再取得する処理を始める。起動時に 1 回だけ呼ぶ。
    /// 取得待ちの本がなければ、DB を 1 回引くだけで何もしない。
    pub fn start_background(self: &Arc<Self>) {
        let state = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(STARTUP_DELAY).await;
            loop {
                state.pending_pass().await;
                tokio::time::sleep(PENDING_INTERVAL).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use bibcontainer_core::library::Origin;
    use bibcontainer_metadata::providers::{Ndl, OpenBd};
    use bibcontainer_metadata::{
        MetaMerged, MetaPriorities, PriorityProfile, Provider, ProviderId, ProviderReport,
        RawResponse, merge,
    };

    use super::*;

    const ONE_PIECE: &str = "9784088725093";

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../crates/metadata/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn one_piece_draft() -> BookDraft {
        let openbd = OpenBd::new()
            .parse(&RawResponse {
                provider: ProviderId::Openbd,
                isbn13: ONE_PIECE.into(),
                body: fixture(&format!("openbd_{ONE_PIECE}.json")),
            })
            .unwrap()
            .unwrap();
        let ndl = Ndl::new()
            .parse(&RawResponse {
                provider: ProviderId::Ndl,
                isbn13: ONE_PIECE.into(),
                body: fixture(&format!("ndl_{ONE_PIECE}.xml")),
            })
            .unwrap()
            .unwrap();
        let merged = merge(ONE_PIECE, &[openbd, ndl], None, &MetaPriorities::default());
        build_draft(&merged, None, None, MetaStatus::Ok)
    }

    #[test]
    fn plan_never_writes_the_isbn_and_writes_the_status_only_when_it_changes() {
        let draft = one_piece_draft();
        let none = BTreeSet::new();
        let p = plan(&draft, MetaStatus::Ok, &none, false, Vec::new());
        assert!(p.changes.iter().all(|c| c.field != "isbn13"));
        assert!(
            p.changes.iter().all(|c| c.field != "meta_status"),
            "状態は変わらない"
        );
        assert!(
            p.changes
                .iter()
                .all(|c| matches!(c.origin, Origin::Provider(_)))
        );
        assert!(p.changes.iter().any(|c| c.field == "title"));
        assert_eq!(p.status, MetaStatus::Ok);

        let p = plan(&draft, MetaStatus::Pending, &none, false, Vec::new());
        let status = p.changes.iter().find(|c| c.field == "meta_status").unwrap();
        assert_eq!(status.value, serde_json::json!("ok"));
    }

    #[test]
    fn plan_keeps_volume_sort_and_series_with_their_manual_partners() {
        let draft = one_piece_draft();
        assert!(draft.series_name.is_some());
        assert!(draft.fields.volume_sort.is_some());

        let manual = |fields: &[&str]| {
            fields
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<BTreeSet<_>>()
        };
        // 巻次が手修正済み: 巻次の数値も書かない
        let p = plan(
            &draft,
            MetaStatus::Ok,
            &manual(&["volume"]),
            false,
            Vec::new(),
        );
        assert!(p.changes.iter().all(|c| c.field != "volume_sort"));
        assert!(p.series.is_some());
        // シリーズが手修正済み: シリーズを探さない
        let p = plan(
            &draft,
            MetaStatus::Ok,
            &manual(&["series_id"]),
            false,
            Vec::new(),
        );
        assert!(p.series.is_none());
        assert!(p.changes.iter().any(|c| c.field == "volume_sort"));
        // 上書きを選んだときは、どちらも書く
        let p = plan(
            &draft,
            MetaStatus::Ok,
            &manual(&["volume", "series_id"]),
            true,
            Vec::new(),
        );
        assert!(p.series.is_some());
        assert!(p.changes.iter().any(|c| c.field == "volume_sort"));
    }

    fn report(provider: ProviderId, outcome: ProviderOutcome) -> ProviderReport {
        ProviderReport {
            provider,
            outcome,
            from_cache: false,
            network: true,
        }
    }

    fn result_of(draft_source: &MetaMerged, reports: Vec<ProviderReport>) -> MetaFetchResult {
        MetaFetchResult {
            isbn13: ONE_PIECE.into(),
            merged: draft_source.clone(),
            profile: PriorityProfile::Japanese,
            candidates: Vec::new(),
            reports,
            offline: false,
        }
    }

    fn sources(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn stale_clears_only_values_of_found_providers_that_the_merge_no_longer_has() {
        // 統合の結果に、どの項目の値もない
        let merged = merge(ONE_PIECE, &[], None, &MetaPriorities::default());
        let draft = build_draft(&merged, None, None, MetaStatus::Ok);
        let result = result_of(
            &merged,
            vec![
                report(ProviderId::Ndl, ProviderOutcome::Found),
                report(ProviderId::Openbd, ProviderOutcome::NotFound),
                report(
                    ProviderId::Google,
                    ProviderOutcome::HttpError {
                        status: 500,
                        message: "x".into(),
                    },
                ),
            ],
        );
        let current = sources(&[
            ("volume", "ndl"),
            ("ndc", "ndl"),
            ("series_id", "ndl"),
            ("authors", "ndl"),
            // 取得元が見つからない・失敗した、問い合わせていない（openlibrary）
            ("publisher", "openbd"),
            ("pages", "google"),
            ("language", "openlibrary"),
            // 取得元の値でない
            ("c_code", "barcode"),
            ("list_price", "manual"),
            ("subtitle", "import"),
            // 消さない項目
            ("title", "ndl"),
            ("cover_hash", "ndl"),
            ("isbn13", "ndl"),
            ("meta_status", "ndl"),
            // 書籍の項目ではない（念のため）
            ("memo", "ndl"),
        ]);
        let stale = stale_clears(&current, &result, &draft);
        let mut cleared: Vec<(&str, &Origin)> = stale
            .iter()
            .map(|c| (c.field.as_str(), &c.origin))
            .collect();
        cleared.sort_by_key(|(f, _)| *f);
        let ndl = Origin::Provider("ndl".into());
        assert_eq!(
            cleared,
            vec![
                ("authors", &ndl),
                ("ndc", &ndl),
                ("series_id", &ndl),
                ("volume", &ndl)
            ]
        );
        assert!(stale.iter().all(|c| c.value == Value::Null));

        // 統合の結果に値があれば（巻次・シリーズ・NDC・著者がある ONE PIECE）、消さずに通常の更新で書く。
        // 値のない項目（内容紹介）だけが消える
        let draft = one_piece_draft();
        for field in ["volume", "ndc", "authors"] {
            assert!(
                draft.fields.to_changes().iter().any(|c| c.field == field),
                "{field}"
            );
        }
        assert!(draft.series_name.is_some());
        let current = sources(&[
            ("volume", "ndl"),
            ("ndc", "ndl"),
            ("series_id", "ndl"),
            ("authors", "ndl"),
            ("description", "ndl"),
        ]);
        let stale = stale_clears(&current, &result, &draft);
        let fields: Vec<&str> = stale.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(fields, vec!["description"]);

        // 取得元が 1 つも見つからなければ、何も消さない
        let result = result_of(&merged, Vec::new());
        assert!(stale_clears(&current, &result, &draft).is_empty());
    }

    #[test]
    fn plan_adds_the_stale_clears_except_for_manual_fields() {
        let draft = one_piece_draft();
        let clear =
            |field: &str| FieldChange::new(field, Value::Null, Origin::Provider("ndl".into()));
        let manual: BTreeSet<String> = ["pages".to_owned()].into();
        for override_manual in [false, true] {
            let p = plan(
                &draft,
                MetaStatus::Ok,
                &manual,
                override_manual,
                vec![clear("language"), clear("pages")],
            );
            assert!(p.changes.contains(&clear("language")));
            assert!(
                !p.changes.contains(&clear("pages")),
                "手修正の項目は、上書きを選んでも消さない"
            );
        }
    }

    #[test]
    fn a_book_with_data_keeps_its_status_when_nothing_was_found() {
        let mut draft = one_piece_draft();
        draft.fields.meta_status.value = MetaStatus::Failed;
        let p = plan(&draft, MetaStatus::Ok, &BTreeSet::new(), false, Vec::new());
        assert_eq!(p.status, MetaStatus::Ok);
        assert!(p.changes.iter().all(|c| c.field != "meta_status"));
        let p = plan(
            &draft,
            MetaStatus::Pending,
            &BTreeSet::new(),
            false,
            Vec::new(),
        );
        assert_eq!(p.status, MetaStatus::Failed);
    }

    #[test]
    fn summary_counts_follow_the_items() {
        let mut s = RefetchSummary::default();
        for (id, outcome) in [
            ("a", RefetchOutcome::Updated),
            ("b", RefetchOutcome::Unchanged),
            ("c", RefetchOutcome::Offline),
            ("d", RefetchOutcome::Skipped),
            ("e", RefetchOutcome::Error),
            ("f", RefetchOutcome::Updated),
        ] {
            s.push(RefetchItem::new(id, outcome));
        }
        assert_eq!(
            (s.updated, s.unchanged, s.offline, s.skipped, s.failed),
            (2, 1, 1, 1, 1)
        );
        assert_eq!(s.items.len(), 6);
    }
}
