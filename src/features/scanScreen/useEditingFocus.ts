// 文字を入力する欄（手入力欄を除く）にフォーカスがあるかを知らせるフック。
// そのあいだ、バーコードリーダーの数字は欄に入ってしまい、読み取りとして拾われない（0008 §8）。
// 画面は、その旨を出して、欄の外をクリックするよう促す。
import { useEffect, useState } from "react";
import { isEditableTarget } from "@/features/scan/dom";

function editingElsewhere(): boolean {
  const active = document.activeElement;
  return (
    isEditableTarget(active) &&
    !(active instanceof Element && active.closest("[data-manual-input]"))
  );
}

export function useEditingFocus(): boolean {
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    const update = () => setEditing(editingElsewhere());
    // focusout の時点では、まだ activeElement が移っていない。次の focusin を待つため、1 フレーム遅らせる
    const afterFocusMoves = () => setTimeout(update, 0);
    document.addEventListener("focusin", update);
    document.addEventListener("focusout", afterFocusMoves);
    update();
    return () => {
      document.removeEventListener("focusin", update);
      document.removeEventListener("focusout", afterFocusMoves);
    };
  }, []);
  return editing;
}
