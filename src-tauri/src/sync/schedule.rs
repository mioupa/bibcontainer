//! 同期を「いつ動かすか」の判定（0013 §8、0016）。時刻は呼び出し側が渡す（ミリ秒。起点は何でもよい）ので、
//! 時計にも非同期処理にも触れない純粋な状態機械。スケジューラ（`controller.rs`）が、合図を受けるたびにここへ
//! 伝え、待ち時間と、いま動かすものをここから聞く。
//!
//! | 合図 | 動き |
//! |---|---|
//! | 自分の変更（`on_local_change`） | 最後の変更から 5 秒後に書き出す。変更が続くときも、最初の変更から 30 秒たてば書き出す |
//! | ファイル監視（`on_folder_event`） | 最後の通知から 2 秒まとめて、取り込む。通知が続くときも、最初の通知から 10 秒たてば取り込む |
//! | ウィンドウが前面に来た（`on_focus`） | すぐ取り込む。ただし前回の取り込みの開始から 3 秒はあける |
//! | 続きがある（`request_import`） | すぐ取り込む |
//! | 60 秒ごと | 取り込み、書き出し（未書き出しがなければ何もしない）、手入れの確認 |

/// 最後の変更から、書き出すまで待つ時間（FR-SYNC-07）。
pub const EXPORT_DELAY_MS: u64 = 5_000;
/// 変更が続いても、最初の変更から、書き出すまでに待つ時間の上限。
pub const EXPORT_MAX_WAIT_MS: u64 = 30_000;
/// ファイル監視の通知をまとめる時間（FR-SYNC-06）。
pub const WATCH_DEBOUNCE_MS: u64 = 2_000;
/// 通知が続いても、最初の通知から、取り込むまでに待つ時間の上限。
pub const WATCH_MAX_WAIT_MS: u64 = 10_000;
/// 定期的な取り込みの間隔（FR-SYNC-06）。
pub const IMPORT_INTERVAL_MS: u64 = 60_000;
/// ウィンドウが前面に来たときの取り込みで、前回の取り込みの開始からあける時間。
pub const FOCUS_MIN_GAP_MS: u64 = 3_000;

/// 手入れ（古い一時ファイルの掃除と、自分のセグメントの統合）をやり直す間隔（ミリ秒。壁時計）。
/// 「1 日 1 回ほど」。起動のたびに毎回は走らせず、ずっと起動していても 1 日に 1 回は走る。
pub const MAINTAIN_INTERVAL_MS: i64 = 20 * 60 * 60 * 1000;

/// 手入れの時期か。前に走った時刻（壁時計）がなければ、時計が戻っていれば、間隔が過ぎていれば走らせる。
pub fn maintenance_due(last_ms: Option<i64>, now_ms: i64) -> bool {
    last_ms.is_none_or(|last| now_ms < last || now_ms - last >= MAINTAIN_INTERVAL_MS)
}

/// 続けて起きた合図の、最初と最後の時刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Burst {
    first: u64,
    last: u64,
}

impl Burst {
    fn new(now: u64) -> Self {
        Self {
            first: now,
            last: now,
        }
    }

    /// まとめて待つ時間と、待つ時間の上限から、動かす時刻を決める。
    fn deadline(self, delay: u64, max_wait: u64) -> u64 {
        (self.last + delay).min(self.first + max_wait)
    }
}

/// いま動かすもの。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Due {
    /// 取り込む。
    pub import: bool,
    /// 書き出す（未書き出しがなければ、エンジンは何もしない）。
    pub export: bool,
    /// 定期の確認の時刻だった（手入れの確認、監視の張り直しの確認をする）。
    pub tick: bool,
}

impl Due {
    /// 何か動かすか。
    pub fn any(self) -> bool {
        self.import || self.export || self.tick
    }
}

/// 同期の動かす時機。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// 書き出しの待ち（自分の変更が続いている間）。
    export: Option<Burst>,
    /// 取り込みの待ち（ファイル監視の通知が続いている間）。
    watch: Option<Burst>,
    /// 取り込みを動かす時刻の希望（前面に来た・続きがある）。
    import_at: Option<u64>,
    /// 前回、取り込みを始めた時刻。
    last_import: Option<u64>,
    /// 次の定期の確認の時刻。
    next_tick: u64,
}

impl Schedule {
    /// `now` に始める。最初の定期の確認は、`now` から 60 秒後。
    pub fn new(now: u64) -> Self {
        Self {
            export: None,
            watch: None,
            import_at: None,
            last_import: None,
            next_tick: now + IMPORT_INTERVAL_MS,
        }
    }

