//! 同期の状態（`SyncStatus`）の組み立て（0016）。
//!
//! 状態は、(1) 手元の DB から数えられる値（`LocalCounters`。ライブラリ ID・端末 ID・未書き出し・届いていない書影）、
//! (2) 端末ごとの設定（同期フォルダ・端末名）、(3) アプリが覚えている直近の結果（`Live`。最終の取り込み・書き出し、
//! 失敗、時計のずれ、同期フォルダにいる端末の一覧）から作る。ここは純粋な関数で、DB にも同期フォルダにも触れない。

use bibcontainer_core::hlc::ms_to_iso8601;
use bibcontainer_sync::{
    ClockSkew, ExportReport, ImportReport, LocalCounters, SyncDevice, SyncError, SyncState,
    SyncStatus,
};

use super::settings::SyncSettings;

/// 直近の失敗。`state` は、画面に出す状態（`error`・`mismatch`・`updateRequired`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub state: SyncState,
    /// 画面に出せる日本語のメッセージ。
    pub message: String,
}

impl Problem {
    pub fn from_error(err: &SyncError) -> Self {
        Self {
            state: err.state(),
            message: err.to_string(),
        }
    }

    /// 重い方（不一致・更新が必要は、ふつうのエラーより先に伝える）。
    fn severity(&self) -> u8 {
        match self.state {
            SyncState::Mismatch | SyncState::UpdateRequired => 2,
            _ => 1,
        }
    }
}

/// アプリが覚えている、直近の同期の結果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    /// 取り込み・書き出しの最中（短い処理では立てない。長引いたときだけ。`controller::SYNCING_AFTER`）。
    pub syncing: bool,
    /// 最後に取り込みを終えた時刻（UNIX ミリ秒）。
    pub last_import_ms: Option<i64>,
    /// 最後に何かを書き出した時刻。
    pub last_export_ms: Option<i64>,
    /// 取り込み側の失敗。次に取り込みが成功すれば消える。
    pub import_problem: Option<Problem>,
    /// 書き出し側の失敗。次に書き出しが成功すれば消える。
    pub export_problem: Option<Problem>,
    /// 取り込みは続けられたが、一部で起きたエラー（読めなかったファイルなど）。
    pub partial: Option<String>,
    /// 他端末の時計のずれ（5 分以上先）。取り込みで新しく見つかるまで覚えておく。
    pub clock_skew: Option<ClockSkew>,
    /// 同期フォルダにいる端末の一覧。同期フォルダを読むので、取り込みのあとなどに更新して覚えておく。
    pub devices: Vec<SyncDevice>,
    /// `devices` を読んだ時刻（UNIX ミリ秒）。
    pub devices_at_ms: Option<i64>,
}

/// 一部で起きたエラーを、1 つのメッセージにまとめる。
fn summarize_partial(errors: &[String]) -> Option<String> {
    let first = errors.first()?;
    Some(if errors.len() == 1 {
        first.clone()
    } else {
        format!("{first}（ほか {} 件）", errors.len() - 1)
    })
}

impl Live {
    /// 取り込みが成功した（一部のエラーがあっても続けられた）。
    pub fn apply_import(&mut self, report: &ImportReport) {
        self.last_import_ms = Some(report.finished_at_ms);
        self.import_problem = None;
        self.partial = summarize_partial(&report.errors);
        if report.clock_skew.is_some() {
            self.clock_skew.clone_from(&report.clock_skew);
        }
    }

    /// 書き出しが成功した（何も書くものがなくても成功）。
    pub fn apply_export(&mut self, report: &ExportReport) {
        self.export_problem = None;
        if let Some(ms) = report.exported_at_ms {
            self.last_export_ms = Some(ms);
        }
    }

    /// 取り込みが失敗した。メッセージが前と変わったとき true（ログに出す合図）。
    pub fn import_failed(&mut self, err: &SyncError) -> bool {
        let problem = Problem::from_error(err);
        let changed = self.import_problem.as_ref() != Some(&problem);
        self.import_problem = Some(problem);
        changed
    }

    /// 書き出しが失敗した。メッセージが前と変わったとき true。
    pub fn export_failed(&mut self, err: &SyncError) -> bool {
        let problem = Problem::from_error(err);
        let changed = self.export_problem.as_ref() != Some(&problem);
        self.export_problem = Some(problem);
        changed
    }

    /// 端末の一覧を更新した。
    pub fn set_devices(&mut self, devices: Vec<SyncDevice>, now_ms: i64) {
        self.devices = devices;
        self.devices_at_ms = Some(now_ms);
    }

    /// 同期の設定が変わった（設定・解除）ので、前の記録を消す。端末名の変更や時計は残さない。
    pub fn reset(&mut self) {
        *self = Live::default();
    }

    /// いま画面に出す失敗（取り込み側を先に。どちらもあれば、重い方）。
    fn problem(&self) -> Option<&Problem> {
        match (&self.import_problem, &self.export_problem) {
            (Some(a), Some(b)) => Some(if b.severity() > a.severity() { b } else { a }),
            (a, b) => a.as_ref().or(b.as_ref()),
        }
    }

