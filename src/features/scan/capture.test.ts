import { describe, expect, it } from "vitest";
import {
  EMPTY_CAPTURE,
  MAX_CAPTURE_DIGITS,
  captureIdle,
  captureKey,
  configForMode,
  digitFromCode,
  stripAffixes,
  type CaptureKey,
  type CaptureState,
  type ScanCaptureConfig,
} from "@/features/scan/capture";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";

const base: ScanCaptureConfig = {
  terminator: "enter",
  maxGapMs: 30,
  minLength: 8,
  idleCommitMs: 100,
  stripLeading: 0,
  stripTrailing: 0,
};

const key = (code: string, at: number, extra: Partial<CaptureKey> = {}): CaptureKey => ({
  code,
  at,
  repeat: false,
  modified: false,
  ...extra,
});
const digitCode = (d: string) => `Digit${d}`;

/** 数字列を interval ミリ秒間隔で打ち、最後に末尾キーを打つ。最後のステップを返す。 */
function type(
  digits: string,
  config: ScanCaptureConfig,
  options: { interval?: number; start?: number; end?: string; state?: CaptureState } = {},
) {
  const interval = options.interval ?? 5;
  let at = options.start ?? 1000;
  let state = options.state ?? EMPTY_CAPTURE;
  const swallowed: boolean[] = [];
  let last = { state, swallow: false, scan: null as string | null };
  for (const d of digits) {
    last = captureKey(state, key(digitCode(d), at), config);
    state = last.state;
    swallowed.push(last.swallow);
    at += interval;
  }
  if (options.end !== undefined) {
    last = captureKey(state, key(options.end, at), config);
    swallowed.push(last.swallow);
  }
  return { ...last, swallowed };
}

describe("digitFromCode", () => {
  it("Digit0〜9 と Numpad0〜9 だけが数字", () => {
    expect(digitFromCode("Digit0")).toBe("0");
    expect(digitFromCode("Digit9")).toBe("9");
    expect(digitFromCode("Numpad5")).toBe("5");
    for (const code of [
      "KeyA",
      "Enter",
      "Tab",
      "NumpadEnter",
      "NumpadAdd",
      "Minus",
      "Digit",
      "Digit10",
      "",
    ]) {
      expect(digitFromCode(code)).toBeNull();
    }
  });
});

describe("captureKey（末尾キー Enter）", () => {
  it("数字列＋Enter で確定し、数字と Enter の既定の動作を止める（FR-SCAN-07）", () => {
    const step = type("9784003101117", base, { end: "Enter" });
    expect(step.scan).toBe("9784003101117");
    expect(step.swallowed.every(Boolean)).toBe(true);
    expect(step.state).toEqual(EMPTY_CAPTURE);
  });

  it("テンキーの数字と NumpadEnter でも読める", () => {
    let state = EMPTY_CAPTURE;
    let at = 0;
    for (const d of "12345678") {
      state = captureKey(state, key(`Numpad${d}`, (at += 5)), base).state;
    }
    const step = captureKey(state, key("NumpadEnter", at + 5), base);
    expect(step.scan).toBe("12345678");
  });

  it("末尾キーが違う（Tab）ときは確定せず、止めもしない", () => {
    const step = type("9784003101117", base, { end: "Tab" });
    expect(step.scan).toBeNull();
    expect(step.swallow).toBe(false);
    expect(step.state).toEqual(EMPTY_CAPTURE);
  });

  it("英字・記号・矢印のキーは無視する（状態を変えず、止めない）。スキャンの途中にあっても読める", () => {
    let state = captureKey(EMPTY_CAPTURE, key(digitCode("1"), 0), base).state;
    for (const code of ["KeyA", "ShiftLeft", "Minus", "ArrowLeft"]) {
      const step = captureKey(state, key(code, 5), base);
      expect(step.swallow).toBe(false);
      expect(step.state).toBe(state);
    }
    state = type("2345678", base, { state, start: 10, end: undefined }).state;
    expect(captureKey(state, key("Enter", 50), base).scan).toBe("12345678");
  });

  it("Ctrl・Meta・Alt 付きのキーはスキャンの一部ではない（途中の数字は捨てる）", () => {
    const state = captureKey(EMPTY_CAPTURE, key(digitCode("1"), 0), base).state;
    const step = captureKey(state, key("KeyN", 5, { modified: true }), base);
    expect(step).toEqual({ state: EMPTY_CAPTURE, swallow: false, scan: null });
    const digit = captureKey(EMPTY_CAPTURE, key(digitCode("1"), 0, { modified: true }), base);
    expect(digit.swallow).toBe(false);
    expect(digit.state).toEqual(EMPTY_CAPTURE);
  });

  it("押しっぱなしの繰り返しは数字に数えない", () => {
    const first = captureKey(EMPTY_CAPTURE, key(digitCode("1"), 0), base);
    const repeated = captureKey(first.state, key(digitCode("1"), 10, { repeat: true }), base);
    expect(repeated.state.digits).toBe("1");
    expect(repeated.swallow).toBe(true);
  });
});

