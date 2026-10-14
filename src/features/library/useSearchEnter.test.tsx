import { act, fireEvent, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isEnterKey, isImeCommitKey, useSearchEnter } from "@/features/library/useSearchEnter";

const ISBN = "9784003101117";
const ISBN_FULLWIDTH = "９７８４００３１０１１１７";

function SearchBox({ onSubmit }: { onSubmit: (text: string) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const enter = useSearchEnter(ref, onSubmit);
  return <input ref={ref} type="search" aria-label="検索" defaultValue="" {...enter} />;
}

function setup() {
  const onSubmit = vi.fn();
  render(<SearchBox onSubmit={onSubmit} />);
  const input = screen.getByRole<HTMLInputElement>("searchbox");
  return { onSubmit, input };
}

/** 次のタスクまで進める（確定の反映を待ってから値を見るため）。 */
const settle = (ms = 1) => {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
};

// IME の動きの、ブラウザごとの違い（このアプリの対象は WKWebView と WebView2）
// - Chromium（WebView2）: keydown（key "Process"・isComposing true・keyCode 229）→ compositionend（値は確定済み）
// - Safari（WKWebView）: compositionend → keydown（key は "Enter" または "Process"・isComposing false・keyCode 229）
const compose = (input: HTMLInputElement, text: string) => {
  fireEvent.compositionStart(input);
  fireEvent.change(input, { target: { value: text } }); // 変換中の文字（未確定）
};
const commitKeyChromium = (input: HTMLInputElement) =>
  fireEvent.keyDown(input, { key: "Process", code: "Enter", keyCode: 229, isComposing: true });
const commitKeySafari = (input: HTMLInputElement) =>
  fireEvent.keyDown(input, { key: "Enter", code: "Enter", keyCode: 229, isComposing: false });

describe("useSearchEnter（検索欄の Enter。FR-SCAN-08／09）", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  describe("通常の Enter", () => {
    it("日本語入力が変換中でなければ、Enter で、欄の値を渡す（すぐ）", () => {
      const { onSubmit, input } = setup();
      fireEvent.change(input, { target: { value: ISBN } });
      fireEvent.keyDown(input, { key: "Enter", code: "Enter", keyCode: 13 });
      expect(onSubmit).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("テンキーの Enter でも同じ", () => {
      const { onSubmit, input } = setup();
      fireEvent.change(input, { target: { value: ISBN } });
      fireEvent.keyDown(input, { key: "Enter", code: "NumpadEnter", keyCode: 13 });
      expect(onSubmit).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("Enter 以外のキーでは呼ばない", () => {
      const { onSubmit, input } = setup();
      fireEvent.change(input, { target: { value: ISBN } });
      fireEvent.keyDown(input, { key: "a", code: "KeyA", keyCode: 65 });
      fireEvent.keyDown(input, { key: "Tab", code: "Tab", keyCode: 9 });
      settle();
      expect(onSubmit).not.toHaveBeenCalled();
    });
  });

  describe("日本語入力がオンのまま、ISBN を読んだとき（1 回の Enter で確定する）", () => {
    it("Chromium（WebView2）: 変換中の Enter で確定した値が ISBN の形なら、確定として扱う（全角の数字でも）", () => {
      const { onSubmit, input } = setup();
      compose(input, ISBN_FULLWIDTH);
      commitKeyChromium(input);
      expect(onSubmit).not.toHaveBeenCalled(); // 確定が反映されるまで待つ
      fireEvent.compositionEnd(input, { data: ISBN_FULLWIDTH });
      settle();
      expect(onSubmit).toHaveBeenCalledExactlyOnceWith(ISBN_FULLWIDTH);
    });

    it("Safari（WKWebView）: compositionend のあとに届く確定の Enter でも、確定として扱う", () => {
      const { onSubmit, input } = setup();
      compose(input, ISBN_FULLWIDTH);
      fireEvent.compositionEnd(input, { data: ISBN_FULLWIDTH });
      commitKeySafari(input);
      settle();
      expect(onSubmit).toHaveBeenCalledExactlyOnceWith(ISBN_FULLWIDTH);
    });

    it("半角の数字が変換中に入っていても同じ。確定のあとの欄の値を渡す", () => {
      const { onSubmit, input } = setup();
      compose(input, "978400310111"); // 途中
      commitKeyChromium(input);
      fireEvent.change(input, { target: { value: ISBN } }); // 確定の反映で値が変わる
      fireEvent.compositionEnd(input);
      settle();
      expect(onSubmit).toHaveBeenCalledExactlyOnceWith(ISBN);
    });

    it("1 回のスキャンで、2 回は呼ばない（確定のあとの通常の Enter が続かなくても、続いても）", () => {
      const { onSubmit, input } = setup();
      compose(input, ISBN_FULLWIDTH);
      commitKeyChromium(input);
      fireEvent.compositionEnd(input);
      settle();
      settle();
      expect(onSubmit).toHaveBeenCalledOnce();
    });
  });

  describe("普通の日本語入力を壊さない", () => {
    it("変換中の Enter で変換を確定するだけ（ISBN の形でない）なら、何も渡さない", () => {
      const { onSubmit, input } = setup();
      compose(input, "吾輩は猫である");
      commitKeyChromium(input);
      fireEvent.compositionEnd(input, { data: "吾輩は猫である" });
      settle();
      expect(onSubmit).not.toHaveBeenCalled();

      compose(input, "わがはい");
      commitKeySafari(input);
      settle();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it("数字の検索語（発行年など。ISBN ではない）の変換の確定でも、何も渡さない", () => {
      const { onSubmit, input } = setup();
      compose(input, "２００１");
      commitKeyChromium(input);
      fireEvent.compositionEnd(input);
      settle();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it("変換中の Enter だけ（確定が来ない）では、渡さない", () => {
      const { onSubmit, input } = setup();
      compose(input, ISBN_FULLWIDTH);
      commitKeyChromium(input);
      settle();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it("Enter を伴わない変換の確定（候補の選択・欄の外のクリック）では、ISBN の形でも渡さない", () => {
      const { onSubmit, input } = setup();
      compose(input, ISBN_FULLWIDTH);
      fireEvent.compositionEnd(input, { data: ISBN_FULLWIDTH });
      settle();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it("Enter から時間がたってから届いた compositionend や、間にほかのキーがあった場合は、確定として扱わない", () => {
      const { onSubmit, input } = setup();
      compose(input, ISBN_FULLWIDTH);
      commitKeyChromium(input);
      settle(600);
      fireEvent.compositionEnd(input);
      settle();
      expect(onSubmit).not.toHaveBeenCalled();

      compose(input, ISBN_FULLWIDTH);
      commitKeyChromium(input);
      fireEvent.keyDown(input, { key: "a", code: "KeyA", keyCode: 65 });
      fireEvent.compositionEnd(input);
      settle();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it("確定のあとの通常の Enter は、今までどおり、すぐ渡す（ISBN でなければ受け手が無視する）", () => {
      const { onSubmit, input } = setup();
      compose(input, "吾輩");
      commitKeyChromium(input);
      fireEvent.compositionEnd(input);
      settle();
      fireEvent.keyDown(input, { key: "Enter", code: "Enter", keyCode: 13 });
      expect(onSubmit).toHaveBeenCalledExactlyOnceWith("吾輩");
    });
  });

  it("画面を離れたら（アンマウント）、待っていた確認はしない", () => {
    const onSubmit = vi.fn();
    const { unmount } = render(<SearchBox onSubmit={onSubmit} />);
    const input = screen.getByRole<HTMLInputElement>("searchbox");
    compose(input, ISBN_FULLWIDTH);
    commitKeyChromium(input);
    fireEvent.compositionEnd(input);
    unmount();
    settle(10);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});

describe("キーの判定", () => {
  it("Enter は、key か code（変換中は key が Process になる）で見る", () => {
    expect(isEnterKey({ key: "Enter", code: "Enter" })).toBe(true);
    expect(isEnterKey({ key: "Process", code: "Enter" })).toBe(true);
    expect(isEnterKey({ key: "Enter", code: "NumpadEnter" })).toBe(true);
    expect(isEnterKey({ key: "a", code: "KeyA" })).toBe(false);
  });

  it("変換を確定する Enter は、isComposing・Process・keyCode 229 のどれかで見る", () => {
    expect(isImeCommitKey({ key: "Enter", isComposing: true, keyCode: 13 })).toBe(true);
    expect(isImeCommitKey({ key: "Process", isComposing: false, keyCode: 13 })).toBe(true);
    expect(isImeCommitKey({ key: "Enter", isComposing: false, keyCode: 229 })).toBe(true);
    expect(isImeCommitKey({ key: "Enter", isComposing: false, keyCode: 13 })).toBe(false);
  });
});
