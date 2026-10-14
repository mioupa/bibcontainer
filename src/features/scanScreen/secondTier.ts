// 2段目（Cコード・本体価格）の扱い（FR-SCAN-05、FR-META-04）。
//
// 2段目は、ISBN の後に読まれることがある。書誌取得は ISBN を読んだ時点で始めるので、取得結果の下書きには
// 2段目の値が入っていない。確定したときに 2段目があれば、下書きの Cコードと本体価格を、
// 由来 `barcode` で上書きする。2段目は、この 2 項目で最優先の取得元だから（優先順位の表は 7.3）。
import type { BookDraft } from "@/bindings/BookDraft";
import type { MetaSecondTier } from "@/bindings/MetaSecondTier";
import type { FetchMetadataResult } from "@/types/contracts";

/** 2段目から分かった値。ペアリングの `SecondTier` と、画面が覚える値のどちらも渡せる。 */
export interface SecondTierValues {
  /** 4 桁。 */
  cCode: string | null;
  /** 本体価格（税抜・円）。「不明」は null。 */
  listPrice: number | null;
}

/** 書誌取得（`fetch_metadata`）と、登録フォームに渡す形にする。2段目がなければ undefined。 */
export function toMetaSecondTier(
  secondTier: SecondTierValues | null | undefined,
): MetaSecondTier | undefined {
  if (!secondTier) return undefined;
  return { cCode: secondTier.cCode, listPrice: secondTier.listPrice };
}

/**
 * 下書きに 2段目の値を重ねる。Cコードと本体価格を、由来 `barcode` で上書きする。
 * null の値（価格が不明など）は上書きしない（取得元の値を消さない）。2段目がなければ、渡した下書きをそのまま返す。
 * 渡した下書きは書き換えない。
 */
export function applySecondTier(
  draft: BookDraft,
  secondTier: SecondTierValues | null | undefined,
): BookDraft {
  if (!secondTier) return draft;
  const hasCCode = secondTier.cCode !== null && secondTier.cCode !== "";
  const hasPrice = secondTier.listPrice !== null;
  if (!hasCCode && !hasPrice) return draft;
  const fields = { ...draft.fields };
  if (hasCCode) fields.c_code = { value: secondTier.cCode ?? "", origin: { kind: "barcode" } };
  if (hasPrice)
    fields.list_price = { value: secondTier.listPrice ?? 0, origin: { kind: "barcode" } };
  return { ...draft, fields };
}

/** 取得結果の下書きに、2段目の値を重ねた取得結果を返す（登録フォームに渡す）。 */
export function withSecondTier(
  result: FetchMetadataResult,
  secondTier: SecondTierValues | null | undefined,
): FetchMetadataResult {
  const draft = applySecondTier(result.draft, secondTier);
  return draft === result.draft ? result : { ...result, draft };
}