    /// 自分の変更があった。
    pub fn on_local_change(&mut self, now: u64) {
        match &mut self.export {
            Some(burst) => burst.last = now,
            None => self.export = Some(Burst::new(now)),
        }
    }

    /// 同期フォルダの中で、ファイルが変わった。
    pub fn on_folder_event(&mut self, now: u64) {
        match &mut self.watch {
            Some(burst) => burst.last = now,
            None => self.watch = Some(Burst::new(now)),
        }
    }

    /// ウィンドウが前面に来た。すぐ取り込む（前回の取り込みの開始から `FOCUS_MIN_GAP_MS` はあける）。
    pub fn on_focus(&mut self, now: u64) {
        let earliest = self
            .last_import
            .map_or(now, |last| (last + FOCUS_MIN_GAP_MS).max(now));
        self.import_at = Some(self.import_at.map_or(earliest, |at| at.min(earliest)));
    }

    /// すぐ取り込みたい（取り込みの続きがある、設定が済んだ、など）。
    pub fn request_import(&mut self, now: u64) {
        self.import_at = Some(self.import_at.map_or(now, |at| at.min(now)));
    }

    fn export_deadline(&self) -> Option<u64> {
        self.export
            .map(|b| b.deadline(EXPORT_DELAY_MS, EXPORT_MAX_WAIT_MS))
    }

    fn watch_deadline(&self) -> Option<u64> {
        self.watch
            .map(|b| b.deadline(WATCH_DEBOUNCE_MS, WATCH_MAX_WAIT_MS))
    }

