// 性能の計測（10.1、AT-14）: 2 万行の絞り込み・並べ替え・件数の集計が、50ms 以内に収まること。
// 計測値は console に出す（`pnpm vitest run src/features/library/perf.test.ts` で確認できる）。
import { beforeAll, describe, expect, it } from "vitest";
import { generateMockRows, MOCK_LOCATIONS } from "@/dev/mockRows";
import {
  DEFAULT_FILTER,
  computeFacets,
  filterRows,
  type BookFilter,
  type FilterContext,
} from "@/features/library/filter";
import { sortRows, type SortContext } from "@/features/library/sort";
import { buildLocationIndex, buildLocationPaths } from "@/features/library/trees";
import type { ListRow } from "@/types/contracts";

const ROW_COUNT = 20_000;
/** 要件の目安。CI の機械が遅いときの余裕は見ていない（最速の回で判定する）。 */
const BUDGET_MS = 50;

let rows: ListRow[];
let ctx: FilterContext;
let sortCtx: SortContext;
let searchIds: Set<string>;

/** 1 回ぶんの時間（ミリ秒）。ウォームアップのあと、数回計って最速と中央値を返す。 */
function measure(fn: () => unknown, runs = 7): { min: number; median: number } {
  fn();
  fn();
  const times: number[] = [];
  for (let i = 0; i < runs; i++) {
    const start = performance.now();
    fn();
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  return { min: times[0] ?? 0, median: times[Math.floor(times.length / 2)] ?? 0 };
}

const results: [string, { min: number; median: number }][] = [];
function record(name: string, fn: () => unknown) {
  const result = measure(fn);
  results.push([name, result]);
  return result;
}

describe(`${ROW_COUNT.toLocaleString("ja-JP")} 行の性能`, () => {
  beforeAll(() => {
    rows = generateMockRows(ROW_COUNT);
    const locationIndex = buildLocationIndex(MOCK_LOCATIONS);
    // 検索結果の ID の集合（全体の 1/3 くらい）
    searchIds = new Set(rows.filter((_, i) => i % 3 === 0).map((r) => r.id));
    ctx = { searchIds: null, locationIndex };
    sortCtx = { locationPaths: buildLocationPaths(MOCK_LOCATIONS) };
  });

  const filters: [string, BookFilter][] = [
    ["ライブラリ=所有", { ...DEFAULT_FILTER, library: "owned" }],
    ["保管場所=自宅（子孫を含む）", { ...DEFAULT_FILTER, locationId: "loc-home" }],
    ["タグ=研究（階層）", { ...DEFAULT_FILTER, tag: "研究" }],
    [
      "ライブラリ+読書状況+自動分類(出版社)",
      {
        ...DEFAULT_FILTER,
        library: "owned",
        readStatus: "read",
        auto: { kind: "publisher", value: "岩波書店" },
      },
    ],
  ];

  it.each(filters)("絞り込み: %s", (_name, filter) => {
    const r = record(`絞り込み: ${_name}`, () => filterRows(rows, filter, ctx));
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("絞り込み: 検索結果の ID の集合との掛け合わせ", () => {
    const withSearch = { ...ctx, searchIds };
    const r = record("絞り込み: 検索結果の ID と掛け合わせ", () =>
      filterRows(rows, DEFAULT_FILTER, withSearch),
    );
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("サイドバーの件数の集計（全グループ 1 回の走査）", () => {
    const r = record("件数の集計（絞り込みなし）", () => computeFacets(rows, DEFAULT_FILTER, ctx));
    expect(r.min).toBeLessThan(BUDGET_MS);
    const r2 = record("件数の集計（所有+タグ）", () =>
      computeFacets(rows, { ...DEFAULT_FILTER, library: "owned", tag: "研究" }, ctx),
    );
    expect(r2.min).toBeLessThan(BUDGET_MS);
  });

  it("並べ替え: 書名（sortTitle）", () => {
    const r = record("並べ替え: 書名", () =>
      sortRows(rows, [{ id: "title", desc: false }], sortCtx),
    );
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("並べ替え: 著者", () => {
    const r = record("並べ替え: 著者", () =>
      sortRows(rows, [{ id: "authors", desc: true }], sortCtx),
    );
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("並べ替え: 3 列の組み合わせ（出版社 → 評価 → 書名）", () => {
    const r = record("並べ替え: 3 列の組み合わせ", () =>
      sortRows(
        rows,
        [
          { id: "publisher", desc: false },
          { id: "rating", desc: true },
          { id: "title", desc: false },
        ],
        sortCtx,
      ),
    );
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("並べ替え: 保管場所（パスの文字列）", () => {
    const r = record("並べ替え: 保管場所", () =>
      sortRows(rows, [{ id: "location", desc: false }], sortCtx),
    );
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("全体: 絞り込み → 並べ替え → 件数の集計を続けて行う（サイドバーを 1 回押したときの処理）", () => {
    const filter: BookFilter = { ...DEFAULT_FILTER, library: "owned", tag: "研究" };
    const r = record("全体（絞り込み+並べ替え+件数の集計）", () => {
      const filtered = filterRows(rows, filter, ctx);
      sortRows(filtered, [{ id: "title", desc: false }], sortCtx);
      computeFacets(rows, filter, ctx);
    });
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("計測値の一覧", () => {
    const lines = results.map(
      ([name, r]) => `${name}: 最速 ${r.min.toFixed(1)} ms / 中央値 ${r.median.toFixed(1)} ms`,
    );
    console.info(
      `\n[性能] ${ROW_COUNT.toLocaleString("ja-JP")} 行（目安 ${BUDGET_MS}ms 以内）\n${lines.join("\n")}`,
    );
    expect(results.length).toBeGreaterThan(0);
  });
});
