// 蔵書データの取得・変更の入口。画面のフック（hooks.ts・actions.ts）は、必ずここを通してデータを扱う。
//
// Tauri のウィンドウの中では IPC（src/lib/ipc.ts）で Rust を呼ぶ。Tauri の外（ブラウザで `pnpm dev` を
// 開いたとき）は IPC がないので、ダミーのデータ（src/dev/mockBackend.ts）で動かす。どちらを使うかは
// 呼ぶたびに `inTauri()` で決める（テストが切り替えられるように）。
import type { CCodeTable as CCodeTableBinding } from "@/bindings/CCodeTable";
import type { ChangeReport } from "@/bindings/ChangeReport";
import type { FetchMetadataResult } from "@/bindings/FetchMetadataResult";
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import type { MetaSecondTier } from "@/bindings/MetaSecondTier";
import type { PriceSettings } from "@/bindings/PriceSettings";
import type { RefetchSummary } from "@/bindings/RefetchSummary";
import type { SeriesInfo } from "@/bindings/SeriesInfo";
import type { StoreCount } from "@/bindings/StoreCount";
import type { TagInfo } from "@/bindings/TagInfo";
import type { UpdateOptions } from "@/bindings/UpdateOptions";
import { mockBackend } from "@/dev/mockBackend";
import { DEFAULT_PRICE_SETTINGS } from "@/features/library/price";
import { ja } from "@/i18n/ja";
import { IpcError, ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";
import type {
  BookDetail,
  CCodeTable,
  FieldChange,
  ListRow,
  Location,
  NewBookDraft,
} from "@/types/contracts";

/** 画面が「モックのデータで動いている」か（ステータスバーに出す）。Tauri の外のとき true。 */
export function usingMockData(): boolean {
  return !inTauri();
}

/** `c_code_table()` の戻り値（`{ code, name }` の配列）を、名称を引く形（コード → 名称）にする。 */
export function toCCodeLookup(table: CCodeTableBinding): CCodeTable {
  const lookup = (entries: CCodeTableBinding["audience"]) =>
    Object.fromEntries(entries.map((entry) => [entry.code, entry.name]));
  return {
    audience: lookup(table.audience),
    format: lookup(table.format),
    contentMajor: lookup(table.contentMajor),
    content: lookup(table.content),
  };
}

/** 画面が使うデータ操作。どちらのバックエンド（IPC かモック）でも同じ形。 */
export interface LibraryApi {
  /** 全書籍の一覧用の行（ゴミ箱のものを含み、完全削除は除く）。 */
  listBooks: () => Promise<ListRow[]>;
  /** 指定した本の行。存在しない・完全削除した ID は含まれない。 */
  getRows: (ids: string[]) => Promise<ListRow[]>;
  listLocations: () => Promise<Location[]>;
  listTags: () => Promise<TagInfo[]>;
  listSeries: () => Promise<SeriesInfo[]>;
  cCodeTable: () => Promise<CCodeTable>;
  /** 検索語に一致した本の ID。 */
  searchBooks: (query: string) => Promise<string[]>;
  /** 変えた本の ID を返す。 */
  trashBooks: (ids: string[]) => Promise<string[]>;
  restoreBooks: (ids: string[]) => Promise<string[]>;
  purgeBooks: (ids: string[]) => Promise<string[]>;
  /** 読書状況・所有状態など、1 項目を手入力として複数の本に書く。変えた本の ID を返す。 */
  setBooksField: (ids: string[], field: string, value: unknown) => Promise<string[]>;

  // ---- 詳細・登録・編集（features/editor、features/detail） ----
  getBookDetail: (id: string) => Promise<BookDetail>;
  updateBook: (
    id: string,
    changes: FieldChange[],
    options?: UpdateOptions,
  ) => Promise<ChangeReport>;
  /** 複数の本のタグを、まとめて足す・外す。変えた本の ID を返す。 */
  modifyTags: (ids: string[], add: string[], remove: string[]) => Promise<string[]>;
  /** 下書きから本を作る（1 トランザクション）。新しい本の ID を渡した順に返す。 */
  createBooksFromDrafts: (drafts: NewBookDraft[]) => Promise<string[]>;
  /** ISBN が一致する本（重複チェック）。 */
  findByIsbn: (isbn: string) => Promise<IsbnMatch[]>;
  /** 購入店の候補（使用回数の多い順）。 */
  purchaseStores: () => Promise<StoreCount[]>;
  getPriceSettings: () => Promise<PriceSettings>;

  // ---- Rust が要る操作（Tauri の外のモックでは使えない） ----
  fetchMetadata: (isbn13: string, secondTier?: MetaSecondTier) => Promise<FetchMetadataResult>;
  refetchBooks: (ids: string[], overrideManual: boolean) => Promise<RefetchSummary>;
  importCoverFile: (path: string) => Promise<string>;
  importCoverBytes: (bytes: Uint8Array) => Promise<string>;
  importCoverUrl: (url: string) => Promise<string>;
  findOrCreateSeries: (name: string) => Promise<string>;
  createLocation: (name: string, parentId: string | null) => Promise<string>;
  renameLocation: (id: string, name: string) => Promise<null>;
  moveLocation: (id: string, newParentId: string | null) => Promise<null>;
  deleteLocation: (id: string) => Promise<null>;
  renameTag: (oldName: string, newName: string) => Promise<string[]>;
  setTagColor: (name: string, color: string | null) => Promise<string>;
}

/** Tauri の外のモックでは、Rust が要る操作を断る（画面が通知で理由を出せるよう、IpcError で拒否する）。 */
function mockUnavailable<A extends unknown[], R>(): (...args: A) => Promise<R> {
  return () =>
    Promise.reject(new IpcError({ kind: "internal", message: ja.toast.mockUnavailable }));
}

const ipcApi: LibraryApi = {
  listBooks: ipc.listBooks,
  getRows: ipc.getRows,
  listLocations: ipc.listLocations,
  listTags: ipc.listTags,
  listSeries: ipc.listSeries,
  cCodeTable: async () => toCCodeLookup(await ipc.cCodeTable()),
  searchBooks: ipc.searchBooks,
  trashBooks: ipc.trashBooks,
  restoreBooks: ipc.restoreBooks,
  purgeBooks: ipc.purgeBooks,
  setBooksField: async (ids, field, value) => {
    const reports = await ipc.updateBooks(ids, [{ field, value, origin: { kind: "manual" } }]);
    return reports.filter((report) => report.written.length > 0).map((report) => report.id);
  },
  getBookDetail: ipc.getBookDetail,
  updateBook: ipc.updateBook,
  modifyTags: ipc.modifyTags,
  createBooksFromDrafts: ipc.createBooksFromDrafts,
  findByIsbn: ipc.findByIsbn,
  purchaseStores: ipc.purchaseStoreSuggestions,
  getPriceSettings: ipc.getPriceSettings,
  fetchMetadata: ipc.fetchMetadata,
  refetchBooks: ipc.refetchBooks,
  importCoverFile: ipc.importCoverFile,
  importCoverBytes: (bytes) => ipc.importCoverBytes(bytes),
  importCoverUrl: ipc.importCoverUrl,
  findOrCreateSeries: ipc.findOrCreateSeries,
  createLocation: ipc.createLocation,
  renameLocation: ipc.renameLocation,
  moveLocation: ipc.moveLocation,
  deleteLocation: ipc.deleteLocation,
  renameTag: ipc.renameTag,
  setTagColor: ipc.setTagColor,
};

const mockApi: LibraryApi = {
  listBooks: mockBackend.listBooks,
  getRows: mockBackend.getRows,
  listLocations: mockBackend.listLocations,
  listTags: mockBackend.listTags,
  listSeries: mockBackend.listSeries,
  cCodeTable: mockBackend.cCodeTable,
  searchBooks: mockBackend.searchBooks,
  trashBooks: mockBackend.trashBooks,
  restoreBooks: mockBackend.restoreBooks,
  purgeBooks: mockBackend.purgeBooks,
  setBooksField: mockBackend.setBooksField,
  getBookDetail: mockBackend.getBookDetail,
  updateBook: mockBackend.updateBook,
  modifyTags: mockBackend.modifyTags,
  createBooksFromDrafts: mockBackend.createBooksFromDrafts,
  findByIsbn: mockBackend.findByIsbn,
  purchaseStores: mockBackend.purchaseStores,
  getPriceSettings: () => Promise.resolve(DEFAULT_PRICE_SETTINGS),
  fetchMetadata: mockUnavailable(),
  refetchBooks: mockUnavailable(),
  importCoverFile: mockUnavailable(),
  importCoverBytes: mockUnavailable(),
  importCoverUrl: mockUnavailable(),
  findOrCreateSeries: mockUnavailable(),
  createLocation: mockUnavailable(),
  renameLocation: mockUnavailable(),
  moveLocation: mockUnavailable(),
  deleteLocation: mockUnavailable(),
  renameTag: mockUnavailable(),
  setTagColor: mockUnavailable(),
};

const backend = (): LibraryApi => (inTauri() ? ipcApi : mockApi);

export const libraryApi: LibraryApi = {
  listBooks: () => backend().listBooks(),
  getRows: (ids) => backend().getRows(ids),
  listLocations: () => backend().listLocations(),
  listTags: () => backend().listTags(),
  listSeries: () => backend().listSeries(),
  cCodeTable: () => backend().cCodeTable(),
  searchBooks: (query) => backend().searchBooks(query),
  trashBooks: (ids) => backend().trashBooks(ids),
  restoreBooks: (ids) => backend().restoreBooks(ids),
  purgeBooks: (ids) => backend().purgeBooks(ids),
  setBooksField: (ids, field, value) => backend().setBooksField(ids, field, value),
  getBookDetail: (id) => backend().getBookDetail(id),
  updateBook: (id, changes, options) => backend().updateBook(id, changes, options),
  modifyTags: (ids, add, remove) => backend().modifyTags(ids, add, remove),
  createBooksFromDrafts: (drafts) => backend().createBooksFromDrafts(drafts),
  findByIsbn: (isbn) => backend().findByIsbn(isbn),
  purchaseStores: () => backend().purchaseStores(),
  getPriceSettings: () => backend().getPriceSettings(),
  fetchMetadata: (isbn13, secondTier) => backend().fetchMetadata(isbn13, secondTier),
  refetchBooks: (ids, overrideManual) => backend().refetchBooks(ids, overrideManual),
  importCoverFile: (path) => backend().importCoverFile(path),
  importCoverBytes: (bytes) => backend().importCoverBytes(bytes),
  importCoverUrl: (url) => backend().importCoverUrl(url),
  findOrCreateSeries: (name) => backend().findOrCreateSeries(name),
  createLocation: (name, parentId) => backend().createLocation(name, parentId),
  renameLocation: (id, name) => backend().renameLocation(id, name),
  moveLocation: (id, newParentId) => backend().moveLocation(id, newParentId),
  deleteLocation: (id) => backend().deleteLocation(id),
  renameTag: (oldName, newName) => backend().renameTag(oldName, newName),
  setTagColor: (name, color) => backend().setTagColor(name, color),
};
