// 同期（フェーズ 2）のモック。Rust の同期エンジンができるまでの間、同期の画面をこれで動かす（0015）。
//
// `SyncApi`（src/features/sync/types.ts）と同じ形で、メモリ上の状態を持つ。本物の同期はしない。
// 開発中に、状態（未設定・同期中・完了・エラー・不一致・更新が必要・時計のずれ・未書き出しあり）と、
// フォルダを選んだときの判定（空・ライブラリあり・別のライブラリ・書き込めない・更新が必要、手元のデータの有無）を
// 切り替えて見られるよう、`control` を用意している（開発用のパネル src/dev/SyncDevPanel.tsx が使う）。
// 最初の状態は URL の `?mockSync=error` などで決められる。
import { IpcError } from "@/lib/ipc";
import type {
  FolderInspection,
  FolderKind,
  SetupMode,
  SyncApi,
  SyncDevice,
  SyncStatus,
} from "@/features/sync/types";

/** 状態の見本。`disabled` が既定（同期フォルダが未設定）。 */
export const MOCK_SYNC_SCENARIOS = [
  "disabled",
  "idle",
  "syncing",
  "error",
  "mismatch",
  "updateRequired",
  "skew",
  "pending",
] as const;
export type MockSyncScenario = (typeof MOCK_SYNC_SCENARIOS)[number];

export const MOCK_SELF_DEVICE_ID = "d-0123456789ab";
/** ブラウザ（`pnpm dev`）にはフォルダ選択のダイアログがないので、選んだことにするパス。 */
export const MOCK_FOLDER_PATH = "/Users/mock/Google Drive/マイドライブ";

/** URL の `?mockSync=error` で最初の状態を決める。知らない値は未設定。 */
export function mockSyncScenarioFromLocation(
  search: string = globalThis.location?.search ?? "",
): MockSyncScenario {
  const raw = new URLSearchParams(search).get("mockSync");
  return MOCK_SYNC_SCENARIOS.find((scenario) => scenario === raw) ?? "disabled";
}

/** フォルダを選んだときの判定の見本（`inspect_sync_folder` の戻り値の元になる）。 */
export interface MockInspectSetting {
  kind: FolderKind;
  localHasData: boolean;
  /** kind が `otherLibrary` のとき、この端末が属しているライブラリ ID。 */
  localLibraryId: string | null;
}

export interface MockSyncOptions {
  now?: () => Date;
  scenario?: MockSyncScenario;
  /** 設定（最初の同期）にかかる時間。画面で「設定中…」が見えるように、既定は少し待つ。テストでは 0。 */
  setupDelayMs?: number;
  /** 「今すぐ同期」にかかる時間。 */
  syncDelayMs?: number;
}

const MIN = 60_000;

function iso(now: Date, minutesAgo: number): string {
  return new Date(now.getTime() - minutesAgo * MIN).toISOString();
}

/** 見本の他の端末（同期フォルダにいる端末）。 */
function otherDevices(now: Date): SyncDevice[] {
  return [
    {
      deviceId: "d-7f3a2c01b9e4",
      name: "Windows デスクトップ",
      os: "windows",
      appVersion: "0.1.0",
      lastExportAt: iso(now, 25),
      isSelf: false,
    },
    {
      deviceId: "d-b41d9e07c2aa",
      name: "iMac",
      os: "macos",
      appVersion: "0.1.0",
      lastExportAt: iso(now, 60 * 30),
      isSelf: false,
    },
  ];
}

function selfDevice(name: string, lastExportAt: string | null): SyncDevice {
  return {
    deviceId: MOCK_SELF_DEVICE_ID,
    name,
    os: "macos",
    appVersion: "0.1.0",
    lastExportAt,
    isSelf: true,
  };
}

function disabledStatus(deviceName: string): SyncStatus {
  return {
    state: "disabled",
    folder: null,
    libraryId: null,
    deviceId: MOCK_SELF_DEVICE_ID,
    deviceName,
    lastExportAt: null,
    lastImportAt: null,
    pendingOps: 0,
    missingCovers: 0,
    error: null,
    clockSkew: null,
    devices: [],
  };
}

const MOCK_LIBRARY_ID = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55";

