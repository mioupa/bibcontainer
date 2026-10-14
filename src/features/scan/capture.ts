// バーコードリーダー（キーボードとして動く HID）の入力を、KeyboardEvent.code から数字列に組み立てる純粋なロジック。
// DOM には触れない。イベントの購読とフォーカスの判定は useScanCapture.ts が行う。
//
// 方針（FR-SCAN-07／08／10／12）:
// - キーは `event.key` や入力欄の値ではなく `code`（Digit0〜9、Numpad0〜9、Enter、Tab）で見る。
//   日本語入力（IME）がオンだと、key は "Process" になったり全角になったりするが、code は物理キーのままなので変わらない。
// - 数字キーと、設定された末尾キーだけを扱う。英字や記号のキーは、スキャンの途中でも無視する（状態を変えず、止めない）。
//   リーダーが付ける先頭・末尾の余分な「数字」は、設定の文字数ぶん捨てる（stripLeading / stripTrailing）。
// - 前のキーから maxGapMs より空いたら、そこまでの数字は捨てて数え直す。
//   メイン画面では maxGapMs をキー間隔の閾値（既定 30ms）にして、人間の手打ちとリーダーを区別する（FR-SCAN-10）。
import type { ScanTerminator } from "@/features/scan/config";

export interface ScanCaptureConfig {
  terminator: ScanTerminator;
  /** 連続するキーの間隔の上限（ミリ秒）。超えたらそこまでの数字は捨てる。 */
  maxGapMs: number;
  /** 確定とみなす最小桁数（先頭・末尾を除いた後）。足りない確定キーは、スキャンではなく普通のキー操作として通す。 */
  minLength: number;
  /** 末尾キーなしのとき、最後のキーからこの時間（ミリ秒）たったら確定する。 */
  idleCommitMs: number;
  stripLeading: number;
  stripTrailing: number;
}

export interface CaptureState {
  digits: string;
  /** 最後に数字を受け取った時刻（ミリ秒）。まだなければ null。 */
  lastAt: number | null;
}

export const EMPTY_CAPTURE: CaptureState = { digits: "", lastAt: null };

/** 1 回のスキャンで集める桁数の上限。これを超えたものは、スキャンではない入力として捨てる。 */
export const MAX_CAPTURE_DIGITS = 40;

export interface CaptureKey {
  /** KeyboardEvent.code。 */
  code: string;
  /** キーの時刻（ミリ秒。KeyboardEvent.timeStamp）。 */
  at: number;
  repeat: boolean;
  /** Ctrl・Meta・Alt のどれかが押されている（ショートカットなので、スキャンの一部ではない）。 */
  modified: boolean;
}

export interface CaptureStep {
  state: CaptureState;
  /** true なら、このキーの既定の動作（入力欄への文字入力、Enter や Tab の動作）を止める。 */
  swallow: boolean;
  /** 確定した数字列（先頭・末尾の除去後）。確定していなければ null。 */
  scan: string | null;
}

/** `Digit5` / `Numpad5` から "5" を取り出す。数字キーでなければ null。 */
export function digitFromCode(code: string): string | null {
  const match = /^(?:Digit|Numpad)([0-9])$/.exec(code);
  return match ? (match[1] ?? null) : null;
}

export function isTerminatorCode(code: string, terminator: ScanTerminator): boolean {
  if (terminator === "enter") return code === "Enter" || code === "NumpadEnter";
  if (terminator === "tab") return code === "Tab";
  return false;
}

/** 先頭から `leading` 文字、末尾から `trailing` 文字を除く。全部なくなるなら空文字。 */
export function stripAffixes(digits: string, leading: number, trailing: number): string {
  const start = Math.max(0, leading);
  const end = digits.length - Math.max(0, trailing);
  return end > start ? digits.slice(start, end) : "";
}

