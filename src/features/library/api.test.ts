import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CCodeTable as CCodeTableBinding } from "@/bindings/CCodeTable";
import { libraryApi, toCCodeLookup, usingMockData } from "@/features/library/api";
import type * as IpcModule from "@/lib/ipc";
import { makeRow } from "@/test/factories";

const tauri = vi.hoisted(() => ({ value: true }));
const ipc = vi.hoisted(() => ({
  listBooks: vi.fn(),
  getRows: vi.fn(),
  listLocations: vi.fn(),
  listTags: vi.fn(),
  listSeries: vi.fn(),
  cCodeTable: vi.fn(),
  searchBooks: vi.fn(),
  trashBooks: vi.fn(),
  restoreBooks: vi.fn(),
  purgeBooks: vi.fn(),
  updateBooks: vi.fn(),
  mergeWorks: vi.fn(),
}));
vi.mock("@/lib/runtime", () => ({ inTauri: () => tauri.value }));
// IpcError（モックの検証のエラー）は本物を使い、ipc（Rust を呼ぶ入口）だけ差し替える
vi.mock("@/lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof IpcModule>()),
  ipc,
}));

describe("toCCodeLookup", () => {
  it("bindings の { code, name } の配列を、コード → 名称の引き表にする", () => {
    const table: CCodeTableBinding = {
      audience: [
        { code: "0", name: "一般" },
        { code: "3", name: "専門" },
      ],
      format: [{ code: "0", name: "単行本" }],
      contentMajor: [{ code: "4", name: "自然科学" }],
      content: [{ code: "42", name: "物理学" }],
    };
    expect(toCCodeLookup(table)).toEqual({
      audience: { "0": "一般", "3": "専門" },
      format: { "0": "単行本" },
      contentMajor: { "4": "自然科学" },
      content: { "42": "物理学" },
    });
  });
});

