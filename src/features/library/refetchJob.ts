// 再取得の作業（0027）の購読。`get_refetch_job`（開いた時点の状態）と `refetch-progress`（進み具合）を、
// どの画面でも受け続け、`refetchJobStore` に入れる。終わったとき、作業ごとに 1 回だけ通知する。
//
// 通知するのは、この画面が「動いている」のを見た作業だけ（画面を開いたときにすでに終わっていた作業は通知しない）。
// 見た・通知した作業の印はモジュールに持つので、画面の切り替え（再マウント）で、終わった作業を通知し直さない。
import { useEffect } from "react";
import { toast } from "sonner";
import type { RefetchJobStatus } from "@/bindings/RefetchJobStatus";
import { libraryApi } from "@/features/library/api";
import { describeRefetchJob } from "@/features/library/refetch";
import { useRefetchJobStore } from "@/stores/refetchJobStore";

const seenRunning = new Set<number>();
const notified = new Set<number>();

/**
 * 新しい状態を取り込む。古い状態（前の作業、終わった作業の「動いている」）で上書きしない。
 *
 * `started` は、この画面が始めた作業（`start_refetch` の戻り値）。すぐ終わった作業（使える取得元がない、など）や、
 * 戻り値より先に終わりのイベントが届いた作業も、通知する。
 */
export function applyRefetchStatus(
  status: RefetchJobStatus,
  options: { started?: boolean } = {},
): void {
  if (options.started === true) seenRunning.add(status.jobId);
  const current = useRefetchJobStore.getState().job;
  let next = status;
  if (current !== null) {
    if (status.jobId < current.jobId) return;
    // 終わったあとに届いた「動いている」は捨てる（終わりの通知がまだなら、ここで出す）
    if (status.jobId === current.jobId && current.state !== "running" && status.state === "running")
      next = current;
  }
  if (next !== current) useRefetchJobStore.getState().set(next);
  if (next.state === "running") {
    seenRunning.add(next.jobId);
    return;
  }
  if (!seenRunning.has(next.jobId) || notified.has(next.jobId)) return;
  notified.add(next.jobId);
  const notice = describeRefetchJob(next);
  if (notice === null) return;
  toast[notice.level](
    notice.message,
    notice.persistent ? { duration: Infinity, closeButton: true } : undefined,
  );
}

/** テスト用: 見た・通知した作業の印と、状態を消す。 */
export function resetRefetchJobSync(): void {
  seenRunning.clear();
  notified.clear();
  useRefetchJobStore.getState().set(null);
}

/** 再取得の作業を購読し続ける。画面の最上位で 1 回だけ呼ぶ。 */
export function useRefetchJobSync(): void {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    // 購読を先に張ってから現在の状態を読む（読んでいる間の進み具合を取りこぼさない）
    void (async () => {
      try {
        const stop = await libraryApi.onRefetchProgress(applyRefetchStatus);
        if (cancelled) stop();
        else unlisten = stop;
      } catch (error) {
        console.error("refetch-progress を購読できませんでした", error);
      }
      try {
        const job = await libraryApi.getRefetchJob();
        if (!cancelled && job !== null) applyRefetchStatus(job);
      } catch (error) {
        console.error("再取得の状態を読めませんでした", error);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