    /// 端末名を変えたので、一覧の自分の行の名前も直す（同期フォルダに書けるまでの間の見え方）。
    pub fn rename_self(&mut self, name: &str) {
        for device in self.devices.iter_mut().filter(|d| d.is_self) {
            device.name = name.to_owned();
        }
    }
}

/// ミリ秒を ISO 8601 の文字列にする（直せなければ None）。
fn iso(ms: i64) -> Option<String> {
    ms_to_iso8601(ms).ok()
}

/// 状態を組み立てる。
///
/// - 設定済みなのは、手元の DB にライブラリ ID があり、かつ同期フォルダの場所が設定にあるとき。どちらかがなければ
///   `disabled`（同期フォルダを選び直すと、同じライブラリなら「再接続」になる。0016）。
/// - `syncing` > 失敗（`error`・`mismatch`・`updateRequired`）> `idle` の順で状態を決める。
/// - 最終の書き出しは、このアプリが覚えている時刻、なければ同期フォルダの自分の端末の記録。
pub fn build_status(
    settings: &SyncSettings,
    device_name: &str,
    counters: &LocalCounters,
    live: &Live,
) -> SyncStatus {
    let folder = settings
        .folder
        .as_ref()
        .filter(|_| counters.library_id.is_some());
    let Some(folder) = folder else {
        return SyncStatus::disabled(counters.device_id.clone(), device_name);
    };
    let problem = live.problem();
    let state = if live.syncing {
        SyncState::Syncing
    } else if let Some(p) = problem {
        p.state
    } else {
        SyncState::Idle
    };
    let error = problem
        .map(|p| p.message.clone())
        .or_else(|| live.partial.clone());
    let last_export_at = live.last_export_ms.and_then(iso).or_else(|| {
        live.devices
            .iter()
            .find(|d| d.is_self)
            .and_then(|d| d.last_export_at.clone())
    });
    SyncStatus {
        state,
        folder: Some(folder.clone()),
        library_id: counters.library_id.clone(),
        device_id: counters.device_id.clone(),
        device_name: device_name.to_owned(),
        last_export_at,
        last_import_at: live.last_import_ms.and_then(iso),
        pending_ops: counters.pending_ops,
        missing_covers: counters.missing_covers,
        error,
        clock_skew: live.clock_skew.clone(),
        devices: live.devices.clone(),
    }
}

#[cfg(test)]
mod tests {
    use bibcontainer_sync::TransportError;
    use bibcontainer_sync::TransportErrorKind;

    use super::*;

    const MS: i64 = 1_791_966_600_000;

    fn counters(library: Option<&str>) -> LocalCounters {
        LocalCounters {
            library_id: library.map(str::to_owned),
            device_id: "d-0123456789ab".to_owned(),
            pending_ops: 3,
            missing_covers: 2,
        }
    }

    fn settings(folder: Option<&str>) -> SyncSettings {
        SyncSettings {
            folder: folder.map(str::to_owned),
            device_name: None,
        }
    }

    fn unavailable() -> SyncError {
        SyncError::Transport(TransportError::new(
            TransportErrorKind::Unavailable,
            "library.json",
            "同期フォルダが見つかりません",
        ))
    }

    fn device(id: &str, is_self: bool) -> SyncDevice {
        SyncDevice {
            device_id: id.to_owned(),
            name: id.to_owned(),
            os: "macos".to_owned(),
            app_version: "0.1.0".to_owned(),
            last_export_at: Some("2026-10-15T01:00:00.000Z".to_owned()),
            is_self,
        }
    }

    #[test]
    fn without_a_library_or_a_folder_the_state_is_disabled() {
        let live = Live::default();
        for (folder, library) in [(None, None), (Some("/f"), None), (None, Some("lib"))] {
            let status = build_status(&settings(folder), "MacBook", &counters(library), &live);
            assert_eq!(status.state, SyncState::Disabled);
            assert_eq!(status.folder, None);
            assert_eq!(status.library_id, None);
            assert_eq!(status.device_id, "d-0123456789ab");
            assert_eq!(status.device_name, "MacBook");
            assert!(status.devices.is_empty());
        }
    }

    #[test]
    fn a_configured_status_carries_counters_times_and_devices() {
        let mut live = Live::default();
        live.apply_import(&ImportReport {
            finished_at_ms: MS,
            ..ImportReport::default()
        });
        live.apply_export(&ExportReport {
            exported_at_ms: Some(MS + 5_000),
            ..ExportReport::default()
        });
        live.set_devices(vec![device("a", true), device("b", false)], MS);
        let status = build_status(
            &settings(Some("/f/lib")),
            "MacBook",
            &counters(Some("L1")),
            &live,
        );
        assert_eq!(status.state, SyncState::Idle);
        assert_eq!(status.folder.as_deref(), Some("/f/lib"));
        assert_eq!(status.library_id.as_deref(), Some("L1"));
        assert_eq!(status.pending_ops, 3);
        assert_eq!(status.missing_covers, 2);
        assert_eq!(status.last_import_at, ms_to_iso8601(MS).ok());
        assert_eq!(status.last_export_at, ms_to_iso8601(MS + 5_000).ok());
        assert_eq!(status.error, None);
        assert_eq!(status.devices.len(), 2);
    }

