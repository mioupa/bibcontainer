import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RefetchJobStatus } from "@/bindings/RefetchJobStatus";
import { mockRefetch, resetMockRefetch } from "@/dev/mockRefetch";
import { ja } from "@/i18n/ja";

const ids = (count: number) => Array.from({ length: count }, (_, i) => `b${i}`);

describe("再取得の作業のモック", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetMockRefetch();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("すぐ running で戻り、400 ms ごとに最大 10 冊ずつ進み、終わる", async () => {
    const seen: RefetchJobStatus[] = [];
    await mockRefetch.onRefetchProgress((status) => seen.push(status));
    const started = await mockRefetch.startRefetch(ids(25), false);
    expect(started).toMatchObject({ state: "running", total: 25, done: 0, remaining: 25 });

    await vi.advanceTimersByTimeAsync(400);
    expect(seen.at(-1)).toMatchObject({ done: 10, remaining: 15, state: "running" });
    await vi.advanceTimersByTimeAsync(800);
    const last = seen.at(-1)!;
    expect(last).toMatchObject({ done: 25, remaining: 0, state: "finished" });
    expect(last.updated + last.unchanged + last.skipped).toBe(25);
    expect(await mockRefetch.getRefetchJob()).toEqual(last);
  });

  it("動いている間に始めると conflict", async () => {
    await mockRefetch.startRefetch(ids(30), false);
    await expect(mockRefetch.startRefetch(ids(3), false)).rejects.toMatchObject({
      kind: "conflict",
      message: ja.toast.refetch.alreadyRunning,
    });
  });

  it("やめると cancelled になり、それ以上進まない", async () => {
    const seen: RefetchJobStatus[] = [];
    await mockRefetch.onRefetchProgress((status) => seen.push(status));
    await mockRefetch.startRefetch(ids(50), true);
    await vi.advanceTimersByTimeAsync(400);
    await mockRefetch.cancelRefetch();
    expect(seen.at(-1)).toMatchObject({
      state: "cancelled",
      stopReason: { kind: "cancelled" },
      done: 10,
      remaining: 40,
      overrideManual: true,
    });
    const count = seen.length;
    await vi.advanceTimersByTimeAsync(2000);
    expect(seen).toHaveLength(count);
  });
});
