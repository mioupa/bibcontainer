// 性能の計測（10.1、AT-14）: 2 万行の絞り込み・並べ替え・件数の集計・巻のグループ化が、50ms 以内に収まること。
// 最初の読み込みで 1 回だけ走る巻のグループ化（全行の書名・著者の正規化から）だけは、300ms 以内（COLD_BUDGET_MS）。
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
import { buildWorkLayout, groupRows, layoutEntries } from "@/features/library/grouping";
import { sortRows, type SortContext } from "@/features/library/sort";
import { buildLocationIndex, buildLocationPaths } from "@/features/library/trees";
import type { ListRow } from "@/types/contracts";

const ROW_COUNT = 20_000;
/** 要件の目安。CI の機械が遅いときの余裕は見ていない（最速の回で判定する）。 */
const BUDGET_MS = 50;
/**
 * 最初の読み込みで 1 回だけ走る処理の上限。要件 10.1 の「検索の応答: 2 万冊でも 300ms 以内」に合わせた
 * （起動から一覧表示まで 2 秒以内、の中にも収まる）。手元の Mac では 50ms 以内だが、CI の機械では
 * 最速の回で 76ms（macOS）・100ms（Windows）だった（2026-10-17。0024）。
 */
const COLD_BUDGET_MS = 300;

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

/**
 * 毎回、行を作り直してから測る（行ごとの計算結果の覚え（WeakMap）が効かない状態）。作り直す時間は含めない。
 * 作品の計算は、行が変わるたびに、その行の分だけやり直す。全行が新しいのは、最初の読み込みのとき。
 */
function measureCold(fn: (fresh: ListRow[]) => unknown, runs = 7): { min: number; median: number } {
  const once = () => {
    const fresh = rows.map((row) => ({ ...row }));
    const start = performance.now();
    fn(fresh);
    return performance.now() - start;
  };
  once();
  once();
  const times: number[] = [];
  for (let i = 0; i < runs; i++) times.push(once());
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

  it("巻のグループ化（FR-LIST-08）: 行ごとの計算が済んでいる状態（絞り込みや並べ替えのやり直し）", () => {
    const sorted = sortRows(rows, [{ id: "title", desc: false }], sortCtx);
    const none: ReadonlySet<string> = new Set();
    const r = record("巻のグループ化（計算済みの行）", () => groupRows(sorted, true, none));
    expect(r.min).toBeLessThan(BUDGET_MS);
    // グループの数は、行の約 25% が作品の巻になるように作ったダミーデータなので、数百〜
    const grouped = groupRows(sorted, true, none);
    const inGroups =
      grouped.bookIds.length - grouped.entries.filter((e) => e.kind === "book").length;
    expect(grouped.groups.size).toBeGreaterThan(100);
    expect(inGroups).toBeGreaterThan(ROW_COUNT * 0.15);
    // ダミーデータは、約 2% の行が手動の作品、約 1% が「作品から外す」（0026）。手動の作品のグループも含めて測っている
    const manualGroups = [...grouped.groups.keys()].filter((key) => key.startsWith("work:\u001f"));
    expect(manualGroups.length).toBeGreaterThan(50);
  });

  it("巻のグループ化: 手動の指定がない行だけ（上書きのない蔵書。手動の作品の処理は走らない）", () => {
    const plain = sortRows(
      rows.map((row) => ({ ...row, workId: null })),
      [{ id: "title", desc: false }],
      sortCtx,
    );
    const none: ReadonlySet<string> = new Set();
    const r = record("巻のグループ化（上書きなし）", () => groupRows(plain, true, none));
    expect(r.min).toBeLessThan(BUDGET_MS);
  });

  it("巻のグループ化: 全行が新しい状態（最初の読み込み。書名・著者の正規化から）", () => {
    const none: ReadonlySet<string> = new Set();
    const cold = measureCold((fresh) => groupRows(fresh, true, none));
    results.push(["巻のグループ化（全行が新しい）", cold]);
    expect(cold.min).toBeLessThan(COLD_BUDGET_MS);
  });

  it("巻のグループ化: 展開の状態だけを変える（グループの計算は使い回す）", () => {
    const layout = buildWorkLayout(rows, true);
    const allOpen: ReadonlySet<string> = new Set(layout.groups.keys());
    const r = record("展開の状態を変える（全グループを展開）", () =>
      layoutEntries(layout, allOpen),
    );
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
