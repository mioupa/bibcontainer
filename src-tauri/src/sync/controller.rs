//! 同期のコントローラ（managed state）。いつ動かすか（0013 §8）と、状態（`SyncStatus`）の持ち方、設定・解除・
//! 置き換えの流れを受け持つ。同期の中身（取り込み・書き出し・スナップショット）は `bibcontainer-sync` のエンジン。
//! 設計の経緯は `docs/decisions/0016-sync-integration.md`。
//!
//! ## スレッドとロック
//!
//! - エンジンの呼び出しは、すべて `spawn_blocking` の中で行う（同期フォルダの読み書きと DB の接続の借用はブロックする）。
//! - **同時に 2 つ動かさない**: 取り込み・書き出し・設定・解除・手入れ・書影の整理は、`op_lock`（`tokio::sync::Mutex`）を取って
//!   1 つずつ実行する。バックアップの復元も、これを取ってから DB を置き換える（`exclusive`）。
//! - DB の接続は、エンジンが短い処理のたびに `Database` の Mutex から借りる（`db::AppSyncDb`）。同期フォルダの
//!   読み書きの間は持たないので、同期が長引いても、画面の操作は待たされない。
//! - 状態（`live`）・最後に送った状態（`published`）・ファイル監視（`watcher`）は、短く持つ `std::sync::Mutex`。
//!   持ったまま `await` や DB・ファイルの操作をしない。
//! - 取り込み・書き出しの時機は、スケジューラの 1 つのタスク（`run_scheduler`）が、合図（`Signal`）と `Schedule`
//!   （純粋な判定。`schedule.rs`）から決める。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bibcontainer_core::{Clock, meta, sync_state};
use bibcontainer_sync::{
    DeviceInfo, FolderInspection, FolderKind, ImportReport, LocalCounters, LocalFolder, SetupMode,
    SetupReport, SyncEngine, SyncError, SyncStatus,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::db::AppSyncDb;
use super::schedule::{Due, Schedule, maintenance_due};
use super::settings::{SyncSettings, validate_device_name};
use super::status::{Live, build_status};
use super::watch::{self, FolderWatcher};
use crate::backups::configured;
use crate::database::{Database, ResetReport};
use crate::error::AppError;
use crate::settings;

/// 手入れ（`maintain`）を最後にやった時刻（壁時計のミリ秒）を覚えておく `local_meta` のキー。
const KEY_LAST_MAINTAIN: &str = "sync_last_maintain_ms";

/// 起動から、最初の同期までの待ち（起動直後の一覧の読み込みと、取り合わないようにする）。
const STARTUP_DELAY: Duration = Duration::from_secs(1);
/// 取り込み・書き出しがこれより長引いたら、状態を「同期中」にする（短い処理で、毎回ちらつかせないため）。
pub const SYNCING_AFTER: Duration = Duration::from_millis(500);
/// 状態の送信をまとめる時間（変更のたびに送らない）。
const PUBLISH_COALESCE: Duration = Duration::from_millis(100);
/// 終了時の書き出しを待つ最大の時間（FR-SYNC-07）。
pub const EXIT_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
/// 「今すぐ同期」・起動時・復元のあとの同期で、取り込みを繰り返す回数の上限（1 回 200 ファイル）。
const SYNC_ROUNDS: usize = 50;
/// 定期の取り込みで、続けて取り込む回数の上限（残りは、すぐ次の取り込みに回す）。
const IMPORT_ROUNDS: usize = 5;
/// 同期フォルダにいる端末の一覧を、変化がなくても読み直す間隔（ミリ秒）。
const DEVICES_REFRESH_MS: i64 = 5 * 60 * 1000;

/// スケジューラへの合図。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// 自分の変更があった（`events::emit_library_changed`）。少し待って、書き出す。
    LocalChange,
    /// 同期フォルダの中のファイルが変わった（ファイル監視）。
    FolderEvent,
    /// ウィンドウが前面に来た。
    Focus,
    /// 取り込みの続きがある・設定が済んだ。すぐ取り込む。
    ImportMore,
}

/// 状態を送る関数・蔵書が変わったことを知らせる関数（本番は `sync-status` と `library-changed` のイベント）。
pub struct Hooks {
    pub status: Box<dyn Fn(&SyncStatus) + Send + Sync>,
    /// 本の ID の一覧、本以外（シリーズなど）も変わったか。**書き出しの合図は送らない**
    /// （他の端末から取り込んだ変更を、また書き出さないため）。
    pub library_changed: Box<dyn Fn(Vec<String>, bool) + Send + Sync>,
}

