// 価格の表示（FR-CONF-05）。本体価格（税抜）を基本とし、設定で税込を併記する。
import type { PriceSettings } from "@/bindings/PriceSettings";
import { ja } from "@/i18n/ja";
import { formatYen } from "@/lib/format";

/** 価格設定の既定値（設定を読み込む前や、読み込めなかったときに使う）。 */
export const DEFAULT_PRICE_SETTINGS: PriceSettings = { showTaxIncluded: false, taxRatePercent: 10 };

/** 税込価格（円未満は切り捨て）。 */
export function taxIncludedPrice(price: number, taxRatePercent: number): number {
  // 浮動小数点の誤差（700 × 1.1 = 770.0000000000001 など）で 1 円ずれないよう、100 倍した整数で計算する
  return Math.floor(Math.round(price * (100 + taxRatePercent) * 1000) / 100_000);
}

/**
 * 一覧・詳細・フォームに出す本体価格。設定が「税込を併記」なら「1,300円（税込 1,430円）」の形。
 * 価格がなければ空文字。
 */
export function formatListPrice(price: number | null, settings: PriceSettings): string {
  if (price === null) return "";
  const base = formatYen(price);
  if (!settings.showTaxIncluded) return base;
  return ja.format.withTaxIncluded(
    base,
    formatYen(taxIncludedPrice(price, settings.taxRatePercent)),
  );
}
