import { describe, expect, it } from "vitest";
import { isSortable, sortRows, type SortContext } from "@/features/library/sort";
import { makeRow } from "@/test/factories";

const ctx: SortContext = {
  locationPaths: new Map([
    ["l1", "自宅 > 書斎"],
    ["l2", "研究室"],
  ]),
};
const titles = (rows: { title: string }[]) => rows.map((r) => r.title);

describe("sortRows", () => {
  it("書名は sortTitle（ヨミがあればヨミ）の順に並ぶ", () => {
    const rows = [
      makeRow({ title: "魔法", sortTitle: "まほう" }),
      makeRow({ title: "愛", sortTitle: "あい" }),
      makeRow({ title: "Zebra", sortTitle: "zebra" }),
      makeRow({ title: "海", sortTitle: "うみ" }),
    ];
    expect(titles(sortRows(rows, [{ id: "title", desc: false }], ctx))).toEqual([
      "Zebra",
      "愛",
      "海",
      "魔法",
    ]);
    expect(titles(sortRows(rows, [{ id: "title", desc: true }], ctx))).toEqual([
      "魔法",
      "海",
      "愛",
      "Zebra",
    ]);
  });

  it("著者は sortAuthors の順に並ぶ", () => {
    const rows = [
      makeRow({ title: "a", sortAuthors: "すずき" }),
      makeRow({ title: "b", sortAuthors: "さとう" }),
      makeRow({ title: "c", sortAuthors: "たなか" }),
    ];
    expect(titles(sortRows(rows, [{ id: "authors", desc: false }], ctx))).toEqual(["b", "a", "c"]);
  });

  it("複数の列を組み合わせられる（先の列が優先）", () => {
    const rows = [
      makeRow({ title: "a", publisher: "B社", rating: 3 }),
      makeRow({ title: "b", publisher: "A社", rating: 1 }),
      makeRow({ title: "c", publisher: "B社", rating: 5 }),
      makeRow({ title: "d", publisher: "A社", rating: 4 }),
    ];
    const sorted = sortRows(
      rows,
      [
        { id: "publisher", desc: false },
        { id: "rating", desc: true },
      ],
      ctx,
    );
    expect(titles(sorted)).toEqual(["d", "b", "c", "a"]);
  });

  it("値のない行（null）は、昇順でも降順でも最後に置く", () => {
    const rows = [
      makeRow({ title: "none", rating: null }),
      makeRow({ title: "two", rating: 2 }),
      makeRow({ title: "five", rating: 5 }),
    ];
    expect(titles(sortRows(rows, [{ id: "rating", desc: false }], ctx))).toEqual([
      "two",
      "five",
      "none",
    ]);
    expect(titles(sortRows(rows, [{ id: "rating", desc: true }], ctx))).toEqual([
      "five",
      "two",
      "none",
    ]);
  });

  it("巻次は数値で比べる（10 巻が 2 巻より後）", () => {
    const rows = [
      makeRow({ title: "s10", sortTitle: "s", volume: "10", volumeSort: 10 }),
      makeRow({ title: "s2", sortTitle: "s", volume: "2", volumeSort: 2 }),
      makeRow({ title: "s1", sortTitle: "s", volume: "1", volumeSort: 1 }),
    ];
    expect(titles(sortRows(rows, [{ id: "volume", desc: false }], ctx))).toEqual([
      "s1",
      "s2",
      "s10",
    ]);
  });

  it("同じ値のときは、書名 → 巻次の順で決める（シリーズが巻の順に並ぶ）", () => {
    const rows = [
      makeRow({ title: "b3", sortTitle: "b", volumeSort: 3, publisher: "X" }),
      makeRow({ title: "a1", sortTitle: "a", volumeSort: 1, publisher: "X" }),
      makeRow({ title: "b1", sortTitle: "b", volumeSort: 1, publisher: "X" }),
    ];
    expect(titles(sortRows(rows, [{ id: "publisher", desc: false }], ctx))).toEqual([
      "a1",
      "b1",
      "b3",
    ]);
  });

  it("保管場所はパスの順に並ぶ。読書状況は未読→読書中→読了→中断", () => {
    const rows = [
      makeRow({ title: "lab", locationId: "l2", readStatus: "read" }),
      makeRow({ title: "study", locationId: "l1", readStatus: "unread" }),
      makeRow({ title: "none", locationId: null, readStatus: "paused" }),
    ];
    // 保管場所のパスはコード単位の順（「研」U+7814 は「自」U+81EA より前）。値のない行は最後
    expect(titles(sortRows(rows, [{ id: "location", desc: false }], ctx))).toEqual([
      "lab",
      "study",
      "none",
    ]);
    expect(titles(sortRows(rows, [{ id: "readStatus", desc: false }], ctx))).toEqual([
      "study",
      "lab",
      "none",
    ]);
  });

  it("元の配列を変えない。並べ替えできない列（書影）の指定は無視する", () => {
    const rows = [makeRow({ title: "b", sortTitle: "b" }), makeRow({ title: "a", sortTitle: "a" })];
    const copy = [...rows];
    const sorted = sortRows(rows, [{ id: "cover", desc: true }], ctx);
    expect(rows).toEqual(copy);
    expect(titles(sorted)).toEqual(["a", "b"]);
    expect(isSortable("cover")).toBe(false);
    expect(isSortable("title")).toBe(true);
  });

  it("日付の列は文字列として昇順に並ぶ（YYYY / YYYY-MM / YYYY-MM-DD が混ざっても）", () => {
    const rows = [
      makeRow({ title: "c", publishedOn: "2021-03-04" }),
      makeRow({ title: "a", publishedOn: "2020" }),
      makeRow({ title: "b", publishedOn: "2020-05" }),
    ];
    expect(titles(sortRows(rows, [{ id: "publishedOn", desc: false }], ctx))).toEqual([
      "a",
      "b",
      "c",
    ]);
  });
});
