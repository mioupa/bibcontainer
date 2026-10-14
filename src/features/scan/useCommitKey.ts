// 確定キー（スペースキー）を、画面全体のキー入力から拾うフック（FR-SCAN-05、9.3）。
// ISBN 保留中の本を、2段目なしで確定するのに使う。テキスト入力欄とダイアログ・メニューの中では拾わない。
//
// ボタンにフォーカスがあるときも拾い、そのボタンが押されたことにならないよう、keydown と keyup の既定の動作を止める
// （スペースキーによるボタンの操作は、keyup で起きる）。
import { useEffect, useRef } from "react";
import { isEditableTarget } from "@/features/scan/dom";

const OVERLAY = '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]';

/** このキーイベントを、確定キーとして扱うか。 */
export function isCommitKey(event: {
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  target: EventTarget | null;
}): boolean {
  if (event.code !== "Space" || event.ctrlKey || event.metaKey || event.altKey) return false;
  if (isEditableTarget(event.target)) return false;
  return !(event.target instanceof Element && event.target.closest(OVERLAY));
}

export interface UseCommitKeyOptions {
  /** false のあいだは拾わない（フォームやダイアログを開いているとき）。 */
  enabled?: boolean;
  onCommit: () => void;
}

export function useCommitKey({ enabled = true, onCommit }: UseCommitKeyOptions): void {
  const onCommitRef = useRef(onCommit);
  useEffect(() => {
    onCommitRef.current = onCommit;
  });

  useEffect(() => {
    if (!enabled) return;
    // keydown を拾ったスペースキーの keyup だけを止める
    let swallowing = false;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isCommitKey(event)) return;
      swallowing = true;
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) onCommitRef.current();
    };
    const handleKeyUp = (event: KeyboardEvent) => {
      if (event.code !== "Space" || !swallowing) return;
      swallowing = false;
      event.preventDefault();
      event.stopPropagation();
    };

    window.addEventListener("keydown", handleKeyDown, { capture: true });
    window.addEventListener("keyup", handleKeyUp, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
      window.removeEventListener("keyup", handleKeyUp, { capture: true });
    };
  }, [enabled]);
}
