import { describe, expect, it } from "vitest";
import { coverUrl } from "@/lib/cover";
import { COVER_ACCENTS, coverAccent, coverAccentVar, hashString } from "@/lib/coverColor";
import { detectPlatform } from "@/lib/platform";

const HASH = "a1b2c3d4e5f60718293a4b5c6d7e8f901234567890abcdef1234567890abcdef";

describe("coverUrl（0005 §7）", () => {
  it("macOS は cover://localhost/<hash>?size=…", () => {
    expect(coverUrl(HASH, 160, "mac")).toBe(`cover://localhost/${HASH}?size=160`);
    expect(coverUrl(HASH, 320, "mac")).toBe(`cover://localhost/${HASH}?size=320`);
  });

  it("Windows は http://cover.localhost/<hash>?size=…", () => {
    expect(coverUrl(HASH, 160, "windows")).toBe(`http://cover.localhost/${HASH}?size=160`);
    expect(coverUrl(HASH, 320, "windows")).toBe(`http://cover.localhost/${HASH}?size=320`);
  });

  it("OS を省略すると、User-Agent から判定する", () => {
    // jsdom の User-Agent は Windows ではないので mac 形式
    expect(coverUrl(HASH, 160)).toBe(`cover://localhost/${HASH}?size=160`);
  });

  it("hash に URL で使えない文字があっても、パスを壊さない", () => {
    expect(coverUrl("a/b?c", 160, "mac")).toBe("cover://localhost/a%2Fb%3Fc?size=160");
  });
});

describe("detectPlatform", () => {
  it("Windows の User-Agent だけを windows とし、それ以外は mac", () => {
    expect(
      detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/130.0"),
    ).toBe("windows");
    expect(
      detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15"),
    ).toBe("mac");
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("mac");
    expect(detectPlatform("")).toBe("mac");
  });
});

describe("仮の表紙の色", () => {
  it("同じ ID なら、いつでも同じ色になる", () => {
    const id = "0192f0a0-1234-7abc-8def-0123456789ab";
    expect(coverAccent(id)).toBe(coverAccent(id));
    expect(coverAccentVar(id)).toBe(`var(--ctp-${coverAccent(id)})`);
  });

  it("ID が違えば色が散らばる（偏らない）", () => {
    const used = new Set<string>();
    for (let i = 0; i < 500; i++) used.add(coverAccent(`id-${i}`));
    expect(used.size).toBe(COVER_ACCENTS.length);
  });

  it("ハッシュは 32 ビットの非負整数", () => {
    for (const text of ["", "a", "日本語", "x".repeat(1000)]) {
      const h = hashString(text);
      expect(Number.isInteger(h)).toBe(true);
      expect(h).toBeGreaterThanOrEqual(0);
      expect(h).toBeLessThan(2 ** 32);
    }
  });
});
