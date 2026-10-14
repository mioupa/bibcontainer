import { describe, expect, it } from "vitest";
import {
  firstImagePath,
  imageBytesFromClipboardItems,
  imageBytesFromFiles,
  isSupportedCoverPath,
  normalizeImageUrl,
} from "@/features/editor/coverImport";

describe("書影の取り込み（FR-COVER-05）", () => {
  it("取り込める拡張子（JPEG・PNG・GIF・WebP）を、大文字小文字を問わず判定する", () => {
    expect(isSupportedCoverPath("/Users/me/書影.JPG")).toBe(true);
    expect(isSupportedCoverPath("C:\\img\\cover.webp")).toBe(true);
    expect(isSupportedCoverPath("/a/b/c.pdf")).toBe(false);
    expect(isSupportedCoverPath("/a/b/noext")).toBe(false);
  });

  it("ドロップされたパスのうち、最初の画像", () => {
    expect(firstImagePath(["/a/x.txt", "/a/y.png", "/a/z.jpg"])).toBe("/a/y.png");
    expect(firstImagePath(["/a/x.txt"])).toBeNull();
    expect(firstImagePath([])).toBeNull();
  });

  it("File（ドロップ・貼り付け）から画像のバイト列を取り出す", async () => {
    const png = new File([new Uint8Array([1, 2, 3])], "c.png", { type: "image/png" });
    const text = new File(["x"], "a.txt", { type: "text/plain" });
    expect(await imageBytesFromFiles([text, png])).toEqual(new Uint8Array([1, 2, 3]));
    expect(await imageBytesFromFiles([text])).toBeNull();
    expect(await imageBytesFromFiles(null)).toBeNull();
  });

  it("クリップボードの項目から、画像のバイト列を取り出す", async () => {
    const blob = new Blob([new Uint8Array([9, 8])], { type: "image/png" });
    const item = {
      types: ["text/plain", "image/png"],
      getType: (type: string) => Promise.resolve(type === "image/png" ? blob : new Blob(["t"])),
    } as unknown as ClipboardItem;
    expect(await imageBytesFromClipboardItems([item])).toEqual(new Uint8Array([9, 8]));
    const textOnly = { types: ["text/plain"], getType: () => Promise.resolve(new Blob(["t"])) };
    expect(await imageBytesFromClipboardItems([textOnly as unknown as ClipboardItem])).toBeNull();
  });

  it("画像 URL は http・https だけ", () => {
    expect(normalizeImageUrl(" https://example.com/c.jpg ")).toBe("https://example.com/c.jpg");
    expect(normalizeImageUrl("ftp://example.com/c.jpg")).toBeNull();
    expect(normalizeImageUrl("file:///a.png")).toBeNull();
    expect(normalizeImageUrl("")).toBeNull();
  });
});
