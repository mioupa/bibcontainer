// タグの名前の変更（サイドバーの管理）で使う純粋関数。タグ名の `/` は階層（`研究/SiC`）を表す。

export type TagNameError = "empty" | "invalid" | "same";

/**
 * 新しいタグ名の検査。前後の空白を除いた名前を返す。
 * 空、`/` で始まる・終わる・続く（空の階層ができる）名前、いまの名前と同じものはエラー。
 */
export function validateTagName(
  oldName: string,
  input: string,
): { ok: true; name: string } | { ok: false; error: TagNameError } {
  const name = input.trim();
  if (name === "") return { ok: false, error: "empty" };
  if (name.split("/").some((part) => part.trim() === "")) return { ok: false, error: "invalid" };
  if (name === oldName) return { ok: false, error: "same" };
  return { ok: true, name };
}

/**
 * タグの名前を変えたあとの、絞り込み中のタグのパス。
 * 改名したタグそのものと、その子孫（`旧/子`）は新しい名前に追随する（絞り込みが外れないように）。
 */
export function renamedTagPath(current: string | null, oldName: string, newName: string) {
  if (current === null) return null;
  if (current === oldName) return newName;
  if (current.startsWith(`${oldName}/`)) return `${newName}${current.slice(oldName.length)}`;
  return current;
}