/** 設定済みで、問題がない状態。ここに見本ごとの違いを重ねる。 */
function configuredStatus(now: Date, deviceName: string): SyncStatus {
  const lastExportAt = iso(now, 3);
  return {
    state: "idle",
    folder: `${MOCK_FOLDER_PATH}/bibcontainer-library`,
    libraryId: MOCK_LIBRARY_ID,
    deviceId: MOCK_SELF_DEVICE_ID,
    deviceName,
    lastExportAt,
    lastImportAt: iso(now, 1),
    pendingOps: 0,
    missingCovers: 0,
    error: null,
    clockSkew: null,
    devices: [selfDevice(deviceName, lastExportAt), ...otherDevices(now)],
  };
}

function scenarioStatus(scenario: MockSyncScenario, now: Date, deviceName: string): SyncStatus {
  if (scenario === "disabled") return disabledStatus(deviceName);
  const base = configuredStatus(now, deviceName);
  switch (scenario) {
    case "idle":
      return base;
    case "syncing":
      return { ...base, state: "syncing" };
    case "error":
      return {
        ...base,
        state: "error",
        pendingOps: 12,
        error: "同期フォルダに書き込めませんでした（権限がないか、容量が足りません）",
      };
    case "mismatch":
      return { ...base, state: "mismatch" };
    case "updateRequired":
      return { ...base, state: "updateRequired" };
    case "skew":
      return {
        ...base,
        clockSkew: {
          deviceId: "d-7f3a2c01b9e4",
          deviceName: "Windows デスクトップ",
          aheadMs: 12 * MIN,
        },
      };
    case "pending":
      return { ...base, pendingOps: 7, missingCovers: 3 };
  }
}

/** パスの区切りに合わせて、選んだフォルダの下の bibcontainer-library のパスにする。選んだフォルダ自体がそれなら、そのまま。 */
export function libraryFolderOf(path: string): string {
  const separator = path.includes("\\") && !path.includes("/") ? "\\" : "/";
  const trimmed = path.replace(/[\\/]+$/, "");
  const last = trimmed.split(/[\\/]/).pop();
  return last === "bibcontainer-library" ? trimmed : `${trimmed}${separator}bibcontainer-library`;
}

function conflict(message: string): IpcError {
  return new IpcError({ kind: "conflict", message });
}

function validation(message: string): IpcError {
  return new IpcError({ kind: "validation", message });
}

