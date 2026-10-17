// フロントエンドから Rust のコマンドを呼ぶ唯一の入口。
// コンポーネントは `invoke` を直接呼ばず、ここの `ipc` を使う。
// コマンドを足すときは、Rust 側の #[tauri::command] と CommandMap の両方に追記する。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppError } from "@/bindings/AppError";
import type { AppInfo } from "@/bindings/AppInfo";
import type { BackupInfo } from "@/bindings/BackupInfo";
import type { BackupSettings } from "@/bindings/BackupSettings";
import type { BookDetail } from "@/bindings/BookDetail";
import type { CCodeInfo } from "@/bindings/CCodeInfo";
import type { CCodeTable } from "@/bindings/CCodeTable";
import type { ChangeReport } from "@/bindings/ChangeReport";
import type { CoversArrived } from "@/bindings/CoversArrived";
import type { DisplaySettings } from "@/bindings/DisplaySettings";
import type { ErrorKind } from "@/bindings/ErrorKind";
import type { FetchMetadataResult } from "@/bindings/FetchMetadataResult";
import type { FieldChange } from "@/bindings/FieldChange";
import type { FolderInspection } from "@/bindings/FolderInspection";
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import type { LibraryChanged } from "@/bindings/LibraryChanged";
import type { ListRow } from "@/bindings/ListRow";
import type { ListSettings } from "@/bindings/ListSettings";
import type { LocationInfo } from "@/bindings/LocationInfo";
import type { MergeWorksResult } from "@/bindings/MergeWorksResult";
import type { MetaSecondTier } from "@/bindings/MetaSecondTier";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import type { PriceSettings } from "@/bindings/PriceSettings";
import type { ProviderId } from "@/bindings/ProviderId";
import type { ProviderSettings } from "@/bindings/ProviderSettings";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { ProviderTestResult } from "@/bindings/ProviderTestResult";
import type { RefetchSummary } from "@/bindings/RefetchSummary";
import type { RestoreReport } from "@/bindings/RestoreReport";
import type { ScanCode } from "@/bindings/ScanCode";
import type { ScanSettings } from "@/bindings/ScanSettings";
import type { SeriesInfo } from "@/bindings/SeriesInfo";
import type { SetupMode } from "@/bindings/SetupMode";
import type { StoreCount } from "@/bindings/StoreCount";
import type { SyncStatus } from "@/bindings/SyncStatus";
import type { TagInfo } from "@/bindings/TagInfo";
import type { UpdateOptions } from "@/bindings/UpdateOptions";

/**
 * コマンドが失敗したときに投げられるエラー。
 * `kind` で種類を判別し、`message` に Rust 側が用意した原因の説明が入る。
 */
export class IpcError extends Error {
  readonly kind: ErrorKind;

  constructor(payload: AppError) {
    super(payload.message);
    this.name = "IpcError";
    this.kind = payload.kind;
  }
}

function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    typeof value.kind === "string" &&
    "message" in value &&
    typeof value.message === "string"
  );
}

/** invoke が拒否した値を IpcError にそろえる。Rust 側の AppError 以外（未登録のコマンドなど）は internal にする。 */
function toIpcError(reason: unknown): IpcError {
  if (reason instanceof IpcError) return reason;
  if (isAppError(reason)) return new IpcError(reason);
  const message = reason instanceof Error ? reason.message : String(reason);
  return new IpcError({ kind: "internal", message });
}

/**
 * コマンド名ごとの引数と戻り値の型。引数がないコマンドは args を省く。
 * 引数のキーは、Rust の引数名（snake_case）を camelCase にしたもの（Tauri の既定）。
 * コマンドの分け方は src-tauri/src/commands/（docs/decisions/0009）。
 */
interface CommandMap {
  app_info: { result: AppInfo };

