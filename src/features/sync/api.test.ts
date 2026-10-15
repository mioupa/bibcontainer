import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockSync } from "@/dev/mockSync";
import * as syncApi from "@/features/sync/api";
import type { FolderInspection, SyncStatus } from "@/features/sync/types";
import type * as IpcModule from "@/lib/ipc";

const tauri = vi.hoisted(() => ({ value: true }));
const ipc = vi.hoisted(() => ({
  getSyncStatus: vi.fn(),
  inspectSyncFolder: vi.fn(),
  setupSync: vi.fn(),
  leaveSync: vi.fn(),
  syncNow: vi.fn(),
  setDeviceName: vi.fn(),
  gcCovers: vi.fn(),
}));
const onSyncStatus = vi.hoisted(() => vi.fn());
vi.mock("@/lib/runtime", () => ({ inTauri: () => tauri.value }));
vi.mock("@/lib/ipc", async (importActual) => ({
  ...(await importActual<typeof IpcModule>()),
  ipc,
  onSyncStatus,
}));

const STATUS: SyncStatus = {
  state: "idle",
  folder: "/Drive/bibcontainer-library",
  libraryId: "lib-1",
  deviceId: "d-self",
  deviceName: "Mac",
  lastExportAt: null,
  lastImportAt: null,
  pendingOps: 0,
  missingCovers: 0,
  error: null,
  clockSkew: null,
  devices: [],
};

const INSPECTION: FolderInspection = {
  kind: "library",
  libraryFolder: "/Drive/bibcontainer-library",
  libraryId: "lib-1",
  devices: [],
  localHasData: false,
  localLibraryId: null,
};

describe("同期の呼び出し口（Tauri の中では IPC、外ではモック）", () => {
  beforeEach(() => {
    for (const fn of [...Object.values(ipc), onSyncStatus]) fn.mockReset();
    tauri.value = true;
    mockSync.control.reset();
  });

  it("Tauri の中では、IPC のコマンドを呼び、戻り値（状態・件数）をそのまま返す", async () => {
    expect(syncApi.usingMockSync()).toBe(false);
    ipc.getSyncStatus.mockResolvedValue(STATUS);
    ipc.inspectSyncFolder.mockResolvedValue(INSPECTION);
    ipc.setupSync.mockResolvedValue(STATUS);
    ipc.leaveSync.mockResolvedValue({ ...STATUS, state: "disabled" });
    ipc.syncNow.mockResolvedValue(STATUS);
    ipc.setDeviceName.mockResolvedValue({ ...STATUS, deviceName: "書斎" });
    ipc.gcCovers.mockResolvedValue(4);

    expect(await syncApi.getSyncStatus()).toBe(STATUS);
    expect(await syncApi.inspectSyncFolder("/Drive")).toBe(INSPECTION);
    expect(await syncApi.setupSync("/Drive", "joinMerge")).toBe(STATUS);
    expect(await syncApi.leaveSync()).toMatchObject({ state: "disabled" });
    expect(await syncApi.syncNow()).toBe(STATUS);
    expect(await syncApi.setDeviceName("書斎")).toMatchObject({ deviceName: "書斎" });
    expect(await syncApi.gcCovers()).toBe(4);

    expect(ipc.inspectSyncFolder).toHaveBeenCalledExactlyOnceWith("/Drive");
    expect(ipc.setupSync).toHaveBeenCalledExactlyOnceWith("/Drive", "joinMerge");
    expect(ipc.setDeviceName).toHaveBeenCalledExactlyOnceWith("書斎");
  });

  it("Tauri の中では、sync-status の購読も IPC（イベント）で、解除する関数を返す", async () => {
    const unlisten = vi.fn();
    onSyncStatus.mockResolvedValue(unlisten);
    const handler = vi.fn();
    const stop = await syncApi.onSyncStatus(handler);
    expect(onSyncStatus).toHaveBeenCalledExactlyOnceWith(handler);
    stop();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("Tauri の外（ブラウザ）では、IPC を呼ばず、モックで動く", async () => {
    tauri.value = false;
    expect(syncApi.usingMockSync()).toBe(true);
    expect(await syncApi.getSyncStatus()).toMatchObject({ state: "disabled" });
    const seen: string[] = [];
    const stop = await syncApi.onSyncStatus((status) => seen.push(status.state));
    mockSync.control.setScenario("idle");
    stop();
    expect(seen).toEqual(["idle"]);
    for (const fn of [...Object.values(ipc), onSyncStatus]) expect(fn).not.toHaveBeenCalled();
  });
});
