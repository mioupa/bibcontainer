// ペアリング reducer（pairing.ts）を画面につなぐフック。
// 状態の保持、期限のタイマー、残り秒数の表示用の時計、効果の実行呼び出しを受け持つ。
//
// 効果（書誌取得の開始・確定した本・音・メッセージ）は reducer の外で実行する。
// React の reducer 関数の中で副作用を実行すると、StrictMode で二重に鳴るため、dispatch の中で同期的に 1 回だけ実行する。
import { useCallback, useEffect, useRef, useState } from "react";
import {
  PAIRING_IDLE,
  deadlineOf,
  pairingReducer,
  remainingSeconds,
  type PairingEffect,
  type PairingState,
} from "@/features/scan/pairing";
import type { ScanCode } from "@/types/contracts";

/** 残り秒数の表示を更新する間隔（ミリ秒）。 */
const COUNTDOWN_INTERVAL_MS = 250;

export interface UsePairingOptions {
  /** タイムアウト T（秒）。設定ストアの scan.timeoutSec を渡す。 */
  timeoutSec: number;
  /** 効果を 1 つずつ受け取る。音を鳴らす、書誌取得を始める、確定した本を一覧に足す、などを行う。 */
  onEffect: (effect: PairingEffect) => void;
  /** 時計（ミリ秒）。テストで差し替える。 */
  now?: () => number;
}

export interface UsePairingResult {
  state: PairingState;
  /** 残り秒数（切り上げ）。待機中は null。 */
  remainingSec: number | null;
  /** 判定済みのバーコードを渡す。 */
  submit: (code: ScanCode) => void;
  /** 確定キー。保留中の本を 2段目なしで確定する。 */
  commit: () => void;
}

export function usePairing({
  timeoutSec,
  onEffect,
  now = Date.now,
}: UsePairingOptions): UsePairingResult {
  const [state, setState] = useState<PairingState>(PAIRING_IDLE);
  const [clock, setClock] = useState(() => now());

  // 最新の値を、依存配列に入れずにコールバックから参照するための ref
  const stateRef = useRef<PairingState>(PAIRING_IDLE);
  const configRef = useRef({ timeoutSec });
  const onEffectRef = useRef(onEffect);
  const nowRef = useRef(now);
  useEffect(() => {
    configRef.current = { timeoutSec };
    onEffectRef.current = onEffect;
    nowRef.current = now;
  });

  const dispatch = useCallback((event: Parameters<typeof pairingReducer>[1]) => {
    const step = pairingReducer(stateRef.current, event, configRef.current);
    stateRef.current = step.state;
    setState(step.state);
    setClock(event.now);
    for (const effect of step.effects) onEffectRef.current(effect);
  }, []);

  const submit = useCallback(
    (code: ScanCode) => dispatch({ type: "scan", code, now: nowRef.current() }),
    [dispatch],
  );
  const commit = useCallback(() => dispatch({ type: "commit", now: nowRef.current() }), [dispatch]);

  // 期限でタイムアウトにする
  const deadline = deadlineOf(state);
  useEffect(() => {
    if (deadline === null) return;
    const timer = setTimeout(
      () => dispatch({ type: "tick", now: nowRef.current() }),
      Math.max(0, deadline - nowRef.current()),
    );
    return () => clearTimeout(timer);
  }, [deadline, dispatch]);

  // 残り秒数の表示を更新する
  useEffect(() => {
    if (deadline === null) return;
    const timer = setInterval(() => setClock(nowRef.current()), COUNTDOWN_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [deadline]);

  return { state, remainingSec: remainingSeconds(state, clock), submit, commit };
}
