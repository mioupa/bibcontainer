// 2 段バーコードのペアリング（FR-SCAN-05／06）。要件書 7.2 の状態遷移図どおりの純粋な reducer。
//
//   待機中  ──ISBN──▶ ISBN 保留（書誌取得を開始）        ──2段目──▶ 待機中（ペアで確定）
//           ──2段目─▶ 2段目 保留                          ──ISBN──▶ 待機中（ペアで確定。書誌取得を開始）
//
// - 入力は ScanCode（判定は IPC の classify_code が済ませたもの）と現在時刻。出力は新しい状態と「効果」の一覧で、
//   効果（書誌取得の開始・確定した本・音・メッセージ）の実行は呼び出し側（usePairing フック）が行う。
// - 時刻は呼び出し側が渡す（ミリ秒）。reducer 自身は時計を持たない。
// - タイムアウトは、スキャンや確定キーの処理の頭でも必ず検査する。tick の間隔が粗くても、順序が狂わない。
import { clampTimeoutSec, type SoundKind } from "@/features/scan/config";
import type { ScanCode } from "@/types/contracts";

export interface SecondTier {
  /** Cコード（4 桁）。 */
  cCode: string;
  /** 本体価格（税抜・円）。00000 は「不明」で null。 */
  listPrice: number | null;
  raw: string;
}

/** 確定した 1 冊。ISBN と、あれば 2段目。 */
export interface ConfirmedBook {
  isbn13: string;
  isbn10: string | null;
  secondTier: SecondTier | null;
}

export type PairingState =
  | { phase: "idle" }
  | { phase: "isbnHeld"; isbn13: string; isbn10: string | null; deadline: number }
  | { phase: "secondHeld"; secondTier: SecondTier; deadline: number };

export const PAIRING_IDLE: PairingState = { phase: "idle" };

export interface PairingConfig {
  /** タイムアウト T（秒）。3〜30 に収める。 */
  timeoutSec: number;
}

export type PairingEvent =
  /** バーコードを 1 つ読んだ（判定済み）。 */
  | { type: "scan"; code: ScanCode; now: number }
  /** 時間の経過。期限を過ぎていればタイムアウトにする。 */
  | { type: "tick"; now: number }
  /** 確定キー。保留中の本を 2段目なしで確定する。 */
  | { type: "commit"; now: number };

/** 本を確定した理由。 */
export type ConfirmReason = "paired" | "nextIsbn" | "commitKey" | "timeout";

export type MessageLevel = "info" | "success" | "warning" | "error";

/** 画面に出すメッセージ。文言は i18n（ja.scan.messages）が持つ。 */
export type PairingMessage =
  | { kind: "isbnHeld"; isbn13: string }
  | { kind: "secondTierHeld"; cCode: string; listPrice: number | null }
  | { kind: "paired"; isbn13: string; cCode: string; listPrice: number | null }
  | { kind: "confirmedWithoutSecondTier"; isbn13: string; reason: "commitKey" | "timeout" }
  | { kind: "duplicateIsbn"; isbn13: string }
  | { kind: "secondTierReplaced"; cCode: string }
  | { kind: "secondTierDiscarded"; reason: "commitKey" | "timeout" }
  | { kind: "rejected"; reason: "magazine" | "otherJan" | "checksum" | "format"; raw: string };

export type PairingEffect =
  /** この ISBN の書誌取得を始める（2段目を待たない）。 */
  | { type: "startLookup"; isbn13: string }
  /** 1 冊が確定した。 */
  | { type: "confirm"; book: ConfirmedBook; reason: ConfirmReason }
  | { type: "sound"; sound: SoundKind }
  | { type: "message"; level: MessageLevel; message: PairingMessage };

export interface PairingStep {
  state: PairingState;
  effects: PairingEffect[];
}

function timeoutMs(config: PairingConfig): number {
  return clampTimeoutSec(config.timeoutSec) * 1000;
}

/** 保留の期限（ミリ秒）。待機中は null。 */
export function deadlineOf(state: PairingState): number | null {
  return state.phase === "idle" ? null : state.deadline;
}

/** 期限までの残り時間（ミリ秒）。待機中は null。期限を過ぎていれば 0。 */
export function remainingMs(state: PairingState, now: number): number | null {
  return state.phase === "idle" ? null : Math.max(0, state.deadline - now);
}

/** 画面に出す残り秒数（切り上げ。「残り 7 秒」）。待機中は null。 */
export function remainingSeconds(state: PairingState, now: number): number | null {
  const ms = remainingMs(state, now);
  return ms === null ? null : Math.ceil(ms / 1000);
}

/** 期限を過ぎた保留を片づける（効果を追記して、待機中の状態を返す）。 */
function expire(
  state: Exclude<PairingState, { phase: "idle" }>,
  reason: "timeout" | "commitKey",
  effects: PairingEffect[],
): PairingState {
  if (state.phase === "isbnHeld") {
    effects.push({
      type: "confirm",
      book: { isbn13: state.isbn13, isbn10: state.isbn10, secondTier: null },
      reason,
    });
    // 時間切れの確定は、ユーザーの操作の直後ではないので音は鳴らさない（確定キーなら成功音）
    if (reason === "commitKey") effects.push({ type: "sound", sound: "success" });
    effects.push({
      type: "message",
      level: "info",
      message: { kind: "confirmedWithoutSecondTier", isbn13: state.isbn13, reason },
    });
  } else {
    // 2段目だけでは 1 冊にならないので捨てる
    effects.push({ type: "sound", sound: "error" });
    effects.push({
      type: "message",
      level: "error",
      message: { kind: "secondTierDiscarded", reason },
    });
  }
  return PAIRING_IDLE;
}

