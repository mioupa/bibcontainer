import { describe, expect, it } from "vitest";
import { compactIsbn, isValidIsbn10, isValidIsbn13, isbn10ToIsbn13, toIsbn13 } from "@/lib/isbn";

describe("compactIsbn", () => {
  it("全角を半角にし、ハイフン・空白・長音符を除く", () => {
    expect(compactIsbn(" 978-4-00-310111-7 ")).toBe("9784003101117");
    expect(compactIsbn("９７８４００３１０１１１７")).toBe("9784003101117");
    expect(compactIsbn("４－００－３１０１１１－１")).toBe("4003101111");
    expect(compactIsbn("4ー00ー310111ー1")).toBe("4003101111");
    expect(compactIsbn("080442957x")).toBe("080442957X");
  });
});

describe("チェックデジット", () => {
  it("ISBN-13（付録 B の 9784003101117）", () => {
    expect(isValidIsbn13("9784003101117")).toBe(true);
    expect(isValidIsbn13("9784003101118")).toBe(false);
    expect(isValidIsbn13("978400310111")).toBe(false);
  });
  it("ISBN-10（4003101111）と X の検査数字", () => {
    expect(isValidIsbn10("4003101111")).toBe(true);
    expect(isValidIsbn10("4003101112")).toBe(false);
    expect(isValidIsbn10("080442957X")).toBe(true);
  });
  it("ISBN-10 を ISBN-13 にする", () => {
    expect(isbn10ToIsbn13("4003101111")).toBe("9784003101117");
    expect(isbn10ToIsbn13("4003101112")).toBeNull();
  });
});

describe("toIsbn13", () => {
  it("ISBN-13・ISBN-10 をハイフン入りや全角でも受け付ける", () => {
    expect(toIsbn13("978-4-00-310111-7")).toBe("9784003101117");
    expect(toIsbn13("4-00-310111-1")).toBe("9784003101117");
    expect(toIsbn13("４００３１０１１１１")).toBe("9784003101117");
  });
  it("2段目（192…）・チェックデジット不一致・桁違いは null", () => {
    // AT-02 の 2段目。チェックデジットは合うが、978・979 で始まらないので ISBN ではない
    expect(toIsbn13("1920193007008")).toBeNull();
    expect(toIsbn13("9784003101118")).toBeNull();
    expect(toIsbn13("978400310")).toBeNull();
    expect(toIsbn13("研究")).toBeNull();
    expect(toIsbn13("")).toBeNull();
  });
});
