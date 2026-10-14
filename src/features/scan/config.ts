// スキャン入力の設定（FR-CONF-03）。値の範囲と既定値をここに集める。

/** バーコードリーダーが数字列の後ろに付けるキー（FR-SCAN-12）。none は「付けない」で、一定時間の無入力で確定する。 */
export type ScanTerminator = "enter" | "tab" | "none";

export const TIMEOUT_SEC = { min: 3, max: 30, default: 10 } as const;
export const KEY_INTERVAL_MS = { min: 5, max: 200, default: 30 } as const;
export const IDLE_COMMIT_MS = { min: 30, max: 1000, default: 100 } as const;
export const STRIP_CHARS = { min: 0, max: 4, default: 0 } as const;

export interface ScanSettings {
  /** 数字列の後ろに付くキー。 */
  terminator: ScanTerminator;
  /** ペアリングのタイムアウト T（秒、3〜30）。FR-SCAN-06。 */
  timeoutSec: number;
  /** メイン画面でスキャンとみなすキー間隔の閾値（ミリ秒）。FR-SCAN-10。 */
  keyIntervalMs: number;
  /** 末尾キーなしのとき、最後のキーからこの時間（ミリ秒）たったら確定する。 */
  idleCommitMs: number;
  /** 集めた数字列の先頭から捨てる文字数（リーダーが付ける余分な数字）。 */
  stripLeading: number;
  /** 集めた数字列の末尾から捨てる文字数。 */
  stripTrailing: number;
}

export const DEFAULT_SCAN_SETTINGS: ScanSettings = {
  terminator: "enter",
  timeoutSec: TIMEOUT_SEC.default,
  keyIntervalMs: KEY_INTERVAL_MS.default,
  idleCommitMs: IDLE_COMMIT_MS.default,
  stripLeading: STRIP_CHARS.default,
  stripTrailing: STRIP_CHARS.default,
};

/** 通知音の種類（FR-SCAN-11）。成功・エラー・重複や注意の 3 種類。 */
export type SoundKind = "success" | "error" | "attention";

export const SOUND_KINDS: readonly SoundKind[] = ["success", "error", "attention"];

/** 通知音 1 種類の設定。 */
export interface SoundKindSettings {
  enabled: boolean;
  /** 0〜1。 */
  volume: number;
}

export interface SoundSettings {
  /** すべての通知音を止める。 */
  muted: boolean;
  /** 種類ごとの有効・音量。 */
  kinds: Record<SoundKind, SoundKindSettings>;
}

const DEFAULT_VOLUME = 0.6;

export const DEFAULT_SOUND_SETTINGS: SoundSettings = {
  muted: false,
  kinds: {
    success: { enabled: true, volume: DEFAULT_VOLUME },
    error: { enabled: true, volume: DEFAULT_VOLUME },
    attention: { enabled: true, volume: DEFAULT_VOLUME },
  },
};

/** 通知音の設定の一部の更新（種類ごとの項目も一部だけでよい）。 */
export interface SoundSettingsPatch {
  muted?: boolean;
  kinds?: Partial<Record<SoundKind, Partial<SoundKindSettings>>>;
}

function clampInt(value: number, range: { min: number; max: number; default: number }): number {
  if (!Number.isFinite(value)) return range.default;
  return Math.min(range.max, Math.max(range.min, Math.round(value)));
}

/** タイムアウト T を 3〜30 秒に収める。数でないときは既定値。 */
export function clampTimeoutSec(value: number): number {
  return clampInt(value, TIMEOUT_SEC);
}

export function clampKeyIntervalMs(value: number): number {
  return clampInt(value, KEY_INTERVAL_MS);
}

export function clampIdleCommitMs(value: number): number {
  return clampInt(value, IDLE_COMMIT_MS);
}

export function clampStripChars(value: number): number {
  return clampInt(value, STRIP_CHARS);
}

export function clampVolume(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_VOLUME;
  return Math.min(1, Math.max(0, value));
}

/** 通知音の設定に、一部の更新を重ねる。音量は 0〜1 に収める。保存された値の読み込みにも使う。 */
export function sanitizeSoundSettings(
  base: SoundSettings,
  patch: SoundSettingsPatch,
): SoundSettings {
  const kinds = {} as Record<SoundKind, SoundKindSettings>;
  for (const kind of SOUND_KINDS) {
    const merged = { ...base.kinds[kind], ...patch.kinds?.[kind] };
    kinds[kind] = {
      enabled: Boolean(merged.enabled),
      volume: clampVolume(merged.volume),
    };
  }
  return { muted: Boolean(patch.muted ?? base.muted), kinds };
}

/** 1 回鳴らすときの指定（実際の音量と、鳴らさないか）。再生器（sound.ts）が受け取る。 */
export interface SoundPlayback {
  /** 0〜1。 */
  volume: number;
  muted: boolean;
}

/** 設定から、その種類を鳴らすときの指定を求める（ミュートか、その種類が無効なら鳴らさない）。 */
export function effectiveSound(settings: SoundSettings, kind: SoundKind): SoundPlayback {
  const { enabled, volume } = settings.kinds[kind];
  return { volume, muted: settings.muted || !enabled };
}

/** 設定の一部の更新を、範囲に収めた形にそろえる。保存された古い値や手で書き換えた値の読み込みにも使う。 */
export function sanitizeScanSettings(
  base: ScanSettings,
  patch: Partial<ScanSettings>,
): ScanSettings {
  const merged = { ...base, ...patch };
  const terminator: ScanTerminator =
    merged.terminator === "tab" || merged.terminator === "none" ? merged.terminator : "enter";
  return {
    terminator,
    timeoutSec: clampTimeoutSec(merged.timeoutSec),
    keyIntervalMs: clampKeyIntervalMs(merged.keyIntervalMs),
    idleCommitMs: clampIdleCommitMs(merged.idleCommitMs),
    stripLeading: clampStripChars(merged.stripLeading),
    stripTrailing: clampStripChars(merged.stripTrailing),
  };
}
