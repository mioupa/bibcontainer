// 一覧から起こす、蔵書を変える操作（ゴミ箱へ移動・戻す・完全削除・一括変更・書き出し）。
// 結果は通知（トースト）で知らせ、失敗したときは理由を出す。
//
// Tauri の中では、Rust が変更のあとに出す `library-changed`（sync.ts）で一覧が更新される。
// Tauri の外（モック）はイベントが来ないので、ここで同じ更新を行う。
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { toast } from "sonner";
import { applyLibraryChange } from "@/features/library/sync";
import { libraryApi, usingMockData } from "@/features/library/api";
import { manualGroupKey } from "@/features/library/grouping";
import { describeRefetch } from "@/features/library/refetch";
import type { WORK_NONE } from "@/features/library/works";
import type { BulkChange } from "@/features/list/bulk";
import { ja } from "@/i18n/ja";
import { chooseSavePath } from "@/lib/dialog";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { useSelectionStore } from "@/stores/selectionStore";

/** 作品の操作のあとの選択を、一覧の行の更新が届くまで待つ時間の上限（過ぎたら、選ばずに捨てる）。 */
const PENDING_WORK_MS = 10_000;

/** `YYYYMMDD`（ローカルの日付）。書き出しの既定のファイル名に使う。 */
export function dateStamp(date: Date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}`;
}

export interface LibraryActions {
  /** ゴミ箱へ移す。元に戻すボタン付きの通知を出す。 */
  trash: (ids: string[]) => Promise<void>;
  /** ゴミ箱から戻す。 */
  restore: (ids: string[]) => Promise<void>;
  /** 完全に削除する（取り消せない。確認は呼び出し側）。 */
  purge: (ids: string[]) => Promise<void>;
  /** 1 つの項目に同じ値を、複数の本へ手入力として書く（一括変更）。 */
  setField: (ids: string[], field: string, value: unknown) => Promise<void>;
  /** 選んだ本を CSV に書き出す（保存先をダイアログで選ぶ）。 */
  exportCsv: (ids: string[]) => Promise<void>;
  /** 一括変更（読書状況・所有状態・評価・保管場所は 1 項目を書き、タグは足す・外す）。 */
  bulk: (change: BulkChange, ids: string[]) => Promise<void>;
  /** 欲しい本を購入済み（所有）にする。購入日が空なら、コアが今日の日付を入れる（FR-WISH-01）。 */
  markPurchased: (ids: string[]) => Promise<void>;
  /**
   * 選んだ本を 1 つの作品にまとめる（0026）。そのあと、一覧の行が更新されたら、できた作品の行を選ぶ
   * （絞り込みで作品が 1 冊になるなら、対象の本を選ぶ）。
   */
  mergeWorks: (ids: string[]) => Promise<void>;
  /** 作品から外す（`"none"`）か、自動に戻す（null）。そのあと、対象の本を選ぶ。 */
  setWork: (ids: string[], value: typeof WORK_NONE | null) => Promise<void>;
  /** 書誌を再取得する。`overrideManual` が false なら、手で直した項目は上書きしない（AT-16）。 */
  refetch: (ids: string[], overrideManual: boolean) => Promise<void>;
}

export function useLibraryActions(): LibraryActions {
  const client = useQueryClient();

  // Tauri の外では、Rust のイベントが来ないので、変えた本の行を自分で取り直す
  const refreshIfMock = useCallback(
    async (changedIds: string[]) => {
      if (usingMockData() && changedIds.length > 0) {
        await applyLibraryChange(client, { books: changedIds, others: true });
      }
    },
    [client],
  );

  const restore = useCallback(
    async (ids: string[]) => {
      try {
        const changed = await libraryApi.restoreBooks(ids);
        await refreshIfMock(changed);
        if (changed.length === 0) toast.info(ja.toast.nothingChanged);
        else toast.success(ja.toast.restored(changed.length));
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock],
  );

  const trash = useCallback(
    async (ids: string[]) => {
      try {
        const changed = await libraryApi.trashBooks(ids);
        await refreshIfMock(changed);
        if (changed.length === 0) {
          toast.info(ja.toast.nothingChanged);
          return;
        }
        toast.success(ja.toast.trashed(changed.length), {
          action: { label: ja.toast.undo, onClick: () => void restore(changed) },
        });
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock, restore],
  );

  const purge = useCallback(
    async (ids: string[]) => {
      try {
        const changed = await libraryApi.purgeBooks(ids);
        await refreshIfMock(changed);
        if (changed.length === 0) toast.info(ja.toast.nothingChanged);
        else toast.success(ja.toast.purged(changed.length));
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock],
  );

  const setField = useCallback(
    async (ids: string[], field: string, value: unknown) => {
      try {
        const changed = await libraryApi.setBooksField(ids, field, value);
        await refreshIfMock(changed);
        if (changed.length === 0) toast.info(ja.toast.nothingChanged);
        else toast.success(ja.toast.bulkChanged(changed.length));
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock],
  );

  const exportCsv = useCallback(async (ids: string[]) => {
    if (usingMockData()) {
      toast.info(ja.toast.exportUnavailable);
      return;
    }
    try {
      const path = await chooseSavePath({
        title: ja.dialog.export.title,
        defaultPath: `${ja.dialog.export.defaultName}-${dateStamp()}.csv`,
        filters: [{ name: ja.dialog.export.csvFilter, extensions: ["csv"] }],
      });
      if (path === null) return;
      const count = await ipc.exportCsv(path, ids);
      toast.success(ja.toast.exported(count));
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }, []);

  const bulk = useCallback(
    async (change: BulkChange, ids: string[]) => {
      if (change.field !== "tags") {
        await setField(ids, change.field, change.value);
        return;
      }
      try {
        const changed = await libraryApi.modifyTags(ids, change.add, change.remove);
        await refreshIfMock(changed);
        if (changed.length === 0) toast.info(ja.toast.nothingChanged);
        else toast.success(ja.toast.tagsChanged(changed.length));
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock, setField],
  );

  const markPurchased = useCallback(
    async (ids: string[]) => {
      try {
        const changed = await libraryApi.setBooksField(ids, "ownership", "owned");
        await refreshIfMock(changed);
        if (changed.length === 0) toast.info(ja.toast.nothingChanged);
        else toast.success(ja.toast.purchased(changed.length));
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock],
  );

  const mergeWorks = useCallback(
    async (ids: string[]) => {
      try {
        const { workId, changed } = await libraryApi.mergeWorks(ids);
        await refreshIfMock(changed);
        if (changed.length === 0) toast.info(ja.toast.nothingChanged);
        else toast.success(ja.toast.worksMerged(ids.length));
        useSelectionStore.getState().setPendingWork({
          workId,
          bookIds: ids,
          groupKey: manualGroupKey(workId),
          expiresAt: Date.now() + PENDING_WORK_MS,
        });
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock],
  );

  const setWork = useCallback(
    async (ids: string[], value: typeof WORK_NONE | null) => {
      try {
        const changed = await libraryApi.setBooksField(ids, "work_id", value);
        await refreshIfMock(changed);
        if (changed.length === 0) {
          toast.info(ja.toast.nothingChanged);
          return;
        }
        toast.success(
          value === null
            ? ja.toast.worksReset(changed.length)
            : ja.toast.worksExcluded(changed.length),
        );
        useSelectionStore.getState().setPendingWork({
          workId: value,
          bookIds: ids,
          groupKey: null,
          expiresAt: Date.now() + PENDING_WORK_MS,
        });
      } catch (error) {
        toast.error(errorMessage(error));
      }
    },
    [refreshIfMock],
  );

  const refetch = useCallback(async (ids: string[], overrideManual: boolean) => {
    try {
      const summary = await libraryApi.refetchBooks(ids, overrideManual);
      const notice = describeRefetch(summary);
      toast[notice.level](notice.message);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }, []);

  return useMemo(
    () => ({
      trash,
      restore,
      purge,
      setField,
      exportCsv,
      bulk,
      markPurchased,
      mergeWorks,
      setWork,
      refetch,
    }),
    [trash, restore, purge, setField, exportCsv, bulk, markPurchased, mergeWorks, setWork, refetch],
  );
}
