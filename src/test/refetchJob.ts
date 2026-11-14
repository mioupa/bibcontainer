import type { RefetchJobStatus } from "@/bindings/RefetchJobStatus";

/** 再取得の作業の状態の見本。 */
export function makeJob(overrides: Partial<RefetchJobStatus> = {}): RefetchJobStatus {
  return {
    jobId: 1,
    state: "running",
    overrideManual: false,
    total: 100,
    done: 0,
    updated: 0,
    unchanged: 0,
    offline: 0,
    skipped: 0,
    failed: 0,
    coversSaved: 0,
    keptManualFields: 0,
    remaining: 100,
    stopReason: null,
    elapsedMs: 0,
    ...overrides,
  };
}