function finish(state: CaptureState, config: ScanCaptureConfig): string | null {
  const stripped = stripAffixes(state.digits, config.stripLeading, config.stripTrailing);
  return stripped.length > 0 && stripped.length >= config.minLength ? stripped : null;
}

function isStale(state: CaptureState, at: number, config: ScanCaptureConfig): boolean {
  return state.lastAt !== null && at - state.lastAt > config.maxGapMs;
}

/** キーを 1 つ受け取って、新しい状態と「止めるか」「確定したか」を返す。 */
export function captureKey(
  state: CaptureState,
  key: CaptureKey,
  config: ScanCaptureConfig,
): CaptureStep {
  if (key.modified) return { state: EMPTY_CAPTURE, swallow: false, scan: null };

  const digit = digitFromCode(key.code);
  if (digit !== null) {
    // 押しっぱなしの繰り返しは数字として数えない（リーダーは繰り返しを出さない）
    if (key.repeat) return { state, swallow: true, scan: null };
    const base = isStale(state, key.at, config) ? EMPTY_CAPTURE : state;
    if (base.digits.length >= MAX_CAPTURE_DIGITS) {
      return { state: EMPTY_CAPTURE, swallow: true, scan: null };
    }
    return {
      state: { digits: base.digits + digit, lastAt: key.at },
      swallow: true,
      scan: null,
    };
  }

  if (isTerminatorCode(key.code, config.terminator)) {
    // 古い（キー間隔を超えた）数字は、スキャンではなく手打ちの途中とみなす。この末尾キーは普通のキー操作として通す
    const fresh = state.digits.length > 0 && !isStale(state, key.at, config);
    const scan = fresh ? finish(state, config) : null;
    return { state: EMPTY_CAPTURE, swallow: scan !== null, scan };
  }

  // 末尾キーではない Enter・Tab は、入力の区切り。途中の数字は捨てて、キーは通す
  if (key.code === "Enter" || key.code === "NumpadEnter" || key.code === "Tab") {
    return { state: EMPTY_CAPTURE, swallow: false, scan: null };
  }

  // 英字・記号・矢印などは、スキャンの途中でも状態を変えない（リーダーが付ける文字は無視する）
  return { state, swallow: false, scan: null };
}

/** 末尾キーなしのとき、最後のキーから idleCommitMs 以上たったら確定する。時刻 `now` は captureKey と同じ時計。 */
export function captureIdle(
  state: CaptureState,
  now: number,
  config: ScanCaptureConfig,
): { state: CaptureState; scan: string | null } {
  if (config.terminator !== "none" || state.digits.length === 0 || state.lastAt === null) {
    return { state, scan: null };
  }
  if (now - state.lastAt < config.idleCommitMs) return { state, scan: null };
  return { state: EMPTY_CAPTURE, scan: finish(state, config) };
}

/** メイン画面（FR-SCAN-10）で、スキャンとみなす最小桁数。EAN-8 の 8 桁から。 */
export const MAIN_SCREEN_MIN_LENGTH = 8;

/** スキャン登録画面（FR-SCAN-07）で、キーの間隔が空いたら途中の数字を捨てるまでの時間。 */
export const SCAN_SCREEN_GAP_MS = 1000;

export type ScanCaptureMode = "scanScreen" | "mainScreen";

/** 画面の種類と設定から、組み立ての設定を作る。 */
export function configForMode(
  mode: ScanCaptureMode,
  settings: {
    terminator: ScanTerminator;
    keyIntervalMs: number;
    idleCommitMs: number;
    stripLeading: number;
    stripTrailing: number;
  },
): ScanCaptureConfig {
  return {
    terminator: settings.terminator,
    maxGapMs: mode === "mainScreen" ? settings.keyIntervalMs : SCAN_SCREEN_GAP_MS,
    minLength: mode === "mainScreen" ? MAIN_SCREEN_MIN_LENGTH : 1,
    idleCommitMs: settings.idleCommitMs,
    stripLeading: settings.stripLeading,
    stripTrailing: settings.stripTrailing,
  };
}
