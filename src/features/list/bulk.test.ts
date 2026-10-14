import { describe, expect, it } from "vitest";
import { isEmptyBulkChange, tagsChange, tagsOfRows } from "@/features/list/bulk";

describe("一括変更（FR-BOOK-08）", () => {
  it("タグの変更は、前後の空白・空・重複を除く", () => {
    expect(tagsChange([" 小説 ", "", "小説", "研究/SiC"], [" 古い"])).toEqual({
      field: "tags",
      add: ["小説", "研究/SiC"],
      remove: ["古い"],
    });
  });

  it("足す・外すがどちらも空なら、何もしない変更", () => {
    expect(isEmptyBulkChange(tagsChange([" "], []))).toBe(true);
    expect(isEmptyBulkChange(tagsChange(["a"]))).toBe(false);
    expect(isEmptyBulkChange({ field: "rating", value: null })).toBe(false);
  });

  it("選んだ本のタグを、重複なく名前順に集める（「タグを外す」の選択肢）", () => {
    expect(tagsOfRows([{ tags: ["b", "a"] }, { tags: ["a", "c"] }, { tags: [] }])).toEqual([
      "a",
      "b",
      "c",
    ]);
  });
});
