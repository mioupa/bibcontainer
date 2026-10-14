import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useShortcuts } from "@/features/shortcuts/useShortcuts";

function press(init: KeyboardEventInit, target: EventTarget = document.body): boolean {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  act(() => {
    target.dispatchEvent(event);
  });
  return event.defaultPrevented;
}

describe("useShortcuts", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("macOS: ⌘N・⇧⌘N・⌘F・⌘I・⌘, でそれぞれのコールバックを呼び、既定の動作を止める", () => {
    const handlers = {
      newBook: vi.fn(),
      scanRegister: vi.fn(),
      focusSearch: vi.fn(),
      toggleDetail: vi.fn(),
      openSettings: vi.fn(),
    };
    renderHook(() => useShortcuts(handlers, "mac"));

    expect(press({ code: "KeyN", metaKey: true })).toBe(true);
    expect(press({ code: "KeyN", metaKey: true, shiftKey: true })).toBe(true);
    press({ code: "KeyF", metaKey: true });
    press({ code: "KeyI", metaKey: true });
    press({ code: "Comma", metaKey: true });
    for (const handler of Object.values(handlers)) expect(handler).toHaveBeenCalledOnce();
  });

  it("Windows: Ctrl で動き、⌘ では動かない", () => {
    const newBook = vi.fn();
    renderHook(() => useShortcuts({ newBook }, "windows"));
    expect(press({ code: "KeyN", metaKey: true })).toBe(false);
    expect(newBook).not.toHaveBeenCalled();
    press({ code: "KeyN", ctrlKey: true });
    expect(newBook).toHaveBeenCalledOnce();
  });

  it("入力欄にフォーカスがあっても、検索欄への移動などの修飾キー付きは効く", () => {
    const focusSearch = vi.fn();
    renderHook(() => useShortcuts({ focusSearch }, "mac"));
    const input = document.createElement("input");
    document.body.append(input);
    press({ code: "KeyF", metaKey: true }, input);
    expect(focusSearch).toHaveBeenCalledOnce();
  });

  it("入力欄にフォーカスがあるときの「元に戻す」は、入力欄に譲る", () => {
    const undo = vi.fn();
    renderHook(() => useShortcuts({ undo }, "mac"));
    const input = document.createElement("input");
    document.body.append(input);
    expect(press({ code: "KeyZ", metaKey: true }, input)).toBe(false);
    expect(undo).not.toHaveBeenCalled();
    press({ code: "KeyZ", metaKey: true });
    expect(undo).toHaveBeenCalledOnce();
  });

  it("一覧だけに効くもの（Enter・ゴミ箱）は、ここでは扱わない", () => {
    const edit = vi.fn();
    const trash = vi.fn();
    renderHook(() => useShortcuts({ edit, trash }, "windows"));
    press({ code: "Enter" });
    press({ code: "Delete" });
    expect(edit).not.toHaveBeenCalled();
    expect(trash).not.toHaveBeenCalled();
  });

  it("日本語入力の変換中のキーは無視する", () => {
    const newBook = vi.fn();
    renderHook(() => useShortcuts({ newBook }, "mac"));
    press({ code: "KeyN", metaKey: true, isComposing: true });
    expect(newBook).not.toHaveBeenCalled();
  });

  it("コールバックを渡していないショートカットは、既定の動作を止めない", () => {
    renderHook(() => useShortcuts({}, "mac"));
    expect(press({ code: "KeyF", metaKey: true })).toBe(false);
  });
});
