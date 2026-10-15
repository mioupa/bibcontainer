// 取得元の項目ごとの優先順位（FR-CONF-02、FR-META-04）の並べ替え。純粋な関数。
// 2段目（バーコード）は、本体価格と Cコードで常に最優先なので、先頭に固定して動かせない。
import type { MetaGroup } from "@/bindings/MetaGroup";
import type { MetaPriorities } from "@/bindings/MetaPriorities";
import type { MetaPrioritySets } from "@/bindings/MetaPrioritySets";
import type { MetaSource } from "@/bindings/MetaSource";
import type { PriorityProfile } from "@/bindings/PriorityProfile";

/** 優先順位の組（区分）。ISBN が 978-4 の本は和書、それ以外の ISBN の本は洋書など（ISBN がない本は和書）。画面に出す順。 */
export const PRIORITY_PROFILES: readonly PriorityProfile[] = ["japanese", "foreign"];

/** 画面に出す項目の行の順（FR-META-04 の表の順）。 */
export const META_GROUPS: readonly MetaGroup[] = [
  "title",
  "title_kana",
  "authors",
  "publisher",
  "published_on",
  "list_price",
  "ref_price_incl",
  "c_code",
  "ndc",
  "pages_and_size",
  "series",
  "language",
  "description",
  "cover",
];

/** 行の優先順位（設定にない行は空）。 */
export function orderOf(priorities: MetaPriorities, group: MetaGroup): MetaSource[] {
  return [...(priorities.groups[group] ?? [])];
}

/** 動かせない取得元（2段目）。 */
export function isFixedSource(source: MetaSource): boolean {
  return source === "barcode";
}

/** `index` の取得元を、1 つ上（-1）か下（1）に動かす。端・2段目をまたぐ動きは何もしない（同じ並びを返す）。 */
export function moveSource(
  order: readonly MetaSource[],
  index: number,
  delta: -1 | 1,
): MetaSource[] {
  const target = index + delta;
  const source = order[index];
  const neighbor = order[target];
  if (source === undefined || neighbor === undefined) return [...order];
  if (isFixedSource(source) || isFixedSource(neighbor)) return [...order];
  const next = [...order];
  next[index] = neighbor;
  next[target] = source;
  return next;
}

/** 1 つの行の順位だけを差し替えた優先順位。ほかの行はそのまま。 */
export function withGroupOrder(
  priorities: MetaPriorities,
  group: MetaGroup,
  order: readonly MetaSource[],
): MetaPriorities {
  return { groups: { ...priorities.groups, [group]: [...order] } };
}

/** 区分 `profile` の 1 つの行の順位だけを差し替えた優先順位の組。ほかの行も、もう一方の区分も、そのまま。 */
export function withProfileGroupOrder(
  sets: MetaPrioritySets,
  profile: PriorityProfile,
  group: MetaGroup,
  order: readonly MetaSource[],
): MetaPrioritySets {
  return { ...sets, [profile]: withGroupOrder(sets[profile], group, order) };
}