describe("libraryApi（Tauri の中では IPC、外ではモック）", () => {
  beforeEach(() => {
    for (const fn of Object.values(ipc)) fn.mockReset();
    tauri.value = true;
  });

  it("Tauri の中では、IPC を呼ぶ", async () => {
    expect(usingMockData()).toBe(false);
    ipc.listBooks.mockResolvedValue([makeRow({ id: "a" })]);
    ipc.getRows.mockResolvedValue([]);
    ipc.searchBooks.mockResolvedValue(["a"]);
    ipc.trashBooks.mockResolvedValue(["a"]);

    expect(await libraryApi.listBooks()).toHaveLength(1);
    await libraryApi.getRows(["a"]);
    expect(await libraryApi.searchBooks("夏目")).toEqual(["a"]);
    expect(await libraryApi.trashBooks(["a"])).toEqual(["a"]);
    expect(ipc.getRows).toHaveBeenCalledWith(["a"]);
    expect(ipc.searchBooks).toHaveBeenCalledWith("夏目");
  });

  it("Cコード表は、引き表の形に変換して返す", async () => {
    ipc.cCodeTable.mockResolvedValue({
      audience: [{ code: "0", name: "一般" }],
      format: [],
      contentMajor: [],
      content: [],
    });
    expect((await libraryApi.cCodeTable()).audience).toEqual({ "0": "一般" });
  });

  it("一括変更は、手入力の由来で update_books を呼び、実際に書いた本の ID だけ返す", async () => {
    ipc.updateBooks.mockResolvedValue([
      { id: "a", written: ["read_status"], skippedManual: [] },
      { id: "b", written: [], skippedManual: [] },
    ]);

    const changed = await libraryApi.setBooksField(["a", "b"], "read_status", "read");

    expect(ipc.updateBooks).toHaveBeenCalledWith(
      ["a", "b"],
      [{ field: "read_status", value: "read", origin: { kind: "manual" } }],
    );
    expect(changed).toEqual(["a"]);
  });

  it("Tauri の外では、IPC を呼ばずにモックで動く", async () => {
    tauri.value = false;
    expect(usingMockData()).toBe(true);

    const rows = await libraryApi.listBooks();
    expect(rows.length).toBeGreaterThan(0);
    expect(Object.keys((await libraryApi.cCodeTable()).audience).length).toBeGreaterThan(0);
    expect(await libraryApi.listLocations()).not.toHaveLength(0);
    for (const fn of Object.values(ipc)) expect(fn).not.toHaveBeenCalled();
  });

  it("モックでも、ゴミ箱への移動・復元・一括変更・完全削除の結果が行に反映される", async () => {
    tauri.value = false;
    const [first, second] = (await libraryApi.listBooks()).filter((r) => !r.deleted);
    const ids = [first!.id, second!.id];

    expect(await libraryApi.trashBooks(ids)).toEqual(ids);
    expect(await libraryApi.trashBooks(ids), "既にゴミ箱にある本は変えない").toEqual([]);
    expect((await libraryApi.getRows(ids)).every((r) => r.deleted)).toBe(true);

    expect(await libraryApi.restoreBooks([ids[0]!])).toEqual([ids[0]]);
    expect((await libraryApi.getRows([ids[0]!]))[0]?.deleted).toBe(false);

    const target = first!.readStatus === "paused" ? "reading" : "paused";
    expect(await libraryApi.setBooksField([ids[0]!], "read_status", target)).toEqual([ids[0]]);
    expect((await libraryApi.getRows([ids[0]!]))[0]?.readStatus).toBe(target);

    expect(await libraryApi.purgeBooks(ids)).toEqual(ids);
    expect(await libraryApi.getRows(ids)).toEqual([]);
  });

  it("Tauri の中では、作品のまとめは merge_works（IPC）を呼び、結果をそのまま返す", async () => {
    ipc.mergeWorks.mockResolvedValue({ workId: "w-1", changed: ["a", "b"] });
    expect(await libraryApi.mergeWorks(["a", "b"])).toEqual({ workId: "w-1", changed: ["a", "b"] });
    expect(ipc.mergeWorks).toHaveBeenCalledWith(["a", "b"]);
  });

  describe("モックの作品のまとめ・外し（0026。Rust の merge_works と同じ規則）", () => {
    /** 上書きのない（workId が null の）本の ID を、n 冊。 */
    async function freeBooks(count: number, skip = 0): Promise<string[]> {
      const rows = await libraryApi.listBooks();
      return rows
        .filter((row) => row.workId === null)
        .slice(skip, skip + count)
        .map((row) => row.id);
    }
    const workIdsOf = async (ids: string[]) =>
      (await libraryApi.getRows(ids)).map((row) => row.workId);

    beforeEach(() => {
      tauri.value = false;
    });

    it("2 冊に満たない（重複を除く。存在しない ID も数えない）なら、検証のエラーで拒否する", async () => {
      const [a] = await freeBooks(1);
      for (const ids of [[], [a!], [a!, a!], [a!, "no-such-book"]]) {
        await expect(libraryApi.mergeWorks(ids)).rejects.toMatchObject({
          name: "IpcError",
          kind: "validation",
        });
      }
      expect(await workIdsOf([a!])).toEqual([null]);
    });

    it("上書きのない本をまとめると、新しい作品の ID を作り、対象の本だけを書き換える", async () => {
      const ids = await freeBooks(3);
      const result = await libraryApi.mergeWorks([ids[2]!, ids[0]!, ids[1]!, ids[0]!]);
      expect(result.workId).toMatch(/^mock-work-/);
      expect(result.changed).toEqual([ids[2], ids[0], ids[1]]);
      expect(await workIdsOf(ids)).toEqual([result.workId, result.workId, result.workId]);
      // もう一度まとめても、変わる本はない
      expect(await libraryApi.mergeWorks(ids)).toEqual({ workId: result.workId, changed: [] });
    });

    it("対象が手動の作品に入っていれば、冊数が最も多い作品の ID を使い、その作品の全巻（対象でない本も）を同じ作品にする", async () => {
      const ids = await freeBooks(7, 10);
      // 作品 mock-b: 3 冊、作品 mock-a: 2 冊
      await libraryApi.setBooksField(ids.slice(0, 3), "work_id", "mock-b");
      await libraryApi.setBooksField(ids.slice(3, 5), "work_id", "mock-a");
      // 作品 mock-b の 1 冊、作品 mock-a の 1 冊、上書きのない 1 冊を選ぶ
      const result = await libraryApi.mergeWorks([ids[3]!, ids[0]!, ids[5]!]);
      expect(result.workId).toBe("mock-b");
      // 選んだ本（渡した順）、続けて、同じ作品の残りを ID 順に。mock-b の本は変わらない
      expect(result.changed.slice(0, 2)).toEqual([ids[3], ids[5]]);
      expect(new Set(result.changed)).toEqual(new Set([ids[3], ids[4], ids[5]]));
      expect(await workIdsOf(ids.slice(0, 6))).toEqual(Array(6).fill("mock-b"));
      expect(await workIdsOf([ids[6]!])).toEqual([null]);
    });

    it("冊数が同じなら、文字列として小さい作品の ID を使う", async () => {
      const ids = await freeBooks(4, 30);
      await libraryApi.setBooksField(ids.slice(0, 2), "work_id", "mock-d");
      await libraryApi.setBooksField(ids.slice(2, 4), "work_id", "mock-c");
      const result = await libraryApi.mergeWorks([ids[0]!, ids[2]!]);
      expect(result.workId).toBe("mock-c");
      expect(await workIdsOf(ids)).toEqual(Array(4).fill("mock-c"));
    });

    it("「作品から外す」（none）と「自動に戻す」（null）は、一括変更（work_id）で書く。外した本は、まとめるときに作品の ID を数えない", async () => {
      const ids = await freeBooks(3, 40);
      expect(await libraryApi.setBooksField(ids, "work_id", "none")).toEqual(ids);
      expect(await workIdsOf(ids)).toEqual(["none", "none", "none"]);
      expect(await libraryApi.setBooksField(ids, "work_id", "none")).toEqual([]);
      // none の本をまとめると、新しい作品の ID（none は作品の ID ではない）
      const merged = await libraryApi.mergeWorks(ids.slice(0, 2));
      expect(merged.workId).toMatch(/^mock-work-/);
      expect(await libraryApi.setBooksField(ids, "work_id", null)).toEqual(ids);
      expect(await workIdsOf(ids)).toEqual([null, null, null]);
    });

    it("詳細（getBookDetail）にも workId が出る", async () => {
      const [a, b] = await freeBooks(2, 50);
      const { workId } = await libraryApi.mergeWorks([a!, b!]);
      expect((await libraryApi.getBookDetail(a!)).workId).toBe(workId);
    });
  });
});
