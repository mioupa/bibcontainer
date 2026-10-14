import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DisplaySettings } from "@/bindings/DisplaySettings";
import type { ListSettings } from "@/bindings/ListSettings";
import type { ScanSettings } from "@/bindings/ScanSettings";
import {
  STORAGE_KEYS,
  createLatestWriter,
  resetSettingsCacheForTest,
  settingsStorage,
} from "@/lib/persist";

const tauri = vi.hoisted(() => ({ value: true }));
const ipc = vi.hoisted(() => ({
  getDisplaySettings: vi.fn(),
  setDisplaySettings: vi.fn(),
  getScanSettings: vi.fn(),
  setScanSettings: vi.fn(),
  getListSettings: vi.fn(),
  setListSettings: vi.fn(),
}));
vi.mock("@/lib/runtime", () => ({ inTauri: () => tauri.value }));
vi.mock("@/lib/ipc", () => ({ ipc }));

const display: DisplaySettings = { theme: "light", fontFamily: null, fontSize: 16 };
const scan: ScanSettings = {
  terminator: "enter",
  timeoutSec: 10,
  keyIntervalMs: 30,
  idleCommitMs: 100,
  stripLeading: 0,
  stripTrailing: 0,
  sounds: {
    success: { enabled: true, volume: 0.6 },
    error: { enabled: true, volume: 0.6 },
    attention: { enabled: true, volume: 0.6 },
  },
  muted: false,
};
const list: ListSettings = {
  columnVisibility: {},
  columnOrder: ["title"],
  columnSizing: {},
  sorting: [{ id: "title", desc: false }],
  sidebarOpen: true,
  detailOpen: true,
};

// persist が呼ぶ StateStorage の形（createJSONStorage の中身）を直接たたく。
const storage = settingsStorage!;
const read = async (name: string) => await storage.getItem(name);
const write = async (name: string, state: unknown) => {
  await storage.setItem(name, { state, version: 1 });
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("設定の保存先（Tauri の中: Rust の local_settings）", () => {
  beforeEach(() => {
    tauri.value = true;
    resetSettingsCacheForTest();
    for (const fn of Object.values(ipc)) fn.mockReset();
    ipc.getDisplaySettings.mockResolvedValue(display);
    ipc.getScanSettings.mockResolvedValue(scan);
    ipc.getListSettings.mockResolvedValue(list);
    ipc.setDisplaySettings.mockResolvedValue(display);
    ipc.setScanSettings.mockResolvedValue(scan);
    ipc.setListSettings.mockResolvedValue(list);
  });

  it("settings ストアは、display と scan のセクションから読む", async () => {
    const stored = await read(STORAGE_KEYS.settings);
    expect(ipc.getDisplaySettings).toHaveBeenCalledOnce();
    expect(ipc.getScanSettings).toHaveBeenCalledOnce();
    expect(stored?.version).toBe(1);
    expect(stored?.state).toMatchObject({ theme: "light", scan: { timeoutSec: 10 } });
  });

  it("settings ストアの変更は、display と scan のセクションに書く", async () => {
    const stored = await read(STORAGE_KEYS.settings);
    const state = stored!.state as Record<string, unknown>;
    await write(STORAGE_KEYS.settings, {
      ...state,
      theme: "dark",
      scan: { ...(state.scan as object), timeoutSec: 20 },
    });
    await flush();

    expect(ipc.setDisplaySettings).toHaveBeenCalledWith({ ...display, theme: "dark" });
    expect(ipc.setScanSettings).toHaveBeenCalledWith({ ...scan, timeoutSec: 20 });
  });

  it("読み込めていないうちは、設定を書かない（既定値で上書きしない）", async () => {
    await write(STORAGE_KEYS.settings, { theme: "dark" });
    await flush();
    expect(ipc.setDisplaySettings).not.toHaveBeenCalled();
    expect(ipc.setScanSettings).not.toHaveBeenCalled();
  });

  it("table と layout は list セクションを分け合う。1 回だけ読み、書くときは両方の変更を重ねる", async () => {
    await read(STORAGE_KEYS.table);
    await read(STORAGE_KEYS.layout);
    expect(ipc.getListSettings).toHaveBeenCalledOnce();

    await write(STORAGE_KEYS.table, {
      sorting: [{ id: "authors", desc: true }],
      columnVisibility: { tags: true },
      columnOrder: ["authors", "title"],
      columnSizing: { title: 480 },
    });
    await write(STORAGE_KEYS.layout, { sidebarOpen: false, detailOpen: true });
    await flush();
    await flush();

    const last = ipc.setListSettings.mock.calls.at(-1)?.[0] as ListSettings;
    expect(last).toEqual({
      ...list,
      sorting: [{ id: "authors", desc: true }],
      columnVisibility: { tags: true },
      columnOrder: ["authors", "title"],
      columnSizing: { title: 480 },
      sidebarOpen: false,
    });
  });

  it("読み込みに失敗したら、読み込みも書き込みも例外にせず、次の機会にやり直す", async () => {
    ipc.getListSettings.mockRejectedValueOnce({ kind: "db_unavailable", message: "開けない" });
    await expect(read(STORAGE_KEYS.table)).rejects.toBeDefined();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    await write(STORAGE_KEYS.layout, { sidebarOpen: false, detailOpen: true });
    await flush();
    // 2 回目の読み込みは成功し、書き込みもできる
    await write(STORAGE_KEYS.layout, { sidebarOpen: false, detailOpen: true });
    await flush();
    expect(ipc.setListSettings).toHaveBeenCalled();
    error.mockRestore();
  });

  it("知らないキーは何も返さない", async () => {
    expect(await read("other")).toBeNull();
  });
});

describe("設定の保存先（Tauri の外: localStorage）", () => {
  beforeEach(() => {
    tauri.value = false;
    localStorage.clear();
    for (const fn of Object.values(ipc)) fn.mockReset();
  });

  it("IPC を呼ばず、localStorage に保存して読み戻す", async () => {
    await write(STORAGE_KEYS.layout, { sidebarOpen: false, detailOpen: false });
    expect(localStorage.getItem(STORAGE_KEYS.layout)).toContain('"sidebarOpen":false');
    const stored = await read(STORAGE_KEYS.layout);
    expect(stored?.state).toEqual({ sidebarOpen: false, detailOpen: false });
    expect(await read(STORAGE_KEYS.table)).toBeNull();
    for (const fn of Object.values(ipc)) expect(fn).not.toHaveBeenCalled();
  });
});

describe("createLatestWriter", () => {
  it("書いている最中に来た値は、最新のものだけを次に書く", async () => {
    const written: number[] = [];
    let release: (() => void) | undefined;
    const write = vi.fn(
      (value: number) =>
        new Promise<void>((resolve) => {
          written.push(value);
          release = resolve;
        }),
    );
    const enqueue = createLatestWriter(write);

    enqueue(1);
    enqueue(2);
    enqueue(3);
    expect(written).toEqual([1]);

    release?.();
    await flush();
    expect(written).toEqual([1, 3]);
    release?.();
    await flush();

    enqueue(4);
    expect(written).toEqual([1, 3, 4]);
  });

  it("書き込みが失敗しても、続きの書き込みは止まらない", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const write = vi
      .fn<(value: number) => Promise<void>>()
      .mockRejectedValueOnce(new Error("失敗"))
      .mockResolvedValue(undefined);
    const enqueue = createLatestWriter(write);
    enqueue(1);
    await flush();
    enqueue(2);
    await flush();
    expect(write.mock.calls.map(([v]) => v)).toEqual([1, 2]);
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });
});
