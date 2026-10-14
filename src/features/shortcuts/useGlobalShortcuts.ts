// どの画面でも効くショートカット（9.5）: ⌘N 手動登録、⇧⌘N スキャン登録、⌘, 設定。
// App（AppShell）が 1 度だけ呼ぶ。メイン画面だけでなく、スキャン登録画面・設定画面でも効く。
// 一覧向けのもの（Enter、⌘⌫）は BookTable、メイン画面の ⌘F（検索欄）・⌘I（詳細ペイン）は MainScreen が受ける。
//
// フォームやダイアログを開いている間は止める（`useModalGuard` が数える。開いたままフォームが差し替わったり、
// 確認のダイアログの裏で画面が替わったりしないように）。
import { useShortcuts, type ShortcutHandlers } from "@/features/shortcuts/useShortcuts";
import { useAnyModalOpen } from "@/lib/useModalGuard";
import { useEditorStore } from "@/stores/editorStore";
import { useScreenStore } from "@/stores/screenStore";

export function useGlobalShortcuts(): void {
  const openNew = useEditorStore((s) => s.openNew);
  const showScan = useScreenStore((s) => s.showScan);
  const showSettings = useScreenStore((s) => s.showSettings);
  const modalOpen = useAnyModalOpen();

  const handlers: ShortcutHandlers = modalOpen
    ? {}
    : {
        newBook: () => openNew(),
        scanRegister: () => showScan(),
        openSettings: () => showSettings(),
      };
  useShortcuts(handlers);
}
