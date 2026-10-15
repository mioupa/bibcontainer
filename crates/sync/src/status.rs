//! 画面（IPC）とやり取りする同期の型。形は `docs/decisions/0013-phase2-sync.md` §9 のとおり（camelCase）。
//!
//! アプリ（`src-tauri`）は、エンジンの結果（`ImportReport` など）と DB・設定から `SyncStatus` を組み立てる。
//! エンジンが直接作るのは `FolderInspection` と `SyncDevice` の一覧。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 同期の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SyncState {
    /// 同期フォルダが未設定。
    Disabled,
    /// 待機中（直近の同期が成功した）。
    Idle,
    /// 取り込み・書き出しの最中。
    Syncing,
    /// 直近の同期が失敗した。
    Error,
    /// 同期フォルダのライブラリが、この端末のライブラリと違う。
    Mismatch,
    /// ライブラリを読むのに、もっと新しいアプリが必要。
    UpdateRequired,
}

/// 同期フォルダにいる端末（自分を含む）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncDevice {
    pub device_id: String,
    pub name: String,
    /// `macos`・`windows`・`linux` など（Rust の `std::env::consts::OS`）。
    pub os: String,
    pub app_version: String,
    /// 最後に書き出した時刻（ISO 8601）。まだ書き出していなければ null。
    pub last_export_at: Option<String>,
    pub is_self: bool,
}

/// 他の端末の時計が進んでいる（FR-SYNC-11。5 分以上）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClockSkew {
    pub device_id: String,
    /// 端末名（同期フォルダの `devices/` に書かれていれば）。
    pub device_name: Option<String>,
    /// この端末の現在時刻より、何ミリ秒先か。
    pub ahead_ms: i64,
}

/// 同期の状態（画面に出す）。`sync-status` イベントのペイロードでもある。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncStatus {
    pub state: SyncState,
    /// `bibcontainer-library` のフォルダの絶対パス。未設定なら null。
    pub folder: Option<String>,
    pub library_id: Option<String>,
    pub device_id: String,
    pub device_name: String,
    /// ISO 8601。
    pub last_export_at: Option<String>,
    pub last_import_at: Option<String>,
    /// 未書き出しの Op の数。
    pub pending_ops: u64,
    /// まだ届いていない書影の数。
    pub missing_covers: u64,
    /// 直近のエラー（日本語）。
    pub error: Option<String>,
    pub clock_skew: Option<ClockSkew>,
    pub devices: Vec<SyncDevice>,
}

impl SyncStatus {
    /// 同期フォルダが未設定のときの状態。
    pub fn disabled(device_id: impl Into<String>, device_name: impl Into<String>) -> Self {
        Self {
            state: SyncState::Disabled,
            folder: None,
            library_id: None,
            device_id: device_id.into(),
            device_name: device_name.into(),
            last_export_at: None,
            last_import_at: None,
            pending_ops: 0,
            missing_covers: 0,
            error: None,
            clock_skew: None,
            devices: Vec::new(),
        }
    }
}

/// 選んだフォルダの中身の判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FolderKind {
    /// ライブラリがない（新しく作れる）。
    Empty,
    /// この端末のライブラリ、または参加できるライブラリ。
    Library,
    /// この端末が属しているのとは別のライブラリ。
    OtherLibrary,
    /// 書き込めない。
    NotWritable,
    /// ライブラリを読むのに、もっと新しいアプリが必要。
    UpdateRequired,
}

/// 選んだフォルダの中身の判定（`inspect_sync_folder` の結果）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderInspection {
    pub kind: FolderKind,
    /// 実際に使う `bibcontainer-library` の絶対パス。
    pub library_folder: String,
    pub library_id: Option<String>,
    pub devices: Vec<SyncDevice>,
    /// この端末に蔵書のデータがあるか。
    pub local_has_data: bool,
    /// この端末の DB が属しているライブラリ ID（まだ属していなければ null）。
    pub local_library_id: Option<String>,
}

/// 初めての設定の方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SetupMode {
    /// 新しいライブラリを作る（手元のデータを書き出す）。
    Create,
    /// 既存のライブラリに参加する（手元が空）。
    Join,
    /// 既存のライブラリに統合する（手元のデータも残す）。
    JoinMerge,
    /// 手元のデータを置き換えて参加する（アプリが DB を作り直したあとで呼ぶ）。
    JoinReplace,
}
