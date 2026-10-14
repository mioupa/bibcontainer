// 新しい本のフォームの「元のデータ」（`NewBookSource`）を、項目ごとの値と由来にまとめる。
//
// 優先順位（後のものが前のものを置き換える）:
//   1. 読んだバーコードの値（`isbn13`、`secondTier` の Cコード・本体価格。由来は barcode）
//   2. 書誌取得の下書き（`fetched.draft`。取得元ごとの由来）
//   3. 先に入れておく値（`presets`。連続登録の共通値、欲しい本としての登録など）
//   4. 既に一度直した下書き（`draft`。draft モードで開き直したとき）
// この上に、フォームで利用者が直した値が重なる（`changes.ts`）。
import type { Origin } from "@/bindings/Origin";
import { draftToChanges } from "@/features/library/drafts";
import type { DraftSeries } from "@/bindings/DraftSeries";
import type { NewBookSource } from "@/stores/editorStore";
import { applyOpValue, emptyFormValues, type BookFormValues } from "@/features/editor/formValues";

export interface BaseField {
  value: unknown;
  origin: Origin;
}

export interface DraftBase {
  /** 項目名（Op 上の名前）→ 値と由来。値の順は入れた順。 */
  fields: Map<string, BaseField>;
  /** シリーズ名と由来。 */
  seriesName: DraftSeries | null;
}

export type FetchState = "fetched" | "offline" | "none";

/** 書誌取得の状態。`fetched` があり offline でなければ取得済み、offline なら取得待ち、なければ未取得。 */
export function fetchStateOf(source: NewBookSource): FetchState {
  if (!source.fetched) return "none";
  return source.fetched.offline ? "offline" : "fetched";
}

export function buildDraftBase(source: NewBookSource): DraftBase {
  const fields = new Map<string, BaseField>();
  const barcode: Origin = { kind: "barcode" };
  let seriesName: DraftSeries | null = null;

  if (source.isbn13) fields.set("isbn13", { value: source.isbn13, origin: barcode });
  if (source.secondTier?.cCode) {
    fields.set("c_code", { value: source.secondTier.cCode, origin: barcode });
  }
  if (source.secondTier?.listPrice !== null && source.secondTier?.listPrice !== undefined) {
    fields.set("list_price", { value: source.secondTier.listPrice, origin: barcode });
  }

  if (source.fetched) {
    for (const change of draftToChanges(source.fetched.draft)) {
      fields.set(change.field, { value: change.value, origin: change.origin });
    }
    seriesName = source.fetched.draft.seriesName;
  }

  for (const change of source.presets ?? []) {
    fields.set(change.field, { value: change.value, origin: change.origin });
  }

  if (source.draft) {
    for (const change of source.draft.changes) {
      fields.set(change.field, { value: change.value, origin: change.origin });
    }
    if (source.draft.seriesName !== undefined) seriesName = source.draft.seriesName;
  }
  return { fields, seriesName };
}

/** 元のデータから、フォームの初期値を作る。 */
export function valuesFromBase(base: DraftBase): BookFormValues {
  const values = emptyFormValues();
  for (const [op, field] of base.fields) applyOpValue(values, op, field.value);
  values.seriesName = base.seriesName?.name ?? "";
  return values;
}
