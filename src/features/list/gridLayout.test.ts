import { describe, expect, it } from "vitest";
import {
  gridColumnCount,
  gridIndex,
  gridMove,
  layoutGrid,
  type GridDirection,
} from "@/features/list/gridLayout";

// 3 列 × 3 行（最後の行は 1 つだけ）
//   k0 k1 k2
//   k3 k4 k5
//   k6
const keys = Array.from({ length: 7 }, (_, i) => `k${i}`);
const rows = layoutGrid(keys, 3);
const move = (from: string | null, direction: GridDirection, pageRows = 2) =>
  gridMove(rows, from, direction, pageRows);

describe("gridColumnCount", () => {
  // カード 128px、間隔 12px、両端の余白 12px
  it("両端の余白を除いた幅に、カードと間隔が何組入るかを数える", () => {
    // 1400 − 24 = 1376。n × 128 + (n − 1) × 12 ≤ 1376 を満たす最大の n は 9（9 個で 1248。10 個は 1388 で超える）
    expect(gridColumnCount(1400, 128, 12, 12)).toBe(9);
    // ちょうど 3 個ぶん（3 × 128 + 2 × 12 + 24 = 432）
    expect(gridColumnCount(432, 128, 12, 12)).toBe(3);
    expect(gridColumnCount(431, 128, 12, 12)).toBe(2);
  });

  it("狭くても、広さが怪しくても、1 以上", () => {
    expect(gridColumnCount(100, 128, 12, 12)).toBe(1);
    expect(gridColumnCount(0, 128, 12, 12)).toBe(1);
    expect(gridColumnCount(-50, 128, 12, 12)).toBe(1);
    expect(gridColumnCount(Number.NaN, 128, 12, 12)).toBe(1);
    expect(gridColumnCount(1400, 0, 12, 12)).toBe(1);
  });
});

describe("layoutGrid", () => {
  it("キーを順に、1 行に columns 個ずつ並べる。最後の行は短くてよい", () => {
    expect(rows.map((row) => row.keys)).toEqual([["k0", "k1", "k2"], ["k3", "k4", "k5"], ["k6"]]);
    expect(rows.every((row) => row.band === undefined)).toBe(true);
  });

  it("キーがなければ行もない。columns が 1 未満・小数・数でないときは 1 以上の整数に直す", () => {
    expect(layoutGrid([], 4)).toEqual([]);
    expect(layoutGrid(["a", "b"], 0).map((row) => row.keys)).toEqual([["a"], ["b"]]);
    expect(layoutGrid(["a", "b", "c"], 2.9).map((row) => row.keys)).toEqual([["a", "b"], ["c"]]);
    expect(layoutGrid(["a", "b"], Number.NaN).map((row) => row.keys)).toEqual([["a"], ["b"]]);
    expect(layoutGrid(["a", "b"], 10).map((row) => row.keys)).toEqual([["a", "b"]]);
  });

  it("元の並びを変えず、元の配列も書き換えない", () => {
    const source = ["x", "y", "z"];
    const grid = layoutGrid(source, 2);
    expect(grid.flatMap((row) => row.keys)).toEqual(["x", "y", "z"]);
    expect(source).toEqual(["x", "y", "z"]);
  });
});

describe("gridIndex", () => {
  it("キーから、何行目の何列目かを引ける", () => {
    const index = gridIndex(rows);
    expect(index.get("k0")).toEqual({ row: 0, col: 0 });
    expect(index.get("k5")).toEqual({ row: 1, col: 2 });
    expect(index.get("k6")).toEqual({ row: 2, col: 0 });
    expect(index.get("nope")).toBeUndefined();
    expect(index.size).toBe(7);
  });
});

