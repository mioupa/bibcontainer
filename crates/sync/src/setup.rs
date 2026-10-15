//! 初めての設定（FR-SYNC-09）、フォルダの中身の判定、解除（0013 §7）。

use bibcontainer_core::db::write_tx;
use bibcontainer_core::ids::new_entity_id;
use bibcontainer_core::sync_state;

use crate::db::SyncDb;
use crate::engine::SyncEngine;
use crate::error::{Result, SyncError};
use crate::format::LibraryFile;
use crate::import::ImportReport;
use crate::layout::LIBRARY_FILE;
use crate::status::{FolderInspection, FolderKind, SetupMode};
use crate::transport::SyncTransport;

/// 設定の結果。
#[derive(Debug, Clone, PartialEq)]
pub struct SetupReport {
    /// 参加したライブラリの ID。
    pub library_id: String,
    /// 新しいライブラリを作ったか。
    pub created: bool,
    /// 手元の状態をスナップショットとして書いたか。
    pub snapshot_written: bool,
    /// 設定の最後の取り込みの結果（新しいライブラリを作ったときは `None`）。
    pub import: Option<ImportReport>,
}

/// 同期を解除する。ライブラリ ID・取り込み済みの範囲・「スナップショットが必要」の印を消す。
/// 手元のデータと同期フォルダの中身はそのまま。同期フォルダには触れないので、フォルダが使えなくても呼べる。
pub fn leave<D: SyncDb>(db: &D) -> Result<()> {
    db.with_conn(|c| {
        let tx = write_tx(c)?;
        sync_state::leave_library(&tx)?;
        tx.commit()?;
        Ok(())
    })
}

