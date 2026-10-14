// 書影がない本の仮の表紙の色を、本の ID から決める。同じ ID なら、いつでも同じ色になる。

/** Catppuccin のアクセント色の名前（src/index.css の --ctp-<名前>）。 */
export const COVER_ACCENTS = [
  "rosewater",
  "flamingo",
  "pink",
  "mauve",
  "red",
  "maroon",
  "peach",
  "yellow",
  "green",
  "teal",
  "sky",
  "sapphire",
  "blue",
  "lavender",
] as const;

export type CoverAccent = (typeof COVER_ACCENTS)[number];

/** FNV-1a（32 ビット）。文字列を、偏りの少ない整数にする。 */
export function hashString(text: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
}

export function coverAccent(id: string): CoverAccent {
  return COVER_ACCENTS[hashString(id) % COVER_ACCENTS.length] as CoverAccent;
}

/** CSS の色（var(--ctp-xxx)）。ライト・ダークで自動的に切り替わる。 */
export function coverAccentVar(id: string): string {
  return `var(--ctp-${coverAccent(id)})`;
}
