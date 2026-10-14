// テーマの React フック。設定（OS に連動／ライト／ダーク）と OS の設定から実際のテーマを決め、<html> に反映する。
import { useEffect, useSyncExternalStore } from "react";
import {
  SYSTEM_DARK_QUERY,
  applyDisplay,
  applyResolvedTheme,
  resolveTheme,
  systemPrefersDark,
  type ResolvedTheme,
} from "@/lib/theme";
import { useSettingsStore } from "@/stores/settingsStore";

function subscribeSystemDark(onChange: () => void): () => void {
  if (typeof globalThis.matchMedia !== "function") return () => {};
  const query = globalThis.matchMedia(SYSTEM_DARK_QUERY);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

/** いま実際に使うテーマ（"system" は OS の設定に解決したもの）。 */
export function useResolvedTheme(): ResolvedTheme {
  const preference = useSettingsStore((s) => s.theme);
  const systemDark = useSyncExternalStore(subscribeSystemDark, systemPrefersDark, () => false);
  return resolveTheme(preference, systemDark);
}

/** アプリの最上位で 1 回だけ呼ぶ。テーマ・フォント・文字サイズが変わるたびに <html> を更新する。 */
export function useThemeSync(): void {
  const resolved = useResolvedTheme();
  const fontFamily = useSettingsStore((s) => s.fontFamily);
  const fontSize = useSettingsStore((s) => s.fontSize);
  useEffect(() => {
    applyResolvedTheme(resolved);
  }, [resolved]);
  useEffect(() => {
    applyDisplay({ fontFamily, fontSize });
  }, [fontFamily, fontSize]);
}