  // 一覧・検索・参照（commands/library.rs）
  list_books: { result: ListRow[] };
  get_rows: { args: { ids: string[] }; result: ListRow[] };
  get_book_detail: { args: { id: string }; result: BookDetail };
  search_books: { args: { query: string }; result: string[] };
  find_by_isbn: { args: { isbn: string }; result: IsbnMatch[] };
  purchase_store_suggestions: { result: StoreCount[] };
  pending_book_ids: { result: string[] };

  // 書籍の変更（commands/books.rs）。成功すると library-changed が届く
  create_book: { args: { changes: FieldChange[] }; result: string };
  create_books: { args: { books: FieldChange[][] }; result: string[] };
  update_book: {
    args: { id: string; changes: FieldChange[]; options?: UpdateOptions };
    result: ChangeReport;
  };
  update_books: {
    args: { ids: string[]; changes: FieldChange[]; options?: UpdateOptions };
    result: ChangeReport[];
  };
  modify_tags: {
    args: { ids: string[]; add: string[]; remove: string[] };
    result: string[];
  };
  trash_books: { args: { ids: string[] }; result: string[] };
  restore_books: { args: { ids: string[] }; result: string[] };
  purge_books: { args: { ids: string[] }; result: string[] };
  /** 選んだ本を 1 つの作品にまとめる（0026）。作品から外す・自動に戻すは update_books で work_id を書く。 */
  merge_works: { args: { ids: string[] }; result: MergeWorksResult };

  // 保管場所・タグ・シリーズ（commands/organize.rs）
  list_locations: { result: LocationInfo[] };
  create_location: { args: { name: string; parentId: string | null }; result: string };
  rename_location: { args: { id: string; name: string }; result: null };
  move_location: { args: { id: string; newParentId: string | null }; result: null };
  reorder_locations: {
    args: { parentId: string | null; orderedIds: string[] };
    result: null;
  };
  delete_location: { args: { id: string }; result: null };
  list_tags: { result: TagInfo[] };
  rename_tag: { args: { old: string; new: string }; result: string[] };
  set_tag_color: { args: { name: string; color: string | null }; result: string };
  list_series: { result: SeriesInfo[] };
  find_or_create_series: { args: { name: string }; result: string };
  rename_series: { args: { id: string; name: string }; result: null };

  // バーコード（commands/barcode.rs）
  classify_code: { args: { raw: string }; result: ScanCode };
  c_code_table: { result: CCodeTable };
  describe_c_code: { args: { code: string }; result: CCodeInfo | null };

  // 端末ごとの設定（commands/settings.rs）。set_* は範囲に収めて保存した値を返す
  get_scan_settings: { result: ScanSettings };
  set_scan_settings: { args: { settings: ScanSettings }; result: ScanSettings };
  get_display_settings: { result: DisplaySettings };
  set_display_settings: { args: { settings: DisplaySettings }; result: DisplaySettings };
  get_list_settings: { result: ListSettings };
  set_list_settings: { args: { settings: ListSettings }; result: ListSettings };
  get_price_settings: { result: PriceSettings };
  set_price_settings: { args: { settings: PriceSettings }; result: PriceSettings };
  get_backup_settings: { result: BackupSettings };
  set_backup_settings: { args: { settings: BackupSettings }; result: BackupSettings };

  // 書き出し・取り込み（commands/io.rs）
  export_csv: { args: { path: string; ids?: string[] }; result: number };
  export_json: { args: { path: string }; result: null };
  parse_isbn_list: { args: { path: string }; result: ScanCode[] };

  // バックアップ（commands/backup.rs）
  list_backups: { result: BackupInfo[] };
  manual_backup: { result: BackupInfo };
  restore_backup: { args: { path: string }; result: RestoreReport };

  // フォルダを開く（commands/system.rs）
  open_logs_dir: { result: null };
  open_backup_dir: { result: null };

