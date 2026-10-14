// テーマ（FR-CONF-04）。「OS に連動／ライト／ダーク」を、<html class="dark"> の付け外しで切り替える。
// 配色の定義は src/index.css（ライトは Catppuccin Latte、ダークは Mocha）。

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export const SYSTEM_DARK_QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(preference: ThemePreference, systemDark: boolean): ResolvedTheme {
  if (preference === "system") return systemDark ? "dark" : "light";
  return preference;
}

export function systemPrefersDark(): boolean {
  return (
    typeof globalThis.matchMedia === "function" && globalThis.matchMedia(SYSTEM_DARK_QUERY).matches
  );
}

/** 解決したテーマを <html> に反映する。 */
export function applyResolvedTheme(
  theme: ResolvedTheme,
  root: HTMLElement = document.documentElement,
) {
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
}

export function applyTheme(
  preference: ThemePreference,
  root: HTMLElement = document.documentElement,
) {
  applyResolvedTheme(resolveTheme(preference, systemPrefersDark()), root);
}

/** 既定の文字サイズ（px）。ブラウザの既定と同じ。 */
export const DEFAULT_FONT_SIZE = 16;

/** 画面の既定のフォント（src/index.css の `--font-sans` と同じ。OS の日本語フォント）。 */
const DEFAULT_FONT_STACK =
  'system-ui, -apple-system, "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic UI", "Meiryo UI", "Segoe UI", sans-serif';

/** 設定のフォント名から、CSS の `font-family` の値を作る（既定の OS のフォントを後ろに続ける）。 */
export function fontStack(fontFamily: string | null): string {
  const name = fontFamily?.replace(/["'\\;{}]/g, "").trim();
  return name ? `"${name}", ${DEFAULT_FONT_STACK}` : DEFAULT_FONT_STACK;
}

/**
 * フォントと文字サイズ（FR-CONF-04）を <html> に反映する。文字サイズは `rem` の基準になるので、画面全体が変わる。
 * 既定の値のときは、指定を外して CSS の既定に任せる。
 */
export function applyDisplay(
  display: { fontFamily: string | null; fontSize: number },
  root: HTMLElement = document.documentElement,
) {
  if (display.fontSize === DEFAULT_FONT_SIZE) root.style.removeProperty("font-size");
  else root.style.fontSize = `${display.fontSize}px`;
  if (display.fontFamily === null) root.style.removeProperty("--font-sans");
  else root.style.setProperty("--font-sans", fontStack(display.fontFamily));
}
