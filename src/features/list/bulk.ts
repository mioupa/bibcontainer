// 一括変更（FR-BOOK-08）。右クリックメニューや一括変更のダイアログで選んだ変更を表す型と、
// 変更の対象にする本（所有状態の変更で、すでにその状態の本は除くなど）を決める関数。
import type { Ownership, ReadStatus } from "@/types/contracts";

/** 一括変更。`field` は Rust 側の項目名（snake_case）。タグは足す・外すの組。 */
export type BulkChange =
  | { field: "read_status"; value: ReadStatus }
  | { field: "ownership"; value: Ownership }
  | { field: "rating"; value: number | null }
  | { field: "location_id"; value: string | null }
  | { field: "tags"; add: string[]; remove: string[] };

/** タグの一括変更で、足すものと外すもの（前後の空白・空・重複を除く）が両方空なら、変更はない。 */
export function tagsChange(add: readonly string[], remove: readonly string[] = []): BulkChange {
  const clean = (tags: readonly string[]) => [
    ...new Set(tags.map((tag) => tag.trim()).filter((tag) => tag !== "")),
  ];
  return { field: "tags", add: clean(add), remove: clean(remove) };
}

/** 変更が何もしないものか（タグの足す・外すがどちらも空）。 */
export function isEmptyBulkChange(change: BulkChange): boolean {
  return change.field === "tags" && change.add.length === 0 && change.remove.length === 0;
}

/** 選んだ本のタグの、重複のない一覧（「タグを外す」で、外せるものを出すため）。名前順。 */
export function tagsOfRows(rows: readonly { tags: readonly string[] }[]): string[] {
  const tags = new Set<string>();
  for (const row of rows) for (const tag of row.tags) tags.add(tag);
  return [...tags].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}
