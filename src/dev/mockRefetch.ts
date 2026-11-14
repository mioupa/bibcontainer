// 書誌の再取得の作業（0027）のモック。ブラウザ（`pnpm dev`）で、ステータスバー・通知の見た目を確かめるためのもの。
//
// Rust の作業と同じく、`start_refetch` はすぐ戻り、作業は時間で進む（400 ms ごとに最大 10 冊）。本物の取得はしない。
import type { RefetchJobStatus } from "@/bindings/RefetchJobStatus";
import { ja } from "@/i18n/ja";
import { IpcError } from "@/lib/ipc";

/** 1 回に進める冊数（Rust のまとまり）。 */
export const MOCK_REFETCH_CHUNK = 10;
/** まとまり 1 つにかかる時間。 */
export const MOCK_REFETCH_INTERVAL_MS = 400;

type Handler = (status: RefetchJobStatus) => void;

const handlers = new Set<Handler>();
let job: RefetchJobStatus | null = null;
let nextJobId = 1;
let timer: ReturnType<typeof setInterval> | null = null;
let startedAt = 0;

function publish(status: RefetchJobStatus): void {
  job = status;
  for (const handler of [...handlers]) handler(status);
}

function stopTimer(): void {
  if (timer !== null) clearInterval(timer);
  timer = null;
}

function tick(): void {
  if (job?.state !== "running") return stopTimer();
  const step = Math.min(MOCK_REFETCH_CHUNK, job.remaining);
  const next = { ...job };
  for (let i = job.done; i < job.done + step; i++) {
    // 見た目の確認用に、結果をばらけさせる
    if (i % 7 === 6) next.skipped += 1;
    else if (i % 5 === 4) next.unchanged += 1;
    else next.updated += 1;
  }
  next.done = job.done + step;
  next.remaining = job.total - next.done;
  next.elapsedMs = Date.now() - startedAt;
  if (next.remaining === 0) {
    next.state = "finished";
    stopTimer();
  }
  publish(next);
}

export const mockRefetch = {
  startRefetch(ids: string[], overrideManual: boolean): Promise<RefetchJobStatus> {
    if (job?.state === "running") {
      return Promise.reject(
        new IpcError({ kind: "conflict", message: ja.toast.refetch.alreadyRunning }),
      );
    }
    const total = new Set(ids).size;
    startedAt = Date.now();
    const status: RefetchJobStatus = {
      jobId: nextJobId++,
      state: total === 0 ? "finished" : "running",
      overrideManual,
      total,
      done: 0,
      updated: 0,
      unchanged: 0,
      offline: 0,
      skipped: 0,
      failed: 0,
      coversSaved: 0,
      keptManualFields: 0,
      remaining: total,
      stopReason: null,
      elapsedMs: 0,
    };
    publish(status);
    if (status.state === "running") timer = setInterval(tick, MOCK_REFETCH_INTERVAL_MS);
    return Promise.resolve(status);
  },
  cancelRefetch(): Promise<null> {
    if (job?.state === "running") {
      stopTimer();
      publish({
        ...job,
        state: "cancelled",
        stopReason: { kind: "cancelled" },
        elapsedMs: Date.now() - startedAt,
      });
    }
    return Promise.resolve(null);
  },
  getRefetchJob(): Promise<RefetchJobStatus | null> {
    return Promise.resolve(job);
  },
  onRefetchProgress(handler: Handler): Promise<() => void> {
    handlers.add(handler);
    return Promise.resolve(() => {
      handlers.delete(handler);
    });
  },
};

/** テスト用: 作業と購読を空にする。 */
export function resetMockRefetch(): void {
  stopTimer();
  job = null;
  nextJobId = 1;
  handlers.clear();
}
