//! ハイブリッド論理時計（HLC）。
//!
//! 文字列形式は `{ミリ秒 15 桁}:{カウンタ 5 桁}:{device_id}`（ゼロ埋め）。
//! 桁数が固定なので、文字列として比較すると論理順（ミリ秒、カウンタ、端末 ID の順）と一致する。

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::ids::is_valid_device_id;

/// ミリ秒の最大値（15 桁に収まる範囲）。
pub const MAX_MS: i64 = 999_999_999_999_999;
/// カウンタの最大値（5 桁に収まる範囲）。
pub const MAX_COUNTER: u32 = 99_999;

/// 現在時刻の供給元。テストでは固定・逆行する時計に差し替える。
pub trait Clock {
    /// UNIX エポックからのミリ秒。
    fn now_ms(&self) -> i64;
}

/// 実際の壁時計。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}

/// 手で動かせる時計（テストや再現用）。
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    /// 指定したミリ秒から始める。
    pub fn new(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    /// 時刻を直接設定する（過去へ戻してもよい）。
    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }

    /// 時刻を進める（負の値なら戻す）。
    pub fn advance(&self, delta_ms: i64) {
        self.0.fetch_add(delta_ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// 端末が最後に採番した（または観測した）時計の値。端末 ID を含まない。
///
/// `local_meta` に `{ミリ秒 15 桁}:{カウンタ 5 桁}` の形で保存する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct HlcState {
    /// UNIX エポックからのミリ秒。
    pub ms: i64,
    /// 同じミリ秒の中での通し番号。
    pub counter: u32,
}

impl HlcState {
    /// 何も採番していない状態。
    pub const ZERO: HlcState = HlcState { ms: 0, counter: 0 };

    /// ローカルの次の値を返す。
    ///
    /// `ms = max(現在時刻, 前回の ms)`。ms が前回と同じならカウンタを 1 増やし、違えば 0 にする。
    /// カウンタが上限を超える場合は ms を 1 進めてカウンタを 0 にする。
    pub fn tick(self, now_ms: i64) -> HlcState {
        let ms = now_ms.max(self.ms);
        if ms != self.ms {
            HlcState { ms, counter: 0 }
        } else if self.counter >= MAX_COUNTER {
            HlcState {
                ms: ms + 1,
                counter: 0,
            }
        } else {
            HlcState {
                ms,
                counter: self.counter + 1,
            }
        }
    }

    /// 他端末の HLC を観測して、自分の値を `max(自分, 受け取った値)` に進める。
    /// この後の `tick` は、受け取った値より必ず大きい値を返す。
    pub fn observe(self, remote: &Hlc) -> HlcState {
        self.max(remote.state())
    }
}

impl fmt::Display for HlcState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:015}:{:05}", self.ms, self.counter)
    }
}

impl FromStr for HlcState {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let (ms, counter) = s
            .split_once(':')
            .ok_or_else(|| Error::InvalidHlc(s.to_owned()))?;
        parse_parts(ms, counter, s).map(|(ms, counter)| HlcState { ms, counter })
    }
}

/// 完成した HLC（端末 ID つき）。`Ord` は文字列順と一致する。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hlc {
    ms: i64,
    counter: u32,
    device: String,
}

impl Hlc {
    /// 範囲と端末 ID の書式を検証して作る。
    pub fn new(ms: i64, counter: u32, device: impl Into<String>) -> Result<Self> {
        let device = device.into();
        if !(0..=MAX_MS).contains(&ms) || counter > MAX_COUNTER || !is_valid_device_id(&device) {
            return Err(Error::InvalidHlc(format!("{ms}:{counter}:{device}")));
        }
        Ok(Self {
            ms,
            counter,
            device,
        })
    }

    /// `{ms:015}:{counter:05}:{device}` を厳密に検証してパースする。
    pub fn parse(s: &str) -> Result<Self> {
        let invalid = || Error::InvalidHlc(s.to_owned());
        let mut parts = s.splitn(3, ':');
        let (Some(ms), Some(counter), Some(device)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(invalid());
        };
        let (ms, counter) = parse_parts(ms, counter, s)?;
        if !is_valid_device_id(device) {
            return Err(invalid());
        }
        Ok(Self {
            ms,
            counter,
            device: device.to_owned(),
        })
    }

    /// 時計の値と端末 ID から作る。
    pub fn from_state(state: HlcState, device: impl Into<String>) -> Result<Self> {
        Self::new(state.ms, state.counter, device)
    }

    /// ミリ秒。
    pub fn ms(&self) -> i64 {
        self.ms
    }

    /// カウンタ。
    pub fn counter(&self) -> u32 {
        self.counter
    }

