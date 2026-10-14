import { describe, expect, it } from "vitest";
import { toHalfWidthDigits } from "@/features/scan/normalize";

describe("toHalfWidthDigits", () => {
  it("全角の数字を半角にする", () => {
    expect(toHalfWidthDigits("９７８４００３１０１１１７")).toBe("9784003101117");
    expect(toHalfWidthDigits("０１２３４５６７８９")).toBe("0123456789");
  });

  it("半角と全角が混ざっていても、数字だけを直す", () => {
    expect(toHalfWidthDigits("978４００3101117")).toBe("9784003101117");
  });

  it("ISBN-10 の検査数字の全角 Ｘ・ｘ を半角の X にする", () => {
    expect(toHalfWidthDigits("４００３１０１１１Ｘ")).toBe("400310111X");
    expect(toHalfWidthDigits("４００３１０１１１ｘ")).toBe("400310111X");
  });

  it("数字以外（ハイフン、空白、かな）はそのまま返す", () => {
    expect(toHalfWidthDigits("978-4-00-310111-7")).toBe("978-4-00-310111-7");
    expect(toHalfWidthDigits("ＩＳＢＮ ９７８")).toBe("ＩＳＢＮ 978");
    expect(toHalfWidthDigits("")).toBe("");
  });
});