    #[test]
    fn the_last_export_falls_back_to_the_record_in_the_sync_folder() {
        let mut live = Live::default();
        live.set_devices(vec![device("b", false), device("a", true)], MS);
        let status = build_status(&settings(Some("/f")), "n", &counters(Some("L")), &live);
        assert_eq!(
            status.last_export_at.as_deref(),
            Some("2026-10-15T01:00:00.000Z")
        );
    }

    #[test]
    fn an_export_that_wrote_nothing_keeps_the_previous_export_time() {
        let mut live = Live::default();
        live.apply_export(&ExportReport {
            exported_at_ms: Some(MS),
            ..ExportReport::default()
        });
        live.apply_export(&ExportReport::default());
        assert_eq!(live.last_export_ms, Some(MS));
    }

    #[test]
    fn syncing_beats_a_problem_and_a_problem_beats_idle() {
        let mut live = Live::default();
        assert!(live.import_failed(&unavailable()));
        let status = build_status(&settings(Some("/f")), "n", &counters(Some("L")), &live);
        assert_eq!(status.state, SyncState::Error);
        assert!(
            status
                .error
                .as_deref()
                .is_some_and(|e| e.contains("同期フォルダ")),
            "{:?}",
            status.error
        );
        live.syncing = true;
        let status = build_status(&settings(Some("/f")), "n", &counters(Some("L")), &live);
        assert_eq!(status.state, SyncState::Syncing);
        assert!(status.error.is_some(), "同期中でも、直近の失敗は見える");
    }

    #[test]
    fn mismatch_and_update_required_take_priority_over_a_plain_error() {
        let mut live = Live::default();
        live.import_failed(&unavailable());
        live.export_failed(&SyncError::LibraryMismatch {
            folder: "f".into(),
            local: "l".into(),
        });
        let status = build_status(&settings(Some("/f")), "n", &counters(Some("L")), &live);
        assert_eq!(status.state, SyncState::Mismatch);

        let mut live = Live::default();
        live.import_failed(&SyncError::UpdateRequired {
            required: "9.0.0".into(),
            current: "0.1.0".into(),
        });
        live.export_failed(&unavailable());
        let status = build_status(&settings(Some("/f")), "n", &counters(Some("L")), &live);
        assert_eq!(status.state, SyncState::UpdateRequired);
    }

    #[test]
    fn a_success_clears_only_its_own_side_and_the_same_failure_is_not_new() {
        let mut live = Live::default();
        assert!(live.import_failed(&unavailable()));
        assert!(
            !live.import_failed(&unavailable()),
            "同じ失敗は、ログに出し直さない"
        );
        live.export_failed(&unavailable());
        live.apply_export(&ExportReport::default());
        assert!(live.import_problem.is_some());
        assert!(live.export_problem.is_none());
        live.apply_import(&ImportReport::default());
        assert!(live.import_problem.is_none());
    }

    #[test]
    fn partial_errors_show_as_a_message_while_the_state_stays_idle() {
        let mut live = Live::default();
        live.apply_import(&ImportReport {
            finished_at_ms: MS,
            errors: vec!["書影を取得できませんでした".into(), "b".into(), "c".into()],
            ..ImportReport::default()
        });
        let status = build_status(&settings(Some("/f")), "n", &counters(Some("L")), &live);
        assert_eq!(status.state, SyncState::Idle);
        assert_eq!(
            status.error.as_deref(),
            Some("書影を取得できませんでした（ほか 2 件）")
        );
        // 次の取り込みでエラーがなければ消える
        live.apply_import(&ImportReport::default());
        assert_eq!(live.partial, None);
    }

    #[test]
    fn the_clock_skew_is_kept_until_a_new_one_is_found() {
        let mut live = Live::default();
        let skew = ClockSkew {
            device_id: "b".into(),
            device_name: Some("Windows".into()),
            ahead_ms: 3_600_000,
        };
        live.apply_import(&ImportReport {
            clock_skew: Some(skew.clone()),
            ..ImportReport::default()
        });
        live.apply_import(&ImportReport::default());
        assert_eq!(live.clock_skew, Some(skew));
        live.reset();
        assert_eq!(live.clock_skew, None);
    }

    #[test]
    fn renaming_this_device_updates_only_its_own_row() {
        let mut live = Live::default();
        live.set_devices(vec![device("a", true), device("b", false)], MS);
        live.rename_self("書斎の Mac");
        assert_eq!(live.devices[0].name, "書斎の Mac");
        assert_eq!(live.devices[1].name, "b");
    }
}
