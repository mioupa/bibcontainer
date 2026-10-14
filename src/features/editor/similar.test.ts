import { describe, expect, it } from "vitest";
import { canSearchSimilar, pickSimilarBooks, SIMILAR_LIMIT } from "@/features/editor/similar";
import { makeRow } from "@/test/factories";

describe("似た本の候補（FR-SCAN-19）", () => {
  it("書名が 2 文字以上のときだけ探す", () => {
    expect(canSearchSimilar("猫")).toBe(false);
    expect(canSearchSimilar(" 猫 ")).toBe(false);
    expect(canSearchSimilar("吾輩")).toBe(true);
  });

  it("検索で見つかった本から、ゴミ箱の本を除き、同じ著者の本を先に出す", () => {
    const rows = [
      makeRow({
        id: "a",
        title: "吾輩は猫である",
        authors: [{ name: "別の人", kana: null, role: null }],
      }),
      makeRow({
        id: "b",
        title: "吾輩は猫である 2",
        authors: [{ name: "夏目漱石", kana: null, role: null }],
      }),
      makeRow({ id: "c", title: "吾輩の猫", deleted: true }),
      makeRow({ id: "d", title: "無関係" }),
    ];
    const found = new Set(["a", "b", "c"]);
    expect(pickSimilarBooks(rows, found, ["夏目漱石"]).map((r) => r.id)).toEqual(["b", "a"]);
    // 著者を入れていなければ、一覧の順
    expect(pickSimilarBooks(rows, found, []).map((r) => r.id)).toEqual(["a", "b"]);
  });

  it("上限の冊数まで", () => {
    const rows = Array.from({ length: 10 }, (_, i) => makeRow({ id: `r${i}` }));
    const found = new Set(rows.map((r) => r.id));
    expect(pickSimilarBooks(rows, found, [])).toHaveLength(SIMILAR_LIMIT);
  });
});
