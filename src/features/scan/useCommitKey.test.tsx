import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { isCommitKey, useCommitKey } from "@/features/scan/useCommitKey";

function press(
  type: "keydown" | "keyup",
  code: string,
  target: EventTarget = document.body,
  init: KeyboardEventInit = {},
): KeyboardEvent {
  const event = new KeyboardEvent(type, { code, bubbles: true, cancelable: true, ...init });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

describe("useCommitKey（確定キー: スペース）", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("スペースキーで onCommit を呼び、既定の動作を止める", () => {
    const onCommit = vi.fn();
    renderHook(() => useCommitKey({ onCommit }));
    const down = press("keydown", "Space");
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(down.defaultPrevented).toBe(true);
    // keyup も止める（フォーカスのあるボタンが押されたことにならないように）
    expect(press("keyup", "Space").defaultPrevented).toBe(true);
  });

  it("押しっぱなしの繰り返しでは、何度も確定しない", () => {
    const onCommit = vi.fn();
    renderHook(() => useCommitKey({ onCommit }));
    press("keydown", "Space");
    press("keydown", "Space", document.body, { repeat: true });
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("スペース以外のキー・修飾キー付きは拾わない", () => {
    const onCommit = vi.fn();
    renderHook(() => useCommitKey({ onCommit }));
    press("keydown", "Enter");
    press("keydown", "KeyA");
    press("keydown", "Space", document.body, { ctrlKey: true });
    press("keydown", "Space", document.body, { metaKey: true });
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("テキスト入力欄の中では拾わない（スペースを入力できる）", () => {
    const onCommit = vi.fn();
    renderHook(() => useCommitKey({ onCommit }));
    const input = document.createElement("input");
    document.body.append(input);
    const down = press("keydown", "Space", input);
    expect(onCommit).not.toHaveBeenCalled();
    expect(down.defaultPrevented).toBe(false);
  });

  it("ダイアログの中では拾わない", () => {
    const onCommit = vi.fn();
    renderHook(() => useCommitKey({ onCommit }));
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    const button = document.createElement("button");
    dialog.append(button);
    document.body.append(dialog);
    press("keydown", "Space", button);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("ボタンにフォーカスがあるときは拾い、そのボタンのスペースによる操作は止める", () => {
    const onCommit = vi.fn();
    renderHook(() => useCommitKey({ onCommit }));
    const button = document.createElement("button");
    document.body.append(button);
    const down = press("keydown", "Space", button);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(down.defaultPrevented).toBe(true);
  });

  it("enabled が false のあいだは拾わない", () => {
    const onCommit = vi.fn();
    const { rerender } = renderHook(({ enabled }) => useCommitKey({ enabled, onCommit }), {
      initialProps: { enabled: false },
    });
    press("keydown", "Space");
    expect(onCommit).not.toHaveBeenCalled();
    rerender({ enabled: true });
    press("keydown", "Space");
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("isCommitKey: IME の変換中でも、物理キー（code）で判定する", () => {
    expect(
      isCommitKey({
        code: "Space",
        ctrlKey: false,
        metaKey: false,
        altKey: false,
        target: document.body,
      }),
    ).toBe(true);
  });
});
