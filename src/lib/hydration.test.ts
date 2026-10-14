import { afterEach, describe, expect, it, vi } from "vitest";
import { whenSettingsHydrated } from "@/lib/hydration";
import { useLayoutStore } from "@/stores/layoutStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";

describe("whenSettingsHydrated", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("保存先が同期（localStorage）なら、すぐ解決する", async () => {
    await expect(whenSettingsHydrated()).resolves.toBeUndefined();
  });

  it("読み込み中のストアがあれば、その完了を待つ", async () => {
    let finish: () => void = () => {};
    vi.spyOn(useSettingsStore.persist, "hasHydrated").mockReturnValue(false);
    vi.spyOn(useSettingsStore.persist, "onFinishHydration").mockImplementation((listener) => {
      finish = () => listener(useSettingsStore.getState());
      return () => {};
    });
    let done = false;
    const waiting = whenSettingsHydrated(5000).then(() => {
      done = true;
    });
    await Promise.resolve();
    expect(done).toBe(false);

    finish();
    await waiting;
    expect(done).toBe(true);
  });

  it("読み込みが終わらない（失敗した）ときは、時間で打ち切って進む", async () => {
    vi.useFakeTimers();
    vi.spyOn(useTableStore.persist, "hasHydrated").mockReturnValue(false);
    vi.spyOn(useTableStore.persist, "onFinishHydration").mockReturnValue(() => {});
    vi.spyOn(useLayoutStore.persist, "hasHydrated").mockReturnValue(true);

    const waiting = whenSettingsHydrated(1000);
    await vi.advanceTimersByTimeAsync(1000);
    await expect(waiting).resolves.toBeUndefined();
  });
});
