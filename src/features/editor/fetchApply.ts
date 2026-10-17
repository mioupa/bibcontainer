// 取得結果（ISBN の書誌取得、検索結果から選んだ 1 件）を、フォームのどの項目に入れるかの決め方（純粋関数）。
//
// 基本の規則（0011 §3）: まだ直していない項目（空のまま、または前の取得のまま）にだけ入れる。手で入れた値は変えない。
// 検索結果を選んだときだけの例外: 書名と著者は、検索語として入れたものなので、直してあっても、取得結果に値があれば置き換える。
import {
  FIELDS,
  isFieldChanged,
  opValue,
  type BookFormValues,
  type FormKey,
} from "@/features/editor/formValues";

/** 検索結果を選んだとき、直してあっても取得結果の値で置き換える項目。 */
export const SEARCH_REPLACED_KEYS: readonly FormKey[] = ["title", "authors"];

/** その項目に、値があるか（空の文字列・空の一覧・null は、ない）。 */
export function hasValue(key: FormKey, values: BookFormValues): boolean {
  const value = opValue(key, values);
  return Array.isArray(value) ? value.length > 0 : value !== null && value !== "";
}

/**
 * 取得結果の値（`next`）をフォームに入れる項目。`initial` は前の「元のデータ」の値、`current` は今のフォームの値。
 * ISBN の欄は、ここでは決めない（取得の元になった値なので、呼び出し側が扱う）。
 */
export function keysToFill(args: {
  initial: BookFormValues;
  current: BookFormValues;
  next: BookFormValues;
  /** 直していても、取得結果に値があれば置き換える項目。 */
  replace?: readonly FormKey[];
}): FormKey[] {
  const { initial, current, next, replace = [] } = args;
  return FIELDS.map((def) => def.key).filter((key) => {
    if (key === "isbn13") return false;
    if (replace.includes(key) && hasValue(key, next)) return true;
    // 直していない項目だけ、新しい値にする
    return !isFieldChanged(key, initial, current) && isFieldChanged(key, initial, next);
  });
}
