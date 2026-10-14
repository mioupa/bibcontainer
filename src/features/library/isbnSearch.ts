// 検索欄で ISBN を確定したときの扱い（FR-SCAN-09）。スキャン（バーコードリーダーが数字と Enter を打つ）でも
// 手入力でも同じ。判定は IPC なしの純粋関数（`lib/isbn`）で行い、登録済みかは、読み込み済みの一覧で調べる。
import { toIsbn13 } from "@/lib/isbn";
import type { ListRow } from "@/types/contracts";

/** 数字・ハイフン類・空白・X だけで書かれているか（長音符は、日本語入力で `-` を押したとき）。 */
const ISBN_CHARS = /^[\d\s\-‐-―−ーX]+$/i;

/**
 * 検索欄の文字列が ISBN（ISBN-13・ISBN-10。ハイフン・全角・先頭の「ISBN」を許す）なら、ISBN-13 を返す。
 * 2段目（`192…`）・一般 JAN・チェックデジット不一致・書名などの文字列は null（ふつうの検索のまま）。
 */
export function detectIsbnSearch(text: string): string | null {
  const body = text
    .normalize("NFKC")
    .trim()
    .replace(/^isbn[\s:-]*/i, "");
  if (body === "" || !ISBN_CHARS.test(body)) return null;
  return toIsbn13(body);
}

export type IsbnSearchResult =
  /** ISBN ではない（ふつうの検索）。 */
  | { kind: "none" }
  /** 登録済み。その本を選んで詳細を出す。 */
  | { kind: "found"; id: string }
  /** ゴミ箱にだけある。 */
  | { kind: "trashed"; id: string; title: string }
  /** 未登録。「登録しますか？」と確認して、スキャン登録に引き渡す。 */
  | { kind: "unregistered"; isbn13: string };

/** 検索欄の文字列を確定したとき、どう扱うか。ゴミ箱でない本があればそれを選ぶ。 */
export function resolveIsbnSearch(rows: readonly ListRow[], text: string): IsbnSearchResult {
  const isbn13 = detectIsbnSearch(text);
  if (isbn13 === null) return { kind: "none" };
  const matches = rows.filter((row) => row.isbn13 === isbn13);
  const live = matches.find((row) => !row.deleted);
  if (live) return { kind: "found", id: live.id };
  const trashed = matches[0];
  if (trashed) return { kind: "trashed", id: trashed.id, title: trashed.title };
  return { kind: "unregistered", isbn13 };
}
