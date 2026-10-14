import { describe, expect, it } from "vitest";
import { pairingReducer, PAIRING_IDLE, type PairingState } from "@/features/scan/pairing";
import { LIST_CLOCK, pairIsbnList } from "@/features/scanScreen/isbnList";
import { ISBN_A, ISBN_B, ISBN_C } from "@/features/scanScreen/testData";
import type { ScanCode } from "@/types/contracts";

const isbn = (isbn13: string): ScanCode => ({ kind: "isbn13", raw: isbn13, isbn13, isbn10: null });
const second = (cCode: string, listPrice: number | null): ScanCode => ({
  kind: "secondTier",
  raw: `192${cCode}00000`,
  cCode,
  listPrice,
});
const CONFIG = { timeoutSec: 10 };

describe("pairIsbnList（ISBN 一覧のペアリング。FR-IO-03）", () => {
  it("ISBN だけの一覧は、1 行が 1 冊（2段目なし）で、ファイルの順に確定する", () => {
    const { books, skipped } = pairIsbnList([isbn(ISBN_A), isbn(ISBN_B), isbn(ISBN_C)], CONFIG);
    expect(books.map((b) => b.isbn13)).toEqual([ISBN_A, ISBN_B, ISBN_C]);
    expect(books.every((b) => b.secondTier === null)).toBe(true);
    expect(skipped).toEqual([]);
  });

  it("2段目が混ざっていれば、ISBN の後でも前でもペアにする", () => {
    const { books } = pairIsbnList(
      [isbn(ISBN_A), second("0193", 700), second("0095", 1200), isbn(ISBN_B), isbn(ISBN_C)],
      CONFIG,
    );
    expect(books.map((b) => [b.isbn13, b.secondTier?.cCode ?? null])).toEqual([
      [ISBN_A, "0193"],
      [ISBN_B, "0095"],
      [ISBN_C, null],
    ]);
  });

  it("最後の ISBN（保留中）も確定する", () => {
    const { books } = pairIsbnList([isbn(ISBN_A)], CONFIG);
    expect(books).toHaveLength(1);
  });

  it("行の間でタイムアウトしない（合成した時刻で通す）", () => {
    // 1 行ごとに実時間で 11 秒進んだとすると、タイムアウト（10 秒）で 2段目が取れなくなる
    let state: PairingState = PAIRING_IDLE;
    let paired = 0;
    const lines = [isbn(ISBN_A), second("0193", 700)];
    lines.forEach((code, index) => {
      const step = pairingReducer(state, { type: "scan", code, now: index * 11_000 }, CONFIG);
      state = step.state;
      paired += step.effects.filter(
        (e) => e.type === "confirm" && e.book.secondTier !== null,
      ).length;
    });
    expect(paired).toBe(0);

    // 合成時刻なら、同じ並びでペアになる
    expect(pairIsbnList(lines, CONFIG).books[0]?.secondTier?.cCode).toBe("0193");
  });

  it("行数が多くても、時刻は進まない（タイムアウトの最短 3 秒でも）", () => {
    const codes = Array.from({ length: 5000 }, (_, i) => isbn(`978${String(i).padStart(9, "0")}0`));
    const { books } = pairIsbnList(codes, { timeoutSec: 3 });
    expect(books).toHaveLength(5000);
    expect(LIST_CLOCK).toBe(0);
  });

  it("雑誌・一般 JAN・不正な行、二度読みは取り込まず、知らせとして返す", () => {
    const { books, skipped } = pairIsbnList(
      [
        isbn(ISBN_A),
        isbn(ISBN_A),
        { kind: "magazine", raw: "4910000000000" },
        { kind: "otherJan", raw: "4901234567894" },
        { kind: "invalid", raw: "123", reason: "format" },
        isbn(ISBN_B),
      ],
      CONFIG,
    );
    expect(books.map((b) => b.isbn13)).toEqual([ISBN_A, ISBN_B]);
    expect(skipped.map((m) => m.kind)).toEqual([
      "duplicateIsbn",
      "rejected",
      "rejected",
      "rejected",
    ]);
  });

  it("対応する ISBN のない 2段目は捨てる（知らせとして返す）", () => {
    const { books, skipped } = pairIsbnList([second("0193", 700)], CONFIG);
    expect(books).toEqual([]);
    expect(skipped.map((m) => m.kind)).toEqual(["secondTierDiscarded"]);
  });

  it("空の一覧は何も返さない", () => {
    expect(pairIsbnList([], CONFIG)).toEqual({ books: [], skipped: [] });
  });
});
