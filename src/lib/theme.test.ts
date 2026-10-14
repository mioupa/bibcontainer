import { afterEach, describe, expect, it, vi } from "vitest";
import { applyDisplay, applyTheme, fontStack, resolveTheme } from "@/lib/theme";

function mockMatchMedia(dark: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("dark") ? dark : false,
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
}

describe("テーマ（FR-CONF-04）", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    document.documentElement.className = "";
    document.documentElement.style.colorScheme = "";
  });

  it("resolveTheme: OS に連動／ライト／ダーク", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("applyTheme: ダークのときだけ html に dark クラスを付け、color-scheme を合わせる", () => {
    mockMatchMedia(false);
    applyTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");

    applyTheme("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.style.colorScheme).toBe("light");
  });

  it("applyTheme: system は OS の設定に従う", () => {
    mockMatchMedia(true);
    applyTheme("system");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    mockMatchMedia(false);
    applyTheme("system");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("matchMedia がない環境では、ライトになる", () => {
    vi.stubGlobal("matchMedia", undefined);
    applyTheme("system");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});

describe("表示（フォントと文字サイズ。FR-CONF-04）", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("style");
  });

  it("文字サイズは <html> の font-size に反映する。既定（16px）のときは指定を外す", () => {
    applyDisplay({ fontFamily: null, fontSize: 20 });
    expect(document.documentElement.style.fontSize).toBe("20px");
    applyDisplay({ fontFamily: null, fontSize: 16 });
    expect(document.documentElement.style.fontSize).toBe("");
  });

  it("フォントを指定すると --font-sans に、OS の既定のフォントを後ろに続けて入れる。null で外す", () => {
    applyDisplay({ fontFamily: "Hiragino Mincho ProN", fontSize: 16 });
    const value = document.documentElement.style.getPropertyValue("--font-sans");
    expect(value.startsWith('"Hiragino Mincho ProN", ')).toBe(true);
    expect(value).toContain("system-ui");
    applyDisplay({ fontFamily: null, fontSize: 16 });
    expect(document.documentElement.style.getPropertyValue("--font-sans")).toBe("");
  });

  it("フォント名に宣言を壊す文字が入っていても、取り除いて使う", () => {
    expect(fontStack('a"; color: red; {x}')).toMatch(/^"a color: red x", /);
    expect(fontStack("   ")).not.toContain('""');
    expect(fontStack(null)).toContain("sans-serif");
  });
});
