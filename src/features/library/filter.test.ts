import { describe, expect, it } from "vitest";
import {
  DEFAULT_FILTER,
  computeFacets,
  filterRows,
  isFilterActive,
  type BookFilter,
  type FilterContext,
} from "@/features/library/filter";
import { UNSET } from "@/features/library/keys";
import { buildLocationIndex } from "@/features/library/trees";
import { makeRow } from "@/test/factories";
import type { Location } from "@/types/contracts";

const locations: Location[] = [
  { id: "home", name: "自宅", parentId: null, sortOrder: 1 },
  { id: "study", name: "書斎", parentId: "home", sortOrder: 1 },
  { id: "shelf", name: "本棚", parentId: "study", sortOrder: 1 },
  { id: "lab", name: "研究室", parentId: null, sortOrder: 2 },
];
const locationIndex = buildLocationIndex(locations);
const ctx = (searchIds: ReadonlySet<string> | null = null): FilterContext => ({
  searchIds,
  locationIndex,
});
const filterWith = (patch: Partial<BookFilter>): BookFilter => ({ ...DEFAULT_FILTER, ...patch });
const ids = (rows: { id: string }[]) => rows.map((r) => r.id);

const rows = [
  makeRow({
    id: "a",
    ownership: "owned",
    readStatus: "read",
    locationId: "shelf",
    tags: ["研究/SiC", "小説"],
    cCode: "0193",
    publisher: "岩波書店",
    publishedOn: "2020-05-01",
    authors: [{ name: "佐藤 太郎", kana: null, role: null }],
  }),
  makeRow({
    id: "b",
    ownership: "owned",
    readStatus: "unread",
    locationId: "lab",
    tags: ["研究/パワー半導体"],
    cCode: "3055",
    publisher: "オーム社",
    publishedOn: "2020",
    authors: [
      { name: "佐藤 太郎", kana: null, role: null },
      { name: "鈴木 花子", kana: null, role: null },
    ],
  }),
  makeRow({
    id: "c",
    ownership: "wanted",
    readStatus: "unread",
    cCode: "0077",
    metaStatus: "pending",
  }),
  makeRow({
    id: "d",
    ownership: "released",
    readStatus: "paused",
    publishedOn: "1999-12",
    metaStatus: "partial",
  }),
  makeRow({
    id: "e",
    ownership: "owned",
    readStatus: "read",
    deleted: true,
    metaStatus: "failed",
    cCode: "0193",
  }),
];

describe("filterRows（ライブラリ）", () => {
  it("すべて: ゴミ箱の本を出さない", () => {
    expect(ids(filterRows(rows, DEFAULT_FILTER, ctx()))).toEqual(["a", "b", "c", "d"]);
  });

  it("所有・欲しい本・手放した: ゴミ箱の本を出さない", () => {
    expect(ids(filterRows(rows, filterWith({ library: "owned" }), ctx()))).toEqual(["a", "b"]);
    expect(ids(filterRows(rows, filterWith({ library: "wanted" }), ctx()))).toEqual(["c"]);
    expect(ids(filterRows(rows, filterWith({ library: "released" }), ctx()))).toEqual(["d"]);
  });

  it("ゴミ箱: deleted の本だけを出す", () => {
    expect(ids(filterRows(rows, filterWith({ library: "trash" }), ctx()))).toEqual(["e"]);
  });
});

