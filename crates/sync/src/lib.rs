//! bibcontainer の同期エンジン。
//!
//! 同期フォルダ（主に Google Drive for desktop）を介して、複数の端末で同じ蔵書を使えるようにする。
//! Tauri には依存せず、`bibcontainer-core` の Op・HLC・DB の操作の上に作る。設計は
//! `docs/decisions/0013-phase2-sync.md` と `docs/decisions/0014-sync-engine.md` を参照。
//!
//! - `SyncTransport`: 同期フォルダへの読み書き。`LocalFolder`（`std::fs`）とテスト用の `MemoryFolder`。
//! - `SyncDb`: DB への短い操作の受け渡し。ファイルの読み書きの間は、DB の接続を持たない。
//! - `SyncEngine`: 1 回分の取り込み（`import`）・書き出し（`export`）・同期（`sync`）、初めての設定
//!   （`inspect`・`setup`・`leave`）、手入れ（`maintain`・`gc_covers`）。
//! - `status`: 画面（IPC）とやり取りする型（`SyncStatus` など。ts-rs で TypeScript にも出す）。

pub mod covers;
pub mod db;
pub mod engine;
pub mod error;
pub mod export;
pub mod format;
pub mod import;
pub mod layout;
pub mod local_folder;
pub mod maintenance;
pub mod memory;
pub mod setup;
pub mod snapshot;
pub mod status;
pub mod transport;

pub use db::{ConnDb, SyncDb};
pub use engine::{DeviceInfo, LocalCounters, SyncEngine, SyncOptions};
pub use error::{Result, SyncError};
pub use export::ExportReport;
pub use import::ImportReport;
pub use local_folder::LocalFolder;
pub use maintenance::{ConsolidationReport, GcReport, MaintenanceReport};
pub use memory::{Delivery, DeliveryPolicy, MemoryFolder, MemoryTransport};
pub use setup::{SetupReport, SyncReport, leave};
pub use snapshot::SnapshotInfo;
pub use status::{
    ClockSkew, FolderInspection, FolderKind, SetupMode, SyncDevice, SyncState, SyncStatus,
};
pub use transport::{
    FileEntry, SyncTransport, TransportError, TransportErrorKind, TransportResult,
};