describe("メイン画面の判定（FR-SCAN-10）", () => {
  it("キー間隔が閾値（30ms）以下なら、スキャンとみなす", () => {
    expect(type("9784003101117", base, { interval: 30, end: "Enter" }).scan).toBe("9784003101117");
  });

  it("閾値を超えて空いたら、そこまでの数字は捨てて数え直す", () => {
    // 5 文字打ったあと 100ms 空けて 8 桁打つ → 後ろの 8 桁だけが残る
    const typed = type("12345", base, { interval: 5 });
    const step = type("87654321", base, { state: typed.state, start: 1200, end: "Enter" });
    expect(step.scan).toBe("87654321");
  });

  it("人の手打ち（間隔が閾値より長い）の数字列は、スキャンにならない。Enter は通す", () => {
    const step = type("9784003101117", base, { interval: 120, end: "Enter" });
    expect(step.scan).toBeNull();
    expect(step.swallow).toBe(false);
  });

  it("最後の数字から Enter までが閾値を超えたら、スキャンにならず Enter を通す", () => {
    const typed = type("9784003101117", base, { interval: 5 });
    const step = captureKey(typed.state, key("Enter", typed.state.lastAt! + 500), base);
    expect(step.scan).toBeNull();
    expect(step.swallow).toBe(false);
  });

  it("桁数が足りない（8 桁未満）ときは、Enter を通す（普通のキー操作）", () => {
    const step = type("1234", base, { end: "Enter" });
    expect(step.scan).toBeNull();
    expect(step.swallow).toBe(false);
  });

  it("数字がないときの Enter は、そのまま通す", () => {
    expect(captureKey(EMPTY_CAPTURE, key("Enter", 0), base)).toEqual({
      state: EMPTY_CAPTURE,
      swallow: false,
      scan: null,
    });
  });
});

describe("スキャン登録画面の判定（FR-SCAN-07）", () => {
  const scanScreen = configForMode("scanScreen", DEFAULT_SCAN_SETTINGS);

  it("キー間隔が閾値より長くても、数字列＋Enter を拾う。短い数字列も渡す（判定は classify_code に任せる）", () => {
    expect(type("9784003101117", scanScreen, { interval: 200, end: "Enter" }).scan).toBe(
      "9784003101117",
    );
    expect(type("123", scanScreen, { end: "Enter" }).scan).toBe("123");
  });

  it("数秒たった古い数字は、次のスキャンに混ぜない", () => {
    const stale = type("99", scanScreen, { start: 0 });
    const step = type("9784003101117", scanScreen, {
      state: stale.state,
      start: 5000,
      end: "Enter",
    });
    expect(step.scan).toBe("9784003101117");
  });

  it("画面ごとの設定を作る", () => {
    expect(configForMode("mainScreen", DEFAULT_SCAN_SETTINGS)).toMatchObject({
      maxGapMs: 30,
      minLength: 8,
    });
    expect(configForMode("scanScreen", DEFAULT_SCAN_SETTINGS)).toMatchObject({
      maxGapMs: 1000,
      minLength: 1,
    });
  });
});

