import { describe, expect, it } from "vitest";
import type { MetaMerged } from "@/bindings/MetaMerged";
import { candidatesFor, coverCandidates, showCandidates } from "@/features/editor/candidates";

const merged: MetaMerged = {
  isbn13: "9784780802047",
  fields: [
    {
      field: "title",
      chosen: { source: "openbd", value: { type: "text", value: "おにぎりレシピ101" } },
      others: [{ source: "ndl", value: { type: "text", value: "おにぎりレシピ 101" } }],
    },
    {
      field: "authors",
      chosen: {
        source: "openbd",
        value: { type: "authors", value: [{ name: "佐藤", kana: "さとう", role: "著" }] },
      },
      others: [],
    },
    {
      field: "list_price",
      chosen: { source: "barcode", value: { type: "number", value: 1300 } },
      others: [{ source: "openbd", value: { type: "number", value: 1280 } }],
    },
    {
      field: "publisher",
      chosen: { source: "openbd", value: { type: "text", value: "ポット出版" } },
      // 同じ値は 1 つにまとめる
      others: [{ source: "ndl", value: { type: "text", value: "ポット出版" } }],
    },
    { field: "ndc", chosen: null, others: [] },
    {
      field: "cover",
      chosen: {
        source: "openbd",
        value: { type: "covers", value: [{ provider: "openbd", url: "https://openbd/c.jpg" }] },
      },
      others: [
        {
          source: "google",
          value: { type: "covers", value: [{ provider: "google", url: "https://google/c.jpg" }] },
        },
      ],
    },
  ],
};

describe("取得元の候補（FR-META-05）", () => {
  it("採用した値、他の取得元の値の順。取得元つき", () => {
    const candidates = candidatesFor(merged, "title");
    expect(candidates.map((c) => [c.source, c.formValue])).toEqual([
      ["openbd", "おにぎりレシピ101"],
      ["ndl", "おにぎりレシピ 101"],
    ]);
  });

  it("数値は文字列にして入れる。Op の値は数値", () => {
    const [chosen, other] = candidatesFor(merged, "listPrice");
    expect(chosen).toMatchObject({ source: "barcode", formValue: "1300", opValue: 1300 });
    expect(other).toMatchObject({ source: "openbd", formValue: "1280", opValue: 1280 });
  });

  it("著者は行の形で入れ、メニューには「名前（役割）」を出す", () => {
    const [chosen] = candidatesFor(merged, "authors");
    expect(chosen?.formValue).toEqual([{ name: "佐藤", kana: "さとう", role: "著" }]);
    expect(chosen?.label).toBe("佐藤（著）");
  });

  it("同じ値は 1 つにまとめる。値がない項目・取得結果がないときは空", () => {
    expect(candidatesFor(merged, "publisher")).toHaveLength(1);
    expect(candidatesFor(merged, "ndc")).toEqual([]);
    expect(candidatesFor(null, "title")).toEqual([]);
    // 取得元の候補がない項目（ISBN、評価など）
    expect(candidatesFor(merged, "isbn13")).toEqual([]);
    expect(candidatesFor(merged, "rating")).toEqual([]);
  });

  it("書影の候補は、採用した書影を除いた、他の取得元の URL", () => {
    expect(coverCandidates(merged)).toEqual([{ source: "google", url: "https://google/c.jpg" }]);
    expect(coverCandidates(null)).toEqual([]);
  });

  it("メニューを出すのは、候補が 2 つ以上、または 1 つで今の値と違うとき", () => {
    const two = candidatesFor(merged, "title");
    expect(showCandidates(two, "おにぎりレシピ101")).toBe(true);
    const one = candidatesFor(merged, "authors");
    expect(showCandidates(one, one[0]?.opValue)).toBe(false);
    expect(showCandidates(one, [])).toBe(true);
    expect(showCandidates([], "x")).toBe(false);
  });
});
