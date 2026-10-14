// キーボードショートカットの定義（9.5）。macOS は ⌘、Windows は Ctrl。判定は KeyboardEvent.code で行う
// （キーボード配列や日本語入力に左右されない）。純粋関数なので、OS を引数で切り替えてテストできる。
import type { Platform } from "@/lib/platform";

export type ShortcutId =
  | "newBook" // 手動登録
  | "scanRegister" // スキャン登録
  | "focusSearch" // 検索欄へ移動
  | "toggleDetail" // 詳細ペインの表示切替
  | "edit" // 選んだ本を編集
  | "trash" // ゴミ箱へ移動
  | "undo" // 元に戻す [SHOULD]
  | "openSettings"; // 設定

/**
 * global: 画面のどこにフォーカスがあっても効く（useShortcuts が受ける）。
 * list: 一覧にフォーカスがあるときだけ効く（一覧の onKeyDown が matchShortcut で判定する。Enter や Delete は、ほかの場所では別の意味を持つため）。
 */
export type ShortcutScope = "global" | "list";

export interface KeyLike {
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

interface Binding {
  code: string;
  /** macOS の ⌘ / Windows の Ctrl を押す。 */
  mod: boolean;
  shift: boolean;
}

interface Definition {
  scope: ShortcutScope;
  /** 入力欄にフォーカスがあるときは譲る（入力欄自身の「元に戻す」などを優先する）。 */
  yieldToEditable: boolean;
  bindings: Record<Platform, Binding>;
  labels: Record<Platform, string>;
}

const both = (b: Binding): Record<Platform, Binding> => ({ mac: b, windows: b });

export const SHORTCUTS: Record<ShortcutId, Definition> = {
  newBook: {
    scope: "global",
    yieldToEditable: false,
    bindings: both({ code: "KeyN", mod: true, shift: false }),
    labels: { mac: "⌘N", windows: "Ctrl+N" },
  },
  scanRegister: {
    scope: "global",
    yieldToEditable: false,
    bindings: both({ code: "KeyN", mod: true, shift: true }),
    labels: { mac: "⇧⌘N", windows: "Ctrl+Shift+N" },
  },
  focusSearch: {
    scope: "global",
    yieldToEditable: false,
    bindings: both({ code: "KeyF", mod: true, shift: false }),
    labels: { mac: "⌘F", windows: "Ctrl+F" },
  },
  toggleDetail: {
    scope: "global",
    yieldToEditable: false,
    bindings: both({ code: "KeyI", mod: true, shift: false }),
    labels: { mac: "⌘I", windows: "Ctrl+I" },
  },
  openSettings: {
    scope: "global",
    yieldToEditable: false,
    bindings: both({ code: "Comma", mod: true, shift: false }),
    labels: { mac: "⌘,", windows: "Ctrl+," },
  },
  undo: {
    scope: "global",
    yieldToEditable: true,
    bindings: both({ code: "KeyZ", mod: true, shift: false }),
    labels: { mac: "⌘Z", windows: "Ctrl+Z" },
  },
  edit: {
    scope: "list",
    yieldToEditable: true,
    bindings: both({ code: "Enter", mod: false, shift: false }),
    labels: { mac: "Enter", windows: "Enter" },
  },
  trash: {
    scope: "list",
    yieldToEditable: true,
    bindings: {
      mac: { code: "Backspace", mod: true, shift: false },
      windows: { code: "Delete", mod: false, shift: false },
    },
    labels: { mac: "⌘⌫", windows: "Delete" },
  },
};

export const SHORTCUT_IDS = Object.keys(SHORTCUTS) as ShortcutId[];

export function shortcutLabel(id: ShortcutId, platform: Platform): string {
  return SHORTCUTS[id].labels[platform];
}

export function shortcutScope(id: ShortcutId): ShortcutScope {
  return SHORTCUTS[id].scope;
}

/** 入力欄にフォーカスがあるとき、このショートカットは譲るべきか。 */
export function yieldsToEditable(id: ShortcutId): boolean {
  return SHORTCUTS[id].yieldToEditable;
}

function matches(event: KeyLike, binding: Binding, platform: Platform): boolean {
  if (event.code !== binding.code) return false;
  if (event.altKey) return false;
  const modDown = platform === "mac" ? event.metaKey : event.ctrlKey;
  const otherDown = platform === "mac" ? event.ctrlKey : event.metaKey;
  if (otherDown) return false;
  if (modDown !== binding.mod) return false;
  return event.shiftKey === binding.shift;
}

/** キーイベントに当てはまるショートカットを返す。なければ null。 */
export function matchShortcut(event: KeyLike, platform: Platform): ShortcutId | null {
  for (const id of SHORTCUT_IDS) {
    if (matches(event, SHORTCUTS[id].bindings[platform], platform)) return id;
  }
  return null;
}