  // 書誌の取得（src-tauri/src/metadata/commands.rs。0010）
  get_provider_settings: { result: ProviderSettings };
  set_provider_settings: { args: { settings: ProviderSettings }; result: ProviderSettings };
  list_providers: { result: ProviderStatus[] };
  test_provider: { args: { id: ProviderId }; result: ProviderTestResult };
  set_api_key: { args: { provider: ProviderId; key: string }; result: null };
  delete_api_key: { args: { provider: ProviderId }; result: null };
  has_api_key: { args: { provider: ProviderId }; result: boolean };
  fetch_metadata: {
    args: { isbn13: string; secondTier?: MetaSecondTier };
    result: FetchMetadataResult;
  };
  create_books_from_drafts: { args: { drafts: NewBookDraft[] }; result: string[] };
  refetch_books: { args: { ids: string[]; overrideManual: boolean }; result: RefetchSummary };
  trigger_pending_refetch: { result: null };
  import_cover_file: { args: { path: string }; result: string };
  /** 画像のバイト列は、Uint8Array をそのまま渡すと、バイナリのまま送られる（大きい画像向け）。 */
  import_cover_bytes: { args: Uint8Array | { bytes: number[] }; result: string };
  import_cover_url: { args: { url: string }; result: string };

  // 同期（src-tauri/src/sync/commands.rs。0016）。状態の変化は sync-status イベントでも届く
  get_sync_status: { result: SyncStatus };
  inspect_sync_folder: { args: { path: string }; result: FolderInspection };
  setup_sync: { args: { path: string; mode: SetupMode }; result: SyncStatus };
  leave_sync: { result: SyncStatus };
  sync_now: { result: SyncStatus };
  set_device_name: { args: { name: string }; result: SyncStatus };
  /** 消した書影の数。 */
  gc_covers: { result: number };
}

type CommandName = keyof CommandMap;
type ArgsOf<C extends CommandName> = CommandMap[C] extends { args: infer A } ? A : never;
type ResultOf<C extends CommandName> = CommandMap[C]["result"];

async function call<C extends CommandName>(
  command: C,
  ...args: [ArgsOf<C>] extends [never] ? [] : [args: ArgsOf<C>]
): Promise<ResultOf<C>> {
  try {
    return await invoke<ResultOf<C>>(command, args[0] as Record<string, unknown> | undefined);
  } catch (reason) {
    throw toIpcError(reason);
  }
}

