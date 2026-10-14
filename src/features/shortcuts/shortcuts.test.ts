import { describe, expect, it } from "vitest";
import {
  SHORTCUT_IDS,
  matchShortcut,
  shortcutLabel,
  shortcutScope,
  type KeyLike,
} from "@/features/shortcuts/shortcuts";

const key = (code: string, mods: Partial<Omit<KeyLike, "code">> = {}): KeyLike => ({
  code,
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("matchShortcut（9.5 の表）", () => {
  it("macOS は ⌘、Windows は Ctrl", () => {
    expect(matchShortcut(key("KeyN", { metaKey: true }), "mac")).toBe("newBook");
    expect(matchShortcut(key("KeyN", { ctrlKey: true }), "windows")).toBe("newBook");
    // 反対側の修飾キーでは動かない
    expect(matchShortcut(key("KeyN", { ctrlKey: true }), "mac")).toBeNull();
    expect(matchShortcut(key("KeyN", { metaKey: true }), "windows")).toBeNull();
  });

  it.each([
    ["手動登録", key("KeyN", { metaKey: true }), key("KeyN", { ctrlKey: true }), "newBook"],
    [
      "スキャン登録",
      key("KeyN", { metaKey: true, shiftKey: true }),
      key("KeyN", { ctrlKey: true, shiftKey: true }),
      "scanRegister",
    ],
    ["検索欄へ移動", key("KeyF", { metaKey: true }), key("KeyF", { ctrlKey: true }), "focusSearch"],
    [
      "詳細ペインの切替",
      key("KeyI", { metaKey: true }),
      key("KeyI", { ctrlKey: true }),
      "toggleDetail",
    ],
    ["設定", key("Comma", { metaKey: true }), key("Comma", { ctrlKey: true }), "openSettings"],
    ["元に戻す", key("KeyZ", { metaKey: true }), key("KeyZ", { ctrlKey: true }), "undo"],
    ["編集", key("Enter"), key("Enter"), "edit"],
    ["ゴミ箱へ移動", key("Backspace", { metaKey: true }), key("Delete"), "trash"],
  ] as const)("%s", (_name, mac, windows, id) => {
    expect(matchShortcut(mac, "mac")).toBe(id);
    expect(matchShortcut(windows, "windows")).toBe(id);
  });

  it("macOS の Delete だけ（⌘ なし）や、Windows の Backspace はゴミ箱ではない", () => {
    expect(matchShortcut(key("Backspace"), "mac")).toBeNull();
    expect(matchShortcut(key("Delete"), "mac")).toBeNull();
    expect(matchShortcut(key("Backspace", { metaKey: true }), "windows")).toBeNull();
  });

  it("Shift の有無を区別する（⌘N と ⇧⌘N は別）。Alt 付きは対象外", () => {
    expect(matchShortcut(key("KeyF", { metaKey: true, shiftKey: true }), "mac")).toBeNull();
    expect(matchShortcut(key("KeyN", { metaKey: true, altKey: true }), "mac")).toBeNull();
    expect(matchShortcut(key("Enter", { shiftKey: true }), "mac")).toBeNull();
  });

  it("keycode で判定するので、日本語入力の key（Process）に左右されない", () => {
    // key を持たない KeyLike でも判定できる（code だけを見る）
    expect(matchShortcut(key("KeyF", { ctrlKey: true }), "windows")).toBe("focusSearch");
  });

  it("ショートカットでないキーは null", () => {
    expect(matchShortcut(key("KeyA"), "mac")).toBeNull();
    expect(matchShortcut(key("Digit1"), "windows")).toBeNull();
    expect(matchShortcut(key("KeyQ", { metaKey: true }), "mac")).toBeNull();
  });

  it("表示用のラベルと範囲", () => {
    expect(shortcutLabel("scanRegister", "mac")).toBe("⇧⌘N");
    expect(shortcutLabel("scanRegister", "windows")).toBe("Ctrl+Shift+N");
    expect(shortcutLabel("trash", "mac")).toBe("⌘⌫");
    expect(shortcutLabel("trash", "windows")).toBe("Delete");
    expect(shortcutScope("edit")).toBe("list");
    expect(shortcutScope("trash")).toBe("list");
    expect(shortcutScope("newBook")).toBe("global");
  });

  it("同じキーの組み合わせが 2 つのショートカットに割り当たっていない", () => {
    for (const platform of ["mac", "windows"] as const) {
      const seen = new Set<string>();
      for (const id of SHORTCUT_IDS) {
        const label = shortcutLabel(id, platform);
        expect(seen.has(label)).toBe(false);
        seen.add(label);
      }
    }
  });
});