    /// 端末 ID。
    pub fn device(&self) -> &str {
        &self.device
    }

    /// 端末 ID を除いた (ms, counter)。
    pub fn state(&self) -> HlcState {
        HlcState {
            ms: self.ms,
            counter: self.counter,
        }
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:015}:{:05}:{}", self.ms, self.counter, self.device)
    }
}

impl FromStr for Hlc {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

fn parse_parts(ms: &str, counter: &str, whole: &str) -> Result<(i64, u32)> {
    let all_digits = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(ms, 15) || !all_digits(counter, 5) {
        return Err(Error::InvalidHlc(whole.to_owned()));
    }
    // 桁数を検証済みなので、どちらもオーバーフローしない。
    let ms = ms.parse().expect("15 桁の数字は i64 に収まる");
    let counter = counter.parse().expect("5 桁の数字は u32 に収まる");
    Ok((ms, counter))
}

/// HLC のミリ秒を ISO 8601 の UTC ミリ秒表記（例: `2026-10-14T08:30:00.123Z`）にする。
/// `updated_at` の値に使う。
pub fn ms_to_iso8601(ms: i64) -> Result<String> {
    let dt = chrono::DateTime::from_timestamp_millis(ms)
        .ok_or_else(|| Error::InvalidHlc(format!("ミリ秒が範囲外: {ms}")))?;
    Ok(dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV_A: &str = "d-aaaaaaaaaaaa";
    const DEV_B: &str = "d-bbbbbbbbbbbb";

    fn hlc(ms: i64, counter: u32, dev: &str) -> Hlc {
        Hlc::new(ms, counter, dev).unwrap()
    }

    #[test]
    fn format_matches_the_spec_example() {
        let h = hlc(1_760_412_345_678, 0, "d-7f3a2c");
        assert_eq!(h.to_string(), "001760412345678:00000:d-7f3a2c");
    }

    #[test]
    fn display_and_parse_roundtrip() {
        for h in [
            hlc(0, 0, DEV_A),
            hlc(1_760_412_345_678, 42, DEV_B),
            hlc(MAX_MS, MAX_COUNTER, DEV_A),
        ] {
            let s = h.to_string();
            assert_eq!(Hlc::parse(&s).unwrap(), h);
            assert_eq!(s.parse::<Hlc>().unwrap(), h);
            assert_eq!(Hlc::parse(&s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn parse_rejects_malformed_strings() {
        for bad in [
            "",
            "x",
            "001760412345678:00000",      // 端末 ID なし
            "001760412345678:00000:",     // 端末 ID が空
            "001760412345678:00000:d:1",  // 端末 ID に ':'
            "01760412345678:00000:d-a",   // ms が 14 桁
            "0001760412345678:00000:d-a", // ms が 16 桁
            "001760412345678:0000:d-a",   // カウンタが 4 桁
            "001760412345678:000000:d-a", // カウンタが 6 桁
            "00176041234567x:00000:d-a",  // 数字以外
            "-01760412345678:00000:d-a",  // 符号
            "+01760412345678:00000:d-a",  // 符号
            "001760412345678:00000:d a",  // 空白
            " 001760412345678:00000:d-a", // 先頭の空白
        ] {
            assert!(
                matches!(Hlc::parse(bad), Err(Error::InvalidHlc(_))),
                "{bad:?} はエラーになるべき"
            );
        }
    }

    #[test]
    fn new_rejects_out_of_range() {
        assert!(Hlc::new(-1, 0, DEV_A).is_err());
        assert!(Hlc::new(MAX_MS + 1, 0, DEV_A).is_err());
        assert!(Hlc::new(0, MAX_COUNTER + 1, DEV_A).is_err());
        assert!(Hlc::new(0, 0, "a:b").is_err());
    }

    #[test]
    fn string_order_matches_logical_order() {
        let ordered = [
            hlc(9, 99_999, DEV_B),
            hlc(10, 0, DEV_A),
            hlc(10, 0, DEV_B),
            hlc(10, 1, DEV_A),
            hlc(10, 9, DEV_A),
            hlc(10, 10, DEV_A), // 桁数が違っても '9' < '10' で逆転しない
            hlc(10, 99_999, DEV_A),
            hlc(11, 0, DEV_A),
            hlc(999_999_999_999_999, 0, DEV_A),
        ];
        for pair in ordered.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            assert!(a < b, "論理順: {a} < {b}");
            assert!(a.to_string() < b.to_string(), "文字列順: {a} < {b}");
        }
    }

    #[test]
    fn state_roundtrip_and_order() {
        let s = HlcState {
            ms: 1_760_412_345_678,
            counter: 7,
        };
        assert_eq!(s.to_string(), "001760412345678:00007");
        assert_eq!(s.to_string().parse::<HlcState>().unwrap(), s);
        assert!("001760412345678".parse::<HlcState>().is_err());
        assert!("001760412345678:00007:d-a".parse::<HlcState>().is_err());
        assert!(HlcState { ms: 5, counter: 99 } < HlcState { ms: 6, counter: 0 });
    }

    #[test]
    fn counter_increments_within_the_same_ms() {
        let s0 = HlcState::ZERO.tick(1000);
        assert_eq!(
            s0,
            HlcState {
                ms: 1000,
                counter: 0
            }
        );
        let s1 = s0.tick(1000);
        let s2 = s1.tick(1000);
        assert_eq!(
            s1,
            HlcState {
                ms: 1000,
                counter: 1
            }
        );
        assert_eq!(
            s2,
            HlcState {
                ms: 1000,
                counter: 2
            }
        );
    }

    #[test]
    fn counter_resets_when_ms_advances() {
        let s = HlcState::ZERO.tick(1000).tick(1000).tick(1000);
        assert_eq!(s.counter, 2);
        assert_eq!(
            s.tick(1001),
            HlcState {
                ms: 1001,
                counter: 0
            }
        );
    }

    #[test]
    fn stays_monotonic_when_the_wall_clock_goes_backwards() {
        let mut state = HlcState::ZERO;
        let mut prev = state;
        for now in [5000, 5001, 4000, 4000, 100, 5001, 5001, 6000, 0, -5] {
            state = state.tick(now);
            assert!(state > prev, "{prev:?} -> {state:?}（now={now}）");
            prev = state;
        }
        assert_eq!(state.ms, 6000);
    }

    #[test]
    fn counter_overflow_carries_into_ms() {
        let at_max = HlcState {
            ms: 1000,
            counter: MAX_COUNTER,
        };
        let next = at_max.tick(1000);
        assert_eq!(
            next,
            HlcState {
                ms: 1001,
                counter: 0
            }
        );
        assert!(next > at_max);
        // 繰り上がった後に壁時計が追いついていなくても、さらに続けて単調に増える。
        let after = next.tick(1000);
        assert_eq!(
            after,
            HlcState {
                ms: 1001,
                counter: 1
            }
        );
        // 壁時計が先に進んでいれば、そちらが優先される。
        assert_eq!(
            at_max.tick(2000),
            HlcState {
                ms: 2000,
                counter: 0
            }
        );
        // 繰り上がった値は 15 桁・5 桁に収まり、文字列順も保たれる。
        assert!(
            Hlc::from_state(next, DEV_A).unwrap().to_string()
                > Hlc::from_state(at_max, DEV_A).unwrap().to_string()
        );
    }

    #[test]
    fn local_value_after_observing_a_remote_one_is_greater() {
        let remote = hlc(9_000, 3, DEV_B);

        // 壁時計が遅れている端末（ms は相手に引きずられる）
        let local = HlcState {
            ms: 1_000,
            counter: 0,
        }
        .observe(&remote);
        assert_eq!(local, remote.state());
        let next = Hlc::from_state(local.tick(1_000), DEV_A).unwrap();
        assert!(next > remote, "{next} > {remote}");

        // 壁時計が進んでいる端末
        let next = Hlc::from_state(HlcState::ZERO.observe(&remote).tick(20_000), DEV_A).unwrap();
        assert!(next > remote);

        // 自分の値の方が新しければ変わらない
        let ahead = HlcState {
            ms: 10_000,
            counter: 5,
        };
        assert_eq!(ahead.observe(&remote), ahead);

        // (ms, counter) が相手と同じでも、端末 ID に関係なく次の値は大きい
        let same = HlcState {
            ms: 9_000,
            counter: 3,
        };
        let next = Hlc::from_state(same.observe(&remote).tick(0), DEV_A).unwrap();
        assert!(next > remote);
    }

    #[test]
    fn iso8601_millis() {
        assert_eq!(ms_to_iso8601(0).unwrap(), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            ms_to_iso8601(1_760_430_600_123).unwrap(),
            "2025-10-14T08:30:00.123Z"
        );
        // 文字列順が時刻順と一致する
        assert!(ms_to_iso8601(1_000).unwrap() < ms_to_iso8601(1_001).unwrap());
    }

    #[test]
    fn manual_clock_can_go_backwards() {
        let c = ManualClock::new(100);
        c.advance(50);
        assert_eq!(c.now_ms(), 150);
        c.set(10);
        assert_eq!(c.now_ms(), 10);
    }

    #[test]
    fn system_clock_is_plausible() {
        // 2020-01-01 より後
        assert!(SystemClock.now_ms() > 1_577_836_800_000);
    }
}