/// 実行環境に依る設定。
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// 書影の本体のフォルダ（`app_local_data_dir/covers`）。
    pub covers_dir: PathBuf,
    /// 既定のバックアップの保存先（置き換えの前のバックアップに使う。設定に保存先があればそちら）。
    pub default_backup_dir: PathBuf,
    /// `std::env::consts::OS`。同期フォルダの端末の情報に書く。
    pub os: String,
    /// アプリの版（`x.y.z`）。`library.json` の最低の版との比較に使う。
    pub app_version: String,
    /// 端末名の既定値（ホスト名）。
    pub default_device_name: String,
}

pub type SharedClock = Arc<dyn Clock + Send + Sync>;

/// 同期フォルダと、それを使う端末の名前。
struct Ctx {
    root: PathBuf,
    name: String,
}

/// 1 回の周期（取り込み・書き出し・手入れ）で、何をするか。
#[derive(Debug, Clone, Copy, Default)]
struct CyclePlan {
    /// `engine.sync()`（スナップショットが要れば先に書く → 取り込む → 書き出す）。起動時・今すぐ同期・復元のあと。
    full: bool,
    import: bool,
    export: bool,
    /// 手入れの時期なら手入れをする。
    maintain: bool,
}

/// 周期の結果。
#[derive(Default)]
struct CycleOutcome {
    /// 同期が設定されていて、実際に動かしたか。
    ran: bool,
    import: Option<ImportReport>,
    /// 取り込み・書き出しの失敗（状態にも記録済み）。
    error: Option<SyncError>,
}

/// 設定（`setup`）の結果。
struct SetupAttempt {
    /// 「置き換えて参加」で、DB を作り直したときの情報。作り直したあとに失敗しても、入る（画面は取り直す）。
    replaced: Option<ResetReport>,
    result: Result<SetupReport, AppError>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// 利用者が選んだフォルダから、実際に使う `bibcontainer-library` のフォルダを決める。絶対パスだけ受け付ける。
pub fn resolve_folder(selected: &str) -> Result<PathBuf, AppError> {
    let selected = selected.trim();
    if selected.is_empty() || !Path::new(selected).is_absolute() {
        return Err(AppError::Validation(
            "入力が不正です（path）: フォルダは絶対パスで指定してください".to_owned(),
        ));
    }
    Ok(LocalFolder::resolve_root(Path::new(selected)))
}

pub struct SyncController {
    db: Arc<Database>,
    config: SyncConfig,
    clock: SharedClock,
    hooks: Hooks,
    /// 取り込み・書き出し・設定・解除・手入れ・書影の整理を、1 つずつ実行するためのロック。
    op_lock: Arc<AsyncMutex<()>>,
    live: Mutex<Live>,
    /// 最後に送った状態（変わったときだけ送る）。
    published: Mutex<Option<SyncStatus>>,
    /// 状態の送信が、もう予約されているか。
    publish_queued: AtomicBool,
    signals: UnboundedSender<Signal>,
    signal_rx: Mutex<Option<UnboundedReceiver<Signal>>>,
    watcher: Mutex<Option<FolderWatcher>>,
    /// 同期が設定されているか（ライブラリ ID と同期フォルダの両方がある）。合図の振り分けと、終了時の判断に使う。
    configured: AtomicBool,
    exiting: AtomicBool,
    /// いま動かしている処理の番号（0 は動かしていない）。長引いたときに「同期中」にするため。
    running: AtomicU64,
    op_seq: AtomicU64,
}

impl SyncController {
    pub fn new(
        db: Arc<Database>,
        config: SyncConfig,
        clock: SharedClock,
        hooks: Hooks,
    ) -> Arc<Self> {
        let (signals, rx) = unbounded_channel();
        Arc::new(Self {
            db,
            config,
            clock,
            hooks,
            op_lock: Arc::new(AsyncMutex::new(())),
            live: Mutex::new(Live::default()),
            published: Mutex::new(None),
            publish_queued: AtomicBool::new(false),
            signals,
            signal_rx: Mutex::new(Some(rx)),
            watcher: Mutex::new(None),
            configured: AtomicBool::new(false),
            exiting: AtomicBool::new(false),
            running: AtomicU64::new(0),
            op_seq: AtomicU64::new(0),
        })
    }

    /// スケジューラを始める（起動の少し後に最初の同期をして、そのあとは合図と定期の確認で動く）。1 回だけ。
    pub fn start(self: &Arc<Self>) {
        let Some(rx) = lock(&self.signal_rx).take() else {
            return;
        };
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move { this.run_scheduler(rx).await });
    }

    /// スケジューラに合図を送る（どのスレッドからでも。すぐ戻る）。
    pub fn signal(&self, signal: Signal) {
        // スケジューラを始めていない（テスト）・止まっているときは、捨てる
        let _ = self.signals.send(signal);
    }

    /// 同期が設定されているか（直近の確認の結果。DB は読まない）。
    pub fn is_configured(&self) -> bool {
        self.configured.load(Ordering::SeqCst)
    }

