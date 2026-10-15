//! DB の操作の受け渡し（`SyncDb`）。
//!
//! 同期エンジンは、同期フォルダのファイルを読み書きしている間（Google Drive では遅いことがある）は
//! DB の接続を持たない。DB に触るのは、`with_conn` に渡した短い処理の中だけで、終わればすぐ接続を返す。
//!
//! アプリ側は、`Database::run`（Mutex で共有している接続を借りる）をブロッキングで呼べる形に包んで
//! 実装する。テストや単体の利用には `ConnDb`（接続を 1 本 Mutex で持つ）を使う。

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

use crate::error::{Result, SyncError};

/// 接続を借りて処理を行う。
pub trait SyncDb {
    /// 接続を借りて `f` を実行する。`f` が終わったら接続を返す。
    ///
    /// `f` の中では、ファイルの読み書きなど時間のかかることをしない（呼び出し側の約束）。
    fn with_conn<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> bibcontainer_core::Result<T>;
}

impl<D: SyncDb + ?Sized> SyncDb for &D {
    fn with_conn<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> bibcontainer_core::Result<T>,
    {
        (**self).with_conn(f)
    }
}

impl<D: SyncDb + ?Sized> SyncDb for Arc<D> {
    fn with_conn<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> bibcontainer_core::Result<T>,
    {
        (**self).with_conn(f)
    }
}

/// 接続を 1 本、Mutex で持つ `SyncDb`。
#[derive(Debug)]
pub struct ConnDb {
    conn: Mutex<Connection>,
}

impl ConnDb {
    /// 接続を受け取る。
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// 接続を直接借りる（テストが DB を読み書きするため）。
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 接続を差し替える（バックアップの復元で、DB のファイルを置き換えて開き直したとき）。
    pub fn replace(&self, conn: Connection) {
        *self.lock() = conn;
    }

    /// 接続を取り出す。
    pub fn into_inner(self) -> Connection {
        self.conn
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl SyncDb for ConnDb {
    fn with_conn<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> bibcontainer_core::Result<T>,
    {
        let mut conn = self.lock();
        f(&mut conn).map_err(SyncError::from)
    }
}