describe("gridMove", () => {
  it("何も選んでいなければ、どの向きでも最初のキー（Home / End は、それぞれ最初・最後）", () => {
    for (const direction of ["left", "right", "up", "down", "pageUp", "pageDown"] as const) {
      expect(move(null, direction), direction).toBe("k0");
    }
    expect(move(null, "home")).toBe("k0");
    expect(move(null, "end")).toBe("k6");
    // 並びにないキーも、選んでいないのと同じ
    expect(move("gone", "down")).toBe("k0");
  });

  it("キーが 1 つもなければ null", () => {
    for (const direction of ["left", "right", "up", "down", "home", "end"] as const) {
      expect(gridMove([], null, direction, 3), direction).toBeNull();
      expect(gridMove([], "k0", direction, 3), direction).toBeNull();
    }
  });

  it("左右は、キーの並びで 1 つ前・後ろ。行の端を越えると、前の行の最後・次の行の最初へ", () => {
    expect(move("k1", "right")).toBe("k2");
    expect(move("k2", "right")).toBe("k3");
    expect(move("k5", "right")).toBe("k6");
    expect(move("k1", "left")).toBe("k0");
    expect(move("k3", "left")).toBe("k2");
    expect(move("k6", "left")).toBe("k5");
  });

  it("端では動かない（左は最初のキー、右は最後のキー）", () => {
    expect(move("k0", "left")).toBe("k0");
    expect(move("k6", "right")).toBe("k6");
  });

  it("上下は、前・次の行の同じ列。端の行では動かない", () => {
    expect(move("k4", "up")).toBe("k1");
    expect(move("k1", "down")).toBe("k4");
    expect(move("k0", "up")).toBe("k0");
    expect(move("k2", "up")).toBe("k2");
    expect(move("k6", "down")).toBe("k6");
  });

  it("行き先の行が短いときは、その行の最後のキーへ（最後の行が短い場合）", () => {
    expect(move("k4", "down")).toBe("k6");
    expect(move("k5", "down")).toBe("k6");
    expect(move("k3", "down")).toBe("k6");
    // そこから上へは、列 0 に戻る（列は覚えない）
    expect(move("k6", "up")).toBe("k3");
  });

  it("Home / End は、最初・最後のキー", () => {
    expect(move("k4", "home")).toBe("k0");
    expect(move("k4", "end")).toBe("k6");
  });

  it("PageUp / PageDown は pageRows 行ぶん動く。先頭・末尾の行で止まり、列は上下と同じ規則", () => {
    const wide = layoutGrid(
      Array.from({ length: 22 }, (_, i) => `w${i}`),
      4,
    ); // 6 行（最後の行は w20 w21 の 2 つ）
    expect(gridMove(wide, "w1", "pageDown", 2)).toBe("w9");
    expect(gridMove(wide, "w9", "pageUp", 2)).toBe("w1");
    expect(gridMove(wide, "w2", "pageUp", 5)).toBe("w2"); // 先頭の行で止まる
    // 末尾の行へ越えると、最後の行（短い）の最後のキー
    expect(gridMove(wide, "w14", "pageDown", 3)).toBe("w21");
    expect(gridMove(wide, "w21", "pageDown", 3)).toBe("w21");
    expect(gridMove(wide, "w13", "pageDown", 4)).toBe("w21");
    // pageRows は 1 以上に直す
    expect(gridMove(wide, "w1", "pageDown", 0)).toBe("w5");
    expect(gridMove(wide, "w1", "pageDown", Number.NaN)).toBe("w5");
  });

  it("1 列だけのグリッドでは、上下と左右の動きが同じになる", () => {
    const column = layoutGrid(["a", "b", "c"], 1);
    expect(gridMove(column, "a", "down", 1)).toBe("b");
    expect(gridMove(column, "b", "right", 1)).toBe("c");
    expect(gridMove(column, "b", "left", 1)).toBe("a");
    expect(gridMove(column, "b", "up", 1)).toBe("a");
  });

  it("gridIndex の結果を渡しても、渡さなくても、同じ答え", () => {
    const index = gridIndex(rows);
    for (const key of keys) {
      for (const direction of ["left", "right", "up", "down", "pageUp", "pageDown"] as const) {
        expect(gridMove(rows, key, direction, 2, index), `${key} ${direction}`).toBe(
          gridMove(rows, key, direction, 2),
        );
      }
    }
  });
});