export function createMockSync(options: MockSyncOptions = {}) {
  const now = options.now ?? (() => new Date());
  let setupDelayMs = options.setupDelayMs ?? 900;
  let syncDelayMs = options.syncDelayMs ?? 1500;
  let deviceName = "MacBook Pro";
  let status = scenarioStatus(options.scenario ?? "disabled", now(), deviceName);
  let inspect: MockInspectSetting = { kind: "library", localHasData: true, localLibraryId: null };
  let garbageCovers = 3;
  const listeners = new Set<(status: SyncStatus) => void>();

  const snapshot = (): SyncStatus => structuredClone(status);
  const emit = () => {
    for (const listener of [...listeners]) listener(snapshot());
  };
  const set = (next: SyncStatus) => {
    status = next;
    emit();
  };
  const sleep = (ms: number) =>
    ms > 0 ? new Promise<void>((resolve) => setTimeout(resolve, ms)) : Promise.resolve();

  const buildInspection = (path: string): FolderInspection => ({
    kind: inspect.kind,
    libraryFolder: libraryFolderOf(path),
    libraryId:
      inspect.kind === "empty" || inspect.kind === "notWritable"
        ? null
        : inspect.kind === "otherLibrary"
          ? "0192aa00-0000-7000-8000-000000000000"
          : MOCK_LIBRARY_ID,
    devices:
      inspect.kind === "library" ||
      inspect.kind === "otherLibrary" ||
      inspect.kind === "updateRequired"
        ? otherDevices(now())
        : [],
    localHasData: inspect.localHasData,
    localLibraryId: inspect.kind === "otherLibrary" ? inspect.localLibraryId : null,
  });

  const api: SyncApi = {
    getSyncStatus: () => Promise.resolve(snapshot()),

    inspectSyncFolder: (path) => Promise.resolve(buildInspection(path)),

    setupSync: async (path, mode: SetupMode) => {
      const inspection = buildInspection(path);
      if (status.state !== "disabled") throw conflict("すでに同期フォルダが設定されています");
      if (inspection.kind === "otherLibrary") throw conflict("別のライブラリのフォルダです");
      if (inspection.kind === "notWritable") throw conflict("このフォルダには書き込めません");
      if (inspection.kind === "updateRequired") throw conflict("アプリの更新が必要です");
      const expected: SetupMode[] =
        inspection.kind === "empty"
          ? ["create"]
          : inspection.localHasData
            ? ["joinMerge", "joinReplace"]
            : ["join"];
      if (!expected.includes(mode)) throw validation(`この状態では「${mode}」は選べません`);

      const at = now();
      set({
        ...configuredStatus(at, deviceName),
        state: "syncing",
        folder: inspection.libraryFolder,
        lastExportAt: null,
        lastImportAt: null,
        devices: [selfDevice(deviceName, null)],
      });
      await sleep(setupDelayMs);

      const done = now();
      const imports = mode !== "create";
      const exports = mode === "create" || mode === "joinMerge";
      const lastExportAt = exports ? done.toISOString() : null;
      set({
        ...configuredStatus(done, deviceName),
        folder: inspection.libraryFolder,
        lastExportAt,
        lastImportAt: imports ? done.toISOString() : null,
        devices: [selfDevice(deviceName, lastExportAt), ...(imports ? otherDevices(done) : [])],
      });
      return snapshot();
    },

    leaveSync: () => {
      // 解除すると、この端末はどのライブラリにも属さなくなる。「別のライブラリ」の判定は、ふつうの「ライブラリあり」になる
      inspect = {
        ...inspect,
        kind: inspect.kind === "otherLibrary" ? "library" : inspect.kind,
        localLibraryId: null,
      };
      set(disabledStatus(deviceName));
      return Promise.resolve(snapshot());
    },

    syncNow: async () => {
      if (status.state === "disabled") throw conflict("同期フォルダが設定されていません");
      if (status.state === "mismatch" || status.state === "updateRequired") {
        throw conflict("今の状態では同期できません");
      }
      set({ ...status, state: "syncing" });
      await sleep(syncDelayMs);
      const done = now().toISOString();
      set({
        ...status,
        state: "idle",
        lastExportAt: done,
        lastImportAt: done,
        pendingOps: 0,
        error: null,
        devices: status.devices.map((d) => (d.isSelf ? { ...d, lastExportAt: done } : d)),
      });
      return snapshot();
    },

    setDeviceName: (name) => {
      const trimmed = name.trim();
      if (trimmed === "") return Promise.reject(validation("端末名を入力してください"));
      if (trimmed.length > 64) {
        return Promise.reject(validation("端末名は 64 文字までです"));
      }
      deviceName = trimmed;
      set({
        ...status,
        deviceName: trimmed,
        devices: status.devices.map((d) => (d.isSelf ? { ...d, name: trimmed } : d)),
      });
      return Promise.resolve(snapshot());
    },

    gcCovers: () => {
      if (status.state === "disabled") {
        return Promise.reject(conflict("同期フォルダが設定されていません"));
      }
      const removed = garbageCovers;
      garbageCovers = 0;
      return Promise.resolve(removed);
    },

    onSyncStatus: (handler) => {
      listeners.add(handler);
      return Promise.resolve(() => {
        listeners.delete(handler);
      });
    },
  };

  /** 開発用の操作（開発用のパネルとテストが使う）。 */
  const control = {
    /** 状態を見本のものに切り替える（`sync-status` を出す）。 */
    setScenario(scenario: MockSyncScenario) {
      set(scenarioStatus(scenario, now(), deviceName));
    },
    /** 状態の一部だけ書き換える（`sync-status` を出す）。 */
    patch(partial: Partial<SyncStatus>) {
      set({ ...status, ...partial });
    },
    /** フォルダを選んだときの判定を切り替える。 */
    setInspect(partial: Partial<MockInspectSetting>) {
      inspect = { ...inspect, ...partial };
    },
    getInspect: (): MockInspectSetting => ({ ...inspect }),
    setGarbageCovers(count: number) {
      garbageCovers = count;
    },
    setDelays(delays: { setupMs?: number; syncMs?: number }) {
      if (delays.setupMs !== undefined) setupDelayMs = delays.setupMs;
      if (delays.syncMs !== undefined) syncDelayMs = delays.syncMs;
    },
    /** 最初の状態に戻す（テスト用）。購読者には知らせる。 */
    reset(scenario: MockSyncScenario = "disabled") {
      deviceName = "MacBook Pro";
      inspect = { kind: "library", localHasData: true, localLibraryId: null };
      garbageCovers = 3;
      set(scenarioStatus(scenario, now(), deviceName));
    },
    listenerCount: () => listeners.size,
  };

  return { api, control };
}

/** アプリ全体で使う 1 つのモック。 */
export const mockSync = createMockSync({ scenario: mockSyncScenarioFromLocation() });