describe("filterRows（そのほかの絞り込み）", () => {
  it("読書状況", () => {
    expect(ids(filterRows(rows, filterWith({ readStatus: "unread" }), ctx()))).toEqual(["b", "c"]);
  });

  it("保管場所は、子孫に置かれた本も含む", () => {
    expect(ids(filterRows(rows, filterWith({ locationId: "home" }), ctx()))).toEqual(["a"]);
    expect(ids(filterRows(rows, filterWith({ locationId: "study" }), ctx()))).toEqual(["a"]);
    expect(ids(filterRows(rows, filterWith({ locationId: "lab" }), ctx()))).toEqual(["b"]);
  });

  it("保管場所なし（UNSET）", () => {
    expect(ids(filterRows(rows, filterWith({ locationId: UNSET }), ctx()))).toEqual(["c", "d"]);
  });

  it("タグは、階層の親を選ぶと子も含む。同じ前置きの別名は含まない", () => {
    expect(ids(filterRows(rows, filterWith({ tag: "研究" }), ctx()))).toEqual(["a", "b"]);
    expect(ids(filterRows(rows, filterWith({ tag: "研究/SiC" }), ctx()))).toEqual(["a"]);
    // 「研究」は「研究室」というタグにはマッチしない
    const extra = [...rows, makeRow({ id: "x", tags: ["研究室"] })];
    expect(ids(filterRows(extra, filterWith({ tag: "研究" }), ctx()))).toEqual(["a", "b"]);
  });

  it("タグなし（UNSET）", () => {
    expect(ids(filterRows(rows, filterWith({ tag: UNSET }), ctx()))).toEqual(["c", "d"]);
  });

  it("自動分類: 内容（大分類・中分類）と発行形態", () => {
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "cMajor", value: "9" } }), ctx())),
    ).toEqual(["a"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "cMinor", value: "55" } }), ctx())),
    ).toEqual(["b"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "format", value: "1" } }), ctx())),
    ).toEqual(["a"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "cMajor", value: UNSET } }), ctx())),
    ).toEqual(["d"]);
  });

  it("自動分類: 出版社・著者・発行年", () => {
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "publisher", value: "オーム社" } }), ctx())),
    ).toEqual(["b"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "author", value: "佐藤 太郎" } }), ctx())),
    ).toEqual(["a", "b"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "author", value: "鈴木 花子" } }), ctx())),
    ).toEqual(["b"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "year", value: "2020" } }), ctx())),
    ).toEqual(["a", "b"]);
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "year", value: UNSET } }), ctx())),
    ).toEqual(["c"]);
  });

  it("自動分類: 取得待ち・情報不足", () => {
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "meta", value: "pending" } }), ctx())),
    ).toEqual(["c"]);
    // ゴミ箱の本（e: 取得失敗）は、すべてのライブラリでは出ない
    expect(
      ids(filterRows(rows, filterWith({ auto: { kind: "meta", value: "incomplete" } }), ctx())),
    ).toEqual(["d"]);
    expect(
      ids(
        filterRows(
          rows,
          filterWith({ library: "trash", auto: { kind: "meta", value: "incomplete" } }),
          ctx(),
        ),
      ),
    ).toEqual(["e"]);
  });

  it("グループどうしは AND で掛け合わせる", () => {
    const filter = filterWith({ library: "owned", readStatus: "read", tag: "研究" });
    expect(ids(filterRows(rows, filter, ctx()))).toEqual(["a"]);
  });

  it("検索結果の ID の集合と掛け合わせる。ゴミ箱の本は検索に当たっても出ない", () => {
    expect(ids(filterRows(rows, DEFAULT_FILTER, ctx(new Set(["b", "e", "zzz"]))))).toEqual(["b"]);
    expect(
      ids(filterRows(rows, filterWith({ library: "trash" }), ctx(new Set(["b", "e"])))),
    ).toEqual(["e"]);
    expect(filterRows(rows, DEFAULT_FILTER, ctx(new Set()))).toEqual([]);
  });

  it("絞り込みの有無", () => {
    expect(isFilterActive(DEFAULT_FILTER)).toBe(false);
    expect(isFilterActive(filterWith({ library: "owned" }))).toBe(true);
    expect(isFilterActive(filterWith({ tag: UNSET }))).toBe(true);
  });
});

