//! 同期エンジンの本体（設定と共通の部品）。
//!
//! 「いつ動かすか」（起動時・前面に来たとき・ファイル監視・定期・書き出しのまとめ）は呼び出し側
//! （`src-tauri`）が決める。エンジンは、1 回分の取り込み（`import`）、書き出し（`export`）、
//! 初めての設定（`setup`）などを関数として提供する。同時に 2 つ以上は呼ばないこと（呼び出し側が 1 つずつ）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use bibcontainer_core::ids::is_valid_device_id;
use bibcontainer_core::sync_state;
use bibcontainer_core::{Clock, meta};

use crate::db::SyncDb;
use crate::error::{Result, SyncError};
use crate::format::{
    DeviceFile, LIBRARY_FORMAT, LibraryFile, SUPPORTED_FORMAT_VERSION, compare_versions,
};
use crate::layout::{
    DEVICES_DIR, LIBRARY_FILE, OPS_DIR, SNAPSHOTS_DIR, device_file, ops_dir,
    parse_device_file_name, parse_segment_name, parse_snapshot_name, snapshots_dir,
};
use crate::status::SyncDevice;
use crate::transport::{FileEntry, SyncTransport, list_or_empty, read_optional};

/// この端末の情報（端末名・OS・アプリの版）。呼び出し側から受け取る。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// 端末名（既定はホスト名）。
    pub name: String,
    /// `std::env::consts::OS` など。
    pub os: String,
    /// アプリの版（`x.y.z`）。`library.json` の最低の版との比較に使う。
    pub app_version: String,
}

/// エンジンの調整値。既定値は 0013 のとおり。
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// 1 回の取り込みで読むセグメントの数の上限。残りは次の回に回す。
    pub max_files_per_import: usize,
    /// 1 つのセグメントに入れる Op の数の上限。
    pub segment_max_ops: usize,
    /// 自分の Op がこの数たまったら、スナップショットを書く。
    pub snapshot_every_ops: i64,
    /// 新しい Op があり、前回のスナップショットからこの日数がたったら、スナップショットを書く。
    pub snapshot_every_days: i64,
    /// この日数より古い自分のセグメントを 1 つにまとめる。
    pub consolidate_after_days: i64,
    /// 統合したセグメントに入れる Op の数の上限。
    pub consolidated_max_ops: usize,
    /// 使われていない書影を消すのは、更新から この日数以上たったものだけ。
    pub cover_gc_days: i64,
    /// 他端末の HLC がこの時間（ミリ秒）以上先なら、時計のずれとして警告する。
    pub skew_threshold_ms: i64,
    /// 1 回の取り込みで、同期フォルダから写す書影の数の上限。
    pub max_cover_fetch: usize,
    /// 1 回の取り込みでスナップショットを適用するときの、1 トランザクションあたりのエンティティ数。
    pub snapshot_batch: usize,
    /// 古い一時ファイル（`.tmp-*`）を消す基準の時間（ミリ秒）。
    pub stale_tmp_ms: i64,
}

impl Default for SyncOptions {
    fn default() -> Self {
        const DAY: i64 = 24 * 60 * 60 * 1000;
        Self {
            max_files_per_import: 200,
            segment_max_ops: 1000,
            snapshot_every_ops: 2000,
            snapshot_every_days: 7,
            consolidate_after_days: 30,
            consolidated_max_ops: 10_000,
            cover_gc_days: 30,
            skew_threshold_ms: 5 * 60 * 1000,
            max_cover_fetch: 500,
            snapshot_batch: 500,
            stale_tmp_ms: DAY,
        }
    }
}

/// 同期エンジン。DB（`SyncDb`）、同期フォルダ（`SyncTransport`）、時計、端末ローカルの書影のフォルダを持つ。
pub struct SyncEngine<'a, D: SyncDb, T: SyncTransport + ?Sized> {
    pub(crate) db: &'a D,
    pub(crate) transport: &'a T,
    pub(crate) clock: &'a dyn Clock,
    pub(crate) covers_dir: PathBuf,
    pub(crate) device: DeviceInfo,
    pub(crate) options: SyncOptions,
}