/// 設定の取り込みで繰り返す回数の上限（1 回 `max_files_per_import` 個のファイル）。
const SETUP_IMPORT_ROUNDS: usize = 1000;

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 選んだフォルダの中身を調べる。`library_folder` は、画面に出す `bibcontainer-library` の絶対パス
    /// （`LocalFolder::resolve_root` の結果）。同期フォルダには何も残さない。
    ///
    /// 判定の順は、更新が必要 > 別のライブラリ > 書き込めない > ライブラリあり／空。
    pub fn inspect(&self, library_folder: &str) -> Result<FolderInspection> {
        let (local_library_id, local_has_data) = self
            .db
            .with_conn(|c| Ok((sync_state::library_id(c)?, sync_state::has_local_data(c)?)))?;
        let writable = self.transport.check_writable().is_ok();
        let mut inspection = FolderInspection {
            kind: FolderKind::Empty,
            library_folder: library_folder.to_owned(),
            library_id: None,
            devices: Vec::new(),
            local_has_data,
            local_library_id: local_library_id.clone(),
        };
        match self.read_library_file()? {
            None => {
                if !writable {
                    inspection.kind = FolderKind::NotWritable;
                }
            }
            Some(lib) => {
                inspection.devices = self.devices()?;
                inspection.kind = if self.check_compat(&lib).is_err() {
                    FolderKind::UpdateRequired
                } else if local_library_id
                    .as_deref()
                    .is_some_and(|l| l != lib.library_id)
                {
                    FolderKind::OtherLibrary
                } else if !writable {
                    FolderKind::NotWritable
                } else {
                    FolderKind::Library
                };
                inspection.library_id = Some(lib.library_id);
            }
        }
        Ok(inspection)
    }

    /// 同期フォルダを設定する。
    ///
    /// - `Create`: フォルダにライブラリがないとき、`library.json` を書き、ライブラリ ID を記録し、
    ///   手元にデータがあればスナップショットを書く。
    /// - `Join`: ライブラリがあり、手元が空のとき、ライブラリ ID を記録して取り込む。
    /// - `JoinMerge`: ライブラリがあり、手元にデータがあるとき、ライブラリ ID を記録し、手元の状態を
    ///   スナップショットとして書いてから取り込む。両方の本がすべて残る（同じ本が別の ID で 2 冊になることがある）。
    /// - `JoinReplace`: アプリが手元の DB をバックアップして作り直した（新しい空の DB、新しい端末 ID）あとで、
    ///   `Join` と同じ手順で参加する。手元にデータがあるままでは `LocalHasData`。
    ///
    /// この端末が既に同じライブラリに参加しているなら（フォルダの場所を変えたなど）、記録を引き継いで取り込み直す。
    /// 別のライブラリに参加していれば、`LibraryMismatch`（先に `leave`）。
    pub fn setup(&self, mode: SetupMode) -> Result<SetupReport> {
        let (local_id, has_data, flag) = self.db.with_conn(|c| {
            Ok((
                sync_state::library_id(c)?,
                sync_state::has_local_data(c)?,
                sync_state::needs_snapshot(c)?,
            ))
        })?;
        let folder = self.read_library_file()?;
        match (mode, folder) {
            (SetupMode::Create, Some(_)) => Err(SyncError::LibraryExists),
            (SetupMode::Create, None) => {
                if let Some(local) = local_id {
                    return Err(SyncError::AlreadyInLibrary { local });
                }
                self.create_library(has_data)
            }
            (_, None) => Err(SyncError::LibraryMissing),
            (_, Some(lib)) => {
                self.check_compat(&lib)?;
                let same = local_id.as_deref() == Some(lib.library_id.as_str());
                if let Some(local) = &local_id
                    && !same
                {
                    return Err(SyncError::LibraryMismatch {
                        folder: lib.library_id,
                        local: local.clone(),
                    });
                }
                if !same && has_data && mode != SetupMode::JoinMerge {
                    return Err(SyncError::LocalHasData);
                }
                // 同じ端末 ID の別の端末（データフォルダの丸ごとコピーなど）に、何も書き込む前に気づく。
                let self_id = self.device_id()?;
                self.check_own_folder(&self_id)?;
                // 同じライブラリへの再設定では、既にある印（復元のあとなど）を引き継ぐ。
                let needs = if same {
                    flag
                } else {
                    mode == SetupMode::JoinMerge && has_data
                };
                self.db.with_conn(|c| {
                    let tx = write_tx(c)?;
                    sync_state::join_library(&tx, &lib.library_id, needs)?;
                    tx.commit()?;
                    Ok(())
                })?;
                if needs {
                    self.write_snapshot()?;
                }
                self.write_device_file(None)?;
                let import = self.import_all(SETUP_IMPORT_ROUNDS)?;
                Ok(SetupReport {
                    library_id: lib.library_id,
                    created: false,
                    snapshot_written: needs,
                    import: Some(import),
                })
            }
        }
    }

    fn create_library(&self, has_data: bool) -> Result<SetupReport> {
        let self_id = self.device_id()?;
        let library_id = new_entity_id();
        let file = LibraryFile::new(
            library_id.clone(),
            bibcontainer_core::hlc::ms_to_iso8601(self.now_ms())?,
            self_id,
        );
        self.transport
            .write_atomic(LIBRARY_FILE, &serde_json::to_vec_pretty(&file)?)?;
        // 同時に別の端末が作った場合は、あとから書いた方が残る。書いたあとに読み直して、自分のものか確かめる。
        match self.read_library_file()? {
            Some(now) if now.library_id == library_id => {}
            _ => return Err(SyncError::LibraryExists),
        }
        self.db.with_conn(|c| {
            let tx = write_tx(c)?;
            sync_state::join_library(&tx, &library_id, has_data)?;
            tx.commit()?;
            Ok(())
        })?;
        if has_data {
            self.write_snapshot()?;
        }
        self.write_device_file(None)?;
        Ok(SetupReport {
            library_id,
            created: true,
            snapshot_written: has_data,
            import: None,
        })
    }

    /// 同期を解除する（`leave` と同じ。同期フォルダを使えなくても解除できるよう、フォルダには触れない）。
    pub fn leave(&self) -> Result<()> {
        leave(self.db)
    }

    /// 手元のデータの件数（「置き換える」前の確認の表示用）。
    pub fn local_summary(&self) -> Result<sync_state::DataSummary> {
        self.db.with_conn(|c| sync_state::data_summary(c))
    }

    /// 1 回分の同期（アプリが「今すぐ同期」や定期実行で呼ぶ）。
    ///
    /// 1. 「スナップショットが必要」の印があれば、先にスナップショットを書く（復元のあとの再開など）。
    /// 2. 取り込む（読むべきファイルがなくなるまで、`max_rounds` 回まで）。
    /// 3. 書き出す。
    ///
    /// 取り込みで `Err`（不一致・更新が必要・端末 ID の重複など）になれば、書き出しはしない。
    pub fn sync(&self, max_rounds: usize) -> Result<SyncReport> {
        self.check_library()?;
        let needs = self.db.with_conn(|c| sync_state::needs_snapshot(c))?;
        let mut snapshot_first = None;
        if needs {
            snapshot_first = Some(self.write_snapshot()?);
        }
        let import = self.import_all(max_rounds)?;
        let mut export = self.export()?;
        if snapshot_first.is_some() {
            export.snapshot_written = true;
        }
        Ok(SyncReport { import, export })
    }
}

/// `sync` の結果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncReport {
    /// 取り込みの結果。
    pub import: ImportReport,
    /// 書き出しの結果。
    pub export: crate::export::ExportReport,
}
