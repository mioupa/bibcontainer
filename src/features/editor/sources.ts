// 値の取得元（`field_sources`）の表示と、取得元の候補を選んだときの由来。
import type { MetaSource } from "@/bindings/MetaSource";
import type { Origin } from "@/bindings/Origin";
import { sameOpValue, type FormKey } from "@/features/editor/formValues";
import { ja } from "@/i18n/ja";

/** 取得元の文字列（`field_sources` の値、`Origin` の名前）の表示名。知らない名前はそのまま出す。 */
export function sourceLabel(source: string): string {
  const known = ja.edit.sources as Record<string, string>;
  return known[source] ?? source;
}

/** `Origin` を、`field_sources` の取得元の文字列にする（`barcode`、`openbd`、`manual` など）。 */
export function originSourceName(origin: Origin): string {
  return origin.kind === "provider" ? origin.name : origin.kind;
}

/** 取得元の文字列を、`Origin` にする（`barcode`・`manual`・`import` 以外は取得元の名前）。 */
export function originFromSource(source: string): Origin {
  switch (source) {
    case "manual":
      return { kind: "manual" };
    case "barcode":
      return { kind: "barcode" };
    case "import":
      return { kind: "import" };
    default:
      return { kind: "provider", name: source };
  }
}

/** 取得元の候補から選んだ値。`value` は、選んだ時点の Op の値。 */
export interface Pick {
  source: MetaSource;
  value: unknown;
}

export type Picks = Partial<Record<FormKey, Pick>>;

/**
 * 保存する値の由来。候補から選んだ値のまま（選んだあと直していない）なら、その取得元。
 * 直したときと、候補を使っていないときは、手入力。
 */
export function originOfChange(pick: Pick | undefined, currentValue: unknown): Origin {
  if (pick !== undefined && sameOpValue(pick.value, currentValue)) {
    return originFromSource(pick.source);
  }
  return { kind: "manual" };
}

/**
 * フォームの項目の横に出す取得元。値を直していなければ元の取得元、直していれば、選んだ候補の取得元か手入力。
 * 元の取得元がなく、直してもいないときは null（何も出さない）。
 */
export function effectiveSource(args: {
  initialSource: string | null;
  changed: boolean;
  pick: Pick | undefined;
  currentValue: unknown;
}): string | null {
  if (!args.changed) return args.initialSource;
  const origin = originOfChange(args.pick, args.currentValue);
  return originSourceName(origin);
}
