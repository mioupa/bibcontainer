import { describe, expect, it } from "vitest";
import { DETAIL_COVER_SIZE, LIST_COVER_SIZE, coverUrl } from "@/lib/cover";
import { COVER_ACCENTS, coverAccent, coverAccentVar, hashString } from "@/lib/coverColor";
import { detectPlatform } from "@/lib/platform";

const HASH = "a1b2c3d4e5f60718293a4b5c6d7e8f901234567890abcdef1234567890abcdef";

describe("coverUrl（0005 §7）", () => {
  it("macOS は cover://localhost/<hash>?size=…", () => {
    expect(coverUrl(HASH, 80, "mac")).toBe(`cover://localhost/${HASH}?size=80`);
    expect(coverUrl(HASH, 160, "mac")).toBe(`cover://localhost/${HASH}?size=160`);
    expect(coverUrl(HASH, 320, "mac")).toBe(`cover://localhost/${HASH}?size=320`);
  });

  it("Windows は http://cover.localhost/<hash>?size=…", () => {
    expect(coverUrl(HASH, 80, "windows")).toBe(`http://cover.localhost/${HASH}?size=80`);
    expect(coverUrl(HASH, 160, "windows")).toBe(`http://cover.localhost/${HASH}?size=160`);
    expect(coverUrl(HASH, 320, "windows")).toBe(`http://cover.localhost/${HASH}?size=320`);
  });

  it("一覧は長辺 80px、詳細ペインとフォームは 320px のサムネイル（0010 §14）", () => {
    expect(LIST_COVER_SIZE).toBe(80);
    expect(DETAIL_COVER_SIZE).toBe(320);
  });

  it("版（同期で書影があとから届いた回数）が 1 以上なら、&v=<版> を足して URL を変える。0 なら付けない", () => {
    expect(coverUrl(HASH, 80, "mac", 0)).toBe(`cover://localhost/${HASH}?size=80`);
    expect(coverUrl(HASH, 80, "mac", 1)).toBe(`cover://localhost/${HASH}?size=80&v=1`);
    expect(coverUrl(HASH, 320, "windows", 3)).toBe(`http://cover.localhost/${HASH}?size=320&v=3`);
    // 版が違えば URL が違う（ブラウザは、同じ URL の失敗や古い応答を使い回せない）
    expect(coverUrl(HASH, 80, "mac", 1)).not.toBe(coverUrl(HASH, 80, "mac", 2));
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
