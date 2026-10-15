import { describe, expect, it } from "vitest";
import type { MetaSource } from "@/bindings/MetaSource";
import { originFromSource, originSourceName, sourceLabel } from "@/features/editor/sources";
import { ja } from "@/i18n/ja";

describe("取得元の表示", () => {
  it("すべての取得元に表示名がある（Open Library を含む）", () => {
    const sources: MetaSource[] = ["barcode", "openbd", "ndl", "google", "openlibrary"];
    for (const source of sources) {
      // 設定画面の優先順位とフォームの取得元の両方
      expect(ja.settings.providers.sources[source]).toBeTruthy();
      expect(sourceLabel(source)).not.toBe(source);
    }
    expect(sourceLabel("openlibrary")).toBe("Open Library");
    expect(ja.settings.providers.sources.openlibrary).toBe("Open Library");
  });

  it("取得元の文字列は、取得元の由来（provider）に戻せる", () => {
    expect(originFromSource("openlibrary")).toEqual({ kind: "provider", name: "openlibrary" });
    expect(originSourceName({ kind: "provider", name: "openlibrary" })).toBe("openlibrary");
  });

  it("知らない取得元は、そのまま出す", () => {
    expect(sourceLabel("unknown-source")).toBe("unknown-source");
  });
});
