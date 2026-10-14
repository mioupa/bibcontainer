import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PairingEffect } from "@/features/scan/pairing";
import { usePairing } from "@/features/scan/usePairing";
import type { ScanCode } from "@/types/contracts";

const ISBN: ScanCode = {
  kind: "isbn13",
  raw: "9784003101117",
  isbn13: "9784003101117",
  isbn10: "4003101111",
};
const SECOND: ScanCode = {
  kind: "secondTier",
  raw: "1920193007008",
  cCode: "0193",
  listPrice: 700,
};

describe("usePairing", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-14T10:00:00Z"));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  function setup(timeoutSec = 10) {
    const effects: PairingEffect[] = [];
    const hook = renderHook(() => usePairing({ timeoutSec, onEffect: (e) => effects.push(e) }));
    return { effects, ...hook };
  }

  it("ISBN → 2段目で確定し、効果が 1 回ずつ実行される", () => {
    const { result, effects } = setup();
    act(() => result.current.submit(ISBN));
    expect(result.current.state.phase).toBe("isbnHeld");
    expect(effects.some((e) => e.type === "startLookup")).toBe(true);

    act(() => result.current.submit(SECOND));
    expect(result.current.state.phase).toBe("idle");
    expect(effects.filter((e) => e.type === "confirm")).toHaveLength(1);
    expect(effects.filter((e) => e.type === "startLookup")).toHaveLength(1);
  });

  it("残り秒数が時間とともに減り、期限で 2段目なしで確定する", () => {
    const { result, effects } = setup(10);
    act(() => result.current.submit(ISBN));
    expect(result.current.remainingSec).toBe(10);

    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(result.current.remainingSec).toBe(7);
    expect(effects.filter((e) => e.type === "confirm")).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(7000);
    });
    expect(result.current.state.phase).toBe("idle");
    expect(result.current.remainingSec).toBeNull();
    const confirms = effects.filter((e) => e.type === "confirm");
    expect(confirms).toHaveLength(1);
    expect(confirms[0]).toMatchObject({ reason: "timeout", book: { secondTier: null } });
  });

  it("2段目 保留のまま期限が来ると、捨ててエラー音を出す", () => {
    const { result, effects } = setup(5);
    act(() => result.current.submit(SECOND));
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(result.current.state.phase).toBe("idle");
    expect(effects.filter((e) => e.type === "sound")).toEqual([
      { type: "sound", sound: "success" },
      { type: "sound", sound: "error" },
    ]);
  });

  it("確定キー（commit）で、保留中の本を 2段目なしで確定する。期限のタイマーは止まる", () => {
    const { result, effects } = setup();
    act(() => result.current.submit(ISBN));
    act(() => result.current.commit());
    expect(result.current.state.phase).toBe("idle");
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(effects.filter((e) => e.type === "confirm")).toHaveLength(1);
  });

  it("アンマウントすると、タイマーが止まる", () => {
    const { result, effects, unmount } = setup();
    act(() => result.current.submit(ISBN));
    unmount();
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(effects.filter((e) => e.type === "confirm")).toHaveLength(0);
  });
});
