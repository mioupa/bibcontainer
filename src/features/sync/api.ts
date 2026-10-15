// 同期の呼び出し口。画面・フックは、必ずここを通して同期を操作する（0013 §9、0015、0016）。
//
// Tauri の中では IPC（Rust の同期エンジン。`ipc.ts` の同期のコマンドと `sync-status` の購読）、外（ブラウザの
// `pnpm dev`）ではモック（src/dev/mockSync.ts）で動く。どちらを使うかを決めるのは、下の `selectBackend` の 1 か所だけ。
import { mockSync } from "@/dev/mockSync";
import type { SetupMode, SyncApi } from "@/features/sync/types";
import { ipc, onSyncStatus as listenSyncStatus } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";

/**
 * IPC 版。`SyncApi` の形（`setupSync`・`leaveSync`・`syncNow`・`setDeviceName` は `SyncStatus`、`gcCovers` は
 * 消した件数）は、Rust のコマンドの戻り値とそのまま同じ。
 */
export const ipcSyncApi: SyncApi = {
  getSyncStatus: () => ipc.getSyncStatus(),
  inspectSyncFolder: (path) => ipc.inspectSyncFolder(path),
  setupSync: (path, mode) => ipc.setupSync(path, mode),
  leaveSync: () => ipc.leaveSync(),
  syncNow: () => ipc.syncNow(),
  setDeviceName: (name) => ipc.setDeviceName(name),
  gcCovers: () => ipc.gcCovers(),
  onSyncStatus: (handler) => listenSyncStatus(handler),
};

/** 差し替え箇所（1 か所）。呼ぶたびに選ぶので、テストはモックの操作（`mockSync.control`）で状態を作れる。 */
function selectBackend(): SyncApi {
  return inTauri() ? ipcSyncApi : mockSync.api;
}

export const getSyncStatus = () => selectBackend().getSyncStatus();
export const inspectSyncFolder = (path: string) => selectBackend().inspectSyncFolder(path);
export const setupSync = (path: string, mode: SetupMode) => selectBackend().setupSync(path, mode);
export const leaveSync = () => selectBackend().leaveSync();
export const syncNow = () => selectBackend().syncNow();
export const setDeviceName = (name: string) => selectBackend().setDeviceName(name);
export const gcCovers = () => selectBackend().gcCovers();

/** `sync-status` イベントを購読する。返した関数で購読をやめる。 */
export const onSyncStatus = (handler: Parameters<SyncApi["onSyncStatus"]>[0]) =>
  selectBackend().onSyncStatus(handler);

/** 同期の画面が、モックで動いているか（開発用のパネルを出すかの判断）。`selectBackend` から導くので、替える箇所は増えない。 */
export function usingMockSync(): boolean {
  return selectBackend() === mockSync.api;
}
