// 書影の URL を組み立てる（0005 §7）。
// WebView へは独自の URI スキーム `cover` で渡す。URL の形は OS で違う。
//   macOS   : cover://localhost/<hash>?size=80
//   Windows : http://cover.localhost/<hash>?size=80
// 同期で書影があとから届いたときは、`&v=<版>` を足して URL を変え、読み込み直させる（0016）。
// 実装（Rust 側のスキーム登録）と CSP の img-src への追記は統合作業で行う。
import { detectPlatform, type Platform } from "@/lib/platform";

/** サムネイルの長辺（px）。Rust 側の `THUMBNAIL_SIZES`（0010 §12・§14）と同じ 80・160・320。 */
export type CoverSize = 80 | 160 | 320;

/**
 * 一覧の書影（幅 28px × 高さ約 39px の枠。2 倍の画面でも 56 × 78px）に使う大きさ。
 * 枠より大きいサムネイルは無駄なので、枠に合わせた 80px（0010 §14）。メモリの目標に効いたのは、
 * この大きさよりも、Rust 側がサムネイルの応答に付ける `Cache-Control: no-store`
 * （どの大きさもキャッシュさせない。`src-tauri/src/metadata/cover_protocol.rs`、docs/decisions/0024）。
 */
export const LIST_COVER_SIZE: CoverSize = 80;
/**
 * 詳細ペインとフォームの書影に使う大きさ。320 も `no-store` なので、本を選び直すたびに読み直す
 * （ローカルのファイルで 1ms 未満。無視できる）。
 */
export const DETAIL_COVER_SIZE: CoverSize = 320;
/**
 * グリッドのカードの書影に使う大きさ。カードの書影は幅 7rem（16px で 112px）なので、2 倍の画面では
 * 約 224 × 314 デバイス px 要る。160px では足りず、原寸では無駄なので、320px。
 * 何千冊ぶんもあるが、画面の外のカードは仮想スクロールで外し、WebKit にも残させない（`no-store`）。
 */
export const GRID_COVER_SIZE: CoverSize = 320;

/**
 * 書影の URL。`version` は、その書影が同期で届いた回数（`covers-arrived`。0 なら付けない）。
 * 書影が手元になかったとき（404）の読み込みを、同じ URL のままだとブラウザが取り直さないことがあるので、
 * 届いたら版を進めて URL を変える（Rust 側は、`v` を読み捨てる）。
 */
export function coverUrl(
  hash: string,
  size: CoverSize,
  platform: Platform = detectPlatform(),
  version = 0,
): string {
  const path = `${encodeURIComponent(hash)}?size=${size}${version > 0 ? `&v=${version}` : ""}`;
  return platform === "windows" ? `http://cover.localhost/${path}` : `cover://localhost/${path}`;
}
