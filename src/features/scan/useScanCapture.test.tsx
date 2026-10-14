import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";
import { useScanCapture } from "@/features/scan/useScanCapture";
import { useSettingsStore } from "@/stores/settingsStore";

interface KeyInit {
  /** イベントの時刻（ミリ秒）。省略すると jsdom の既定値。 */
  at?: number;
  key?: string;
  isComposing?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
}

/** keydown を発火して、既定の動作が止められたか（defaultPrevented）を返す。 */
function press(code: string, target: EventTarget = document.body, init: KeyInit = {}): boolean {
  const event = new KeyboardEvent("keydown", {
    code,
    key: init.key ?? code,
    isComposing: init.isComposing ?? false,
    ctrlKey: init.ctrlKey ?? false,
    metaKey: init.metaKey ?? false,
    bubbles: true,
    cancelable: true,
  });
  if (init.at !== undefined) Object.defineProperty(event, "timeStamp", { value: init.at });
  act(() => {
    target.dispatchEvent(event);
  });
  return event.defaultPrevented;
}

/** 数字列を打つ（interval ミリ秒間隔）。 */
function typeDigits(
  digits: string,
  options: { interval?: number; start?: number; ime?: boolean; target?: EventTarget } = {},
) {
  let at = options.start ?? 1000;
  const prevented: boolean[] = [];
  for (const d of digits) {
    prevented.push(
      press(`Digit${d}`, options.target, {
        at,
        // 日本語入力がオンのとき: key は "Process"、変換中を示す isComposing が立つ
        ...(options.ime ? { key: "Process", isComposing: true } : { key: d }),
      }),
    );
    at += options.interval ?? 5;
  }
  return { prevented, end: at };
}

const ISBN = "9784003101117";

