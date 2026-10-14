import { describe, expect, it } from "vitest";
import {
  COLUMN_IDS,
  moveColumnId,
  resolveColumnOrder,
  sanitizeColumnOrder,
} from "@/features/list/columnMeta";

describe("列の並び順（FR-LIST-01）", () => {
  it("保存された並びが空なら、既定の順", () => {
    expect(resolveColumnOrder([])).toEqual([...COLUMN_IDS]);
  });

  it("保存された並びを先頭に置き、載っていない列は既定の順のまま末尾に足す", () => {
    const order = resolveColumnOrder(["rating", "title"]);
    expect(order.slice(0, 2)).toEqual(["rating", "title"]);
    expect(order).toHaveLength(COLUMN_IDS.length);
    expect(order.slice(2)).toEqual(COLUMN_IDS.filter((id) => id !== "rating" && id !== "title"));
  });

  it("知らない列と重複は捨てる", () => {
    expect(resolveColumnOrder(["bogus", "title", "title", ""]).slice(0, 2)).toEqual([
      "title",
      "cover",
    ]);
    expect(sanitizeColumnOrder(["title", "bogus", "title", "cover"])).toEqual(["title", "cover"]);
    expect(sanitizeColumnOrder([])).toEqual([]);
  });

  it("1 つ上・下へ動かすと、隣と入れ替わった全列の並びを返す", () => {
    const up = moveColumnId([], "authors", "up");
    expect(up.slice(0, 4)).toEqual(["cover", "title", "authors", "volume"]);
    expect(up).toHaveLength(COLUMN_IDS.length);
    const down = moveColumnId(up, "authors", "down");
    expect(down).toEqual([...COLUMN_IDS]);
  });

  it("先頭を上へ、末尾を下へは動かせない（並びは変わらない）", () => {
    expect(moveColumnId([], "cover", "up")).toEqual([...COLUMN_IDS]);
    const last = COLUMN_IDS.at(-1)!;
    expect(moveColumnId([], last, "down")).toEqual([...COLUMN_IDS]);
  });

  it("入力の並びは書き換えない", () => {
    const saved = ["rating", "title"];
    moveColumnId(saved, "title", "up");
    expect(saved).toEqual(["rating", "title"]);
  });
});
