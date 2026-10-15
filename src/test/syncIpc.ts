// テスト用: 「Tauri の中」を模したテスト（`isTauri` が true、`invoke` と `listen` を差し替え）で、同期のコマンドと
// `sync-status` を、モックの同期（src/dev/mockSync.ts。状態を切り替えられる）につなぐ。
// 同期の画面は、Tauri の中では IPC（ipc.ts）を呼ぶので、画面のテストはこれで IPC の層を通しつつ、状態をモックで決める。
import { mockSync } from "@/dev/mockSync";
import type { SetupMode, SyncStatus } from "@/features/sync/types";

type Args = Record<string, unknown> | undefined;

/** `invoke` の返答の表（コマンド名 → 返答）に足す、同期のコマンド。 */
export const syncInvokeHandlers = {
  get_sync_status: () => mockSync.api.getSyncStatus(),
  inspect_sync_folder: (args: Args) => mockSync.api.inspectSyncFolder(args?.path as string),
  setup_sync: (args: Args) => mockSync.api.setupSync(args?.path as string, args?.mode as SetupMode),
  leave_sync: () => mockSync.api.leaveSync(),
  sync_now: () => mockSync.api.syncNow(),
  set_device_name: (args: Args) => mockSync.api.setDeviceName(args?.name as string),
  gc_covers: () => mockSync.api.gcCovers(),
};

/**
 * `listen` の返答。`sync-status` はモックの同期につなぎ（状態を切り替えると届く）、ほかのイベントは `null`
 * （呼び出し側が処理する）。
 */
export function listenToMockSync(
  name: string,
  callback: (event: { payload: SyncStatus }) => void,
): Promise<() => void> | null {
  if (name !== "sync-status") return null;
  return mockSync.api.onSyncStatus((status) => callback({ payload: status }));
}
