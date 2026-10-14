// `FlowDeps`（flow.ts）の本物の実装。IPC・ストア・通知音・フォームを呼ぶ。
import { toast } from "sonner";
import type { FlowDeps } from "@/features/scanScreen/flow";
import { awaitLookup, startLookup } from "@/features/scanScreen/lookup";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { playSound } from "@/features/scan/playSound";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { useEditorStore } from "@/stores/editorStore";

/** 登録・編集フォームが閉じている状態になるまで待つ（すでに閉じていれば、すぐ解決する）。 */
export function whenEditorClosed(): Promise<void> {
  if (useEditorStore.getState().request === null) return Promise.resolve();
  return new Promise((resolve) => {
    const stop = useEditorStore.subscribe((state) => {
      if (state.request === null) {
        stop();
        resolve();
      }
    });
  });
}

/** スキャン登録画面が表示されている数（画面を離れたあとは、フォームや確認を新しく出さないために使う）。 */
const screenActivity = { count: 0 };

/** 画面が表示されている間、呼んでおく。返した関数で、表示が終わったことを知らせる。 */
export function trackScreenActive(): () => void {
  screenActivity.count += 1;
  return () => {
    screenActivity.count -= 1;
  };
}

export function createLiveDeps(isActive: () => boolean = () => screenActivity.count > 0): FlowDeps {
  const store = () => useScanSessionStore.getState();
  return {
    findByIsbn: (isbn13) => ipc.findByIsbn(isbn13),
    startLookup: (isbn13, options) => {
      void startLookup(isbn13, options);
    },
    awaitLookup,
    openNewForm: async (source) => {
      // ほかのフォームを開いているあいだは、それが閉じるのを待つ（開いているフォームを置き換えない）
      await whenEditorClosed();
      useEditorStore.getState().openNew(source);
      await whenEditorClosed();
    },
    confirmMoveToOwned: (match) =>
      new Promise<boolean>((resolve) => {
        store().setPrompt({
          match,
          resolve: (move) => {
            store().setPrompt(null);
            resolve(move);
          },
        });
      }),
    moveToOwned: async (bookId) => {
      // 所有状態を owned にする。欲しい本から所有に移すと、購入日はコアが今日の日付を入れる（FR-SCAN-18）
      await ipc.updateBook(bookId, [
        { field: "ownership", value: "owned", origin: { kind: "manual" } },
      ]);
    },
    addRow: (book, duplicate) => {
      store().addRow(book, duplicate);
    },
    hasRow: (isbn13) => store().rows.some((row) => row.isbn13 === isbn13),
    playSound,
    setResult: (result) => store().setResult(result),
    setLast: (last) => store().setLast(last),
    reportError: (error) => {
      toast.error(errorMessage(error));
    },
    isActive,
  };
}
