// 表示用の整形。数や日付の書き方をここにそろえる。

import { ja } from "@/i18n/ja";

const numberFormat = new Intl.NumberFormat("ja-JP");

export function formatInteger(value: number): string {
  return numberFormat.format(value);
}

/** 金額（円）。税込・税抜の区別は呼び出し側で付ける。 */
export function formatYen(value: number | null | undefined): string {
  return value === null || value === undefined ? "" : ja.format.yen(numberFormat.format(value));
}

/** YYYY / YYYY-MM / YYYY-MM-DD をそのまま返す（欠けていても表示できる）。空は空文字。 */
export function formatPartialDate(value: string | null | undefined): string {
  return value ?? "";
}

/** ISO 8601 の日時から日付部分（YYYY-MM-DD）を取り出す。 */
export function formatDateOnly(iso: string | null | undefined): string {
  return iso ? iso.slice(0, 10) : "";
}
