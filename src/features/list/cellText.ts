// 一覧と詳細ペインで共通の、行から文字列を作る関数。
import type { PriceSettings } from "@/bindings/PriceSettings";
import { ja } from "@/i18n/ja";
import type { ColumnId } from "@/features/list/columnMeta";
import type { CCodeTable, ListRow } from "@/types/contracts";

export interface CellContext {
  cCodes: CCodeTable;
  /** 保管場所 id → 「自宅 > 書斎」の形のパス。 */
  locationPaths: ReadonlyMap<string, string>;
  /** 価格の表示設定（税込を併記するか。FR-CONF-05）。なければ税抜だけ。 */
  price?: PriceSettings;
}

export function authorsText(row: ListRow): string {
  return row.authors.map((a) => a.name).join(ja.format.listSeparator);
}

/** セルに付ける title（切れた文字をツールチップで読めるようにする）。ReactNode でない素の文字列で返す。 */
export function cellTitle(id: ColumnId, row: ListRow, ctx: CellContext): string | undefined {
  switch (id) {
    case "title":
      return [row.title, row.subtitle].filter(Boolean).join(" ");
    case "authors":
      return authorsText(row) || undefined;
    case "publisher":
      return row.publisher ?? undefined;
    case "location":
      return row.locationId === null ? undefined : ctx.locationPaths.get(row.locationId);
    case "tags":
      return row.tags.join(ja.format.listSeparator) || undefined;
    default:
      return undefined;
  }
}