function reject(code: ScanCode, effects: PairingEffect[]) {
  let reason: "magazine" | "otherJan" | "checksum" | "format";
  switch (code.kind) {
    case "magazine":
      reason = "magazine";
      break;
    case "otherJan":
      reason = "otherJan";
      break;
    case "invalid":
      reason = code.reason;
      break;
    default:
      return;
  }
  effects.push({ type: "sound", sound: "error" });
  effects.push({
    type: "message",
    level: "error",
    message: { kind: "rejected", reason, raw: code.raw },
  });
}

function handleScan(
  state: PairingState,
  code: ScanCode,
  now: number,
  config: PairingConfig,
  effects: PairingEffect[],
): PairingState {
  const deadline = now + timeoutMs(config);

  switch (code.kind) {
    case "magazine":
    case "otherJan":
    case "invalid":
      // 受け付けない。保留中の状態と期限はそのまま
      reject(code, effects);
      return state;

    case "isbn13": {
      if (state.phase === "idle") {
        effects.push({ type: "startLookup", isbn13: code.isbn13 });
        effects.push({ type: "sound", sound: "success" });
        effects.push({
          type: "message",
          level: "info",
          message: { kind: "isbnHeld", isbn13: code.isbn13 },
        });
        return { phase: "isbnHeld", isbn13: code.isbn13, isbn10: code.isbn10, deadline };
      }
      if (state.phase === "isbnHeld") {
        if (state.isbn13 === code.isbn13) {
          // 二度読み。無視して注意音（期限も延ばさない）
          effects.push({ type: "sound", sound: "attention" });
          effects.push({
            type: "message",
            level: "warning",
            message: { kind: "duplicateIsbn", isbn13: code.isbn13 },
          });
          return state;
        }
        // 別の ISBN。保留中の本は 2段目なしで確定し、新しい ISBN を保留する
        effects.push({
          type: "confirm",
          book: { isbn13: state.isbn13, isbn10: state.isbn10, secondTier: null },
          reason: "nextIsbn",
        });
        effects.push({ type: "startLookup", isbn13: code.isbn13 });
        effects.push({ type: "sound", sound: "success" });
        effects.push({
          type: "message",
          level: "info",
          message: { kind: "isbnHeld", isbn13: code.isbn13 },
        });
        return { phase: "isbnHeld", isbn13: code.isbn13, isbn10: code.isbn10, deadline };
      }
      // 2段目 保留中に ISBN を読んだ: ペアにして確定
      effects.push({ type: "startLookup", isbn13: code.isbn13 });
      effects.push({
        type: "confirm",
        book: { isbn13: code.isbn13, isbn10: code.isbn10, secondTier: state.secondTier },
        reason: "paired",
      });
      effects.push({ type: "sound", sound: "success" });
      effects.push({
        type: "message",
        level: "success",
        message: {
          kind: "paired",
          isbn13: code.isbn13,
          cCode: state.secondTier.cCode,
          listPrice: state.secondTier.listPrice,
        },
      });
      return PAIRING_IDLE;
    }

    case "secondTier": {
      const secondTier: SecondTier = {
        cCode: code.cCode,
        listPrice: code.listPrice,
        raw: code.raw,
      };
      if (state.phase === "idle") {
        effects.push({ type: "sound", sound: "success" });
        effects.push({
          type: "message",
          level: "info",
          message: { kind: "secondTierHeld", cCode: code.cCode, listPrice: code.listPrice },
        });
        return { phase: "secondHeld", secondTier, deadline };
      }
      if (state.phase === "isbnHeld") {
        // ISBN 保留中に 2段目を読んだ: この本に 2段目を付けて確定
        effects.push({
          type: "confirm",
          book: { isbn13: state.isbn13, isbn10: state.isbn10, secondTier },
          reason: "paired",
        });
        effects.push({ type: "sound", sound: "success" });
        effects.push({
          type: "message",
          level: "success",
          message: {
            kind: "paired",
            isbn13: state.isbn13,
            cCode: code.cCode,
            listPrice: code.listPrice,
          },
        });
        return PAIRING_IDLE;
      }
      // 2段目の連続: 古いほうを捨てて新しいほうを保留する（注意音）
      effects.push({ type: "sound", sound: "attention" });
      effects.push({
        type: "message",
        level: "warning",
        message: { kind: "secondTierReplaced", cCode: code.cCode },
      });
      return { phase: "secondHeld", secondTier, deadline };
    }
  }
}

/** 状態遷移。`event.now` は呼び出し側の時計（ミリ秒）。 */
export function pairingReducer(
  state: PairingState,
  event: PairingEvent,
  config: PairingConfig,
): PairingStep {
  const effects: PairingEffect[] = [];

  // まず期限を過ぎていないか見る（tick が遅れても、先にタイムアウトとして片づく）
  let current = state;
  if (current.phase !== "idle" && event.now >= current.deadline) {
    current = expire(current, "timeout", effects);
  }

  switch (event.type) {
    case "tick":
      return { state: current, effects };
    case "commit":
      if (current.phase !== "idle") current = expire(current, "commitKey", effects);
      return { state: current, effects };
    case "scan":
      return { state: handleScan(current, event.code, event.now, config, effects), effects };
  }
}