describe("useScanCapture", () => {
  beforeEach(() => {
    useSettingsStore.setState({ scan: { ...DEFAULT_SCAN_SETTINGS } });
  });
  afterEach(() => {
    document.body.innerHTML = "";
    vi.useRealTimers();
  });

  describe("スキャン登録画面（scanScreen）", () => {
    it("フォーカスがなくても数字列＋Enter を拾い、数字と Enter の既定の動作を止める（FR-SCAN-07）", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));

      const { prevented, end } = typeDigits(ISBN);
      expect(prevented.every(Boolean)).toBe(true);
      expect(onScan).not.toHaveBeenCalled();
      expect(press("Enter", document.body, { at: end })).toBe(true);
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("日本語入力がオン（key が Process、isComposing）でも、code で同じに読める（FR-SCAN-08、AT-09）", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));

      const { end } = typeDigits(ISBN, { ime: true });
      press("Enter", document.body, { at: end, key: "Process", isComposing: true });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("テンキーの数字と NumpadEnter も読める", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      let at = 0;
      for (const d of "9784003101117") press(`Numpad${d}`, document.body, { at: (at += 5) });
      press("NumpadEnter", document.body, { at: at + 5 });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("入力欄にフォーカスがあるときは拾わず、既定の動作も止めない（人が打つ数字を邪魔しない）", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      const input = document.createElement("input");
      document.body.append(input);
      input.focus();

      const { prevented, end } = typeDigits(ISBN, { target: input });
      expect(prevented.some(Boolean)).toBe(false);
      expect(press("Enter", input, { at: end })).toBe(false);
      expect(onScan).not.toHaveBeenCalled();
    });

    it("スキャン専用の入力欄（data-scan-target）にフォーカスがあるときは拾う", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      const input = document.createElement("input");
      input.dataset.scanTarget = "";
      document.body.append(input);
      input.focus();

      const { end } = typeDigits(ISBN, { target: input });
      press("Enter", input, { at: end });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("チェックボックスやボタンにフォーカスがあるときは拾う（文字を入力する場所ではない）", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      document.body.append(checkbox);
      checkbox.focus();

      const { end } = typeDigits(ISBN, { target: checkbox });
      press("Enter", checkbox, { at: end });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("Ctrl・⌘ を押しているときは拾わない（ショートカット）", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      expect(press("Digit1", document.body, { at: 0, metaKey: true })).toBe(false);
      expect(press("Digit1", document.body, { at: 5, ctrlKey: true })).toBe(false);
    });

    it("enabled が false のあいだは拾わない", () => {
      const onScan = vi.fn();
      const { rerender } = renderHook(
        ({ enabled }) => useScanCapture({ mode: "scanScreen", enabled, onScan }),
        { initialProps: { enabled: false } },
      );
      const first = typeDigits(ISBN);
      press("Enter", document.body, { at: first.end });
      expect(onScan).not.toHaveBeenCalled();
      expect(first.prevented.some(Boolean)).toBe(false);

      rerender({ enabled: true });
      const second = typeDigits(ISBN);
      press("Enter", document.body, { at: second.end });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("アンマウントすると、拾うのをやめる", () => {
      const onScan = vi.fn();
      const { unmount } = renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      unmount();
      const { end } = typeDigits(ISBN);
      press("Enter", document.body, { at: end });
      expect(onScan).not.toHaveBeenCalled();
    });
  });

  describe("メイン画面（mainScreen、FR-SCAN-10）", () => {
    it("キー間隔が閾値（既定 30ms）以下の数字列＋Enter をスキャンとみなす", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "mainScreen", onScan }));
      const { end } = typeDigits(ISBN, { interval: 20 });
      expect(press("Enter", document.body, { at: end })).toBe(true);
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("人の手打ち（間隔が閾値より長い）はスキャンにならず、Enter も奪わない", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "mainScreen", onScan }));
      const { end } = typeDigits(ISBN, { interval: 150 });
      expect(press("Enter", document.body, { at: end })).toBe(false);
      expect(onScan).not.toHaveBeenCalled();
    });

    it("テキスト入力欄にフォーカスがあるときは拾わない", () => {
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "mainScreen", onScan }));
      const search = document.createElement("input");
      search.type = "search";
      document.body.append(search);
      search.focus();
      const { end } = typeDigits(ISBN, { interval: 5, target: search });
      press("Enter", search, { at: end });
      expect(onScan).not.toHaveBeenCalled();
    });

    it("設定の閾値に従う", () => {
      useSettingsStore.getState().setScan({ keyIntervalMs: 100 });
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "mainScreen", onScan }));
      const { end } = typeDigits(ISBN, { interval: 80 });
      press("Enter", document.body, { at: end });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });
  });

  describe("設定（FR-SCAN-12）", () => {
    it("末尾キーが Tab のとき、Tab で確定する", () => {
      useSettingsStore.getState().setScan({ terminator: "tab" });
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      const { end } = typeDigits(ISBN);
      expect(press("Enter", document.body, { at: end })).toBe(false);
      expect(onScan).not.toHaveBeenCalled();
      const second = typeDigits(ISBN, { start: 3000 });
      expect(press("Tab", document.body, { at: second.end })).toBe(true);
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("末尾キーなしのとき、最後のキーから一定時間たつと確定する", () => {
      vi.useFakeTimers();
      useSettingsStore.getState().setScan({ terminator: "none", idleCommitMs: 100 });
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      typeDigits(ISBN);
      expect(onScan).not.toHaveBeenCalled();
      act(() => {
        vi.advanceTimersByTime(99);
      });
      expect(onScan).not.toHaveBeenCalled();
      act(() => {
        vi.advanceTimersByTime(2);
      });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("先頭・末尾の余分な数字を、設定の文字数ぶん除く", () => {
      useSettingsStore.getState().setScan({ stripLeading: 1, stripTrailing: 2 });
      const onScan = vi.fn();
      renderHook(() => useScanCapture({ mode: "scanScreen", onScan }));
      const { end } = typeDigits(`0${ISBN}99`);
      press("Enter", document.body, { at: end });
      expect(onScan).toHaveBeenCalledExactlyOnceWith(ISBN);
    });
  });
});
