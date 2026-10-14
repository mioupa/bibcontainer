// 通知音の AudioContext を、最初のキー入力・クリックで作って再開しておく（自動再生の制限の解除。FR-SCAN-11）。
//
// 音を鳴らすのは、バーコードを読んだあとの IPC（classify_code など）の完了のあと、つまりユーザーの操作の外。
// WKWebView（macOS）などは、そこで初めて AudioContext を作ると、止まったまま（suspended）で、最初の 1 回が鳴らない。
// ユーザーの操作（キー・クリック）の中で作って resume() しておけば、あとはいつでも鳴らせる。
// 修飾キーだけの keydown などは操作として数えられないことがあるので、鳴らせる状態（running）になるまで、
// 続くキー入力・クリックでもやり直す。
import { useEffect } from "react";
import { soundPlayer, type SoundPlayer } from "@/features/scan/sound";

const UNLOCK_EVENTS = ["keydown", "pointerdown", "pointerup"] as const;

export function useSoundUnlock(player: Pick<SoundPlayer, "unlock"> = soundPlayer): void {
  useEffect(() => {
    let finished = false;
    let pending = false;

    const stop = () => {
      finished = true;
      for (const name of UNLOCK_EVENTS) window.removeEventListener(name, onEvent, true);
    };
    const onEvent = () => {
      if (finished || pending) return;
      pending = true;
      void player.unlock().then((unlocked) => {
        pending = false;
        if (unlocked) stop();
      });
    };

    // 捕捉（capture）フェーズで受ける（スキャンの入力が、キーのイベントの伝播を止めても受け取れるように）
    for (const name of UNLOCK_EVENTS) {
      window.addEventListener(name, onEvent, { capture: true, passive: true });
    }
    return stop;
  }, [player]);
}
