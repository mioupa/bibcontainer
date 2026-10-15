import { describe, expect, it, vi } from "vitest";
import {
  MOCK_FOLDER_PATH,
  MOCK_LIBRARY_ID,
  MOCK_SELF_DEVICE_ID,
  createMockSync,
  libraryFolderOf,
  mockSyncScenarioFromLocation,
  type MockSyncOptions,
  type MockSyncScenario,
} from "@/dev/mockSync";
import type { SyncStatus } from "@/features/sync/types";
import { IpcError } from "@/lib/ipc";

const NOW = new Date(2026, 9, 15, 13, 0);

function make(options: MockSyncOptions = {}) {
  return createMockSync({ now: () => NOW, setupDelayMs: 0, syncDelayMs: 0, ...options });
}

describe("mockSyncScenarioFromLocation", () => {
  it("?mockSync= で最初の状態を決める。知らない値・なしは未設定", () => {
    expect(mockSyncScenarioFromLocation("?mockSync=error")).toBe("error");
    expect(mockSyncScenarioFromLocation("?mockRows=20&mockSync=skew")).toBe("skew");
    expect(mockSyncScenarioFromLocation("?mockSync=nope")).toBe("disabled");
    expect(mockSyncScenarioFromLocation("")).toBe("disabled");
  });
});

describe("libraryFolderOf", () => {
  it("選んだフォルダの下に bibcontainer-library を付ける。選んだフォルダ自体がそれなら、そのまま", () => {
    expect(libraryFolderOf("/Drive/マイドライブ")).toBe("/Drive/マイドライブ/bibcontainer-library");
    expect(libraryFolderOf("/Drive/マイドライブ/")).toBe(
      "/Drive/マイドライブ/bibcontainer-library",
    );
    expect(libraryFolderOf("/Drive/bibcontainer-library")).toBe("/Drive/bibcontainer-library");
    expect(libraryFolderOf("G:\\マイドライブ")).toBe("G:\\マイドライブ\\bibcontainer-library");
  });
});

describe("見本の状態", () => {
  const states = async (scenario: MockSyncScenario) => make({ scenario }).api.getSyncStatus();

  it("未設定: フォルダなし・端末の一覧なし", async () => {
    const status = await states("disabled");
    expect(status).toMatchObject({ state: "disabled", folder: null, libraryId: null, devices: [] });
    expect(status.deviceId).toBe(MOCK_SELF_DEVICE_ID);
  });

  it("完了・同期中・エラー・不一致・更新が必要・時計のずれ・未書き出しあり", async () => {
    expect(await states("idle")).toMatchObject({ state: "idle", pendingOps: 0, error: null });
    expect(await states("syncing")).toMatchObject({ state: "syncing" });
    const failed = await states("error");
    expect(failed.state).toBe("error");
    expect(typeof failed.error).toBe("string");
    expect(await states("mismatch")).toMatchObject({ state: "mismatch" });
    expect(await states("updateRequired")).toMatchObject({ state: "updateRequired" });
    expect(await states("skew")).toMatchObject({
      state: "idle",
      clockSkew: { deviceName: "Windows デスクトップ", aheadMs: 12 * 60_000 },
    });
    expect(await states("pending")).toMatchObject({
      state: "idle",
      pendingOps: 7,
      missingCovers: 3,
    });
  });

  it("設定済みの端末の一覧には、自分（isSelf）が 1 つだけ入る", async () => {
    const status = await states("idle");
    expect(status.devices.filter((d) => d.isSelf)).toHaveLength(1);
    expect(status.devices.length).toBeGreaterThan(1);
  });
});

describe("sync-status イベント", () => {
  it("状態を切り替えるたびに届く。購読をやめれば届かない", async () => {
    const { api, control } = make();
    const seen: SyncStatus[] = [];
    const stop = await api.onSyncStatus((status) => seen.push(status));
    control.setScenario("syncing");
    control.setScenario("idle");
    expect(seen.map((s) => s.state)).toEqual(["syncing", "idle"]);
    expect(control.listenerCount()).toBe(1);
    stop();
    expect(control.listenerCount()).toBe(0);
    control.setScenario("error");
    expect(seen).toHaveLength(2);
  });

  it("届く状態は複製（受け取り側が書き換えても、モックの状態は変わらない）", async () => {
    const { api, control } = make({ scenario: "idle" });
    let received: SyncStatus | undefined;
    await api.onSyncStatus((status) => (received = status));
    control.patch({ pendingOps: 1 });
    received!.pendingOps = 999;
    expect((await api.getSyncStatus()).pendingOps).toBe(1);
  });
});