    /// 次に動かす時刻（何かを待っているとき、いちばん早いもの。定期の確認があるので、いつも `Some`）。
    pub fn next_wake(&self) -> Option<u64> {
        [
            self.export_deadline(),
            self.watch_deadline(),
            self.import_at,
            Some(self.next_tick),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// `now` に動かすものを取り出す（取り出したものは、待ちから外す）。
    pub fn take_due(&mut self, now: u64) -> Due {
        let tick = now >= self.next_tick;
        let import = tick
            || self.import_at.is_some_and(|at| at <= now)
            || self.watch_deadline().is_some_and(|at| at <= now);
        // 自分の変更を待っているときは、その時刻まで書き出さない（定期の確認では書き出さない）。
        // 待っていないときの定期の確認は、前の書き出しの失敗のやり直しも兼ねる。
        let export = match self.export_deadline() {
            Some(at) => at <= now,
            None => tick,
        };
        if import {
            self.import_at = None;
            self.watch = None;
            self.last_import = Some(now);
        }
        if export {
            self.export = None;
        }
        if tick {
            self.next_tick = now + IMPORT_INTERVAL_MS;
        }
        Due {
            import,
            export,
            tick,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;

    #[test]
    fn nothing_but_the_tick_is_scheduled_at_first() {
        let s = Schedule::new(T0);
        assert_eq!(s.next_wake(), Some(T0 + IMPORT_INTERVAL_MS));
        let mut s = s;
        assert_eq!(s.take_due(T0 + 1_000), Due::default());
    }

    #[test]
    fn a_change_is_exported_five_seconds_after_the_last_change() {
        let mut s = Schedule::new(T0);
        s.on_local_change(T0);
        assert_eq!(s.next_wake(), Some(T0 + EXPORT_DELAY_MS));
        // 3 秒後にもう 1 回変更 → 待ちが延びる
        s.on_local_change(T0 + 3_000);
        assert_eq!(s.next_wake(), Some(T0 + 3_000 + EXPORT_DELAY_MS));
        assert!(!s.take_due(T0 + 7_999).export);
        let due = s.take_due(T0 + 8_000);
        assert!(due.export);
        assert!(!due.import && !due.tick);
        // 取り出したら、待ちは残らない
        assert_eq!(s.next_wake(), Some(T0 + IMPORT_INTERVAL_MS));
        assert!(!s.take_due(T0 + 20_000).export);
    }

    #[test]
    fn continuous_changes_are_exported_at_the_maximum_wait() {
        let mut s = Schedule::new(T0);
        // 2 秒おきに変更し続ける。5 秒の待ちは、いつまでも来ない
        for i in 0..=14 {
            s.on_local_change(T0 + i * 2_000);
        }
        // 最後の変更は T0 + 28 秒。5 秒後は 33 秒だが、最初の変更から 30 秒で書き出す
        assert_eq!(s.next_wake(), Some(T0 + EXPORT_MAX_WAIT_MS));
        assert!(!s.take_due(T0 + EXPORT_MAX_WAIT_MS - 1).export);
        assert!(s.take_due(T0 + EXPORT_MAX_WAIT_MS).export);
    }

    #[test]
    fn folder_events_are_coalesced_for_two_seconds() {
        let mut s = Schedule::new(T0);
        s.on_folder_event(T0);
        s.on_folder_event(T0 + 1_500);
        assert_eq!(s.next_wake(), Some(T0 + 1_500 + WATCH_DEBOUNCE_MS));
        assert!(!s.take_due(T0 + 3_499).import);
        let due = s.take_due(T0 + 3_500);
        assert!(due.import);
        assert!(!due.export, "取り込みだけ。書き出しは自分の変更を待つ");
    }

    #[test]
    fn a_long_stream_of_folder_events_still_imports_at_the_maximum_wait() {
        let mut s = Schedule::new(T0);
        for i in 0..=20 {
            s.on_folder_event(T0 + i * 1_000);
        }
        assert_eq!(s.next_wake(), Some(T0 + WATCH_MAX_WAIT_MS));
        assert!(s.take_due(T0 + WATCH_MAX_WAIT_MS).import);
    }

    #[test]
    fn focus_imports_now_but_keeps_a_gap_after_the_last_import() {
        let mut s = Schedule::new(T0);
        s.on_focus(T0 + 100);
        assert_eq!(s.next_wake(), Some(T0 + 100), "初めてなら、すぐ");
        assert!(s.take_due(T0 + 100).import);

        // すぐあとにもう一度前面に来ても、3 秒はあける
        s.on_focus(T0 + 1_000);
        assert_eq!(s.next_wake(), Some(T0 + 100 + FOCUS_MIN_GAP_MS));
        assert!(!s.take_due(T0 + 3_000).import);
        assert!(s.take_due(T0 + 3_100).import);

        // 十分あいていれば、すぐ
        s.on_focus(T0 + 20_000);
        assert_eq!(s.next_wake(), Some(T0 + 20_000));
    }

    #[test]
    fn requesting_an_import_runs_it_now_and_the_earliest_request_wins() {
        let mut s = Schedule::new(T0);
        s.on_focus(T0);
        assert!(s.take_due(T0).import);
        s.on_focus(T0 + 500); // 3 秒あけるので T0 + 3000
        s.request_import(T0 + 600); // 続きがある → すぐ
        assert_eq!(s.next_wake(), Some(T0 + 600));
        assert!(s.take_due(T0 + 600).import);
        assert_eq!(s.next_wake(), Some(T0 + IMPORT_INTERVAL_MS));
    }

    #[test]
    fn the_tick_imports_and_exports_unless_a_change_is_being_waited_for() {
        let mut s = Schedule::new(T0);
        let due = s.take_due(T0 + IMPORT_INTERVAL_MS);
        assert_eq!(
            due,
            Due {
                import: true,
                export: true,
                tick: true
            },
            "待っている変更がなければ、前の書き出しの失敗のやり直しを兼ねて書き出す"
        );
        assert_eq!(s.next_wake(), Some(T0 + 2 * IMPORT_INTERVAL_MS));

        // 変更を待っている間の定期の確認は、書き出さない（まとめ書きを崩さない）
        let tick_at = T0 + 2 * IMPORT_INTERVAL_MS;
        s.on_local_change(tick_at - 1_000);
        let due = s.take_due(tick_at);
        assert!(due.import && due.tick);
        assert!(!due.export);
        assert!(s.take_due(tick_at + 4_000).export, "待ちの時刻には書き出す");
    }

    #[test]
    fn an_import_clears_pending_watch_and_request_but_not_the_export_wait() {
        let mut s = Schedule::new(T0);
        s.on_folder_event(T0);
        s.on_local_change(T0);
        s.request_import(T0);
        let due = s.take_due(T0);
        assert!(due.import);
        assert!(!due.export);
        assert_eq!(s.next_wake(), Some(T0 + EXPORT_DELAY_MS));
    }

    #[test]
    fn maintenance_runs_when_never_run_after_the_interval_or_when_the_clock_went_back() {
        let now = 1_800_000_000_000_i64;
        assert!(maintenance_due(None, now));
        assert!(!maintenance_due(Some(now - 60_000), now));
        assert!(!maintenance_due(Some(now - MAINTAIN_INTERVAL_MS + 1), now));
        assert!(maintenance_due(Some(now - MAINTAIN_INTERVAL_MS), now));
        assert!(
            maintenance_due(Some(now + 5), now),
            "時計が戻ったら走らせる"
        );
    }
}
