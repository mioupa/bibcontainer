// `covers-arrived`（同期で書影が手元に届いたときに Rust が出すイベント）を受けて、書影の版を進める（0016）。
//
// イベントは `{ hashes: string[] }`。本の行が先に届いて書影があとから届くと、画面は仮の表紙を出している。
// 版（`coverVersionStore`）が進んだ書影を、`BookCover` が読み込み直す。
import { useEffect } from "react";
import { onCoversArrived } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";
import { useCoverVersionStore } from "@/stores/coverVersionStore";

/** `covers-arrived` を購読して、書影の版を進め続ける。画面の最上位で 1 回だけ呼ぶ。Tauri の外では何もしない。 */
export function useCoverArrivalSync(): void {
  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onCoversArrived(({ hashes }) => useCoverVersionStore.getState().bump(hashes))
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch((error: unknown) => {
        console.error("covers-arrived を購読できませんでした", error);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