/// 端末ローカルの状態から数えられる、画面用の値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCounters {
    /// 参加しているライブラリの ID。
    pub library_id: Option<String>,
    /// この端末の ID。
    pub device_id: String,
    /// 未書き出しの Op の数。
    pub pending_ops: u64,
    /// まだ手元に届いていない書影の数。
    pub missing_covers: u64,
}

impl<'a, D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'a, D, T> {
    /// エンジンを作る。`covers_dir` は端末ローカルの書影のフォルダ（`<先頭2文字>/<sha256>.jpg`）。
    pub fn new(
        db: &'a D,
        transport: &'a T,
        clock: &'a dyn Clock,
        covers_dir: impl Into<PathBuf>,
        device: DeviceInfo,
    ) -> Self {
        Self {
            db,
            transport,
            clock,
            covers_dir: covers_dir.into(),
            device,
            options: SyncOptions::default(),
        }
    }

    /// 調整値を差し替える。
    pub fn with_options(mut self, options: SyncOptions) -> Self {
        self.options = options;
        self
    }

    /// 調整値。
    pub fn options(&self) -> &SyncOptions {
        &self.options
    }

    /// この端末の情報。
    pub fn device(&self) -> &DeviceInfo {
        &self.device
    }

    pub(crate) fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    /// この端末の ID。
    pub fn device_id(&self) -> Result<String> {
        self.db.with_conn(|c| meta::device_id(c))
    }

    // -----------------------------------------------------------------------
    // library.json
    // -----------------------------------------------------------------------

    /// `library.json` を読む。なければ `None`。
    pub fn read_library_file(&self) -> Result<Option<LibraryFile>> {
        let Some(bytes) = read_optional(self.transport, LIBRARY_FILE)? else {
            return Ok(None);
        };
        let file: LibraryFile =
            serde_json::from_slice(&bytes).map_err(|e| SyncError::BadLibraryFile(e.to_string()))?;
        if file.format != LIBRARY_FORMAT {
            return Err(SyncError::BadLibraryFile(format!(
                "形式が違います（{}）",
                file.format
            )));
        }
        if file.library_id.is_empty() {
            return Err(SyncError::BadLibraryFile(
                "ライブラリ ID が空です".to_owned(),
            ));
        }
        Ok(Some(file))
    }

    /// このアプリで読めるライブラリか（形式の版、読める最低のアプリの版）。
    pub(crate) fn check_compat(&self, lib: &LibraryFile) -> Result<()> {
        let too_new_format = lib.format_version > SUPPORTED_FORMAT_VERSION;
        let too_new_app = compare_versions(&lib.min_app_version, &self.device.app_version).is_gt();
        if too_new_format || too_new_app {
            return Err(SyncError::UpdateRequired {
                required: lib.min_app_version.clone(),
                current: self.device.app_version.clone(),
            });
        }
        Ok(())
    }

    /// 同期フォルダのライブラリが、この端末のライブラリか確かめる（FR-SYNC-10）。
    /// 同期が設定されていなければ `NotConfigured`、ライブラリがなければ `LibraryMissing`。
    /// 戻り値はライブラリ ID。
    pub(crate) fn check_library(&self) -> Result<String> {
        let local = self
            .db
            .with_conn(|c| sync_state::library_id(c))?
            .ok_or(SyncError::NotConfigured)?;
        let lib = self.read_library_file()?.ok_or(SyncError::LibraryMissing)?;
        self.check_compat(&lib)?;
        if lib.library_id != local {
            return Err(SyncError::LibraryMismatch {
                folder: lib.library_id,
                local,
            });
        }
        Ok(local)
    }