    /// 取り込み・書き出しなどが動いていない間、ほかの処理を止める（バックアップの復元が DB を置き換える間）。
    pub async fn exclusive(&self) -> OwnedMutexGuard<()> {
        Arc::clone(&self.op_lock).lock_owned().await
    }

    // -----------------------------------------------------------------------
    // 小さな部品（ブロックする。`spawn_blocking` の中、または DB をすぐ借りられる場面で呼ぶ）
    // -----------------------------------------------------------------------

    /// `f` を、別のスレッドで実行する。
    async fn blocking<T, F>(self: &Arc<Self>, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&Self) -> T + Send + 'static,
    {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || f(&this))
            .await
            .map_err(|err| AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}")))
    }

    fn with_live<R>(&self, f: impl FnOnce(&mut Live) -> R) -> R {
        f(&mut lock(&self.live))
    }

    fn load_settings(&self) -> Result<SyncSettings, AppError> {
        self.db.run_blocking(|conn, _| settings::load(conn))
    }

    fn save_settings(&self, value: SyncSettings) -> Result<SyncSettings, AppError> {
        self.db
            .run_blocking(move |conn, _| settings::save(conn, value))
    }

    fn name_of(&self, settings: &SyncSettings) -> String {
        settings.effective_device_name(&self.config.default_device_name)
    }

    fn library_id(&self) -> Result<Option<String>, AppError> {
        self.db
            .run_blocking(|conn, _| Ok(sync_state::library_id(conn)?))
    }

    /// 同期が設定されているとき（ライブラリ ID と同期フォルダの両方がある）、その同期フォルダと端末名。
    fn ctx(&self) -> Result<Option<Ctx>, AppError> {
        let settings = self.load_settings()?;
        let name = self.name_of(&settings);
        Ok(match (settings.folder, self.library_id()?) {
            (Some(folder), Some(_)) => Some(Ctx {
                root: PathBuf::from(folder),
                name,
            }),
            _ => None,
        })
    }

    /// エンジンを作って `f` を呼ぶ。エンジンは借用だけを持つので、呼び出しのたびに作ってよい。
    fn with_engine<T>(
        &self,
        root: &Path,
        name: &str,
        f: impl FnOnce(&SyncEngine<'_, AppSyncDb, LocalFolder>) -> T,
    ) -> T {
        let db = AppSyncDb::new(Arc::clone(&self.db));
        let transport = LocalFolder::new(root);
        let engine = SyncEngine::new(
            &db,
            &transport,
            &*self.clock,
            self.config.covers_dir.clone(),
            DeviceInfo {
                name: name.to_owned(),
                os: self.config.os.clone(),
                app_version: self.config.app_version.clone(),
            },
        );
        f(&engine)
    }

    fn backup_dir(&self) -> Result<PathBuf, AppError> {
        let default_dir = self.config.default_backup_dir.clone();
        self.db
            .run_blocking(move |conn, _| Ok(configured(conn, &default_dir)?.0))
    }

    fn note_import_failure(&self, err: &SyncError) {
        if self.with_live(|live| live.import_failed(err)) {
            tracing::warn!(error = %err, "同期フォルダの取り込みに失敗しました");
        }
    }

    fn note_export_failure(&self, err: &SyncError) {
        if self.with_live(|live| live.export_failed(err)) {
            tracing::warn!(error = %err, "同期フォルダへの書き出しに失敗しました");
        }
    }

    /// 同期フォルダにいる端末の一覧を読み直す（変化があったとき・覚えている一覧が古いとき・空のとき）。
    fn refresh_devices(&self, engine: &SyncEngine<'_, AppSyncDb, LocalFolder>, force: bool) {
        let now = self.clock.now_ms();
        let stale = self.with_live(|live| {
            live.devices.is_empty()
                || live
                    .devices_at_ms
                    .is_none_or(|at| now < at || now - at >= DEVICES_REFRESH_MS)
        });
        if !(force || stale) {
            return;
        }
        match engine.devices() {
            Ok(devices) => self.with_live(|live| live.set_devices(devices, now)),
            Err(err) => tracing::warn!(error = %err, "同期フォルダの端末の一覧を読めませんでした"),
        }
    }

    /// 手入れの時期なら、手入れをする（古い一時ファイルの掃除、自分のセグメントの統合）。
    fn maintain_if_due(&self, engine: &SyncEngine<'_, AppSyncDb, LocalFolder>) {
        let now = self.clock.now_ms();
        let last = self
            .db
            .run_blocking(|conn, _| Ok(meta::get(conn, KEY_LAST_MAINTAIN)?))
            .ok()
            .flatten()
            .and_then(|s| s.parse::<i64>().ok());
        if !maintenance_due(last, now) {
            return;
        }
        match engine.maintain() {
            Ok(report) => {
                tracing::info!(
                    tmp_removed = report.tmp_removed,
                    consolidated = report.consolidation.files_written,
                    "同期フォルダの手入れをしました"
                );
            }
            Err(err) => tracing::warn!(error = %err, "同期フォルダの手入れに失敗しました"),
        }
        // 失敗しても、次の定期の確認のたびにやり直さない（1 日後にやり直す）
        let stamp = now.to_string();
        if let Err(err) = self
            .db
            .run_blocking(move |conn, _| Ok(meta::set(conn, KEY_LAST_MAINTAIN, &stamp)?))
        {
            tracing::warn!(error = %err, "手入れの時刻を記録できませんでした");
        }
    }

    // -----------------------------------------------------------------------
    // 状態
    // -----------------------------------------------------------------------

    /// いまの状態を組み立てる（手元の DB と、覚えている直近の結果から。同期フォルダには触れない）。
    fn build_status(&self) -> Result<SyncStatus, AppError> {
        let settings = self.load_settings()?;
        let name = self.name_of(&settings);
        let root = settings.folder.clone().unwrap_or_default();
        let counters: LocalCounters = self
            .with_engine(Path::new(&root), &name, |engine| engine.local_counters())
            .map_err(AppError::from)?;
        let live = self.with_live(|live| live.clone());
        Ok(build_status(&settings, &name, &counters, &live))
    }

    /// 変わっていれば `sync-status` を送る。
    fn emit_if_changed(&self, status: &SyncStatus) {
        let mut last = lock(&self.published);
        if last.as_ref() != Some(status) {
            *last = Some(status.clone());
            (self.hooks.status)(status);
        }
    }

    /// 状態を組み立てて、変わっていれば送る。
    async fn publish(self: &Arc<Self>) -> Result<SyncStatus, AppError> {
        let status = self.blocking(Self::build_status).await??;
        self.emit_if_changed(&status);
        Ok(status)
    }

    /// 状態の送信を予約する（少しまとめる。変更のたびに DB を読まないため）。失敗してもログに出すだけ。
    fn request_publish(self: &Arc<Self>) {
        if self.publish_queued.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(PUBLISH_COALESCE).await;
            // 送っている間に起きた変更で、もう一度予約できるよう、先に下ろす
            this.publish_queued.store(false, Ordering::SeqCst);
            if let Err(err) = this.publish().await {
                tracing::debug!(error = %err, "同期の状態を送れませんでした");
            }
        });
    }

    /// 設定されているかを確かめ直して覚える。DB を開けないときは、設定されていないものとする。
    async fn refresh_configured(self: &Arc<Self>) -> bool {
        let configured = self
            .blocking(|t| t.ctx())
            .await
            .and_then(|r| r)
            .map(|ctx| ctx.is_some())
            .unwrap_or_else(|err| {
                tracing::debug!(error = %err, "同期の設定を読めませんでした");
                false
            });
        self.configured.store(configured, Ordering::SeqCst);
        configured
    }

    /// 取得できる状態（コマンド `get_sync_status`）。
    pub async fn status(self: &Arc<Self>) -> Result<SyncStatus, AppError> {
        self.publish().await
    }

    // -----------------------------------------------------------------------
    // 取り込み・書き出し・手入れ（周期）
    // -----------------------------------------------------------------------

    /// 処理の開始を記録する。`immediate` でなければ、`SYNCING_AFTER` より長引いたときだけ「同期中」にする。
    fn begin_op(self: &Arc<Self>, immediate: bool) -> u64 {
        let id = self.op_seq.fetch_add(1, Ordering::SeqCst) + 1;
        self.running.store(id, Ordering::SeqCst);
        if immediate {
            self.with_live(|live| live.syncing = true);
            self.request_publish();
        } else {
            let this = Arc::clone(self);
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(SYNCING_AFTER).await;
                if this.running.load(Ordering::SeqCst) == id {
                    this.with_live(|live| live.syncing = true);
                    this.request_publish();
                }
            });
        }
        id
    }

    fn end_op(&self, id: u64) {
        let _ = self
            .running
            .compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst);
        self.with_live(|live| live.syncing = false);
    }

    /// 1 回の周期（ブロックする）。
    fn cycle_blocking(&self, plan: CyclePlan) -> Result<CycleOutcome, AppError> {
        let Some(ctx) = self.ctx()? else {
            return Ok(CycleOutcome::default());
        };
        Ok(self.with_engine(&ctx.root, &ctx.name, |engine| {
            let mut out = CycleOutcome {
                ran: true,
                ..CycleOutcome::default()
            };
            let mut wrote = false;
            if plan.full {
                match engine.sync(SYNC_ROUNDS) {
                    Ok(report) => {
                        wrote = report.export.exported_at_ms.is_some();
                        self.with_live(|live| {
                            live.apply_import(&report.import);
                            live.apply_export(&report.export);
                        });
                        out.import = Some(report.import);
                    }
                    Err(err) => {
                        self.note_import_failure(&err);
                        out.error = Some(err);
                    }
                }
            } else {
                if plan.import {
                    match engine.import_all(IMPORT_ROUNDS) {
                        Ok(report) => {
                            self.with_live(|live| live.apply_import(&report));
                            out.import = Some(report);
                        }
                        Err(err) => {
                            self.note_import_failure(&err);
                            out.error = Some(err);
                        }
                    }
                }
                // 取り込みが止まる失敗（不一致・フォルダが使えないなど）のときは、書き出しも止まる
                if plan.export && out.error.is_none() {
                    match engine.export() {
                        Ok(report) => {
                            wrote = report.exported_at_ms.is_some();
                            self.with_live(|live| live.apply_export(&report));
                        }
                        Err(err) => {
                            self.note_export_failure(&err);
                            out.error = Some(err);
                        }
                    }
                }
            }
            if out.error.is_none() {
                if plan.maintain {
                    self.maintain_if_due(engine);
                }
                let imported = out
                    .import
                    .as_ref()
                    .is_some_and(|r| r.files_applied > 0 || r.snapshots_applied > 0);
                self.refresh_devices(engine, imported || wrote);
            }
            if let Some(report) = &out.import
                && (report.files_applied > 0 || report.snapshots_applied > 0)
            {
                tracing::info!(
                    files = report.files_applied,
                    snapshots = report.snapshots_applied,
                    books = report.changed_book_ids.len(),
                    covers = report.covers_fetched,
                    "他の端末の変更を取り込みました"
                );
            }
            out
        }))
    }

    /// 周期を実行して、結果（本の変更の通知、取り込みの続き、状態）を反映する。
    async fn cycle(
        self: &Arc<Self>,
        plan: CyclePlan,
        immediate: bool,
    ) -> Result<CycleOutcome, AppError> {
        let guard = self.op_lock.lock().await;
        let op = self.begin_op(immediate);
        let result = self.blocking(move |t| t.cycle_blocking(plan)).await;
        self.end_op(op);
        drop(guard);
        let outcome = result??;
        if let Some(import) = &outcome.import {
            if import.changed_anything() {
                (self.hooks.library_changed)(import.changed_book_ids.clone(), import.other_changed);
            }
            if import.remaining_files > 0 {
                self.signal(Signal::ImportMore);
            }
        }
        if let Err(err) = self.publish().await {
            tracing::debug!(error = %err, "同期の状態を送れませんでした");
        }
        Ok(outcome)
    }

    /// 取り込み・書き出しを、`engine.sync()` の順で今すぐ行う（コマンド `sync_now`）。
    pub async fn sync_now(self: &Arc<Self>) -> Result<SyncStatus, AppError> {
        let plan = CyclePlan {
            full: true,
            maintain: true,
            ..CyclePlan::default()
        };
        let outcome = self.cycle(plan, true).await?;
        if !outcome.ran {
            return Err(SyncError::NotConfigured.into());
        }
        if let Some(err) = outcome.error {
            return Err(err.into());
        }
        self.publish().await
    }

    /// スケジューラが決めた周期を動かす。失敗は状態に記録済みで、ここでは返さない。
    pub async fn run_due(self: &Arc<Self>, due: Due) {
        if !self.is_configured() {
            return;
        }
        let plan = CyclePlan {
            import: due.import,
            export: due.export,
            maintain: due.tick,
            full: false,
        };
        if let Err(err) = self.cycle(plan, false).await {
            tracing::warn!(error = %err, "同期の周期を実行できませんでした");
        }
    }

    /// 起動時の同期（`engine.sync()` と手入れ）。
    async fn startup_cycle(self: &Arc<Self>) {
        let plan = CyclePlan {
            full: true,
            maintain: true,
            ..CyclePlan::default()
        };
        if let Err(err) = self.cycle(plan, false).await {
            tracing::warn!(error = %err, "起動時の同期を実行できませんでした");
        }
    }

    /// バックアップから復元した直後に呼ぶ。復元した DB は新しい端末 ID と「スナップショットが必要」の印を持つので、
    /// 先にスナップショットを書き、古い端末 ID の変更と、他の端末の変更を取り込み直す（0014 §11）。
    /// 復元した DB の設定（同期フォルダ）が変わっていることがあるので、設定の確認と監視の張り直しもする。
    pub async fn after_restore(self: &Arc<Self>) {
        self.refresh_configured().await;
        self.restart_watcher().await;
        self.request_publish();
        if self.is_configured() {
            self.startup_cycle().await;
        }
    }

    // -----------------------------------------------------------------------
    // 終了時
    // -----------------------------------------------------------------------

    /// 終了の最初の要求なら true（2 回目以降は、書き出しを終えたあとの終了なので止めない）。
    pub fn begin_exit(&self) -> bool {
        !self.exiting.swap(true, Ordering::SeqCst)
    }

    /// 終了時に、未書き出しの変更を書き出す。`timeout` までに終わらなければ、あきらめる（書き出しは、
    /// 途中で止まっても次の起動でやり直せる）。
    pub async fn flush_on_exit(self: &Arc<Self>, timeout: Duration) {
        if !self.is_configured() {
            return;
        }
        let work = async {
            let _guard = self.op_lock.lock().await;
            let plan = CyclePlan {
                export: true,
                ..CyclePlan::default()
            };
            if let Err(err) = self.blocking(move |t| t.cycle_blocking(plan)).await? {
                tracing::warn!(error = %err, "終了時の書き出しに失敗しました");
            }
            Ok::<(), AppError>(())
        };
        match tokio::time::timeout(timeout, work).await {
            Ok(Ok(())) => tracing::debug!("終了時の書き出しを終えました"),
            Ok(Err(err)) => tracing::warn!(error = %err, "終了時の書き出しを実行できませんでした"),
            Err(_) => {
                tracing::warn!("終了時の書き出しが間に合いませんでした。次の起動でやり直します")
            }
        }
    }

    // -----------------------------------------------------------------------
    // 設定・解除・置き換え・再接続
    // -----------------------------------------------------------------------

    /// 選んだフォルダの中身を調べる（コマンド `inspect_sync_folder`）。何も残さない。
    pub async fn inspect(self: &Arc<Self>, path: String) -> Result<FolderInspection, AppError> {
        let root = resolve_folder(&path)?;
        self.blocking(move |t| {
            let settings = t.load_settings()?;
            let name = t.name_of(&settings);
            let root_str = path_string(&root);
            t.with_engine(&root, &name, |engine| engine.inspect(&root_str))
                .map_err(AppError::from)
        })
        .await?
    }

    /// 同期フォルダを設定する（コマンド `setup_sync`）。
    ///
    /// - 同じライブラリを選び直した（フォルダの ID が、手元のライブラリ ID と同じ）ときは、統合も置き換えもせず、
    ///   フォルダの場所だけを付け替える（取り込み済みの範囲は引き継ぎ、スナップショットは書かない）。
    /// - `JoinReplace`: 今の DB を手動バックアップしてから、新しい空の DB（新しい端末 ID）に作り直し、`Join` する。
    ///   端末ごとの設定は引き継ぐ。
    pub async fn setup(
        self: &Arc<Self>,
        path: String,
        mode: SetupMode,
    ) -> Result<SyncStatus, AppError> {
        let root = resolve_folder(&path)?;
        let guard = self.op_lock.lock().await;
        let result = self.blocking(move |t| t.setup_blocking(&root, mode)).await;
        drop(guard);
        // 何も変えずに断ったときは、ここで返す
        let attempt = result??;
        // 失敗しても、ライブラリへの参加は記録されていることがある（次の同期が続きから行う）。確かめ直す。
        self.refresh_configured().await;
        self.restart_watcher().await;
        if attempt.replaced.is_some() {
            // DB が丸ごと入れ替わった（作り直したあとで失敗しても）。画面は全部取り直す
            (self.hooks.library_changed)(Vec::new(), true);
        }
        match attempt.result {
            Ok(report) => {
                if attempt.replaced.is_none()
                    && let Some(import) = &report.import
                    && import.changed_anything()
                {
                    (self.hooks.library_changed)(
                        import.changed_book_ids.clone(),
                        import.other_changed,
                    );
                }
                // 設定の直後の取り込み・書き出しの続きと、最終の取り込みの時刻を整える
                self.signal(Signal::ImportMore);
                self.publish().await
            }
            Err(err) => {
                self.request_publish();
                Err(err)
            }
        }
    }

    /// 設定の本体（ブロックする）。外側の `Err` は、何も変えずに断ったとき。
    fn setup_blocking(&self, root: &Path, mode: SetupMode) -> Result<SetupAttempt, AppError> {
        let settings = self.load_settings()?;
        let name = self.name_of(&settings);
        let root_str = path_string(root);
        let before = self.library_id()?;
        let mut engine_mode = mode;
        let mut replaced = None;

        if mode == SetupMode::JoinReplace {
            let inspection = self
                .with_engine(root, &name, |engine| engine.inspect(&root_str))
                .map_err(AppError::from)?;
            let same = inspection.library_id.is_some()
                && inspection.library_id == inspection.local_library_id;
            if same {
                // 同じライブラリの選び直し。DB は作り直さず、場所だけ付け替える。
                engine_mode = SetupMode::Join;
            } else {
                // DB を作り直す前に、参加できるフォルダか確かめる（作り直したあとで断ると、手元が空になる）
                match inspection.kind {
                    FolderKind::Library => {}
                    FolderKind::Empty => return Err(SyncError::LibraryMissing.into()),
                    FolderKind::OtherLibrary => {
                        return Err(SyncError::AlreadyInLibrary {
                            local: inspection.local_library_id.unwrap_or_default(),
                        }
                        .into());
                    }
                    FolderKind::NotWritable => {
                        return Err(AppError::Conflict(
                            "このフォルダには書き込めません。書き込める場所のフォルダを選んでください"
                                .to_owned(),
                        ));
                    }
                    FolderKind::UpdateRequired => {
                        return Err(AppError::Conflict(
                            "このライブラリを使うには、新しい版のアプリが必要です。アプリを更新してください"
                                .to_owned(),
                        ));
                    }
                }
                let backup_dir = self.backup_dir()?;
                replaced = Some(self.db.reset_keeping_settings(&backup_dir)?);
                engine_mode = SetupMode::Join;
            }
        }

        let result = self.with_engine(root, &name, |engine| engine.setup(engine_mode));
        let after = self.library_id()?;
        let result = match result {
            Ok(report) => {
                let mut settings = self.load_settings()?;
                settings.folder = Some(root_str);
                self.save_settings(settings)?;
                self.with_live(|live| {
                    live.reset();
                    if let Some(import) = &report.import {
                        live.apply_import(import);
                    }
                });
                self.with_engine(root, &name, |engine| self.refresh_devices(engine, true));
                tracing::info!(
                    library_id = %report.library_id,
                    created = report.created,
                    snapshot = report.snapshot_written,
                    replaced = replaced.is_some(),
                    "同期フォルダを設定しました"
                );
                Ok(report)
            }
            Err(err) => {
                // 参加を記録したあとで失敗した（取り込みの途中など）ときは、フォルダも覚えて、次の同期が続きから行う。
                if after.is_some() && after != before {
                    let mut settings = self.load_settings()?;
                    settings.folder = Some(root_str);
                    self.save_settings(settings)?;
                    self.with_live(|live| {
                        live.reset();
                        live.import_failed(&err);
                    });
                }
                Err(AppError::from(err))
            }
        };
        Ok(SetupAttempt { replaced, result })
    }

    /// 同期を解除する（コマンド `leave_sync`）。手元のデータと同期フォルダの中身はそのまま。
    pub async fn leave(self: &Arc<Self>) -> Result<SyncStatus, AppError> {
        let guard = self.op_lock.lock().await;
        let result = self
            .blocking(|t| {
                bibcontainer_sync::leave(&AppSyncDb::new(Arc::clone(&t.db)))?;
                let mut settings = t.load_settings()?;
                settings.folder = None;
                t.save_settings(settings)?;
                t.with_live(Live::reset);
                Ok::<(), AppError>(())
            })
            .await;
        drop(guard);
        result??;
        self.refresh_configured().await;
        self.restart_watcher().await;
        tracing::info!("同期を解除しました");
        self.publish().await
    }

    /// 端末名を変える（コマンド `set_device_name`）。保存して、同期フォルダの自分の記録も書き直す
    /// （書けなければ、次の書き出しのときに直る）。
    pub async fn set_device_name(self: &Arc<Self>, name: String) -> Result<SyncStatus, AppError> {
        let name = validate_device_name(&name)?;
        let guard = self.op_lock.lock().await;
        let result = self
            .blocking(move |t| {
                let mut settings = t.load_settings()?;
                settings.device_name = Some(name.clone());
                t.save_settings(settings)?;
                t.with_live(|live| live.rename_self(&name));
                if let Some(ctx) = t.ctx()? {
                    let written = t.with_engine(&ctx.root, &ctx.name, |engine| {
                        engine.write_device_file(None)
                    });
                    if let Err(err) = written {
                        tracing::warn!(error = %err, "端末名を同期フォルダに書けませんでした。次の書き出しのときに直します");
                    }
                }
                Ok::<(), AppError>(())
            })
            .await;
        drop(guard);
        result??;
        self.publish().await
    }

    /// 使われていない書影を整理して、消した数を返す（コマンド `gc_covers`。FR-SYNC-13）。
    /// 同期が設定されていれば、先に他の端末の変更を取り込む（参照している本が届いていないと、使われている
    /// 書影を消してしまうため）。取り込めなければ、整理しない。
    pub async fn gc_covers(self: &Arc<Self>) -> Result<u32, AppError> {
        let guard = self.op_lock.lock().await;
        let result = self
            .blocking(|t| {
                let settings = t.load_settings()?;
                let name = t.name_of(&settings);
                let ctx = t.ctx()?;
                let (root, imported) = match &ctx {
                    Some(ctx) => (ctx.root.clone(), true),
                    None => (PathBuf::new(), false),
                };
                t.with_engine(&root, &name, |engine| -> bibcontainer_sync::Result<usize> {
                    if imported {
                        let report = engine.import_all(SYNC_ROUNDS)?;
                        t.with_live(|live| live.apply_import(&report));
                        if report.changed_anything() {
                            (t.hooks.library_changed)(
                                report.changed_book_ids.clone(),
                                report.other_changed,
                            );
                        }
                    }
                    let gc = engine.gc_covers()?;
                    Ok(gc.local_removed + gc.remote_removed)
                })
                .map_err(AppError::from)
            })
            .await;
        drop(guard);
        let removed = result??;
        tracing::info!(removed, "使われていない書影を整理しました");
        self.request_publish();
        Ok(u32::try_from(removed).unwrap_or(u32::MAX))
    }

    // -----------------------------------------------------------------------
    // ファイル監視
    // -----------------------------------------------------------------------

    /// 監視を作る（設定されていて、同期フォルダがあるとき）。
    fn make_watcher(&self) -> Option<FolderWatcher> {
        let ctx = self.ctx().ok().flatten()?;
        let device_id = self
            .db
            .run_blocking(|conn, _| Ok(meta::device_id(conn)?))
            .ok()?;
        let signals = self.signals.clone();
        match watch::start(&ctx.root, &device_id, move || {
            let _ = signals.send(Signal::FolderEvent);
        }) {
            Ok(watcher) => {
                tracing::debug!(folder = %ctx.root.display(), "同期フォルダの監視を始めました");
                Some(watcher)
            }
            Err(err) => {
                // 同期フォルダがない（Google Drive 未接続など）。定期の確認のたびに、張り直しを試す
                tracing::debug!(error = %err, folder = %ctx.root.display(), "同期フォルダを監視できません");
                None
            }
        }
    }

    /// 監視を張り直す（設定・解除・復元のあと。端末 ID が変わるので、自分の書き込みの見分けも直す）。
    async fn restart_watcher(self: &Arc<Self>) {
        let watcher = self.blocking(Self::make_watcher).await.ok().flatten();
        *lock(&self.watcher) = watcher;
    }

    /// テスト用: ファイル監視をやめる（Windows は、監視しているフォルダの親フォルダを、名前の変更で動かせない）。
    #[cfg(test)]
    pub(crate) fn drop_watcher(&self) {
        *lock(&self.watcher) = None;
    }

    /// 設定されているのに監視していなければ、張り直しを試す（同期フォルダが後からつながったとき）。
    async fn ensure_watcher(self: &Arc<Self>) {
        if self.is_configured() && lock(&self.watcher).is_none() {
            self.restart_watcher().await;
        }
    }

    // -----------------------------------------------------------------------
    // スケジューラ
    // -----------------------------------------------------------------------

    fn on_signal(self: &Arc<Self>, schedule: &mut Schedule, signal: Signal, now: u64) {
        match signal {
            Signal::ImportMore => schedule.request_import(now),
            // 同期が設定されていなければ、ほかの合図は要らない
            _ if !self.is_configured() => {}
            Signal::LocalChange => {
                schedule.on_local_change(now);
                // 未書き出しの件数が増えたことを、画面に伝える
                self.request_publish();
            }
            Signal::FolderEvent => schedule.on_folder_event(now),
            Signal::Focus => schedule.on_focus(now),
        }
    }

    async fn run_scheduler(self: Arc<Self>, mut rx: UnboundedReceiver<Signal>) {
        tokio::time::sleep(STARTUP_DELAY).await;
        self.refresh_configured().await;
        self.restart_watcher().await;
        if self.is_configured() {
            self.startup_cycle().await;
        } else {
            self.request_publish();
        }

        let origin = Instant::now();
        let now = move || u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut schedule = Schedule::new(now());
        loop {
            let wait = schedule.next_wake().map_or(Duration::from_secs(60), |at| {
                Duration::from_millis(at.saturating_sub(now()))
            });
            tokio::select! {
                signal = rx.recv() => {
                    let Some(signal) = signal else { return };
                    self.on_signal(&mut schedule, signal, now());
                }
                () = tokio::time::sleep(wait) => {}
            }
            // 溜まっている合図は、待たずにまとめて受ける
            while let Ok(signal) = rx.try_recv() {
                self.on_signal(&mut schedule, signal, now());
            }
            let due = schedule.take_due(now());
            if due.tick {
                self.ensure_watcher().await;
            }
            if due.any() {
                self.run_due(due).await;
            }
        }
    }
}