describe("末尾キー Tab・なし（FR-SCAN-12）", () => {
  it("Tab が末尾キーのとき、Tab で確定する。Tab の既定の動作（フォーカス移動）を止める", () => {
    const config = { ...base, terminator: "tab" } as const;
    const step = type("9784003101117", config, { end: "Tab" });
    expect(step.scan).toBe("9784003101117");
    expect(step.swallow).toBe(true);
    // Enter は確定にならない
    expect(type("9784003101117", config, { end: "Enter" }).scan).toBeNull();
  });

  it("末尾キーなしのとき、最後のキーから一定時間たつと確定する（captureIdle）", () => {
    const config = { ...base, terminator: "none" } as const;
    const typed = type("9784003101117", config, { interval: 5, start: 0 });
    expect(typed.scan).toBeNull();
    const lastAt = typed.state.lastAt!;
    expect(captureIdle(typed.state, lastAt + 99, config)).toEqual({
      state: typed.state,
      scan: null,
    });
    expect(captureIdle(typed.state, lastAt + 100, config)).toEqual({
      state: EMPTY_CAPTURE,
      scan: "9784003101117",
    });
  });

  it("末尾キーなしで桁数が足りなければ、確定せずに捨てる", () => {
    const config = { ...base, terminator: "none" } as const;
    const typed = type("123", config, { start: 0 });
    expect(captureIdle(typed.state, typed.state.lastAt! + 100, config)).toEqual({
      state: EMPTY_CAPTURE,
      scan: null,
    });
  });

  it("末尾キーが Enter のときは、captureIdle は何もしない", () => {
    const typed = type("9784003101117", base, { start: 0 });
    expect(captureIdle(typed.state, 99_999, base).scan).toBeNull();
  });

  it("末尾キーなしのとき、Enter や Tab は通す", () => {
    const config = { ...base, terminator: "none" } as const;
    const typed = type("9784003101117", config, { start: 0 });
    expect(captureKey(typed.state, key("Enter", 100), config).swallow).toBe(false);
    expect(captureKey(typed.state, key("Tab", 100), config).swallow).toBe(false);
  });
});

describe("先頭・末尾の余分な文字の除去（FR-SCAN-12）", () => {
  it("stripAffixes", () => {
    expect(stripAffixes("012345", 1, 0)).toBe("12345");
    expect(stripAffixes("012345", 0, 2)).toBe("0123");
    expect(stripAffixes("012345", 1, 1)).toBe("1234");
    expect(stripAffixes("12", 1, 1)).toBe("");
    expect(stripAffixes("12", 5, 5)).toBe("");
    expect(stripAffixes("12", -1, -1)).toBe("12");
  });

  it("確定した数字列から、先頭・末尾の指定した桁数を除く", () => {
    const config = { ...base, stripLeading: 1, stripTrailing: 2 };
    expect(type("0" + "9784003101117" + "99", config, { end: "Enter" }).scan).toBe("9784003101117");
  });

  it("除いたあとの桁数で、最小桁数を判定する", () => {
    const config = { ...base, stripLeading: 3, stripTrailing: 3, minLength: 8 };
    expect(type("123456789012", config, { end: "Enter" }).scan).toBeNull();
  });
});

describe("上限", () => {
  it("1 回で集める桁数の上限を超えたら、スキャンではない入力として捨てる", () => {
    const step = type("1".repeat(MAX_CAPTURE_DIGITS + 5), base);
    expect(step.state.digits.length).toBeLessThanOrEqual(MAX_CAPTURE_DIGITS);
  });
});