    // -----------------------------------------------------------------------
    // 端末の情報
    // -----------------------------------------------------------------------

    /// `devices/*.json` を読む（読めないもの、端末 ID の書式でない名前の衝突コピーは無視する）。
    pub fn read_device_files(&self) -> Result<Vec<DeviceFile>> {
        let mut out = Vec::new();
        for entry in list_or_empty(self.transport, DEVICES_DIR)? {
            if entry.is_dir {
                continue;
            }
            let Some(dev) = parse_device_file_name(&entry.name) else {
                continue;
            };
            let Some(bytes) = read_optional(self.transport, &device_file(dev))? else {
                continue;
            };
            match serde_json::from_slice::<DeviceFile>(&bytes) {
                Ok(f) if f.device_id == dev => out.push(f),
                Ok(_) | Err(_) => {
                    tracing::warn!(dev, "端末の情報（devices/*.json）を読めないので無視した");
                }
            }
        }
        out.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        Ok(out)
    }

    /// 同期フォルダにいる端末の一覧（自分を含む）。
    pub fn devices(&self) -> Result<Vec<SyncDevice>> {
        let me = self.device_id()?;
        Ok(self
            .read_device_files()?
            .into_iter()
            .map(|f| SyncDevice {
                is_self: f.device_id == me,
                device_id: f.device_id,
                name: f.name,
                os: f.os,
                app_version: f.app_version,
                last_export_at: f.last_export_at,
            })
            .collect())
    }

    /// `devices/<自分>.json` を書き直す（端末名の変更のあとなどに、呼び出し側が呼んでもよい）。
    /// `last_export_ms` は最後に書き出した時刻。まだなら `None`。
    pub fn write_device_file(&self, last_export_ms: Option<i64>) -> Result<()> {
        let (id, max_seq) = self
            .db
            .with_conn(|c| Ok((meta::device_id(c)?, sync_state::current_seq(c)?)))?;
        let last_export_at = match last_export_ms {
            Some(ms) => Some(bibcontainer_core::hlc::ms_to_iso8601(ms)?),
            // 渡されなければ、前に書いたものを引き継ぐ。
            None => self
                .read_device_files()?
                .into_iter()
                .find(|f| f.device_id == id)
                .and_then(|f| f.last_export_at),
        };
        let file = DeviceFile {
            device_id: id.clone(),
            name: self.device.name.clone(),
            os: self.device.os.clone(),
            app_version: self.device.app_version.clone(),
            last_export_at,
            max_seq,
        };
        let data = serde_json::to_vec_pretty(&file)?;
        self.transport.write_atomic(&device_file(&id), &data)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 画面用の値
    // -----------------------------------------------------------------------

    /// 手元の状態から数えられる値（ライブラリ ID、未書き出しの Op、届いていない書影）。
    /// 同期フォルダには触れない。
    pub fn local_counters(&self) -> Result<LocalCounters> {
        let (library_id, device_id, pending_ops, hashes) = self.db.with_conn(|c| {
            Ok((
                sync_state::library_id(c)?,
                meta::device_id(c)?,
                sync_state::pending_count(c)?,
                sync_state::referenced_cover_hashes(c)?,
            ))
        })?;
        let missing = crate::covers::count_missing(&self.covers_dir, &hashes);
        Ok(LocalCounters {
            library_id,
            device_id,
            pending_ops,
            missing_covers: missing,
        })
    }
}

/// 一覧の項目のうち、読む対象でも衝突コピーでもない、無視してよいファイル名か
/// （一時ファイル、OS が作る隠しファイル）。
pub(crate) fn is_ignorable_name(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("desktop.ini")
        || name.eq_ignore_ascii_case("thumbs.db")
}

/// セグメントのファイル 1 つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SegmentRef {
    pub dev: String,
    pub from: i64,
    pub to: i64,
    pub name: String,
    pub entry: FileEntry,
}

