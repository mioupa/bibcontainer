// 書影の URL を組み立てる（0005 §7）。
// WebView へは独自の URI スキーム `cover` で渡す。URL の形は OS で違う。
//   macOS   : cover://localhost/<hash>?size=80
//   Windows : http://cover.localhost/<hash>?size=80
// 実装（Rust 側のスキーム登録）と CSP の img-src への追記は統合作業で行う。
import { detectPlatform, type Platform } from "@/lib/platform";

/** サムネイルの長辺（px）。Rust 側の `THUMBNAIL_SIZES`（0010 §12・§14）と同じ 80・160・320。 */
export type CoverSize = 80 | 160 | 320;

/**
 * 一覧の書影（幅 28px × 高さ約 39px の枠。2 倍の画面でも 56 × 78px）に使う大きさ。
 * 枠より大きいサムネイルは無駄なので、枠に合わせた 80px（0010 §14）。メモリの目標に効いたのは、
 * この大きさよりも、Rust 側が一覧用（80）の応答に付ける `Cache-Control: no-store`。
 */
export const LIST_COVER_SIZE: CoverSize = 80;
/** 詳細ペインとフォームの書影に使う大きさ。 */
export const DETAIL_COVER_SIZE: CoverSize = 320;

export function coverUrl(
  hash: string,
  size: CoverSize,
  platform: Platform = detectPlatform(),
): string {
  const path = `${encodeURIComponent(hash)}?size=${size}`;
  return platform === "windows" ? `http://cover.localhost/${path}` : `cover://localhost/${path}`;
}
