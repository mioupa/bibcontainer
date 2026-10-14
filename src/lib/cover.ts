// 書影の URL を組み立てる（0005 §7）。
// WebView へは独自の URI スキーム `cover` で渡す。URL の形は OS で違う。
//   macOS   : cover://localhost/<hash>?size=160
//   Windows : http://cover.localhost/<hash>?size=160
// 実装（Rust 側のスキーム登録）と CSP の img-src への追記は統合作業で行う。
import { detectPlatform, type Platform } from "@/lib/platform";

/** サムネイルの長辺（px）。0005 §7 のとおり 160 と 320 の 2 種類。 */
export type CoverSize = 160 | 320;

export function coverUrl(
  hash: string,
  size: CoverSize,
  platform: Platform = detectPlatform(),
): string {
  const path = `${encodeURIComponent(hash)}?size=${size}`;
  return platform === "windows" ? `http://cover.localhost/${path}` : `cover://localhost/${path}`;
}
