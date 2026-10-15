//! 手入れ（0013 §3・§10）: 古い一時ファイルの掃除、自分のセグメントの統合、書影の整理。

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bibcontainer_core::sync_state;

use crate::covers::hash_of_cover_name;
use crate::db::SyncDb;
use crate::engine::{SegmentRef, SyncEngine};
use crate::error::Result;
use crate::format::{decode_segment, encode_segment};
use crate::layout::{
    COVERS_DIR, DEVICES_DIR, cover_remote_dir, ops_dir, segment_name, segment_path, snapshots_dir,
};
use crate::transport::{SyncTransport, TMP_PREFIX, list_or_empty};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// 自分のセグメントの統合の結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsolidationReport {
    /// 書いた統合後のファイルの数。
    pub files_written: usize,
    /// 消した元のファイルの数。
    pub files_removed: usize,
}

/// 書影の整理の結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GcReport {
    /// 手元（端末ローカル）から消した書影の数。
    pub local_removed: usize,
    /// 同期フォルダから消した書影の数。
    pub remote_removed: usize,
    /// 使われていないが、更新から日が浅いので残した書影の数。
    pub kept_recent: usize,
    /// 手元から消して空いた大きさ（バイト）。
    pub local_bytes_freed: u64,
}

/// 手入れの結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaintenanceReport {
    /// 消した古い一時ファイルの数。
    pub tmp_removed: usize,
    /// セグメントの統合の結果。
    pub consolidation: ConsolidationReport,
}

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 古い一時ファイルの掃除と、自分のセグメントの統合（起動時や 1 日 1 回ほどで呼ぶ）。
    pub fn maintain(&self) -> Result<MaintenanceReport> {
        self.check_library()?;
        let tmp_removed = self.cleanup_tmp()?;
        let consolidation = self.consolidate_own_segments()?;
        Ok(MaintenanceReport {
            tmp_removed,
            consolidation,
        })
    }

    /// 同期フォルダに残った古い一時ファイル（`.tmp-*`）を消す。消した数を返す。
    ///
    /// 自分のフォルダ（`ops/<自分>`、`snapshots/<自分>`）、`devices/`、`covers/<xx>/` が対象。
    /// 書き込み中のファイルを消さないよう、更新から `stale_tmp_ms` 以上たったものだけ消す。
    pub fn cleanup_tmp(&self) -> Result<usize> {
        let self_id = self.device_id()?;
        let now = self.now_ms();
        let mut dirs = vec![
            ops_dir(&self_id),
            snapshots_dir(&self_id),
            DEVICES_DIR.to_owned(),
        ];
        for entry in list_or_empty(self.transport, COVERS_DIR)? {
            if entry.is_dir {
                dirs.push(format!("{COVERS_DIR}/{}", entry.name));
            }
        }
        let mut removed = 0;
        for dir in dirs {
            for entry in list_or_empty(self.transport, &dir)? {
                if entry.is_dir || !entry.name.starts_with(TMP_PREFIX) {
                    continue;
                }
                let stale = entry
                    .modified_ms
                    .is_some_and(|m| now - m >= self.options.stale_tmp_ms);
                if stale {
                    self.transport.delete(&format!("{dir}/{}", entry.name))?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    /// 30 日より古い自分のセグメントを、連続する範囲ごとに 1 つにまとめ、元のファイルを消す。
    ///
    /// 古さは、そのセグメントの最後の Op の HLC（outbox から引く。なければファイルの更新時刻）で決める。
    /// 書き出し済み（`flushed`）の範囲だけが対象。まとめたファイルを書いて確かめてから、元のファイルを消す。
    /// 読む側は、取り込み済みの範囲で覆われていれば読まないので、重複して適用されない。
    pub fn consolidate_own_segments(&self) -> Result<ConsolidationReport> {
        let self_id = self.device_id()?;
        let now = self.now_ms();
        let cutoff = now - self.options.consolidate_after_days * DAY_MS;
        let (segments, _) = self.list_segments(&self_id)?;
        let flushed_to = self.db.with_conn(|c| {
            let max: i64 = c.query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM outbox WHERE flushed = 1",
                [],
                |r| r.get(0),
            )?;
            Ok(max)
        })?;

        // 古い（かつ書き出し済みの）セグメントだけを残して、連続する範囲ごとの組にする。
        let mut runs: Vec<Vec<SegmentRef>> = Vec::new();
        for seg in segments {
            let hlc_ms = self
                .db
                .with_conn(|c| sync_state::outbox_hlc_ms(c, seg.to))?
                .or(seg.entry.modified_ms);
            let old = seg.to <= flushed_to && hlc_ms.is_some_and(|ms| ms < cutoff);
            if !old {
                // 古くないものに当たったら、そこで組を切る（連続性を保つため）。
                runs.push(Vec::new());
                continue;
            }
            match runs.last_mut() {
                Some(run) if run.last().is_some_and(|prev| prev.to + 1 == seg.from) => {
                    run.push(seg);
                }
                _ => runs.push(vec![seg]),
            }
        }

        let mut report = ConsolidationReport::default();
        for run in runs.into_iter().filter(|r| r.len() >= 2) {
            let Some(merged) = self.read_run(&self_id, &run)? else {
                continue;
            };
            let (written, removed) = self.write_merged(&self_id, &run, merged)?;
            report.files_written += written;
            report.files_removed += removed;
        }
        Ok(report)
    }

    /// 組のセグメントをすべて読み、seq 順に並べた Op を返す。読めないものがあれば `None`（統合しない）。
    fn read_run(
        &self,
        self_id: &str,
        run: &[SegmentRef],
    ) -> Result<Option<Vec<bibcontainer_core::op::Op>>> {
        let mut ops = Vec::new();
        for seg in run {
            let path = segment_path(self_id, seg.from, seg.to);
            let bytes = match self.transport.read(&path) {
                Ok(b) => b,
                Err(e) if e.is_not_found() => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            match decode_segment(&bytes, self_id, seg.from, seg.to) {
                Ok(content) if content.unknown == 0 && content.invalid == 0 => {
                    ops.extend(content.ops);
                }
                // 自分のファイルが壊れている・知らない Op を含む。触らない。
                _ => return Ok(None),
            }
        }
        ops.sort_by_key(|op| op.seq);
        ops.dedup_by_key(|op| op.seq);
        Ok(Some(ops))
    }

    /// 統合したファイルを書き、元のファイルを消す。(書いた数, 消した数)。
    fn write_merged(
        &self,
        self_id: &str,
        run: &[SegmentRef],
        ops: Vec<bibcontainer_core::op::Op>,
    ) -> Result<(usize, usize)> {
        let max_ops = self
            .options
            .consolidated_max_ops
            .max(self.options.segment_max_ops);
        let mut new_names: BTreeSet<String> = BTreeSet::new();
        let mut written = 0;
        let mut group: Vec<bibcontainer_core::op::Op> = Vec::new();
        let mut groups: Vec<Vec<bibcontainer_core::op::Op>> = Vec::new();
        for op in ops {
            if group.len() >= max_ops || group.last().is_some_and(|p| op.seq != p.seq + 1) {
                groups.push(std::mem::take(&mut group));
            }
            group.push(op);
        }
        if !group.is_empty() {
            groups.push(group);
        }
        for g in &groups {
            let (from, to) = (g[0].seq, g[g.len() - 1].seq);
            let name = segment_name(from, to);
            new_names.insert(name.clone());
            // 元のファイルと同じ範囲なら、書き直さない。
            if run.iter().any(|s| s.name == name) {
                continue;
            }
            self.transport
                .write_atomic(&segment_path(self_id, from, to), &encode_segment(g)?)?;
            written += 1;
        }
        let mut removed = 0;
        for seg in run {
            if !new_names.contains(&seg.name) {
                self.transport
                    .delete(&segment_path(self_id, seg.from, seg.to))?;
                removed += 1;
            }
        }
        Ok((written, removed))
    }

    /// 使われていない書影を、手元と同期フォルダから消す（FR-SYNC-13。手動の操作だけ）。
    ///
    /// どの本（完全削除していないもの）からも参照されておらず、ファイルの更新時刻が `cover_gc_days` 日より
    /// 前の書影だけを消す。他端末の変更を取り込んだあとに呼ぶこと（参照している本が届いていないと、
    /// 使われているのに消えてしまうため。更新から日が浅い書影は残すので、通常は問題にならない）。
    /// 同期が設定されていなければ、手元の書影だけを整理する。
    pub fn gc_covers(&self) -> Result<GcReport> {
        let now = self.now_ms();
        let cutoff = now - self.options.cover_gc_days * DAY_MS;
        let (library, referenced) = self.db.with_conn(|c| {
            Ok((
                sync_state::library_id(c)?,
                sync_state::referenced_cover_hashes(c)?
                    .into_iter()
                    .collect::<BTreeSet<_>>(),
            ))
        })?;
        let mut report = GcReport::default();
        self.gc_local_covers(&referenced, cutoff, &mut report);
        if library.is_some() {
            self.check_library()?;
            self.gc_remote_covers(&referenced, cutoff, &mut report)?;
        }
        Ok(report)
    }

    fn gc_local_covers(&self, referenced: &BTreeSet<String>, cutoff: i64, report: &mut GcReport) {
        let Ok(prefixes) = fs::read_dir(&self.covers_dir) else {
            return;
        };
        for prefix in prefixes.flatten() {
            let path = prefix.path();
            if !path.is_dir() {
                continue;
            }
            let Ok(files) = fs::read_dir(&path) else {
                continue;
            };
            for file in files.flatten() {
                let Ok(name) = file.file_name().into_string() else {
                    continue;
                };
                let Some(hash) = hash_of_cover_name(&name) else {
                    continue;
                };
                if referenced.contains(hash) {
                    continue;
                }
                let Ok(meta) = file.metadata() else { continue };
                let modified = modified_ms(&meta);
                if modified.is_none_or(|m| m >= cutoff) {
                    report.kept_recent += 1;
                    continue;
                }
                if remove_file(&file.path()) {
                    report.local_removed += 1;
                    report.local_bytes_freed += meta.len();
                }
            }
        }
    }

    fn gc_remote_covers(
        &self,
        referenced: &BTreeSet<String>,
        cutoff: i64,
        report: &mut GcReport,
    ) -> Result<()> {
        let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in list_or_empty(self.transport, COVERS_DIR)? {
            if entry.is_dir {
                by_dir.insert(entry.name.clone(), Vec::new());
            }
        }
        for prefix in by_dir.keys().cloned().collect::<Vec<_>>() {
            for entry in list_or_empty(self.transport, &cover_remote_dir(&prefix))? {
                if entry.is_dir {
                    continue;
                }
                let Some(hash) = hash_of_cover_name(&entry.name) else {
                    continue;
                };
                if referenced.contains(hash) {
                    continue;
                }
                // 更新時刻が分からないものは、消さない。
                if entry.modified_ms.is_none_or(|m| m >= cutoff) {
                    report.kept_recent += 1;
                    continue;
                }
                self.transport
                    .delete(&format!("{}/{}", cover_remote_dir(&prefix), entry.name))?;
                report.remote_removed += 1;
            }
        }
        Ok(())
    }
}

fn modified_ms(meta: &fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

fn remove_file(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "書影を消せなかった");
            false
        }
    }
}