describe("今すぐ同期", () => {
  it("同期中 → 完了と変わる。時刻を今にし、未書き出しとエラーを消す", async () => {
    vi.useFakeTimers();
    try {
      const { api } = createMockSync({ now: () => NOW, scenario: "error", syncDelayMs: 1000 });
      const seen: string[] = [];
      await api.onSyncStatus((s) => seen.push(s.state));
      const done = api.syncNow();
      expect(seen).toEqual(["syncing"]);
      await vi.advanceTimersByTimeAsync(1000);
      const status = await done;
      expect(seen).toEqual(["syncing", "idle"]);
      expect(status).toMatchObject({
        state: "idle",
        pendingOps: 0,
        error: null,
        lastExportAt: NOW.toISOString(),
        lastImportAt: NOW.toISOString(),
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("未設定・不一致・更新が必要のときは、できない（conflict）", async () => {
    for (const scenario of ["disabled", "mismatch", "updateRequired"] as const) {
      const rejection = make({ scenario }).api.syncNow();
      await expect(rejection).rejects.toBeInstanceOf(IpcError);
      await expect(rejection).rejects.toMatchObject({ kind: "conflict" });
    }
  });
});

describe("設定と解除", () => {
  const inspectionOf = (api: ReturnType<typeof make>["api"]) =>
    api.inspectSyncFolder(MOCK_FOLDER_PATH);

  it("inspect は、見本の判定どおり。空のフォルダには端末がなく、ライブラリには端末がいる", async () => {
    const { api, control } = make();
    control.setInspect({ kind: "empty", localHasData: false });
    expect(await inspectionOf(api)).toMatchObject({
      kind: "empty",
      libraryId: null,
      devices: [],
      localHasData: false,
      libraryFolder: `${MOCK_FOLDER_PATH}/bibcontainer-library`,
    });
    control.setInspect({ kind: "library", localHasData: true });
    const library = await inspectionOf(api);
    expect(library.libraryId).not.toBeNull();
    expect(library.devices.length).toBeGreaterThan(0);
    control.setInspect({ kind: "otherLibrary", localLibraryId: "lib-local" });
    expect(await inspectionOf(api)).toMatchObject({
      kind: "otherLibrary",
      localLibraryId: "lib-local",
    });
  });

  it("新しく作る（create）: 設定済みになり、書き出しの時刻が入る", async () => {
    const { api, control } = make();
    control.setInspect({ kind: "empty" });
    const status = await api.setupSync(MOCK_FOLDER_PATH, "create");
    expect(status).toMatchObject({
      state: "idle",
      folder: `${MOCK_FOLDER_PATH}/bibcontainer-library`,
      lastExportAt: NOW.toISOString(),
      lastImportAt: null,
    });
    expect(status.devices.map((d) => d.isSelf)).toEqual([true]);
  });

  it("参加（join）・統合（joinMerge）・置き換え（joinReplace）: 取り込んだ時刻が入り、他の端末が見える", async () => {
    for (const [mode, localHasData] of [
      ["join", false],
      ["joinMerge", true],
      ["joinReplace", true],
    ] as const) {
      const { api, control } = make();
      control.setInspect({ kind: "library", localHasData });
      const status = await api.setupSync(MOCK_FOLDER_PATH, mode);
      expect(status.state).toBe("idle");
      expect(status.lastImportAt).toBe(NOW.toISOString());
      expect(status.devices.length).toBeGreaterThan(1);
    }
  });

  it("設定中は「同期中」の状態を出す", async () => {
    const { api, control } = make();
    control.setInspect({ kind: "empty" });
    const seen: string[] = [];
    await api.onSyncStatus((s) => seen.push(s.state));
    await api.setupSync(MOCK_FOLDER_PATH, "create");
    expect(seen).toEqual(["syncing", "idle"]);
  });

  it("判定と合わない方法・同期できないフォルダ・設定済みでの再設定は断る", async () => {
    const { api, control } = make();
    control.setInspect({ kind: "library", localHasData: true });
    await expect(api.setupSync(MOCK_FOLDER_PATH, "join")).rejects.toMatchObject({
      kind: "validation",
    });
    await expect(api.setupSync(MOCK_FOLDER_PATH, "create")).rejects.toMatchObject({
      kind: "validation",
    });
    control.setInspect({ kind: "library", localHasData: false });
    await expect(api.setupSync(MOCK_FOLDER_PATH, "joinReplace")).rejects.toMatchObject({
      kind: "validation",
    });
    for (const kind of ["otherLibrary", "notWritable", "updateRequired"] as const) {
      control.setInspect({ kind });
      await expect(api.setupSync(MOCK_FOLDER_PATH, "join")).rejects.toMatchObject({
        kind: "conflict",
      });
    }
    control.setInspect({ kind: "empty" });
    await api.setupSync(MOCK_FOLDER_PATH, "create");
    await expect(api.setupSync(MOCK_FOLDER_PATH, "create")).rejects.toMatchObject({
      kind: "conflict",
    });
  });

  it("同じライブラリの選び直し（再接続）: join だけ受け付ける。統合・置き換えは断る（エンジンは同じ手順にする）", async () => {
    const { api, control } = make();
    control.setInspect({ kind: "library", localHasData: true, localLibraryId: MOCK_LIBRARY_ID });
    expect(await inspectionOf(api)).toMatchObject({
      kind: "library",
      libraryId: MOCK_LIBRARY_ID,
      localLibraryId: MOCK_LIBRARY_ID,
    });
    for (const mode of ["create", "joinMerge", "joinReplace"] as const) {
      await expect(api.setupSync(MOCK_FOLDER_PATH, mode)).rejects.toMatchObject({
        kind: "validation",
      });
    }
    const status = await api.setupSync(MOCK_FOLDER_PATH, "join");
    expect(status.state).toBe("idle");
    expect(status.lastImportAt).toBe(NOW.toISOString());
    expect(status.lastExportAt).toBeNull();
  });

  it("手元がライブラリに属しているのに空のフォルダを選ぶと、新しく作れない（conflict）", async () => {
    const { api, control } = make();
    control.setInspect({ kind: "empty", localLibraryId: "lib-local" });
    expect(await inspectionOf(api)).toMatchObject({ kind: "empty", localLibraryId: "lib-local" });
    const error = await api.setupSync(MOCK_FOLDER_PATH, "create").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(IpcError);
    expect(error).toMatchObject({ kind: "conflict" });
    expect((error as IpcError).message).toContain("同期を解除");
    expect((await api.getSyncStatus()).state).toBe("disabled");
    // 解除すれば、作れる
    await api.leaveSync();
    expect(control.getInspect().localLibraryId).toBeNull();
    await expect(api.setupSync(MOCK_FOLDER_PATH, "create")).resolves.toMatchObject({
      state: "idle",
    });
  });

  it("解除すると未設定に戻る。「別のライブラリ」の判定は、ふつうの「ライブラリあり」になる", async () => {
    const { api, control } = make({ scenario: "idle" });
    control.setInspect({ kind: "otherLibrary", localLibraryId: "lib-local" });
    const status = await api.leaveSync();
    expect(status).toMatchObject({ state: "disabled", folder: null, devices: [] });
    expect(control.getInspect()).toMatchObject({ kind: "library", localLibraryId: null });
  });
});

describe("端末名・書影の整理", () => {
  it("端末名を変えると、状態と端末の一覧の自分の名前が変わる。前後の空白は除く", async () => {
    const { api } = make({ scenario: "idle" });
    const status = await api.setDeviceName("  リビングの Mac  ");
    expect(status.deviceName).toBe("リビングの Mac");
    expect(status.devices.find((d) => d.isSelf)?.name).toBe("リビングの Mac");
    // 解除しても名前は残る
    expect((await api.leaveSync()).deviceName).toBe("リビングの Mac");
  });

  it("空の名前・長すぎる名前は断る（validation）", async () => {
    const { api } = make();
    await expect(api.setDeviceName("   ")).rejects.toMatchObject({ kind: "validation" });
    await expect(api.setDeviceName("あ".repeat(65))).rejects.toMatchObject({ kind: "validation" });
    expect((await api.setDeviceName("あ".repeat(64))).deviceName).toHaveLength(64);
  });

  it("書影の整理は、消した件数を返す（2 回目は 0）。未設定ではできない", async () => {
    const { api, control } = make({ scenario: "idle" });
    control.setGarbageCovers(5);
    expect(await api.gcCovers()).toBe(5);
    expect(await api.gcCovers()).toBe(0);
    control.reset("disabled");
    await expect(api.gcCovers()).rejects.toMatchObject({ kind: "conflict" });
  });
});
