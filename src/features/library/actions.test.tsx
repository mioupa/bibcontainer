import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type * as IpcModule from "@/lib/ipc";
import { dateStamp, useLibraryActions } from "@/features/library/actions";
import { libraryApi } from "@/features/library/api";
import type { RefetchSummary } from "@/bindings/RefetchSummary";
import { ja } from "@/i18n/ja";
import { chooseSavePath } from "@/lib/dialog";
import { IpcError, ipc } from "@/lib/ipc";
import { useSelectionStore } from "@/stores/selectionStore";

const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
);
const mock = vi.hoisted(() => ({ value: false }));
vi.mock("sonner", () => ({ toast }));
vi.mock("@/features/library/api", () => ({
  libraryApi: {
    trashBooks: vi.fn(),
    restoreBooks: vi.fn(),
    purgeBooks: vi.fn(),
    setBooksField: vi.fn(),
    mergeWorks: vi.fn(),
    modifyTags: vi.fn(),
    refetchBooks: vi.fn(),
    getRows: vi.fn().mockResolvedValue([]),
  },
  usingMockData: () => mock.value,
}));
vi.mock("@/lib/dialog", () => ({ chooseSavePath: vi.fn() }));
vi.mock("@/lib/ipc", async (importOriginal) => {
  const original = await importOriginal<typeof IpcModule>();
  return { ...original, ipc: { exportCsv: vi.fn() } };
});

const api = vi.mocked(libraryApi);

function setup() {
  const client = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(() => useLibraryActions(), { wrapper }).result.current;
}

