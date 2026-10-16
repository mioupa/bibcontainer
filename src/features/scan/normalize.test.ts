import { describe, expect, it } from "vitest";
import { isCompleteCode, normalizeManualInput, toHalfWidthDigits } from "@/features/scan/normalize";

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

describe("normalizeManualInput", () => {
  it("全角を半角にし、空白（全角を含む）を取り除く", () => {
    expect(normalizeManualInput("９７８　４００３ １０１１１７ ")).toBe("9784003101117");
    expect(normalizeManualInput("　 ")).toBe("");
  });

  it("ハイフンなど、空白以外はそのまま（判定は Rust に任せる）", () => {
    expect(normalizeManualInput("978-4-00-310111-7")).toBe("978-4-00-310111-7");
  });
});

describe("isCompleteCode", () => {
  it("13 桁の数字は完全", () => {
    expect(isCompleteCode("9784003101117")).toBe(true);
    expect(isCompleteCode("1920193007008")).toBe(true);
  });

  it("10 桁は完全。末尾だけ X・x でもよい", () => {
    expect(isCompleteCode("4003101111")).toBe(true);
    expect(isCompleteCode("400310111X")).toBe(true);
    expect(isCompleteCode("400310111x")).toBe(true);
  });

  it("途中までの桁数、余分な桁、途中の X、数字以外を含む値は完全ではない", () => {
    expect(isCompleteCode("")).toBe(false);
    expect(isCompleteCode("9784")).toBe(false);
    expect(isCompleteCode("978400310111")).toBe(false);
    expect(isCompleteCode("97840031011177")).toBe(false);
    expect(isCompleteCode("40031011X1")).toBe(false);
    expect(isCompleteCode("978-4-00-310111-7")).toBe(false);
    expect(isCompleteCode("９７８４００３１０１１１７")).toBe(false);
  });
});
