import { describe, expect, it } from "vitest";
import { detectIsbnSearch, resolveIsbnSearch } from "@/features/library/isbnSearch";
import { makeRow } from "@/test/factories";

describe("検索欄の ISBN 判定（FR-SCAN-09）", () => {
  it("ISBN-13 を、ハイフン・全角・空白・先頭の ISBN つきでも ISBN-13 にする", () => {
    expect(detectIsbnSearch("9784003101117")).toBe("9784003101117");
    expect(detectIsbnSearch(" 978-4-00-310111-7 ")).toBe("9784003101117");
    expect(detectIsbnSearch("９７８４００３１０１１１７")).toBe("9784003101117");
    expect(detectIsbnSearch("ISBN978-4-00-310111-7")).toBe("9784003101117");
    expect(detectIsbnSearch("ISBN: 9784003101117")).toBe("9784003101117");
  });

  it("ISBN-10 は ISBN-13 に直す（X の検査数字も）", () => {
    expect(detectIsbnSearch("4003101111")).toBe("9784003101117");
    expect(detectIsbnSearch("4-00-310111-1")).toBe("9784003101117");
    expect(detectIsbnSearch("080442957X")).toBe("9780804429573");
    expect(detectIsbnSearch("080442957x")).toBe("9780804429573");
  });

  it("ISBN でないものは null（ふつうの検索のまま）", () => {
    expect(detectIsbnSearch("")).toBeNull();
    expect(detectIsbnSearch("吾輩は猫である")).toBeNull();
    expect(detectIsbnSearch("夏目 漱石")).toBeNull();
    // チェックデジット不一致
    expect(detectIsbnSearch("9784003101118")).toBeNull();
    expect(detectIsbnSearch("4003101112")).toBeNull();
    // 2段目（AT-02）と、雑誌・一般 JAN は ISBN ではない
    expect(detectIsbnSearch("1920193007008")).toBeNull();
    // 発行年の範囲など、ハイフンを含む数字の検索語
    expect(detectIsbnSearch("2001-2010")).toBeNull();
    expect(detectIsbnSearch("12345")).toBeNull();
  });
});

describe("resolveIsbnSearch", () => {
  const rows = [
    makeRow({ id: "a", isbn13: "9784003101117", title: "登録済み" }),
    makeRow({ id: "b", isbn13: "9784780802047", title: "ゴミ箱の本", deleted: true }),
    makeRow({ id: "c", isbn13: "9784780802047", title: "同じ ISBN の 2 冊目" }),
  ];

  it("登録済みなら、その本（ISBN-10 で入力しても）", () => {
    expect(resolveIsbnSearch(rows, "9784003101117")).toEqual({ kind: "found", id: "a" });
    expect(resolveIsbnSearch(rows, "4003101111")).toEqual({ kind: "found", id: "a" });
  });

  it("同じ ISBN の本が複数あれば、ゴミ箱でない本を選ぶ", () => {
    expect(resolveIsbnSearch(rows, "9784780802047")).toEqual({ kind: "found", id: "c" });
  });

  it("ゴミ箱にだけあるときは、そのことを返す", () => {
    expect(resolveIsbnSearch(rows.slice(0, 2), "9784780802047")).toEqual({
      kind: "trashed",
      id: "b",
      title: "ゴミ箱の本",
    });
  });

  it("未登録の ISBN は、ISBN-13 を返す（登録の確認に使う）。ISBN でなければ none", () => {
    expect(resolveIsbnSearch(rows, "978-4-14-081900-5")).toEqual({
      kind: "unregistered",
      isbn13: "9784140819005",
    });
    expect(resolveIsbnSearch(rows, "夏目")).toEqual({ kind: "none" });
  });
});