describe("useLibraryActions", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mock.value = false;
    api.getRows.mockResolvedValue([]);
  });

  it("ゴミ箱へ移すと、冊数を通知し、「元に戻す」で戻せる", async () => {
    api.trashBooks.mockResolvedValue(["a", "b"]);
    api.restoreBooks.mockResolvedValue(["a", "b"]);
    const actions = setup();

    await actions.trash(["a", "b", "c"]);

    expect(api.trashBooks).toHaveBeenCalledWith(["a", "b", "c"]);
    expect(toast.success).toHaveBeenCalledOnce();
    const [message, options] = toast.success.mock.calls[0] as [
      string,
      { action: { label: string; onClick: () => void } },
    ];
    expect(message).toBe(ja.toast.trashed(2));
    expect(options.action.label).toBe(ja.toast.undo);

    options.action.onClick();
    await vi.waitFor(() => expect(api.restoreBooks).toHaveBeenCalledWith(["a", "b"]));
    await vi.waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.toast.restored(2)));
  });

  it("変わった本がなければ、その旨を通知する（元に戻すは出さない）", async () => {
    api.trashBooks.mockResolvedValue([]);
    await setup().trash(["a"]);
    expect(toast.info).toHaveBeenCalledWith(ja.toast.nothingChanged);
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("失敗したら、Rust が用意した理由を通知する。理由がなければ汎用の文言", async () => {
    api.purgeBooks.mockRejectedValueOnce(new IpcError({ kind: "conflict", message: "戻せません" }));
    api.restoreBooks.mockRejectedValueOnce(new Error("boom"));
    const actions = setup();

    await actions.purge(["a"]);
    expect(toast.error).toHaveBeenLastCalledWith("戻せません");
    await actions.restore(["a"]);
    expect(toast.error).toHaveBeenLastCalledWith(ja.toast.failed);
  });

  it("一括変更は、項目と値を渡して、変えた冊数を通知する", async () => {
    api.setBooksField.mockResolvedValue(["a"]);
    await setup().setField(["a", "b"], "ownership", "released");
    expect(api.setBooksField).toHaveBeenCalledWith(["a", "b"], "ownership", "released");
    expect(toast.success).toHaveBeenCalledWith(ja.toast.bulkChanged(1));
  });

  describe("一括変更（FR-BOOK-08）の引数", () => {
    beforeEach(() => {
      api.setBooksField.mockResolvedValue(["a", "b"]);
      api.modifyTags.mockResolvedValue(["a", "b"]);
    });

    it("読書状況・所有状態・評価・保管場所は、1 項目を手入力として書く（評価なしは null）", async () => {
      const actions = setup();
      await actions.bulk({ field: "read_status", value: "read" }, ["a", "b"]);
      await actions.bulk({ field: "ownership", value: "owned" }, ["a", "b"]);
      await actions.bulk({ field: "rating", value: 4 }, ["a", "b"]);
      await actions.bulk({ field: "rating", value: null }, ["a", "b"]);
      await actions.bulk({ field: "location_id", value: "loc-1" }, ["a", "b"]);
      await actions.bulk({ field: "location_id", value: null }, ["a", "b"]);
      expect(api.setBooksField.mock.calls).toEqual([
        [["a", "b"], "read_status", "read"],
        [["a", "b"], "ownership", "owned"],
        [["a", "b"], "rating", 4],
        [["a", "b"], "rating", null],
        [["a", "b"], "location_id", "loc-1"],
        [["a", "b"], "location_id", null],
      ]);
      expect(api.modifyTags).not.toHaveBeenCalled();
    });

    it("タグは、足すものと外すものを modify_tags に渡す", async () => {
      const actions = setup();
      await actions.bulk({ field: "tags", add: ["小説"], remove: [] }, ["a", "b"]);
      await actions.bulk({ field: "tags", add: [], remove: ["古い"] }, ["a", "b"]);
      expect(api.modifyTags.mock.calls).toEqual([
        [["a", "b"], ["小説"], []],
        [["a", "b"], [], ["古い"]],
      ]);
      expect(toast.success).toHaveBeenCalledWith(ja.toast.tagsChanged(2));
      expect(api.setBooksField).not.toHaveBeenCalled();
    });

    it("変わった本がなければ、その旨を通知する。失敗したら理由を通知する", async () => {
      api.modifyTags.mockResolvedValueOnce([]);
      api.modifyTags.mockRejectedValueOnce(new IpcError({ kind: "validation", message: "不正" }));
      const actions = setup();
      await actions.bulk({ field: "tags", add: ["x"], remove: [] }, ["a"]);
      expect(toast.info).toHaveBeenCalledWith(ja.toast.nothingChanged);
      await actions.bulk({ field: "tags", add: ["x"], remove: [] }, ["a"]);
      expect(toast.error).toHaveBeenCalledWith("不正");
    });
  });

  it("欲しい本を購入済みにすると、所有に変える（購入日は、コアが空なら今日を入れる。FR-WISH-01）", async () => {
    api.setBooksField.mockResolvedValue(["w"]);
    await setup().markPurchased(["w"]);
    expect(api.setBooksField).toHaveBeenCalledWith(["w"], "ownership", "owned");
    expect(toast.success).toHaveBeenCalledWith(ja.toast.purchased(1));
  });

  // 作品の手動のまとめ・外し・自動に戻す（0026）
  describe("作品のまとめ・外し", () => {
    beforeEach(() => {
      useSelectionStore.getState().clear();
    });

    it("まとめると、merge_works に対象の本を渡し、対象の冊数を通知する。できた作品の行を選ぶ予約をする", async () => {
      api.mergeWorks.mockResolvedValue({ workId: "w-1", changed: ["b"] });
      await setup().mergeWorks(["a", "b", "c"]);
      expect(api.mergeWorks).toHaveBeenCalledWith(["a", "b", "c"]);
      expect(toast.success).toHaveBeenCalledWith(ja.toast.worksMerged(3));
      const pending = useSelectionStore.getState().pendingWork;
      expect(pending).toMatchObject({
        workId: "w-1",
        bookIds: ["a", "b", "c"],
        groupKey: "work:\u001fw-1",
      });
      expect(pending?.expiresAt).toBeGreaterThan(Date.now());
    });

    it("まとめても書き換える本がなければ、その旨を通知する（作品の行は選ぶ）", async () => {
      api.mergeWorks.mockResolvedValue({ workId: "w-1", changed: [] });
      await setup().mergeWorks(["a", "b"]);
      expect(toast.info).toHaveBeenCalledWith(ja.toast.nothingChanged);
      expect(toast.success).not.toHaveBeenCalled();
      expect(useSelectionStore.getState().pendingWork).not.toBeNull();
    });

    it("まとめるのに失敗したら、Rust が用意した理由を通知する。選ぶ予約はしない", async () => {
      api.mergeWorks.mockRejectedValue(
        new IpcError({ kind: "validation", message: "2 冊以上を選んでください" }),
      );
      await setup().mergeWorks(["a"]);
      expect(toast.error).toHaveBeenCalledWith("2 冊以上を選んでください");
      expect(useSelectionStore.getState().pendingWork).toBeNull();
    });

    it("外すと、work_id に none を書き、変わった冊数を通知する。対象の本を選ぶ予約をする", async () => {
      api.setBooksField.mockResolvedValue(["a"]);
      await setup().setWork(["a", "b"], "none");
      expect(api.setBooksField).toHaveBeenCalledWith(["a", "b"], "work_id", "none");
      expect(toast.success).toHaveBeenCalledWith(ja.toast.worksExcluded(1));
      expect(useSelectionStore.getState().pendingWork).toMatchObject({
        workId: "none",
        bookIds: ["a", "b"],
        groupKey: null,
      });
    });

    it("自動に戻すと、work_id に null を書き、変わった冊数を通知する", async () => {
      api.setBooksField.mockResolvedValue(["a", "b"]);
      await setup().setWork(["a", "b"], null);
      expect(api.setBooksField).toHaveBeenCalledWith(["a", "b"], "work_id", null);
      expect(toast.success).toHaveBeenCalledWith(ja.toast.worksReset(2));
      expect(useSelectionStore.getState().pendingWork).toMatchObject({
        workId: null,
        bookIds: ["a", "b"],
      });
    });

    it("変わった本がなければ、その旨を通知する（選択には触らない）。失敗したら理由を通知する", async () => {
      api.setBooksField.mockResolvedValueOnce([]);
      api.setBooksField.mockRejectedValueOnce(new IpcError({ kind: "io", message: "書けません" }));
      const actions = setup();
      await actions.setWork(["a"], "none");
      expect(toast.info).toHaveBeenCalledWith(ja.toast.nothingChanged);
      expect(useSelectionStore.getState().pendingWork).toBeNull();
      await actions.setWork(["a"], null);
      expect(toast.error).toHaveBeenCalledWith("書けません");
    });

    it("モック（Tauri の外）では、変えた本の行を自分で取り直す。まとめるときは、書き換えた本（対象とは限らない）を取り直す", async () => {
      mock.value = true;
      api.mergeWorks.mockResolvedValue({ workId: "w-1", changed: ["a", "z"] });
      api.setBooksField.mockResolvedValue(["b"]);
      const actions = setup();
      await actions.mergeWorks(["a", "b"]);
      expect(api.getRows).toHaveBeenLastCalledWith(["a", "z"]);
      await actions.setWork(["b"], "none");
      expect(api.getRows).toHaveBeenLastCalledWith(["b"]);
    });
  });

  describe("書誌の再取得（FR-META-13）", () => {
    const summary = (patch: Partial<RefetchSummary> = {}): RefetchSummary => ({
      requested: 2,
      updated: 1,
      unchanged: 1,
      offline: 0,
      skipped: 0,
      failed: 0,
      coversSaved: 0,
      noActiveProviders: false,
      items: [],
      ...patch,
    });

    it("手で直した項目を上書きするかを、そのまま渡す（AT-16。既定の呼び出しは false）", async () => {
      api.refetchBooks.mockResolvedValue(summary());
      const actions = setup();
      await actions.refetch(["a", "b"], false);
      await actions.refetch(["a"], true);
      expect(api.refetchBooks.mock.calls).toEqual([
        [["a", "b"], false],
        [["a"], true],
      ]);
    });

    it("結果を通知する。手で直した項目を残した件数も出す", async () => {
      api.refetchBooks.mockResolvedValue(
        summary({
          items: [
            {
              id: "a",
              outcome: "updated",
              metaStatus: "ok",
              written: ["publisher"],
              skippedManual: ["title"],
              message: null,
            },
          ],
        }),
      );
      await setup().refetch(["a", "b"], false);
      expect(toast.success).toHaveBeenCalledWith(
        ja.toast.refetch.result({
          updated: 1,
          unchanged: 1,
          offline: 0,
          skipped: 0,
          failed: 0,
          keptManualFields: 1,
        }),
      );
    });

    it("使える取得元がないとき・通信できないときは、その理由を出す", async () => {
      api.refetchBooks.mockResolvedValueOnce(
        summary({ noActiveProviders: true, updated: 0, unchanged: 0, skipped: 2 }),
      );
      api.refetchBooks.mockResolvedValueOnce(summary({ updated: 0, unchanged: 0, offline: 2 }));
      const actions = setup();
      await actions.refetch(["a", "b"], false);
      expect(toast.error).toHaveBeenLastCalledWith(ja.toast.refetch.noProviders);
      await actions.refetch(["a", "b"], false);
      expect(toast.info).toHaveBeenLastCalledWith(ja.toast.refetch.offline);
    });

    it("失敗したら、Rust が用意した理由を通知する", async () => {
      api.refetchBooks.mockRejectedValue(
        new IpcError({ kind: "network", message: "つながりません" }),
      );
      await setup().refetch(["a"], false);
      expect(toast.error).toHaveBeenCalledWith("つながりません");
    });
  });

  it("モック（Tauri の外）では、イベントが来ないので、変えた本の行を自分で取り直す", async () => {
    mock.value = true;
    api.trashBooks.mockResolvedValue(["a"]);
    await setup().trash(["a"]);
    expect(api.getRows).toHaveBeenCalledWith(["a"]);
  });

  it("Tauri の中では、取り直しは library-changed に任せる", async () => {
    api.trashBooks.mockResolvedValue(["a"]);
    await setup().trash(["a"]);
    expect(api.getRows).not.toHaveBeenCalled();
  });

  describe("CSV の書き出し", () => {
    it("保存先を選び、選んだ本の ID を渡して、冊数を通知する", async () => {
      vi.mocked(chooseSavePath).mockResolvedValue("/tmp/蔵書.csv");
      vi.mocked(ipc.exportCsv).mockResolvedValue(2);

      await setup().exportCsv(["a", "b"]);

      expect(chooseSavePath).toHaveBeenCalledWith(
        expect.objectContaining({
          filters: [{ name: ja.dialog.export.csvFilter, extensions: ["csv"] }],
        }),
      );
      expect(vi.mocked(chooseSavePath).mock.calls[0]?.[0]?.defaultPath).toMatch(
        /^bibcontainer-\d{8}\.csv$/,
      );
      expect(ipc.exportCsv).toHaveBeenCalledWith("/tmp/蔵書.csv", ["a", "b"]);
      expect(toast.success).toHaveBeenCalledWith(ja.toast.exported(2));
    });

    it("保存先の選択をやめたら、何もしない", async () => {
      vi.mocked(chooseSavePath).mockResolvedValue(null);
      vi.mocked(ipc.exportCsv).mockReset();
      await setup().exportCsv(["a"]);
      expect(ipc.exportCsv).not.toHaveBeenCalled();
      expect(toast.success).not.toHaveBeenCalled();
    });

    it("モック（Tauri の外）では、書き出せない旨を通知する", async () => {
      mock.value = true;
      await setup().exportCsv(["a"]);
      expect(toast.info).toHaveBeenCalledWith(ja.toast.exportUnavailable);
      expect(chooseSavePath).not.toHaveBeenCalled();
    });
  });
});

describe("dateStamp", () => {
  it("ローカルの日付を YYYYMMDD にする", () => {
    expect(dateStamp(new Date(2026, 9, 4, 23, 59))).toBe("20261004");
    expect(dateStamp(new Date(2026, 0, 31))).toBe("20260131");
  });
});
