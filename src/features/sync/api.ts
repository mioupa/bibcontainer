// 同期の呼び出し口。画面・フックは、必ずここを通して同期を操作する（0013 §9、0015）。
//
// いまは、Tauri の中でもモック（src/dev/mockSync.ts）で動かす。Rust の同期エンジンができたら、
// **下の `selectBackend` だけ**を、IPC 版（`ipc.ts` の同期のコマンド）を返すように直す:
//
//   function selectBackend(): SyncApi {
//     return inTauri() ? ipcSyncApi : mockSync.api;
//   }
//
// `ipcSyncApi` は、`ipc.ts` に足した `get_sync_status`・`inspect_sync_folder`・`setup_sync`・`leave_sync`・
// `sync_now`・`set_device_name`・`gc_covers` と、`sync-status` の購読（`onSyncStatus`）を `SyncApi` の形に包んだもの。
import { mockSync } from "@/dev/mockSync";
import type { SetupMode, SyncApi } from "@/features/sync/types";

/** 差し替え箇所（1 か所）。呼ぶたびに選ぶので、テストはモックの操作（`mockSync.control`）で状態を作れる。 */
function selectBackend(): SyncApi {
  return mockSync.api;
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
