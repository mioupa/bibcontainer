//! 取り込み（0013 §4）。

use std::collections::BTreeSet;

use bibcontainer_core::db::write_tx;
use bibcontainer_core::hlc::Hlc;
use bibcontainer_core::schema::Entity;
use bibcontainer_core::store::{apply_op, observe_hlc};
use bibcontainer_core::sync_state::{self, apply_entity_state};

use crate::db::SyncDb;
use crate::engine::{RemoteListing, SegmentRef, SyncEngine};
use crate::error::{Result, SyncError};
use crate::format::{
    DecodedSnapshot, SegmentContent, SnapshotRead, decode_segment, decode_snapshot,
};
use crate::layout::{segment_path, snapshot_path};
use crate::status::ClockSkew;
use crate::transport::SyncTransport;

/// 1 回分の取り込みの結果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    /// 適用したセグメントのファイルの数。
    pub files_applied: usize,
    /// 適用したスナップショットの数。
    pub snapshots_applied: usize,
    /// 適用に回した Op の数（古いために何も変えなかったものも含む）。
    pub ops_seen: usize,
    /// 変わった本の ID（重複なし、昇順）。
    pub changed_book_ids: Vec<String>,
    /// 本以外（シリーズ・保管場所・タグ・本棚）が変わったか。
    pub other_changed: bool,
    /// 書き込み途中か同期途中とみなして、読まずに飛ばしたファイルの数（次の回にもう一度確かめる）。
    pub skipped_incomplete: usize,
    /// 知らない `ent`・`v` を含んでいて、範囲を記録しなかったファイル（スナップショット）の数。
    pub unknown_skipped: usize,
    /// 壊れていたなどで捨てた Op・項目の数。
    pub invalid_dropped: usize,
    /// 衝突コピーなど、読まずに警告したファイルの数。
    pub conflict_copies: usize,
    /// 時計のずれ（他端末の HLC がこの端末の現在時刻より 5 分以上先）。ずれが最大の端末。
    pub clock_skew: Option<ClockSkew>,
    /// 同期フォルダから写した書影の数。
    pub covers_fetched: usize,
    /// まだ手元に届いていない書影の数。
    pub missing_covers: u64,
    /// 上限のため次の回に回した、読むべきファイル（セグメントと書影）の数。0 でなければ、もう一度 `import` を呼ぶ。
    pub remaining_files: usize,
    /// 取り込みを続けられたが、一部で起きたエラー（ファイルを読めない、書影を写せないなど）。
    pub errors: Vec<String>,
    /// 取り込みを終えた時刻（UNIX ミリ秒）。
    pub finished_at_ms: i64,
}

impl ImportReport {
    /// 画面の再取得が要る変化があったか。
    pub fn changed_anything(&self) -> bool {
        !self.changed_book_ids.is_empty() || self.other_changed
    }

    /// `other` の結果を足し合わせる（`import_all` で、何回かの結果をまとめる）。
    pub fn merge(&mut self, other: ImportReport) {
        self.files_applied += other.files_applied;
        self.snapshots_applied += other.snapshots_applied;
        self.ops_seen += other.ops_seen;
        let mut ids: BTreeSet<String> = std::mem::take(&mut self.changed_book_ids)
            .into_iter()
            .collect();
        ids.extend(other.changed_book_ids);
        self.changed_book_ids = ids.into_iter().collect();
        self.other_changed |= other.other_changed;
        // 飛ばした数・衝突コピー・届いていない書影は、最後の回の値が今の状態を表す。
        self.skipped_incomplete = other.skipped_incomplete;
        self.unknown_skipped = other.unknown_skipped;
        self.conflict_copies = other.conflict_copies;
        self.invalid_dropped += other.invalid_dropped;
        if other.clock_skew.as_ref().map(|s| s.ahead_ms)
            > self.clock_skew.as_ref().map(|s| s.ahead_ms)
        {
            self.clock_skew = other.clock_skew;
        }
        self.covers_fetched += other.covers_fetched;
        self.missing_covers = other.missing_covers;
        self.remaining_files = other.remaining_files;
        self.errors.extend(other.errors);
        self.finished_at_ms = other.finished_at_ms;
    }
}

