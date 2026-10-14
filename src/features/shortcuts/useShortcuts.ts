// 画面全体のキーボードショートカット（9.5）を受け、対応するコールバックを呼ぶフック。
// 一覧にだけ効くもの（編集の Enter、ゴミ箱）は、一覧の onKeyDown が matchShortcut で判定する。
import { useEffect, useRef } from "react";
import { isEditableTarget } from "@/features/scan/dom";
import {
  matchShortcut,
  shortcutScope,
  yieldsToEditable,
  type ShortcutId,
} from "@/features/shortcuts/shortcuts";
import { detectPlatform, type Platform } from "@/lib/platform";

export type ShortcutHandlers = Partial<Record<ShortcutId, () => void>>;

export function useShortcuts(
  handlers: ShortcutHandlers,
  platform: Platform = detectPlatform(),
): void {
  const handlersRef = useRef(handlers);
  useEffect(() => {
    handlersRef.current = handlers;
  });

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // 日本語入力の変換中のキーは、ショートカットではない
      if (event.isComposing) return;
      const id = matchShortcut(event, platform);
      if (id === null || shortcutScope(id) !== "global") return;
      if (yieldsToEditable(id) && isEditableTarget(event.target)) return;
      const handler = handlersRef.current[id];
      if (!handler) return;
      event.preventDefault();
      handler();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [platform]);
}
