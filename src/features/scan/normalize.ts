// 手入力欄（ISBN などを手で打つ欄）向けの正規化（FR-SCAN-08）。
// バーコードリーダーの入力は KeyboardEvent.code で拾うので、ここは通らない。

/**
 * 全角の数字（０〜９）を半角にする。ISBN-10 の末尾に付く検査数字の全角の Ｘ・ｘ も、半角の X にする。
 * それ以外の文字はそのまま返す（ハイフンや空白の扱いは、判定する Rust 側 classify_code に任せる）。
 */
export function toHalfWidthDigits(input: string): string {
  return input.replace(/[０-９Ｘｘ]/g, (ch) => {
    const code = ch.charCodeAt(0);
    if (code >= 0xff10 && code <= 0xff19) return String.fromCharCode(code - 0xff10 + 0x30);
    return "X";
  });
}
