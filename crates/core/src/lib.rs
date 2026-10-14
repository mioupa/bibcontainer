//! bibcontainer のコアライブラリ。
//!
//! ローカル DB（SQLite）の接続とマイグレーション、ハイブリッド論理時計（HLC）、
//! 変更ログ（Op）の記録と適用を提供する。Tauri には依存しない。
//!
//! 設計の経緯は `docs/decisions/0003-local-db-and-migrations.md` と
//! `docs/decisions/0004-hlc-and-op-log.md` を参照。

pub mod db;
pub mod dump;
pub mod error;
pub mod hlc;
pub mod ids;
pub mod meta;
pub mod migrate;
pub mod op;
pub mod schema;
pub mod store;

pub use db::{open, open_in_memory, open_with_migrations, write_tx};
pub use dump::dump_canonical;
pub use error::{Error, Result};
pub use hlc::{Clock, Hlc, HlcState, ManualClock, SystemClock};
pub use migrate::{LATEST_VERSION, MIGRATIONS, Migration, MigrationReport};
pub use op::Op;
pub use schema::{Entity, Fields};
pub use store::{
    ApplyOutcome, apply_op, observe_hlc, pending_ops, record_change, record_change_committed,
};
