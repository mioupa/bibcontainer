// 同期（フェーズ 2）の型。0013 §9 の IPC の型（camelCase）と同じ形の、暫定の定義。
//
// 統合のとき、Rust 側の ts-rs の生成物（src/bindings/）の再エクスポートに置き換える。
// 画面・フック・モックは、この型を `@/features/sync/types` から import する（bindings を直接 import しない）。
// 置き換えても import 先が変わらないようにするため。

/** 同期の状態。disabled は同期フォルダが未設定。 */
export type SyncState = "disabled" | "idle" | "syncing" | "error" | "mismatch" | "updateRequired";

/** 同期フォルダで見つかった端末（自分を含む）。 */
export interface SyncDevice {
  deviceId: string;
  name: string;
  /** `macos`／`windows`／`linux` など（Rust の `std::env::consts::OS`）。 */
  os: string;
  appVersion: string;
  /** 最後に書き出した時刻（ISO 8601）。まだ書き出していなければ null。 */
  lastExportAt: string | null;
  isSelf: boolean;
}

/** 他の端末の時計が進んでいる（FR-SYNC-11。5 分以上）。 */
export interface ClockSkew {
  deviceId: string;
  deviceName: string | null;
  aheadMs: number;
}

export interface SyncStatus {
  state: SyncState;
  /** bibcontainer-library のフォルダの絶対パス。未設定なら null。 */
  folder: string | null;
  libraryId: string | null;
  deviceId: string;
  deviceName: string;
  /** ISO 8601。 */
  lastExportAt: string | null;
  lastImportAt: string | null;
  /** 未書き出しの Op の数。 */
  pendingOps: number;
  /** まだ届いていない書影の数。 */
  missingCovers: number;
  /** 直近のエラー（日本語）。 */
  error: string | null;
  clockSkew: ClockSkew | null;
  devices: SyncDevice[];
}

/** 選んだフォルダの中身の判定（`inspect_sync_folder`）。 */
export type FolderKind = "empty" | "library" | "otherLibrary" | "notWritable" | "updateRequired";

export interface FolderInspection {
  kind: FolderKind;
  /** 実際に使う bibcontainer-library の絶対パス。 */
  libraryFolder: string;
  libraryId: string | null;
  devices: SyncDevice[];
  /** この端末に蔵書のデータがあるか。 */
  localHasData: boolean;
  /** この端末の DB が属しているライブラリ ID（まだ属していなければ null）。 */
  localLibraryId: string | null;
}

/** 初めての設定の方法。create＝新しく作る、join＝参加（手元が空）、joinMerge＝統合、joinReplace＝置き換え。 */
export type SetupMode = "create" | "join" | "joinMerge" | "joinReplace";

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
