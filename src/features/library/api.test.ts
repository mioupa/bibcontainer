import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CCodeTable as CCodeTableBinding } from "@/bindings/CCodeTable";
import { libraryApi, toCCodeLookup, usingMockData } from "@/features/library/api";
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
}));
vi.mock("@/lib/runtime", () => ({ inTauri: () => tauri.value }));
vi.mock("@/lib/ipc", () => ({ ipc }));

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
});
