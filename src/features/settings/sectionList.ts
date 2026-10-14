// 設定画面の節の並び順（左の一覧）。
import type { SettingsSection } from "@/stores/screenStore";

/** 節の並び順（左の一覧）。 */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  "sync",
  "providers",
  "scan",
  "display",
  "price",
  "backup",
  "about",
];
