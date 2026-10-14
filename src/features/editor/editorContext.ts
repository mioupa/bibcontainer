// フォームの各部品（項目の行・候補・書影）が共有する、フォームの外の状態。
// 値そのものは react-hook-form が持ち、ここには、初期値・取得元・候補・選択肢など、読むだけのものを置く。
import { createContext, useContext } from "react";
import type { MetaMerged } from "@/bindings/MetaMerged";
import type { PriceSettings } from "@/bindings/PriceSettings";
import type { FieldCandidate } from "@/features/editor/candidates";
import type { CoverImporter } from "@/features/editor/useCoverImport";
import type { BookFormValues, FormKey } from "@/features/editor/formValues";
import type { Picks } from "@/features/editor/sources";
import type { CCodeTable, Location } from "@/types/contracts";

export type EditorMode = "edit" | "new" | "draft";

export interface EditorContextValue {
  mode: EditorMode;
  /** フォームを開いたときの値（変えた項目の判定に使う）。 */
  initial: BookFormValues;
  /** Op 上の項目名 → 元の取得元（`field_sources`、新規なら下書きの由来）。 */
  initialSources: Readonly<Record<string, string | null>>;
  /** 取得元の候補から選んだ値。 */
  picks: Picks;
  pickCandidate: (key: FormKey, candidate: FieldCandidate) => void;
  /** 取得元の候補（統合結果）。 */
  merged: MetaMerged | null;
  cCodes: CCodeTable;
  price: PriceSettings;
  locations: readonly Location[];
  tagSuggestions: readonly string[];
  seriesSuggestions: readonly string[];
  storeSuggestions: readonly string[];
  /** 書影の取り込み（ファイル選択・ドロップ・貼り付け・URL）。ドロップと貼り付けは、どのタブでも受ける。 */
  cover: { importer: CoverImporter; dragging: boolean };
  /** 書影の ID（仮の表紙の色）に使う本の ID。新規は仮の値。 */
  coverId: string;
}

export const EditorContext = createContext<EditorContextValue | null>(null);

export function useEditorContext(): EditorContextValue {
  const value = useContext(EditorContext);
  if (value === null) throw new Error("EditorContext の外で使われています");
  return value;
}
