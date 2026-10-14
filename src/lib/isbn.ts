// ISBN の判定と変換（検索欄・登録フォームで使う小さな純粋関数）。
// 正式な判定（雑誌・2段目・一般 JAN の区別）は Rust の `classify_code` が行う。ここは、画面の中で
// 「この文字列は ISBN か」を IPC なしで決めたいとき（検索欄の Enter、フォームの検証）のためのもの。
// 正規化は Rust 側（`barcode::normalize`）に合わせる: 全角を半角にし、ハイフン・空白・長音符を除く。

/** 全角・半角のハイフン類、空白、長音符（日本語入力で `-` を押したとき）。 */
const SEPARATORS = /[\s\-‐-―−ー]/g;

/** 前後の空白、ハイフン類、空白を除き、全角を半角にして、末尾の x を大文字にする。 */
export function compactIsbn(raw: string): string {
  const compact = raw.normalize("NFKC").replace(SEPARATORS, "");
  return compact.endsWith("x") ? `${compact.slice(0, -1)}X` : compact;
}

/** ISBN-13（EAN-13）として正しいか（13 桁の半角数字で、チェックデジットが合う）。 */
export function isValidIsbn13(value: string): boolean {
  if (!/^\d{13}$/.test(value)) return false;
  return isbn13CheckDigit(value.slice(0, 12)) === Number(value[12]);
}

/** 先頭 12 桁から、ISBN-13 のチェックデジットを求める。 */
function isbn13CheckDigit(first12: string): number {
  let sum = 0;
  for (let i = 0; i < 12; i++) sum += Number(first12[i]) * (i % 2 === 0 ? 1 : 3);
  return (10 - (sum % 10)) % 10;
}

/** ISBN-10 として正しいか（9 桁の数字＋数字か X のチェックデジットが合う）。 */
export function isValidIsbn10(value: string): boolean {
  if (!/^\d{9}[\dX]$/.test(value)) return false;
  let sum = 0;
  for (let i = 0; i < 9; i++) sum += Number(value[i]) * (10 - i);
  sum += value[9] === "X" ? 10 : Number(value[9]);
  return sum % 11 === 0;
}

/** ISBN-10 を ISBN-13 にする。ISBN-10 として正しくなければ null。 */
export function isbn10ToIsbn13(value: string): string | null {
  if (!isValidIsbn10(value)) return null;
  const body = `978${value.slice(0, 9)}`;
  return `${body}${isbn13CheckDigit(body)}`;
}

/**
 * 文字列を ISBN-13 にする。ハイフン・空白・全角を許し、ISBN-10 は ISBN-13 に直す。
 * 先頭が 978・979 の ISBN-13 と、正しい ISBN-10 だけを受け付ける（2段目の `192…` や一般 JAN は null）。
 */
export function toIsbn13(raw: string): string | null {
  const compact = compactIsbn(raw);
  if (/^\d{13}$/.test(compact)) {
    return (compact.startsWith("978") || compact.startsWith("979")) && isValidIsbn13(compact)
      ? compact
      : null;
  }
  return isbn10ToIsbn13(compact);
}
