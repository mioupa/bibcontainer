//! スナップショットの書き出し（0013 §6）。

use bibcontainer_core::sync_state;

use crate::db::SyncDb;
use crate::engine::SyncEngine;
use crate::error::{Result, SyncError};
use crate::format::{SNAPSHOT_VERSION, SnapshotHeader, encode_snapshot};
use crate::layout::{parse_snapshot_name, snapshot_name, snapshot_path};
use crate::transport::SyncTransport;
use bibcontainer_core::hlc::Hlc;

/// 自分のスナップショットを残す数（新しい方から）。
pub const KEEP_OWN_SNAPSHOTS: usize = 2;

/// 書いたスナップショットの情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    /// ファイル名。
    pub name: String,
    /// 含まれるエンティティの数。
    pub entities: usize,
    /// ファイルの大きさ（gzip 後。バイト）。
    pub bytes: usize,
    /// 含まれる自分の Op の seq。
    pub seq: i64,
    /// 同期フォルダへ写した書影の数。
    pub covers_uploaded: usize,
}

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 手元の全状態をスナップショットとして書く。
    ///
    /// 1. 状態・seq・各端末の反映済みの位置を、1 つのトランザクションで読む（接続はすぐ返す）。
    /// 2. 本が参照している書影のうち、同期フォルダにないものを写す（スナップショットより先）。
    /// 3. gzip した JSON Lines を書く。
    /// 4. 書けたら、状態に含めた seq までの Op を `flushed = 1` にし、「スナップショットが必要」の印を下ろし、
    ///    復元のときに退避した `outbox_restored` を捨て、書いた位置を覚える。
    /// 5. 自分の古いスナップショットを、新しい 2 つを残して消す。
    ///
    /// 状態を読んだあとに作られた Op は、`flushed = 0` のまま残るので、次の書き出しでセグメントに書かれる。
    pub fn write_snapshot(&self) -> Result<SnapshotInfo> {
        let library_id = self
            .db
            .with_conn(|c| sync_state::library_id(c))?
            .ok_or(SyncError::NotConfigured)?;
        let now = self.now_ms();
        let basis = self
            .db
            .with_conn(|c| sync_state::export_snapshot_basis(c, now))?;
        let covers_uploaded = self.upload_covers(basis.cover_hashes.iter().cloned())?;

        let hlc = Hlc::parse(&basis.hlc)?;
        let header = SnapshotHeader {
            v: SNAPSHOT_VERSION,
            kind: "snapshot".to_owned(),
            library_id,
            dev: basis.device_id.clone(),
            hlc: basis.hlc.clone(),
            covers: basis.covers.clone(),
        };
        let bytes = encode_snapshot(&header, &basis.entities)?;
        let name = snapshot_name(hlc.ms(), hlc.counter());
        self.transport
            .write_atomic(&snapshot_path(&basis.device_id, &name), &bytes)?;

        let seq = basis.seq;
        self.db.with_conn(|c| {
            let tx = bibcontainer_core::db::write_tx(c)?;
            sync_state::mark_flushed_through(&tx, seq)?;
            sync_state::set_needs_snapshot(&tx, false)?;
            sync_state::clear_outbox_restored(&tx)?;
            sync_state::set_last_snapshot(&tx, seq, now)?;
            tx.commit()?;
            Ok(())
        })?;

        if let Err(e) = self.prune_own_snapshots(&basis.device_id) {
            tracing::warn!(error = %e, "古いスナップショットを消せなかった");
        }
        Ok(SnapshotInfo {
            name,
            entities: basis.entities.len(),
            bytes: bytes.len(),
            seq,
            covers_uploaded,
        })
    }

    /// 自分のスナップショットを、新しい `KEEP_OWN_SNAPSHOTS` 個だけ残す。
    pub(crate) fn prune_own_snapshots(&self, self_id: &str) -> Result<usize> {
        let (names, _) = self.list_snapshots(self_id)?;
        let mut removed = 0;
        for name in names.iter().skip(KEEP_OWN_SNAPSHOTS) {
            if parse_snapshot_name(name).is_some() {
                self.transport.delete(&snapshot_path(self_id, name))?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}
