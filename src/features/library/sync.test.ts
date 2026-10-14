import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { libraryApi } from "@/features/library/api";
import { libraryKeys } from "@/features/library/hooks";
import { applyLibraryChange, mergeRows, reloadLibrary } from "@/features/library/sync";
import { makeRow } from "@/test/factories";
import type { ListRow } from "@/types/contracts";

vi.mock("@/features/library/api", () => ({
  libraryApi: { getRows: vi.fn() },
  usingMockData: () => false,
}));

const getRows = vi.mocked(libraryApi.getRows);

const ids = (rows: readonly ListRow[]) => rows.map((r) => r.id);

describe("mergeRows", () => {
  const a = makeRow({ id: "a", title: "A" });
  const b = makeRow({ id: "b", title: "B" });
  const c = makeRow({ id: "c", title: "C" });

  it("既にある本は、位置を変えずに新しい行に置き換える", () => {
    const b2 = makeRow({ id: "b", title: "B2" });
    const merged = mergeRows([a, b, c], ["b"], [b2]);
    expect(ids(merged)).toEqual(["a", "b", "c"]);
    expect(merged[1]?.title).toBe("B2");
    expect(merged[0]).toBe(a);
  });

  it("要求したのに返らなかった本（完全削除など）は、一覧から外す", () => {
    expect(ids(mergeRows([a, b, c], ["b", "c"], [makeRow({ id: "c" })]))).toEqual(["a", "c"]);
  });

  it("新しい本は、ID 順（登録順）の位置に足す", () => {
    const d = makeRow({ id: "d" });
    const bb = makeRow({ id: "bb" });
    expect(ids(mergeRows([a, b, c], ["d"], [d]))).toEqual(["a", "b", "c", "d"]);
    expect(ids(mergeRows([a, b, c], ["bb"], [bb]))).toEqual(["a", "b", "bb", "c"]);
  });

  it("要求していない行が返っても、取り込まない。何も変わらなければ同じ配列を返す", () => {
    const current = [a, b];
    expect(mergeRows(current, ["zzz"], [])).toBe(current);
    expect(mergeRows(current, ["a"], [makeRow({ id: "x" })])).toEqual([b]);
  });

  it("渡された配列は書き換えない", () => {
    const current = [a, b];
    mergeRows(current, ["a"], []);
    expect(ids(current)).toEqual(["a", "b"]);
  });
});

describe("applyLibraryChange", () => {
  let client: QueryClient;
  beforeEach(() => {
    getRows.mockReset();
    client = new QueryClient();
    client.setQueryData(libraryKeys.books, [
      makeRow({ id: "a", title: "A" }),
      makeRow({ id: "b", title: "B" }),
    ]);
  });

  it("変わった本の行だけ取り直して、キャッシュに差し込む", async () => {
    getRows.mockResolvedValue([makeRow({ id: "b", title: "B2" })]);

    await applyLibraryChange(client, { books: ["b"], others: false });

    expect(getRows).toHaveBeenCalledExactlyOnceWith(["b"]);
    const rows = client.getQueryData<ListRow[]>(libraryKeys.books)!;
    expect(rows.map((r) => r.title)).toEqual(["A", "B2"]);
  });

  it("others が true なら、保管場所・タグ・シリーズを取り直す印を付ける（検索結果も）", async () => {
    for (const key of [
      libraryKeys.locations,
      libraryKeys.tags,
      libraryKeys.series,
      libraryKeys.search("夏目"),
    ]) {
      client.setQueryData(key, []);
    }
    await applyLibraryChange(client, { books: [], others: true });

    expect(getRows).not.toHaveBeenCalled();
    for (const key of [
      libraryKeys.locations,
      libraryKeys.tags,
      libraryKeys.series,
      libraryKeys.search("夏目"),
    ]) {
      expect(client.getQueryState(key)?.isInvalidated, JSON.stringify(key)).toBe(true);
    }
    expect(client.getQueryState(libraryKeys.books)?.isInvalidated).toBe(false);
  });

  it("本だけの変更では、保管場所などは取り直さない。検索結果は古くなるので取り直す", async () => {
    getRows.mockResolvedValue([]);
    client.setQueryData(libraryKeys.locations, []);
    client.setQueryData(libraryKeys.search("a"), ["a"]);

    await applyLibraryChange(client, { books: ["a"], others: false });

    expect(client.getQueryState(libraryKeys.locations)?.isInvalidated).toBe(false);
    expect(client.getQueryState(libraryKeys.search("a"))?.isInvalidated).toBe(true);
  });

  it("本が変わったら、詳細（詳細ペイン）と購入店の候補も取り直す印を付ける", async () => {
    getRows.mockResolvedValue([]);
    client.setQueryData(libraryKeys.bookDetail("a"), { id: "a" });
    client.setQueryData(libraryKeys.purchaseStores, []);

    await applyLibraryChange(client, { books: ["a"], others: false });

    expect(client.getQueryState(libraryKeys.bookDetail("a"))?.isInvalidated).toBe(true);
    expect(client.getQueryState(libraryKeys.purchaseStores)?.isInvalidated).toBe(true);
  });

  it("一覧をまだ読み込んでいなければ、キャッシュを作らない", async () => {
    const empty = new QueryClient();
    getRows.mockResolvedValue([makeRow({ id: "a" })]);
    await applyLibraryChange(empty, { books: ["a"], others: false });
    expect(empty.getQueryData(libraryKeys.books)).toBeUndefined();
  });

  it("reloadLibrary は、すべてのクエリを取り直す印を付ける", async () => {
    client.setQueryData(libraryKeys.locations, []);
    await reloadLibrary(client);
    expect(client.getQueryState(libraryKeys.books)?.isInvalidated).toBe(true);
    expect(client.getQueryState(libraryKeys.locations)?.isInvalidated).toBe(true);
  });
});
