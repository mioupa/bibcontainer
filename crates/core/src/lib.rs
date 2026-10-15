//! bibcontainer のコアライブラリ。
//!
//! ローカル DB（SQLite）の接続とマイグレーション、ハイブリッド論理時計（HLC）、
//! 変更ログ（Op）の記録と適用、書籍などの操作・検索・書き出し・バックアップを提供する。
//! Tauri には依存しない。
//!
//! 設計の経緯は `docs/decisions/0003-local-db-and-migrations.md`、
//! `docs/decisions/0004-hlc-and-op-log.md`、`docs/decisions/0005-phase1-architecture.md`、
//! `docs/decisions/0006-core-library.md`、同期の DB 側の操作は `docs/decisions/0014-sync-engine.md` を参照。

pub mod backup;
pub mod barcode;
pub mod db;
pub mod derived;
pub mod dump;
pub mod error;
pub mod export;
pub mod hlc;
pub mod ids;
pub mod library;
pub mod meta;
pub mod migrate;
pub mod normalize;
pub mod op;
pub mod rows;
pub mod schema;
pub mod search;
pub mod settings;
pub mod store;
pub mod sync_state;
pub mod values;
pub mod volume;

pub use db::{open, open_in_memory, open_with_migrations, write_tx};
pub use dump::dump_canonical;
pub use error::{Error, Result};
pub use hlc::{Clock, Hlc, HlcState, ManualClock, SystemClock};
pub use migrate::{LATEST_VERSION, MIGRATIONS, Migration, MigrationReport};
pub use normalize::normalize_for_search;
pub use op::Op;
pub use rows::{Author, BookDetail, ListRow, get_book_detail, get_rows, list_rows};
pub use schema::{Entity, Fields};
pub use search::search_books;
pub use store::{
    ApplyOutcome, apply_op, observe_hlc, pending_ops, record_change, record_change_committed,
};