export const ipc = {
  /** アプリ名・バージョン・DB の状態を返す（動作確認と診断用）。 */
  appInfo: () => call("app_info"),

  // ---- 一覧・検索・参照 ----
  /** 全書籍の一覧用の行（ゴミ箱のものを含み、完全削除は除く）。 */
  listBooks: () => call("list_books"),
  /** 指定した本の行。存在しない・完全削除した ID は含まれない。 */
  getRows: (ids: string[]) => call("get_rows", { ids }),
  getBookDetail: (id: string) => call("get_book_detail", { id }),
  /** 検索語に一致した本の ID。 */
  searchBooks: (query: string) => call("search_books", { query }),
  /** ISBN が一致する本（重複チェック）。 */
  findByIsbn: (isbn: string) => call("find_by_isbn", { isbn }),
  purchaseStoreSuggestions: () => call("purchase_store_suggestions"),
  pendingBookIds: () => call("pending_book_ids"),

  // ---- 書籍の変更 ----
  createBook: (changes: FieldChange[]) => call("create_book", { changes }),
  createBooks: (books: FieldChange[][]) => call("create_books", { books }),
  updateBook: (id: string, changes: FieldChange[], options?: UpdateOptions) =>
    call("update_book", { id, changes, options }),
  updateBooks: (ids: string[], changes: FieldChange[], options?: UpdateOptions) =>
    call("update_books", { ids, changes, options }),
  modifyTags: (ids: string[], add: string[], remove: string[]) =>
    call("modify_tags", { ids, add, remove }),
  trashBooks: (ids: string[]) => call("trash_books", { ids }),
  restoreBooks: (ids: string[]) => call("restore_books", { ids }),
  purgeBooks: (ids: string[]) => call("purge_books", { ids }),
  /** 選んだ本（と、その本が入っている手動の作品の全巻）を、1 つの手動の作品にまとめる。 */
  mergeWorks: (ids: string[]) => call("merge_works", { ids }),

  // ---- 保管場所・タグ・シリーズ ----
  listLocations: () => call("list_locations"),
  createLocation: (name: string, parentId: string | null = null) =>
    call("create_location", { name, parentId }),
  renameLocation: (id: string, name: string) => call("rename_location", { id, name }),
  moveLocation: (id: string, newParentId: string | null) =>
    call("move_location", { id, newParentId }),
  reorderLocations: (parentId: string | null, orderedIds: string[]) =>
    call("reorder_locations", { parentId, orderedIds }),
  deleteLocation: (id: string) => call("delete_location", { id }),
  listTags: () => call("list_tags"),
  /** タグ名を変える。`親/子` の子孫もまとめて変わる。変わった本の ID を返す。 */
  renameTag: (oldName: string, newName: string) =>
    call("rename_tag", { old: oldName, new: newName }),
  setTagColor: (name: string, color: string | null) => call("set_tag_color", { name, color }),
  listSeries: () => call("list_series"),
  findOrCreateSeries: (name: string) => call("find_or_create_series", { name }),
  renameSeries: (id: string, name: string) => call("rename_series", { id, name }),

  // ---- バーコード ----
  /** 読み取った文字列を判定する（ISBN-13・2段目・雑誌・一般 JAN・不正）。 */
  classifyCode: (raw: string) => call("classify_code", { raw }),
  /** Cコード表（配列の形。引きやすい形への変換は features/library/api.ts）。 */
  cCodeTable: () => call("c_code_table"),
  describeCCode: (code: string) => call("describe_c_code", { code }),

  // ---- 設定（端末ごと） ----
  getScanSettings: () => call("get_scan_settings"),
  setScanSettings: (settings: ScanSettings) => call("set_scan_settings", { settings }),
  getDisplaySettings: () => call("get_display_settings"),
  setDisplaySettings: (settings: DisplaySettings) => call("set_display_settings", { settings }),
  getListSettings: () => call("get_list_settings"),
  setListSettings: (settings: ListSettings) => call("set_list_settings", { settings }),
  getPriceSettings: () => call("get_price_settings"),
  setPriceSettings: (settings: PriceSettings) => call("set_price_settings", { settings }),
  getBackupSettings: () => call("get_backup_settings"),
  setBackupSettings: (settings: BackupSettings) => call("set_backup_settings", { settings }),

  // ---- 書き出し・取り込み ----
  /** CSV に書き出す。`ids` を渡すと、その本だけを渡した順に書く。書いた冊数を返す。 */
  exportCsv: (path: string, ids?: string[]) => call("export_csv", { path, ids }),
  exportJson: (path: string) => call("export_json", { path }),
  /** ISBN 一覧のファイルを読み、各行を判定した結果を返す。 */
  parseIsbnList: (path: string) => call("parse_isbn_list", { path }),

  // ---- バックアップ ----
  listBackups: () => call("list_backups"),
  manualBackup: () => call("manual_backup"),
  /** バックアップから復元する。終わったら、呼び出し側は全データを取り直すこと。 */
  restoreBackup: (path: string) => call("restore_backup", { path }),

  // ---- フォルダを開く ----
  openLogsDir: () => call("open_logs_dir"),
  openBackupDir: () => call("open_backup_dir"),

  // ---- 書誌の取得 ----
  getProviderSettings: () => call("get_provider_settings"),
  setProviderSettings: (settings: ProviderSettings) => call("set_provider_settings", { settings }),
  /** 取得元の一覧（能力・クレジット表記・有効・キーの有無・実際に使うか）。 */
  listProviders: () => call("list_providers"),
  /** 既知の ISBN を取得して、取得元の接続を確かめる。 */
  testProvider: (id: ProviderId) => call("test_provider", { id }),
  /** API キーを OS のキーチェーンに保存する。キーを返すコマンドはない。 */
  setApiKey: (provider: ProviderId, key: string) => call("set_api_key", { provider, key }),
  deleteApiKey: (provider: ProviderId) => call("delete_api_key", { provider }),
  hasApiKey: (provider: ProviderId) => call("has_api_key", { provider }),
  /** ISBN の書誌を取得して、登録フォームの下書きを返す（DB には書かない）。 */
  fetchMetadata: (isbn13: string, secondTier?: MetaSecondTier) =>
    call("fetch_metadata", { isbn13, secondTier }),
  /** 下書きから本を作る（全体で 1 トランザクション）。新しい本の ID を渡した順に返す。 */
  createBooksFromDrafts: (drafts: NewBookDraft[]) => call("create_books_from_drafts", { drafts }),
  /** 選んだ本の書誌を再取得する。`overrideManual` が false なら、手で直した項目は上書きしない。 */
  refetchBooks: (ids: string[], overrideManual: boolean) =>
    call("refetch_books", { ids, overrideManual }),
  /** 取得待ちの本の再取得を、今すぐ試す（すぐ戻る）。 */
  triggerPendingRefetch: () => call("trigger_pending_refetch"),
  /** 書影を取り込んで、保存した書影のハッシュを返す。本への設定は、`update_book` で `cover_hash` を書く。 */
  importCoverFile: (path: string) => call("import_cover_file", { path }),
  /** バイト列は、`Uint8Array` ならバイナリのまま送る。数値の配列は `{ bytes }` で送る。 */
  importCoverBytes: (bytes: Uint8Array | number[]) =>
    call("import_cover_bytes", Array.isArray(bytes) ? { bytes } : bytes),
  importCoverUrl: (url: string) => call("import_cover_url", { url }),

  // ---- 同期 ----
  getSyncStatus: () => call("get_sync_status"),
  /** 選んだフォルダの中身を調べる（何も残さない）。`libraryFolder` が実際に使う bibcontainer-library。 */
  inspectSyncFolder: (path: string) => call("inspect_sync_folder", { path }),
  /** 同期フォルダを設定する。最初の取り込みが済むまで待ち、そのあとの状態を返す。 */
  setupSync: (path: string, mode: SetupMode) => call("setup_sync", { path, mode }),
  leaveSync: () => call("leave_sync"),
  /** 取り込みと書き出しを今すぐ行い、終わったあとの状態を返す。失敗したらエラー（状態にも記録される）。 */
  syncNow: () => call("sync_now"),
  setDeviceName: (name: string) => call("set_device_name", { name }),
  /** 使われていない書影を整理して、消した数を返す。 */
  gcCovers: () => call("gc_covers"),
} as const;

/** 蔵書の変更の通知（`library-changed`）を受ける。返した関数で購読をやめる。 */
export function onLibraryChanged(handler: (change: LibraryChanged) => void): Promise<UnlistenFn> {
  return listen<LibraryChanged>("library-changed", (event) => handler(event.payload));
}

/** 同期の状態の通知（`sync-status`）を受ける。返した関数で購読をやめる。 */
export function onSyncStatus(handler: (status: SyncStatus) => void): Promise<UnlistenFn> {
  return listen<SyncStatus>("sync-status", (event) => handler(event.payload));
}

/** 同期で書影が手元に届いた通知（`covers-arrived`）を受ける。返した関数で購読をやめる。 */
export function onCoversArrived(handler: (arrived: CoversArrived) => void): Promise<UnlistenFn> {
  return listen<CoversArrived>("covers-arrived", (event) => handler(event.payload));
}