/// 変わったエンティティの集計。
#[derive(Default)]
struct Changes {
    books: BTreeSet<String>,
    other: bool,
}

impl Changes {
    fn add(&mut self, ent: Entity, id: String) {
        if ent == Entity::Book {
            self.books.insert(id);
        } else {
            self.other = true;
        }
    }
}

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 1 回分の取り込み。
    ///
    /// 1. `library.json` でライブラリ ID を確かめる（違えば `LibraryMismatch`）。
    /// 2. 他端末のスナップショット（端末ごとに最新のもの）のうち、まだ読んでいないものを読んで適用する。
    ///    スナップショットは、書き出していない Op（新規作成・統合・復元のとき）の内容も運ぶので、
    ///    取り込み済みの範囲で覆われていても、1 回は適用する（項目ごとの LWW なので、重ねても害はない）。
    /// 3. 他端末のセグメントのうち、取り込み済みの範囲で覆われていないものを、上限の数まで読んで適用する。
    /// 4. 本が参照している書影のうち、手元にないものを同期フォルダから写す。
    ///
    /// ファイル 1 つの読み書きの失敗は `errors` に入れて続ける。ライブラリの不一致・更新が必要・
    /// 端末 ID の重複・同期フォルダが使えない（一覧もできない）ときは `Err`。
    pub fn import(&self) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        let self_id = self.device_id()?;
        self.check_library()?;
        self.check_own_folder(&self_id)?;

        let mut skew: Option<(String, i64)> = None;
        let listing = self.list_remote(&self_id)?;
        report.conflict_copies = listing.conflict_copies;
        report.errors.extend(listing.errors.iter().cloned());
        let mut changes = Changes::default();

        self.import_snapshots(&listing, &self_id, &mut report, &mut changes)?;
        self.import_segments(&listing, &mut report, &mut changes, &mut skew)?;

        let covers = self.fetch_missing_covers()?;
        report.covers_fetched = covers.fetched;
        report.missing_covers = covers.still_missing;
        report.remaining_files += covers.deferred;
        if let Some(e) = covers.error {
            report
                .errors
                .push(format!("書影を取得できませんでした: {e}"));
        }

        report.changed_book_ids = changes.books.into_iter().collect();
        report.other_changed = changes.other;
        if let Some((dev, ahead_ms)) = skew {
            let device_name = self
                .read_device_files()
                .ok()
                .and_then(|files| files.into_iter().find(|f| f.device_id == dev))
                .map(|f| f.name);
            report.clock_skew = Some(ClockSkew {
                device_id: dev,
                device_name,
                ahead_ms,
            });
        }
        report.finished_at_ms = self.now_ms();
        Ok(report)
    }

    /// 読むべきファイルがなくなるまで（または `max_rounds` 回まで）`import` を繰り返し、結果をまとめる。
    pub fn import_all(&self, max_rounds: usize) -> Result<ImportReport> {
        let mut total = ImportReport::default();
        for round in 0..max_rounds.max(1) {
            let one = self.import()?;
            let done = one.remaining_files == 0;
            if round == 0 {
                total = one;
            } else {
                total.merge(one);
            }
            if done {
                break;
            }
        }
        Ok(total)
    }

    /// 自分のフォルダに、自分の seq より先のセグメントがあれば、同じ端末 ID を別の端末が使っている。
    pub(crate) fn check_own_folder(&self, self_id: &str) -> Result<()> {
        let (own, _) = self.list_segments(self_id)?;
        let Some(max_to) = own.iter().map(|s| s.to).max() else {
            return Ok(());
        };
        let current = self.db.with_conn(|c| sync_state::current_seq(c))?;
        if max_to > current {
            return Err(SyncError::DuplicateDevice {
                device_id: self_id.to_owned(),
                detail: format!(
                    "同期フォルダには seq {max_to} までの変更ログがあるが、この端末は {current} までしか持っていない"
                ),
            });
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // スナップショット
    // -----------------------------------------------------------------------

    fn import_snapshots(
        &self,
        listing: &RemoteListing,
        self_id: &str,
        report: &mut ImportReport,
        changes: &mut Changes,
    ) -> Result<()> {
        if listing.snapshots.is_empty() {
            return Ok(());
        }
        let seen = self.db.with_conn(|c| sync_state::snapshot_seen(c))?;
        // 新しいスナップショットを持つ端末から順に見る。
        let mut devices: Vec<(&String, &Vec<String>)> = listing.snapshots.iter().collect();
        devices.sort_by(|a, b| b.1[0].cmp(&a.1[0]));
        for (dev, names) in devices {
            let last_seen = seen.get(dev.as_str());
            for name in names {
                // 名前は新しい順。確かめ済みの名前に着いたら、それ以降は見なくてよい。
                if last_seen.is_some_and(|s| name.as_str() <= s.as_str()) {
                    break;
                }
                let bytes = match self.transport.read(&snapshot_path(dev, name)) {
                    Ok(b) => b,
                    Err(e) if e.is_not_found() => continue,
                    Err(e) => {
                        report
                            .errors
                            .push(format!("スナップショット {dev}/{name} を読めません: {e}"));
                        continue;
                    }
                };
                match decode_snapshot(&bytes, dev) {
                    Err(why) => {
                        // 途中のファイル。1 つ古いスナップショットがあれば、それを試す。
                        tracing::debug!(dev = %dev, name, %why, "スナップショットがまだ途中のようなので、あとで確かめる");
                        report.skipped_incomplete += 1;
                    }
                    Ok(SnapshotRead::Unusable(why)) => {
                        tracing::warn!(dev = %dev, name, %why, "使えないスナップショットを飛ばした");
                        report
                            .errors
                            .push(format!("スナップショット {dev}/{name} は使えません: {why}"));
                    }
                    Ok(SnapshotRead::Snapshot(snap)) => {
                        self.apply_snapshot(dev, name, snap, self_id, report, changes)?;
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_snapshot(
        &self,
        dev: &str,
        name: &str,
        snap: DecodedSnapshot,
        self_id: &str,
        report: &mut ImportReport,
        changes: &mut Changes,
    ) -> Result<()> {
        let local_library = self.db.with_conn(|c| sync_state::library_id(c))?;
        if local_library.as_deref() != Some(snap.header.library_id.as_str()) {
            tracing::warn!(
                dev,
                name,
                "別のライブラリのスナップショットなので、使わなかった"
            );
            self.db
                .with_conn(|c| sync_state::mark_snapshot_seen(c, dev, name))?;
            return Ok(());
        }
        let max_hlc = snap
            .entities
            .iter()
            .flat_map(|e| e.fields.values())
            .filter_map(|(_, h)| Hlc::parse(h).ok())
            .chain(Hlc::parse(&snap.header.hlc).ok())
            .max();
        if let Some(h) = &max_hlc {
            self.db.with_conn(|c| {
                let tx = write_tx(c)?;
                observe_hlc(&tx, h)?;
                tx.commit()?;
                Ok(())
            })?;
        }
        // 長く書き込みロックを持たないよう、いくつかずつトランザクションを分ける。
        // 途中で止まっても、項目ごとの LWW なので、状態は正しいまま（次の回に最初から適用し直す）。
        for batch in snap.entities.chunks(self.options.snapshot_batch.max(1)) {
            let changed = self.db.with_conn(|c| {
                let tx = write_tx(c)?;
                let mut changed = Vec::new();
                for e in batch {
                    if apply_entity_state(&tx, e)?.changed() {
                        changed.push((e.ent, e.id.clone()));
                    }
                }
                tx.commit()?;
                Ok(changed)
            })?;
            for (ent, id) in changed {
                changes.add(ent, id);
            }
        }
        report.invalid_dropped += snap.invalid;
        if snap.unknown == 0 {
            self.db.with_conn(|c| {
                let tx = write_tx(c)?;
                for (d, &seq) in &snap.header.covers {
                    if d != self_id && seq > 0 {
                        sync_state::record_applied(&tx, d, 1, seq)?;
                    }
                }
                sync_state::mark_snapshot_seen(&tx, dev, name)?;
                tx.commit()?;
                Ok(())
            })?;
        } else {
            // 知らないエンティティを含む。範囲は記録せず、アプリを更新したあとに適用し直す。
            tracing::warn!(
                dev,
                name,
                unknown = snap.unknown,
                "知らないエンティティを含むスナップショットを、知っている分だけ適用した"
            );
            report.unknown_skipped += 1;
        }
        report.snapshots_applied += 1;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // セグメント
    // -----------------------------------------------------------------------

    fn import_segments(
        &self,
        listing: &RemoteListing,
        report: &mut ImportReport,
        changes: &mut Changes,
        skew: &mut Option<(String, i64)>,
    ) -> Result<()> {
        // スナップショットの適用で範囲が増えたかもしれないので、ここで読み直す。
        let applied = self.db.with_conn(|c| sync_state::applied_ranges(c))?;
        let mut todo: Vec<&SegmentRef> = listing
            .segments
            .values()
            .flatten()
            .filter(|s| !applied.covers(&s.dev, s.from, s.to))
            .collect();
        let limit = self.options.max_files_per_import.max(1);
        if todo.len() > limit {
            report.remaining_files = todo.len() - limit;
            todo.truncate(limit);
        }
        for seg in todo {
            let path = segment_path(&seg.dev, seg.from, seg.to);
            let bytes = match self.transport.read(&path) {
                Ok(b) => b,
                // 読む前に統合されて消えたなど。次の回の一覧で決まる。
                Err(e) if e.is_not_found() => continue,
                Err(e) => {
                    report.errors.push(format!("{path} を読めません: {e}"));
                    continue;
                }
            };
            match decode_segment(&bytes, &seg.dev, seg.from, seg.to) {
                Err(why) => {
                    tracing::debug!(path, %why, "セグメントがまだ途中のようなので、あとで取り込む");
                    report.skipped_incomplete += 1;
                }
                Ok(content) => self.apply_segment(seg, content, report, changes, skew)?,
            }
        }
        Ok(())
    }

    /// セグメント 1 つを、1 つのトランザクションで適用する。
    fn apply_segment(
        &self,
        seg: &SegmentRef,
        content: SegmentContent,
        report: &mut ImportReport,
        changes: &mut Changes,
        skew: &mut Option<(String, i64)>,
    ) -> Result<()> {
        let now = self.now_ms();
        let max_hlc = content
            .ops
            .iter()
            .filter_map(|o| Hlc::parse(&o.hlc).ok())
            .max();
        if let Some(h) = &max_hlc {
            // 他端末の時計が進みすぎていないか（FR-SYNC-11）。適用はする。
            let ahead = h.ms() - now;
            if ahead >= self.options.skew_threshold_ms
                && skew.as_ref().is_none_or(|(_, a)| ahead > *a)
            {
                tracing::warn!(dev = %seg.dev, ahead_ms = ahead, "他端末の時計が進んでいる");
                *skew = Some((seg.dev.clone(), ahead));
            }
        }
        let record = content.unknown == 0;
        let changed = self.db.with_conn(|c| {
            let tx = write_tx(c)?;
            if let Some(h) = &max_hlc {
                observe_hlc(&tx, h)?;
            }
            let mut changed = Vec::new();
            for op in &content.ops {
                if apply_op(&tx, op)?.changed() {
                    changed.push((op.ent, op.id.clone()));
                }
            }
            if record {
                sync_state::record_applied(&tx, &seg.dev, seg.from, seg.to)?;
            }
            tx.commit()?;
            Ok(changed)
        })?;
        for (ent, id) in changed {
            changes.add(ent, id);
        }
        report.files_applied += 1;
        report.ops_seen += content.ops.len();
        report.invalid_dropped += content.invalid;
        if !record {
            tracing::warn!(dev = %seg.dev, name = %seg.name, unknown = content.unknown, "知らない ent・v の Op を含むセグメントを、知っている分だけ適用した");
            report.unknown_skipped += 1;
        }
        Ok(())
    }
}
