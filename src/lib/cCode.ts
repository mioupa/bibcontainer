// Cコード（4 桁）の分解と名称の表示。名称表は外から受け取る（後で IPC の c_code_table() から渡す）。
//   1 桁目: 販売対象 / 2 桁目: 発行形態 / 3〜4 桁目: 内容（3 桁目が大分類）
import { ja } from "@/i18n/ja";
import type { CCodeTable } from "@/types/contracts";

export interface CCodeParts {
  audience: string; // 1 桁
  format: string; // 1 桁
  contentMajor: string; // 1 桁（3 桁目）
  content: string; // 2 桁（3〜4 桁目）
}

/** 4 桁の数字でなければ null。 */
export function parseCCode(code: string | null | undefined): CCodeParts | null {
  if (!code || !/^\d{4}$/.test(code)) return null;
  return {
    audience: code.slice(0, 1),
    format: code.slice(1, 2),
    contentMajor: code.slice(2, 3),
    content: code.slice(2, 4),
  };
}

export interface CCodeDescription {
  code: string;
  audience: string | null;
  format: string | null;
  /** 3〜4 桁目の名称。表にないコードは null（その場合も contentMajor は使える）。 */
  content: string | null;
  contentMajor: string | null;
  /** 3〜4 桁目（表引きに使った 2 桁）。 */
  contentCode: string;
}

export function describeCCode(
  table: CCodeTable,
  code: string | null | undefined,
): CCodeDescription | null {
  const parts = parseCCode(code);
  if (!parts || !code) return null;
  return {
    code,
    audience: table.audience[parts.audience] ?? null,
    format: table.format[parts.format] ?? null,
    content: table.content[parts.content] ?? null,
    contentMajor: table.contentMajor[parts.contentMajor] ?? null,
    contentCode: parts.content,
  };
}

/** 内容の名称。表にないコードは「未定義（xx）」（FR-SCAN-03）。 */
export function contentLabel(table: CCodeTable, code: string | null | undefined): string | null {
  const d = describeCCode(table, code);
  if (!d) return null;
  return d.content ?? ja.cCode.undefinedContent(d.contentCode);
}

/** 「一般・文庫・日本文学、小説・物語」の形。販売対象と発行形態が表にないときは省く。 */
export function cCodeNames(table: CCodeTable, code: string | null | undefined): string | null {
  const d = describeCCode(table, code);
  if (!d) return null;
  return [d.audience, d.format, d.content ?? ja.cCode.undefinedContent(d.contentCode)]
    .filter((s): s is string => s !== null)
    .join(ja.cCode.nameSeparator);
}

/** 「0193（全角空白）一般・文庫・日本文学、小説・物語」の形（9.2 の表示）。 */
export function cCodeFullLabel(table: CCodeTable, code: string | null | undefined): string | null {
  const names = cCodeNames(table, code);
  return names === null || !code ? null : `${code}\u3000${names}`;
}
