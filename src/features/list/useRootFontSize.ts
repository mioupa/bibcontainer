// <html> の文字サイズ（px。`rem` の基準）。グリッドのカードの大きさは rem で決め、仮想スクロールの行の高さは px で要るので、
// 文字サイズの設定（FR-CONF-04）が変わるたびに読み直す。
import { useEffect, useState } from "react";
import { useSettingsStore } from "@/stores/settingsStore";

const FALLBACK_PX = 16;

/** いまの <html> の文字サイズ（px）。読めなければ 16。 */
export function readRootFontSize(): number {
  if (typeof document === "undefined") return FALLBACK_PX;
  const px = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(px) && px > 0 ? px : FALLBACK_PX;
}

/**
 * <html> の文字サイズ（px）。設定の文字サイズが変わったら、読み直す。
 * <html> へ反映するのは、アプリの最上位の effect（`useThemeSync`）。子の effect はその前に動くので、
 * その場では古い値を読んでしまう。effect が全部終わったあとに読むため、マイクロタスクに回す。
 */
export function useRootFontSize(): number {
  const setting = useSettingsStore((s) => s.fontSize);
  const [px, setPx] = useState(readRootFontSize);
  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      const next = readRootFontSize();
      setPx((current) => (current === next ? current : next));
    });
    return () => {
      cancelled = true;
    };
  }, [setting]);
  return px;
}
