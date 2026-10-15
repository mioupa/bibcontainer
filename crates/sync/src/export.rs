//! 書き出し（0013 §5）。

use bibcontainer_core::op::Op;
use bibcontainer_core::sync_state;

use crate::db::SyncDb;
use crate::engine::{SegmentRef, SyncEngine};
use crate::error::{Result, SyncError};
use crate::format::encode_segment;
use crate::layout::{segment_name, segment_path};
use crate::transport::{SyncTransport, read_optional};

/// 1 回分の書き出しの結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// セグメントに書いた Op の数。
    pub ops_exported: usize,
    /// 書いたセグメントのファイルの数。
    pub segments_written: usize,
    /// 同期フォルダへ写した書影の数。
    pub covers_uploaded: usize,
    /// スナップショットを書いたか。
    pub snapshot_written: bool,
    /// 書き出したあとに残っている未書き出しの Op の数。
    pub pending_after: u64,
    /// 書き出した時刻（UNIX ミリ秒）。何も書き出さなかったときは `None`。
    pub exported_at_ms: Option<i64>,
}

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 1 回分の書き出し。
    ///
    /// 未書き出しの Op も、スナップショットの必要もなければ、同期フォルダには何も触らない。
    ///
    /// 1. 「スナップショットが必要」の印があれば、スナップショットを書く（その時点の未書き出しの Op はそこに含まれる）。
    /// 2. 未書き出しの Op を seq 順に読み、参照している書影を先に同期フォルダへ写してから、
    ///    セグメント（1 ファイル最大 1,000 Op）を書き、書けたら `flushed = 1` にする。
    /// 3. 前回のスナップショットから Op が 2,000 件以上たまった、または 7 日以上たって新しい Op があれば、
    ///    スナップショットを書く。
    /// 4. 何か書いたら `devices/<自分>.json` を書き直す。
    ///
    /// 書こうとした範囲と重なる既存のファイルがあり、内容が違うときは、同じ端末 ID を別の端末が使っている
    /// とみなして `DuplicateDevice` を返す（同期を止める）。
    pub fn export(&self) -> Result<ExportReport> {
        let mut report = ExportReport::default();
        let now = self.now_ms();
        let (library, pending, needs_snapshot, due) = self.db.with_conn(|c| {
            let current = sync_state::current_seq(c)?;
            let due = match sync_state::last_snapshot(c)? {
                Some((seq, ms)) => {
                    current - seq >= self.options.snapshot_every_ops
                        || (current > seq
                            && now - ms >= self.options.snapshot_every_days * 24 * 60 * 60 * 1000)
                }
                None => {
                    // 初めて見る。7 日の数え始めをここにする。
                    sync_state::set_last_snapshot(c, 0, now)?;
                    current >= self.options.snapshot_every_ops
                }
            };
            Ok((
                sync_state::library_id(c)?,
                sync_state::pending_count(c)?,
                sync_state::needs_snapshot(c)?,
                due,
            ))
        })?;
        if library.is_none() {
            return Err(SyncError::NotConfigured);
        }
        if pending == 0 && !needs_snapshot && !due {
            return Ok(report);
        }
        self.check_library()?;
        let self_id = self.device_id()?;

        if needs_snapshot {
            self.write_snapshot()?;
            report.snapshot_written = true;
        }

        let (own, _) = self.list_segments(&self_id)?;
        let max_ops = self.options.segment_max_ops.max(1);
        let mut after = 0i64;
        loop {
            let chunk = self
                .db
                .with_conn(|c| sync_state::pending_ops_after(c, after, max_ops))?;
            let run = contiguous_run(chunk);
            let (Some(first), Some(last)) = (run.first(), run.last()) else {
                break;
            };
            let (from, to) = (first.seq, last.seq);
            if let Some(bad) = run.iter().find(|op| op.dev != self_id) {
                return Err(SyncError::Internal(format!(
                    "outbox に別の端末 ID（{}）の Op がある（seq {}）",
                    bad.dev, bad.seq
                )));
            }
            report.covers_uploaded += self.upload_covers(cover_hashes_of(&run))?;
            let data = encode_segment(&run)?;
            let wrote = self.write_own_segment(&self_id, &own, from, to, &data)?;
            self.db
                .with_conn(|c| sync_state::mark_flushed(c, from, to))?;
            if wrote {
                report.segments_written += 1;
            }
            report.ops_exported += run.len();
            after = to;
        }

        // スナップショットが必要になったかを、書き出したあとで確かめ直す（書き出しで seq は増えない）。
        if !report.snapshot_written && due {
            self.write_snapshot()?;
            report.snapshot_written = true;
        }

        if report.ops_exported > 0 || report.snapshot_written {
            let done = self.now_ms();
            self.write_device_file(Some(done))?;
            report.exported_at_ms = Some(done);
        }
        report.pending_after = self.db.with_conn(|c| sync_state::pending_count(c))?;
        Ok(report)
    }

    /// 自分のセグメントを 1 つ書く。同じ名前で同じ内容のファイルが既にあれば書かない（`false`）。
    /// 重なる既存のファイルの内容が違えば `DuplicateDevice`。
    fn write_own_segment(
        &self,
        self_id: &str,
        existing: &[SegmentRef],
        from: i64,
        to: i64,
        data: &[u8],
    ) -> Result<bool> {
        let name = segment_name(from, to);
        let path = segment_path(self_id, from, to);
        let overlapping: Vec<&SegmentRef> = existing
            .iter()
            .filter(|s| s.from <= to && from <= s.to)
            .collect();
        match overlapping.as_slice() {
            [] => {}
            [seg] if seg.name == name => {
                if read_optional(self.transport, &path)?.is_some_and(|bytes| bytes == data) {
                    // 前回、書いたあと `flushed` にする前に落ちた。同じ内容なので、書き直さなくてよい。
                    return Ok(false);
                }
                return Err(duplicate_device(self_id, from, to, &seg.name));
            }
            [seg, ..] => return Err(duplicate_device(self_id, from, to, &seg.name)),
        }
        self.transport.write_atomic(&path, data)?;
        Ok(true)
    }
}

fn duplicate_device(self_id: &str, from: i64, to: i64, existing: &str) -> SyncError {
    SyncError::DuplicateDevice {
        device_id: self_id.to_owned(),
        detail: format!(
            "同期フォルダに、この端末が書こうとした範囲（{from}-{to}）と重なる別の内容のファイル {existing} がある"
        ),
    }
}

/// seq が連続している先頭の部分だけを取り出す（欠番があれば、そこで切る）。
fn contiguous_run(chunk: Vec<Op>) -> Vec<Op> {
    let mut run: Vec<Op> = Vec::with_capacity(chunk.len());
    for op in chunk {
        if run.last().is_some_and(|prev| op.seq != prev.seq + 1) {
            break;
        }
        run.push(op);
    }
    run
}

/// Op が参照する書影のハッシュ（`cover_hash` を設定する Op の値）。
fn cover_hashes_of(ops: &[Op]) -> Vec<String> {
    ops.iter()
        .filter(|op| op.ent == bibcontainer_core::Entity::Book)
        .filter_map(|op| op.set.get("cover_hash").and_then(|v| v.as_str()))
        .map(str::to_owned)
        .collect()
}
