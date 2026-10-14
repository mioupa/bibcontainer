// 書影の差し替え（FR-COVER-05）で使う、画像の取り出しの純粋関数。
// 取り込みの本体は Rust（`import_cover_file`・`import_cover_bytes`・`import_cover_url`）。
// Rust が読める形式は JPEG・PNG・GIF・WebP（0007 §8）。

/** 取り込める画像の拡張子（ドットなし、小文字）。 */
export const COVER_EXTENSIONS = ["jpg", "jpeg", "png", "gif", "webp"] as const;

/** パスの拡張子が、取り込める画像のものか。 */
export function isSupportedCoverPath(path: string): boolean {
  const match = /\.([A-Za-z0-9]+)$/.exec(path);
  const extension = match?.[1]?.toLowerCase();
  return extension !== undefined && (COVER_EXTENSIONS as readonly string[]).includes(extension);
}

/** ドロップされたパスのうち、最初の画像ファイル。なければ null。 */
export function firstImagePath(paths: readonly string[]): string | null {
  return paths.find(isSupportedCoverPath) ?? null;
}

/** MIME タイプが画像か。 */
function isImageType(type: string): boolean {
  return type.startsWith("image/");
}

/** `File` の並び（ドロップ・貼り付け）から、最初の画像のバイト列を取り出す。なければ null。 */
export async function imageBytesFromFiles(
  files: ArrayLike<File> | null | undefined,
): Promise<Uint8Array | null> {
  if (!files) return null;
  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    if (file && isImageType(file.type)) return new Uint8Array(await file.arrayBuffer());
  }
  return null;
}

/** `navigator.clipboard.read()` の結果から、最初の画像のバイト列を取り出す。なければ null。 */
export async function imageBytesFromClipboardItems(
  items: readonly ClipboardItem[],
): Promise<Uint8Array | null> {
  for (const item of items) {
    const type = item.types.find(isImageType);
    if (type !== undefined) {
      const blob = await item.getType(type);
      return new Uint8Array(await blob.arrayBuffer());
    }
  }
  return null;
}

/** 入力された画像 URL を整える。http・https 以外は null（Rust も同じ規則で拒否する）。 */
export function normalizeImageUrl(input: string): string | null {
  const trimmed = input.trim();
  return /^https?:\/\/\S+$/i.test(trimmed) ? trimmed : null;
}
