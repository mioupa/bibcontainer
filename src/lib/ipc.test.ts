import { beforeEach, describe, expect, it, vi } from "vitest";
import { IpcError, ipc, onLibraryChanged, onSyncStatus } from "@/lib/ipc";

const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

describe("ipc", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("appInfo は app_info コマンドを引数なしで呼ぶ", async () => {
    const info = {
      name: "bibcontainer",
      version: "0.1.0",
      deviceId: "d-0123456789ab",
      schemaVersion: 1,
      dbPath: "/data/library.sqlite3",
      hlcLast: null,
      pendingOps: 0,
    };
    invoke.mockResolvedValue(info);

    await expect(ipc.appInfo()).resolves.toEqual(info);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("app_info", undefined);
  });

  it("Rust の AppError（{ kind, message }）は IpcError として投げ直す", async () => {
    invoke.mockRejectedValue({ kind: "schema_too_new", message: "新しすぎる" });

    const error = await ipc.appInfo().catch((e: unknown) => e);

    expect(error).toBeInstanceOf(IpcError);
    expect(error).toMatchObject({ kind: "schema_too_new", message: "新しすぎる" });
  });

  it("AppError の形でない失敗は internal の IpcError にする", async () => {
    invoke.mockRejectedValue("command app_info not found");

    const error = await ipc.appInfo().catch((e: unknown) => e);

    expect(error).toBeInstanceOf(IpcError);
    expect(error).toMatchObject({ kind: "internal", message: "command app_info not found" });
  });

  describe("コマンドの呼び方（引数のキーは Rust の引数名を camelCase にしたもの）", () => {
    beforeEach(() => {
      invoke.mockResolvedValue(null);
    });

    it("参照のコマンド", async () => {
      await ipc.listBooks();
      await ipc.getRows(["a", "b"]);
      await ipc.searchBooks("夏目");
      await ipc.findByIsbn("978-4-00-310111-7");
      expect(invoke.mock.calls).toEqual([
        ["list_books", undefined],
        ["get_rows", { ids: ["a", "b"] }],
        ["search_books", { query: "夏目" }],
        ["find_by_isbn", { isbn: "978-4-00-310111-7" }],
      ]);
    });

    it("書籍の変更は、由来つきの FieldChange をそのまま渡す", async () => {
      const changes = [
        { field: "title", value: "吾輩は猫である", origin: { kind: "manual" as const } },
      ];
      await ipc.updateBook("b1", changes, { overrideManual: true });
      await ipc.updateBooks(["b1", "b2"], changes);
      await ipc.modifyTags(["b1"], ["a"], ["b"]);
      await ipc.trashBooks(["b1"]);
      expect(invoke.mock.calls).toEqual([
        ["update_book", { id: "b1", changes, options: { overrideManual: true } }],
        ["update_books", { ids: ["b1", "b2"], changes, options: undefined }],
        ["modify_tags", { ids: ["b1"], add: ["a"], remove: ["b"] }],
        ["trash_books", { ids: ["b1"] }],
      ]);
    });

    it("保管場所・タグ・シリーズ", async () => {
      await ipc.createLocation("書斎");
      await ipc.moveLocation("l1", null);
      await ipc.reorderLocations("l0", ["l2", "l1"]);
      await ipc.renameTag("研究", "研究2");
      await ipc.setTagColor("研究2", null);
      expect(invoke.mock.calls).toEqual([
        ["create_location", { name: "書斎", parentId: null }],
        ["move_location", { id: "l1", newParentId: null }],
        ["reorder_locations", { parentId: "l0", orderedIds: ["l2", "l1"] }],
        ["rename_tag", { old: "研究", new: "研究2" }],
        ["set_tag_color", { name: "研究2", color: null }],
      ]);
    });

    it("設定・書き出し・バックアップ", async () => {
      await ipc.getScanSettings();
      await ipc.exportCsv("/tmp/a.csv", ["x"]);
      await ipc.exportJson("/tmp/a.json");
      await ipc.parseIsbnList("/tmp/isbn.txt");
      await ipc.restoreBackup("/tmp/b.sqlite3");
      await ipc.openLogsDir();
      expect(invoke.mock.calls).toEqual([
        ["get_scan_settings", undefined],
        ["export_csv", { path: "/tmp/a.csv", ids: ["x"] }],
        ["export_json", { path: "/tmp/a.json" }],
        ["parse_isbn_list", { path: "/tmp/isbn.txt" }],
        ["restore_backup", { path: "/tmp/b.sqlite3" }],
        ["open_logs_dir", undefined],
      ]);
    });

    it("書誌の取得: 2段目は secondTier、再取得は overrideManual で渡す", async () => {
      await ipc.fetchMetadata("9784780802047", { cCode: "0076", listPrice: 1500 });
      await ipc.fetchMetadata("9784780802047");
      await ipc.refetchBooks(["a"], true);
      await ipc.setApiKey("google", "key");
      await ipc.testProvider("openbd");
      await ipc.createBooksFromDrafts([{ changes: [], seriesName: null }]);
      expect(invoke.mock.calls).toEqual([
        [
          "fetch_metadata",
          { isbn13: "9784780802047", secondTier: { cCode: "0076", listPrice: 1500 } },
        ],
        ["fetch_metadata", { isbn13: "9784780802047", secondTier: undefined }],
        ["refetch_books", { ids: ["a"], overrideManual: true }],
        ["set_api_key", { provider: "google", key: "key" }],
        ["test_provider", { id: "openbd" }],
        ["create_books_from_drafts", { drafts: [{ changes: [], seriesName: null }] }],
      ]);
    });

    it("書影のバイト列は、Uint8Array ならそのまま（バイナリ）、数値の配列なら { bytes } で渡す", async () => {
      const bytes = new Uint8Array([0xff, 0xd8, 0xff]);
      await ipc.importCoverBytes(bytes);
      await ipc.importCoverBytes([1, 2, 3]);
      expect(invoke.mock.calls[0]?.[0]).toBe("import_cover_bytes");
      expect(invoke.mock.calls[0]?.[1]).toBe(bytes);
      expect(invoke.mock.calls[1]).toEqual(["import_cover_bytes", { bytes: [1, 2, 3] }]);
    });

    it("同期: 引数のキーは path・mode・name。戻り値は状態（書影の整理だけ件数）", async () => {
      await ipc.getSyncStatus();
      await ipc.inspectSyncFolder("/Drive");
      await ipc.setupSync("/Drive", "joinMerge");
      await ipc.leaveSync();
      await ipc.syncNow();
      await ipc.setDeviceName("書斎の Mac");
      await ipc.gcCovers();
      expect(invoke.mock.calls).toEqual([
        ["get_sync_status", undefined],
        ["inspect_sync_folder", { path: "/Drive" }],
        ["setup_sync", { path: "/Drive", mode: "joinMerge" }],
        ["leave_sync", undefined],
        ["sync_now", undefined],
        ["set_device_name", { name: "書斎の Mac" }],
        ["gc_covers", undefined],
      ]);
    });

    it("新しい種類のエラー（validation・conflict など）も kind のまま届く", async () => {
      invoke.mockRejectedValue({ kind: "conflict", message: "本が残っています" });
      const error = await ipc.deleteLocation("l1").catch((e: unknown) => e);
      expect(error).toBeInstanceOf(IpcError);
      expect(error).toMatchObject({ kind: "conflict", message: "本が残っています" });
    });
  });

  describe("library-changed", () => {
    it("イベントのペイロードをハンドラに渡し、購読を解除する関数を返す", async () => {
      const unlisten = vi.fn();
      let callback: ((event: { payload: unknown }) => void) | undefined;
      listen.mockImplementation((name: string, cb: typeof callback) => {
        expect(name).toBe("library-changed");
        callback = cb;
        return Promise.resolve(unlisten);
      });
      const handler = vi.fn();

      const stop = await onLibraryChanged(handler);
      callback?.({ payload: { books: ["a"], others: true } });

      expect(handler).toHaveBeenCalledExactlyOnceWith({ books: ["a"], others: true });
      stop();
      expect(unlisten).toHaveBeenCalledOnce();
    });
  });

  describe("sync-status", () => {
    it("イベントのペイロード（状態）をハンドラに渡し、購読を解除する関数を返す", async () => {
      const unlisten = vi.fn();
      let callback: ((event: { payload: unknown }) => void) | undefined;
      listen.mockImplementation((name: string, cb: typeof callback) => {
        expect(name).toBe("sync-status");
        callback = cb;
        return Promise.resolve(unlisten);
      });
      const handler = vi.fn();

      const stop = await onSyncStatus(handler);
      callback?.({ payload: { state: "idle", folder: "/Drive/bibcontainer-library" } });

      expect(handler).toHaveBeenCalledExactlyOnceWith({
        state: "idle",
        folder: "/Drive/bibcontainer-library",
      });
      stop();
      expect(unlisten).toHaveBeenCalledOnce();
    });
  });
});
