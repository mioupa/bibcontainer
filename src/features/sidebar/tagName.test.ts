import { describe, expect, it } from "vitest";
import { renamedTagPath, validateTagName } from "@/features/sidebar/tagName";

describe("validateTagName", () => {
  it("前後の空白を除いた名前を返す。階層（/）は使える", () => {
    expect(validateTagName("研究", " 研究室 ")).toEqual({ ok: true, name: "研究室" });
    expect(validateTagName("研究", "仕事/研究")).toEqual({ ok: true, name: "仕事/研究" });
  });

  it("空・空の階層ができる名前・同じ名前はエラー", () => {
    expect(validateTagName("研究", "  ")).toEqual({ ok: false, error: "empty" });
    for (const bad of ["/研究", "研究/", "研究//SiC", "研究/ /SiC"]) {
      expect(validateTagName("x", bad)).toEqual({ ok: false, error: "invalid" });
    }
    expect(validateTagName("研究", " 研究 ")).toEqual({ ok: false, error: "same" });
  });
});

describe("renamedTagPath", () => {
  it("改名したタグと子孫の絞り込みは、新しい名前に追随する。無関係なタグは変えない", () => {
    expect(renamedTagPath("研究", "研究", "仕事")).toBe("仕事");
    expect(renamedTagPath("研究/SiC", "研究", "仕事")).toBe("仕事/SiC");
    expect(renamedTagPath("研究室", "研究", "仕事")).toBe("研究室");
    expect(renamedTagPath("旧/研究", "研究", "仕事")).toBe("旧/研究");
    expect(renamedTagPath(null, "研究", "仕事")).toBeNull();
  });
});
