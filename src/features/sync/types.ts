// 同期の型。Rust 側（crates/sync の status.rs）の ts-rs の生成物（src/bindings/）をそのまま再エクスポートする。
// 画面・フック・モックは、この型を `@/features/sync/types` から import する（bindings を直接 import しない）。
import type { FolderInspection } from "@/bindings/FolderInspection";
import type { SetupMode } from "@/bindings/SetupMode";
import type { SyncStatus } from "@/bindings/SyncStatus";

export type { ClockSkew } from "@/bindings/ClockSkew";
export type { FolderInspection } from "@/bindings/FolderInspection";
export type { FolderKind } from "@/bindings/FolderKind";
export type { SetupMode } from "@/bindings/SetupMode";
export type { SyncDevice } from "@/bindings/SyncDevice";
export type { SyncState } from "@/bindings/SyncState";
export type { SyncStatus } from "@/bindings/SyncStatus";

/**
 * 同期の呼び出し口（0013 §9 のコマンドに 1 対 1 で対応する）。
 * 実装は 2 つ: IPC（Rust の同期エンジン）とモック（src/dev/mockSync.ts）。どちらを使うかは api.ts が決める。
 */
export interface SyncApi {
  getSyncStatus: () => Promise<SyncStatus>;
  inspectSyncFolder: (path: string) => Promise<FolderInspection>;
  /** 設定が済む（最初の同期が終わる）まで待って、そのあとの状態を返す。 */
  setupSync: (path: string, mode: SetupMode) => Promise<SyncStatus>;
  /** 同期を解除する。手元のデータと同期フォルダの中身は消えない。解除後の状態を返す。 */
  leaveSync: () => Promise<SyncStatus>;
  /** 取り込みと書き出しを今すぐ行う。終わったあとの状態を返す。 */
  syncNow: () => Promise<SyncStatus>;
  setDeviceName: (name: string) => Promise<SyncStatus>;
  /** 使われていない書影を整理して、消した件数を返す（FR-SYNC-13）。 */
  gcCovers: () => Promise<number>;
  /** `sync-status` イベントを購読する。返した関数で購読をやめる。 */
  onSyncStatus: (handler: (status: SyncStatus) => void) => Promise<() => void>;
}
