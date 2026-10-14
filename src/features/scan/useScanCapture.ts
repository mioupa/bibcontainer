// バーコードリーダーの入力を、画面全体のキー入力から拾うフック（FR-SCAN-07／08／10／12）。
// 組み立ては capture.ts の純粋関数。ここはイベントの購読と、フォーカスの判定、既定の動作を止める処理だけを持つ。
//
// 使い方:
//   mode "scanScreen": スキャン登録画面。入力欄にフォーカスがなくても数字列＋末尾キーを拾う（FR-SCAN-07）。
//   mode "mainScreen": メイン画面。テキスト入力欄にフォーカスがなく、キー間隔が閾値以下の数字列だけをスキャンとみなす（FR-SCAN-10）。
// 設定（末尾キー・キー間隔・先頭末尾の除去）は設定ストアから読む。
import { useEffect, useMemo, useRef } from "react";
import {
  EMPTY_CAPTURE,
  captureIdle,
  captureKey,
  configForMode,
  type CaptureState,
  type ScanCaptureMode,
} from "@/features/scan/capture";
import { isEditableTarget, isScanTarget } from "@/features/scan/dom";
import { useSettingsStore } from "@/stores/settingsStore";

export interface UseScanCaptureOptions {
  mode: ScanCaptureMode;
  /** false のあいだは拾わない（ダイアログを開いているときなど）。 */
  enabled?: boolean;
  /** 確定した数字列（先頭・末尾の除去後）を受け取る。 */
  onScan: (digits: string) => void;
}

export function useScanCapture({ mode, enabled = true, onScan }: UseScanCaptureOptions): void {
  const scan = useSettingsStore((s) => s.scan);
  const config = useMemo(() => configForMode(mode, scan), [mode, scan]);

  const onScanRef = useRef(onScan);
  useEffect(() => {
    onScanRef.current = onScan;
  });

  useEffect(() => {
    if (!enabled) return;

    let state: CaptureState = EMPTY_CAPTURE;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const clearTimer = () => {
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      // 文字を入力する場所では、人が打つ数字を邪魔しない（スキャン専用の欄は除く）
      const typing =
        isEditableTarget(event.target) && !(mode === "scanScreen" && isScanTarget(event.target));
      if (typing) {
        state = EMPTY_CAPTURE;
        clearTimer();
        return;
      }

      // IME がオンでも（isComposing や key が "Process" でも）、code だけを見るので同じに読める
      const step = captureKey(
        state,
        {
          code: event.code,
          at: event.timeStamp,
          repeat: event.repeat,
          modified: event.ctrlKey || event.metaKey || event.altKey,
        },
        config,
      );
      state = step.state;
      if (step.swallow) {
        event.preventDefault();
        event.stopPropagation();
      }

      clearTimer();
      if (step.scan !== null) {
        onScanRef.current(step.scan);
      } else if (config.terminator === "none" && state.digits.length > 0) {
        timer = setTimeout(() => {
          if (state.lastAt === null) return;
          const idle = captureIdle(state, state.lastAt + config.idleCommitMs, config);
          state = idle.state;
          if (idle.scan !== null) onScanRef.current(idle.scan);
        }, config.idleCommitMs);
      }
    };

    // 捕捉（capture）フェーズで受ける。ほかの要素より先に、Enter や Tab の動作を止められる
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
      clearTimer();
    };
  }, [enabled, mode, config]);
}
