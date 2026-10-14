import { describe, expect, it } from "vitest";
import { PROVISIONAL_C_CODE_TABLE as table } from "@/data/cCodeTable";
import { cCodeFullLabel, cCodeNames, contentLabel, describeCCode, parseCCode } from "@/lib/cCode";

describe("Cコード", () => {
  it("4 桁に分解する。4 桁の数字でなければ null", () => {
    expect(parseCCode("0193")).toEqual({
      audience: "0",
      format: "1",
      contentMajor: "9",
      content: "93",
    });
    for (const bad of ["019", "01934", "abcd", "", null, undefined])
      expect(parseCCode(bad)).toBeNull();
  });

  it("付録 A の表で名称を引く（AT-02: 0193 は 一般・文庫・日本文学、小説・物語）", () => {
    expect(describeCCode(table, "0193")).toMatchObject({
      audience: "一般",
      format: "文庫",
      content: "日本文学、小説・物語",
      contentMajor: "文学",
    });
    expect(cCodeNames(table, "0193")).toBe("一般・文庫・日本文学、小説・物語");
    // 9.2 の表示: 数字と名称を並べる（全角空白）
    expect(cCodeFullLabel(table, "0193")).toBe("0193\u3000一般・文庫・日本文学、小説・物語");
    expect(cCodeFullLabel(table, "0077")).toBe("0077\u3000一般・単行本・家事");
    expect(cCodeFullLabel(table, "3055")).toBe("3055\u3000専門・単行本・電子通信");
  });

  it("表にない内容は「未定義（xx）」とし、大分類は使える（FR-SCAN-03）", () => {
    expect(contentLabel(table, "0194")).toBe("未定義（94）");
    expect(cCodeNames(table, "0194")).toBe("一般・文庫・未定義（94）");
    expect(describeCCode(table, "0194")?.contentMajor).toBe("文学");
  });

  it("コードがなければ null", () => {
    expect(contentLabel(table, null)).toBeNull();
    expect(cCodeFullLabel(table, "xx")).toBeNull();
  });

  it("表が空（まだ取得できていない）でも、コードの表示は壊れない", () => {
    const empty = { audience: {}, format: {}, contentMajor: {}, content: {} };
    expect(cCodeNames(empty, "0193")).toBe("未定義（93）");
  });
});
