// 手動登録のとき、書名・著者が似た本を候補に出す（FR-SCAN-19）。
// 検索は Rust の `search_books`（書名を空白で区切った語をすべて含む本）。ここでは、結果を並べ替えて絞るだけ。
import type { ListRow } from "@/types/contracts";

/** 候補に出す冊数の上限。 */
export const SIMILAR_LIMIT = 5;

/** 書名の入力が、似た本を探す長さか（1 文字では広すぎる）。 */
export function canSearchSimilar(title: string): boolean {
  return Array.from(title.trim()).length >= 2;
}

/**
 * 検索で見つかった本（ID の集合）から、候補にする本を選ぶ。ゴミ箱の本は除く。
 * 入力した著者の名前を持つ本を先に、同じ順位の中では一覧の順。
 */
export function pickSimilarBooks(
  rows: readonly ListRow[],
  foundIds: ReadonlySet<string>,
  authorNames: readonly string[],
  limit = SIMILAR_LIMIT,
): ListRow[] {
  const wanted = authorNames.map((name) => name.trim()).filter(Boolean);
  const matched = rows.filter((row) => foundIds.has(row.id) && !row.deleted);
  const authorMatch = (row: ListRow) =>
    wanted.length > 0 && row.authors.some((author) => wanted.includes(author.name)) ? 0 : 1;
  return matched
    .map((row, index) => ({ row, index }))
    .sort((a, b) => authorMatch(a.row) - authorMatch(b.row) || a.index - b.index)
    .slice(0, limit)
    .map((entry) => entry.row);
}
