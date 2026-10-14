// 開いているダイアログの数を数える。ダイアログを開いている間は、画面全体のショートカット（⌘N など）と
// メイン画面のスキャン検出を止めるため（9.5）。ダイアログを持つ部品が `useModalGuard(open)` を呼ぶ。
import { useEffect } from "react";
import { create } from "zustand";

interface ModalState {
  count: number;
}

export const useModalStore = create<ModalState>()(() => ({ count: 0 }));

/** ダイアログが 1 つでも開いているか。 */
export function useAnyModalOpen(): boolean {
  return useModalStore((s) => s.count > 0);
}

/** `open` が true のあいだ、開いているダイアログとして数える。 */
export function useModalGuard(open: boolean): void {
  useEffect(() => {
    if (!open) return;
    useModalStore.setState((s) => ({ count: s.count + 1 }));
    return () => useModalStore.setState((s) => ({ count: Math.max(0, s.count - 1) }));
  }, [open]);
}
