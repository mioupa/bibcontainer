// 書誌の再取得の作業（0027）の、今の状態。`useRefetchJobSync`（App）が `get_refetch_job` と `refetch-progress` で更新する。
// 一覧の帯のボタン・右クリック・詳細ペイン・ステータスバーが読む。
import { create } from "zustand";
import type { RefetchJobStatus } from "@/bindings/RefetchJobStatus";

interface RefetchJobState {
  job: RefetchJobStatus | null;
  set: (job: RefetchJobStatus | null) => void;
}

export const useRefetchJobStore = create<RefetchJobState>()((set) => ({
  job: null,
  set: (job) => set({ job }),
}));

/** 再取得が動いているか。 */
export function useRefetchRunning(): boolean {
  return useRefetchJobStore((state) => state.job?.state === "running");
}