describe("computeFacets", () => {
  it("何も絞らないとき: ライブラリごとの件数、N 冊中 M 冊", () => {
    const f = computeFacets(rows, DEFAULT_FILTER, ctx());
    expect(f.library).toEqual({ all: 4, owned: 2, wanted: 1, released: 1, trash: 1 });
    expect(f.total).toBe(4);
    expect(f.visible).toBe(4);
    expect(f.readStatus).toEqual({ unread: 2, reading: 0, read: 1, paused: 1 });
  });

  it("ゴミ箱を選ぶと、N と M はゴミ箱の冊数になる", () => {
    const f = computeFacets(rows, filterWith({ library: "trash" }), ctx());
    expect(f.total).toBe(1);
    expect(f.visible).toBe(1);
    expect(f.readStatus.unread).toBe(0);
  });

  it("ほかのグループの選択を掛けた件数になり、自分のグループの選択は外して数える", () => {
    // 読書状況=未読 を選ぶ → ライブラリの件数は未読だけ、読書状況の件数は絞らない
    const f = computeFacets(rows, filterWith({ readStatus: "unread" }), ctx());
    expect(f.library).toEqual({ all: 2, owned: 1, wanted: 1, released: 0, trash: 0 });
    expect(f.readStatus).toEqual({ unread: 2, reading: 0, read: 1, paused: 1 });
    expect(f.visible).toBe(2);
    expect(f.total).toBe(4);
  });

  it("保管場所の件数は直接の件数、タグは階層ごと（同じ本の複数タグは親を 1 冊と数える）", () => {
    const f = computeFacets(rows, DEFAULT_FILTER, ctx());
    expect(f.locations.get("shelf")).toBe(1);
    expect(f.locations.get("lab")).toBe(1);
    expect(f.locations.get(UNSET)).toBe(2);
    expect(f.tags.get("研究")).toBe(2);
    expect(f.tags.get("研究/SiC")).toBe(1);
    expect(f.tags.get("小説")).toBe(1);
    expect(f.tags.get(UNSET)).toBe(2);

    const dup = computeFacets(
      [makeRow({ tags: ["研究/SiC", "研究/論文"] })],
      DEFAULT_FILTER,
      ctx(),
    );
    expect(dup.tags.get("研究")).toBe(1);
  });

  it("自動分類の件数", () => {
    const f = computeFacets(rows, DEFAULT_FILTER, ctx());
    expect(f.auto.cMajor.get("9")).toBe(1);
    expect(f.auto.cMajor.get("5")).toBe(1);
    expect(f.auto.cMajor.get("7")).toBe(1);
    expect(f.auto.cMajor.get(UNSET)).toBe(1);
    expect(f.auto.cMinor.get("93")).toBe(1);
    expect(f.auto.format.get("1")).toBe(1);
    expect(f.auto.publisher.get("岩波書店")).toBe(1);
    expect(f.auto.author.get("佐藤 太郎")).toBe(2);
    expect(f.auto.year.get("2020")).toBe(2);
    expect(f.auto.year.get("1999")).toBe(1);
    expect(f.auto.metaPending).toBe(1);
    expect(f.auto.metaIncomplete).toBe(1);
  });

  it("検索結果の集合を掛けた件数になる。N は検索で変わらない", () => {
    const f = computeFacets(rows, DEFAULT_FILTER, ctx(new Set(["a", "c"])));
    expect(f.visible).toBe(2);
    expect(f.total).toBe(4);
    expect(f.library.all).toBe(2);
  });

  it("filterRows と同じ件数になる（M と表示行数）", () => {
    for (const filter of [
      DEFAULT_FILTER,
      filterWith({ library: "owned", tag: "研究" }),
      filterWith({ locationId: "home", readStatus: "read" }),
      filterWith({ auto: { kind: "author", value: "佐藤 太郎" } }),
      filterWith({ library: "trash" }),
    ]) {
      expect(computeFacets(rows, filter, ctx()).visible).toBe(
        filterRows(rows, filter, ctx()).length,
      );
    }
  });
});
