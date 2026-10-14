// 確定した本を、登録済みの本と突き合わせたときの分岐（FR-SCAN-17／18、FR-BOOK-10）。
import type { IsbnMatch } from "@/bindings/IsbnMatch";

export type ConfirmDecision =
  /** 所有で登録済み。警告して、既定では登録しない。 */
  | { kind: "owned"; match: IsbnMatch }
  /** 欲しい本に登録済み。「所有に移しますか？」と確認する。新しい本は作らない。 */
  | { kind: "wanted"; match: IsbnMatch }
  /** 新しい本として登録する。 */
  | { kind: "new" };

/**
 * `find_by_isbn` の結果から分岐を決める。
 * - ゴミ箱の本は無いものとして扱う（ゴミ箱の本と同じ ISBN は、新しい本として登録できる）。
 * - 所有（owned）が 1 冊でもあれば、欲しい本より先に重複とする。
 * - 手放した本（released）だけのときは新しい本（買い直したものとして登録できる）。
 */
export function decideConfirmed(matches: readonly IsbnMatch[]): ConfirmDecision {
  const live = matches.filter((match) => !match.deleted);
  const owned = live.find((match) => match.ownership === "owned");
  if (owned) return { kind: "owned", match: owned };
  const wanted = live.find((match) => match.ownership === "wanted");
  if (wanted) return { kind: "wanted", match: wanted };
  return { kind: "new" };
}
