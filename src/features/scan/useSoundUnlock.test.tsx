import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useSoundUnlock } from "@/features/scan/useSoundUnlock";

/** 何回目の unlock で running になるかを決められる偽の再生器。 */
function fakePlayer(unlockedAt: number) {
  let calls = 0;
  const unlock = vi.fn(() => Promise.resolve(++calls >= unlockedAt));
  return { unlock };
}

const dispatch = async (type: string) => {
  await act(async () => {
    window.dispatchEvent(new Event(type, { bubbles: true }));
    await Promise.resolve();
  });
};

describe("通知音の unlock（最初のキー入力・クリックで AudioContext を作る）", () => {
  it("最初のキー入力で unlock し、鳴らせる状態になったら、それ以後は呼ばない", async () => {
    const player = fakePlayer(1);
    renderHook(() => useSoundUnlock(player));
    expect(player.unlock).not.toHaveBeenCalled(); // 操作がなければ、作らない

    await dispatch("keydown");
    expect(player.unlock).toHaveBeenCalledOnce();

    await dispatch("keydown");
    await dispatch("pointerdown");
    expect(player.unlock).toHaveBeenCalledOnce();
  });

  it("クリック（pointerdown）でも unlock する", async () => {
    const player = fakePlayer(1);
    renderHook(() => useSoundUnlock(player));
    await dispatch("pointerdown");
    expect(player.unlock).toHaveBeenCalledOnce();
  });

  it("まだ鳴らせる状態にならなかったとき（修飾キーだけの keydown など）は、次の操作でやり直す", async () => {
    const player = fakePlayer(2);
    renderHook(() => useSoundUnlock(player));
    await dispatch("keydown");
    expect(player.unlock).toHaveBeenCalledTimes(1);
    await dispatch("keydown");
    expect(player.unlock).toHaveBeenCalledTimes(2);
    await dispatch("keydown"); // 2 回目で running になったので、もう呼ばない
    expect(player.unlock).toHaveBeenCalledTimes(2);
  });

  it("unlock の完了を待っている間の操作では、重ねて呼ばない", async () => {
    let finish: (unlocked: boolean) => void = () => {};
    const unlock = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          finish = resolve;
        }),
    );
    renderHook(() => useSoundUnlock({ unlock }));
    await dispatch("keydown");
    await dispatch("keydown");
    expect(unlock).toHaveBeenCalledOnce();
    await act(async () => {
      finish(false);
      await Promise.resolve();
    });
    await dispatch("keydown");
    expect(unlock).toHaveBeenCalledTimes(2);
  });

  it("画面を離れたら（アンマウント）、イベントの受け取りをやめる", async () => {
    const player = fakePlayer(99);
    const { unmount } = renderHook(() => useSoundUnlock(player));
    unmount();
    await dispatch("keydown");
    expect(player.unlock).not.toHaveBeenCalled();
  });

  it("スキャンの入力がキーの伝播を止めても（stopPropagation）、受け取る", async () => {
    const player = fakePlayer(1);
    renderHook(() => useSoundUnlock(player));
    // 捕捉フェーズで、別の受け手が伝播を止める（useScanCapture と同じ）
    const stopper = new AbortController();
    window.addEventListener("keydown", (event) => event.stopPropagation(), {
      capture: true,
      signal: stopper.signal,
    });
    await act(async () => {
      document.body.dispatchEvent(new KeyboardEvent("keydown", { code: "Digit9", bubbles: true }));
      await Promise.resolve();
    });
    stopper.abort();
    expect(player.unlock).toHaveBeenCalledOnce();
  });
});
