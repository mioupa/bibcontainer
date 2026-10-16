// 手入力欄を、スキャン登録画面のフォーカスの定位置にするための関数と、それを使う場面。
//
// 手入力欄は、バーコードリーダーの入力も、手で打つ ISBN も受ける（0012 §3）。画面を開いたとき、
// フォームやダイアログを閉じたとき、共通値の入力や選択を終えたときに、ここへフォーカスを戻す。
// 戻す先は DOM から探す（`ManualInput` の `<input>` の `data-manual-field`。共通値の欄など、離れた部品からも、props を渡さずに呼べるように）。
import { useEffect, useRef } from "react";
import { isEditableTarget } from "@/features/scan/dom";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { useAnyModalOpen } from "@/lib/useModalGuard";
import { useEditorStore } from "@/stores/editorStore";

const DIALOGS = '[role="dialog"], [role="alertdialog"]';

/** 手入力欄以外の入力欄に、人が打っている最中か（閉じかけのダイアログの中は数えない）。 */
function typingElsewhere(field: HTMLElement): boolean {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || active === field) return false;
  return isEditableTarget(active) && active.closest(DIALOGS) === null;
}

/**
 * 手入力欄にフォーカスを移す。画面になければ何もしない（false）。
 * `unlessTyping` なら、ほかの入力欄に人が打っている最中は奪わない。
 */
export function focusManualInput(options: { unlessTyping?: boolean } = {}): boolean {
  const field = document.querySelector<HTMLInputElement>("input[data-manual-field]");
  if (field === null) return false;
  if (options.unlessTyping === true && typingElsewhere(field)) return false;
  field.focus();
  return true;
}

/**
 * 画面を開いたとき、と、フォームや確認のダイアログがすべて閉じたときに、手入力欄にフォーカスを戻す。
 * 開いている間は奪わない。閉じたとき、ほかの入力欄に人が打っている最中でも奪わない。
 *
 * 「閉じた」は、開いているものの数が 1 つ以上から 0 に戻ったことで知る:
 * フォーム（`useEditorStore` の request）、欲しい本の確認（セッションの prompt）、
 * `useModalGuard` で数えているダイアログ（一覧を空にする確認、本物のフォーム）。
 * Radix のダイアログは、開き口（Trigger）がなければ閉じたあとフォーカスをどこにも戻さないので、ここで受ける。
 */
export function useManualFocusHome(): void {
  const editorOpen = useEditorStore((s) => s.request !== null);
  const promptOpen = useScanSessionStore((s) => s.prompt !== null);
  const modalOpen = useAnyModalOpen();
  const blocked = editorOpen || promptOpen || modalOpen;

  useEffect(() => {
    focusManualInput();
  }, []);

  const wasBlocked = useRef(blocked);
  useEffect(() => {
    if (wasBlocked.current && !blocked) focusManualInput({ unlessTyping: true });
    wasBlocked.current = blocked;
  }, [blocked]);
}