/// ほかの端末のフォルダの一覧。
#[derive(Debug, Default)]
pub(crate) struct RemoteListing {
    /// 端末 ID → セグメント（`from`・`to` の順）。
    pub segments: BTreeMap<String, Vec<SegmentRef>>,
    /// 端末 ID → スナップショットのファイル名（新しい順）。
    pub snapshots: BTreeMap<String, Vec<String>>,
    /// 衝突コピーなど、読まずに警告したファイル・フォルダの数。
    pub conflict_copies: usize,
    /// 一覧に失敗したフォルダのメッセージ。
    pub errors: Vec<String>,
}

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 端末のフォルダ（`ops/<dev>`）のセグメントを一覧する。フォルダがなければ空。
    /// 戻り値の 2 つめは、読まずに警告したファイルの数。
    pub(crate) fn list_segments(&self, dev: &str) -> Result<(Vec<SegmentRef>, usize)> {
        let mut out = Vec::new();
        let mut conflicts = 0;
        for entry in list_or_empty(self.transport, &ops_dir(dev))? {
            if entry.is_dir || is_ignorable_name(&entry.name) {
                continue;
            }
            match parse_segment_name(&entry.name) {
                Some((from, to)) => out.push(SegmentRef {
                    dev: dev.to_owned(),
                    from,
                    to,
                    name: entry.name.clone(),
                    entry,
                }),
                None => {
                    tracing::warn!(dev, name = %entry.name, "セグメントの名前でないファイルを読まなかった（衝突コピーなど）");
                    conflicts += 1;
                }
            }
        }
        out.sort_by_key(|s| (s.from, s.to));
        Ok((out, conflicts))
    }

    /// 端末のフォルダ（`snapshots/<dev>`）のスナップショットのファイル名を、新しい順に返す。
    pub(crate) fn list_snapshots(&self, dev: &str) -> Result<(Vec<String>, usize)> {
        let mut out = Vec::new();
        let mut conflicts = 0;
        for entry in list_or_empty(self.transport, &snapshots_dir(dev))? {
            if entry.is_dir || is_ignorable_name(&entry.name) {
                continue;
            }
            if parse_snapshot_name(&entry.name).is_some() {
                out.push(entry.name);
            } else {
                tracing::warn!(dev, name = %entry.name, "スナップショットの名前でないファイルを読まなかった");
                conflicts += 1;
            }
        }
        out.sort_by(|a, b| b.cmp(a));
        Ok((out, conflicts))
    }

    /// 自分以外の端末のフォルダ（`ops/*`、`snapshots/*`）を一覧する。
    pub(crate) fn list_remote(&self, self_id: &str) -> Result<RemoteListing> {
        let mut listing = RemoteListing::default();
        for (root, is_ops) in [(OPS_DIR, true), (SNAPSHOTS_DIR, false)] {
            for entry in list_or_empty(self.transport, root)? {
                if is_ignorable_name(&entry.name) {
                    continue;
                }
                if !entry.is_dir || !is_valid_device_id(&entry.name) {
                    tracing::warn!(root, name = %entry.name, "端末のフォルダでないものを読まなかった");
                    listing.conflict_copies += 1;
                    continue;
                }
                if entry.name == self_id {
                    continue;
                }
                let dev = entry.name;
                let result = if is_ops {
                    self.list_segments(&dev).map(|(segments, conflicts)| {
                        if !segments.is_empty() {
                            listing.segments.insert(dev.clone(), segments);
                        }
                        conflicts
                    })
                } else {
                    self.list_snapshots(&dev).map(|(names, conflicts)| {
                        if !names.is_empty() {
                            listing.snapshots.insert(dev.clone(), names);
                        }
                        conflicts
                    })
                };
                match result {
                    Ok(conflicts) => listing.conflict_copies += conflicts,
                    Err(e) => listing.errors.push(format!("{root}/{dev}: {e}")),
                }
            }
        }
        Ok(listing)
    }
}
