// フロントの各ファイルが使う、Rust との間でやり取りするデータの型。
//
// 型の本体は src/bindings/（ts-rs が Rust から生成する。手で編集しない）。フロントの各ファイルは
// ListRow などをこのファイル経由で import し、src/bindings/ を直接 import しない
// （型の置き場所が変わっても、直すのをここだけにするため）。
//
// bindings と形の違うものだけ、このファイルで定義する（下の Location と CCodeTable）。
import type { ListRow } from "@/bindings/ListRow";
import type { LocationInfo } from "@/bindings/LocationInfo";

export type { Author } from "@/bindings/Author";
export type { ListRow } from "@/bindings/ListRow";
export type { LocationInfo } from "@/bindings/LocationInfo";
export type { ScanCode } from "@/bindings/ScanCode";
export type { BookDetail } from "@/bindings/BookDetail";
export type { FieldChange } from "@/bindings/FieldChange";
export type { FetchMetadataResult } from "@/bindings/FetchMetadataResult";
export type { MetaSecondTier } from "@/bindings/MetaSecondTier";
export type { NewBookDraft } from "@/bindings/NewBookDraft";

// 列挙値の型。ListRow の項目から取る（Rust 側の定義が正）。
export type Ownership = ListRow["ownership"];
export type WishPriority = NonNullable<ListRow["wishPriority"]>;
export type ReadStatus = ListRow["readStatus"];
export type MetaStatus = ListRow["metaStatus"];

/**
 * 保管場所（ツリーの 1 ノード）。画面のツリー・パスの組み立てが使う項目だけにした `LocationInfo` の部分集合
 * （`list_locations` の戻り値はそのまま渡せる。`path` と `depth` は、必要な画面が `LocationInfo` で受ける）。
 */
export type Location = Pick<LocationInfo, "id" | "name" | "parentId" | "sortOrder">;

/**
 * Cコード表（付録 A）の、名称を引く形。キーは桁の文字列で、値は名称。
 * `c_code_table()` の戻り値（bindings の `CCodeTable`。`{ code, name }` の配列）からは
 * `features/library/api.ts` の `toCCodeLookup` で変換する。
 * - audience: 1 桁目 "0"〜"9"（販売対象）
 * - format: 2 桁目 "0"〜"9"（発行形態）
 * - contentMajor: 3 桁目 "0"〜"9"（内容の大分類）
 * - content: 3〜4 桁目 "00"〜"99"（内容の中分類）
 */
export type CCodeTable = {
  audience: Record<string, string>;
  format: Record<string, string>;
  contentMajor: Record<string, string>;
  content: Record<string, string>;
};
