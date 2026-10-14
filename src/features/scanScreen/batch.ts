// 連続登録（FR-SCAN-14／15）の一覧の、行のモデルと純粋な関数。React にも IPC にも触れない。
//
// - 行（BatchRow）は、読み取った本の ISBN・2段目と、状態に関わる最小限の情報だけを持つ。書誌の取得結果は ISBN ごとに
//   別に覚えておき（LookupOutcome）、行の状態と保存する下書きは、行と取得結果から導く。
// - 全冊共通の値（BatchPresets）は、保存するときに各行へ重ねる。
import type { BookDraft } from "@/bindings/BookDraft";
import type { FieldChange } from "@/bindings/FieldChange";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import type { SecondTier } from "@/features/scan/pairing";
import { applySecondTier } from "@/features/scanScreen/secondTier";
import { toNewBookDraft } from "@/features/library/drafts";
import type { Author, FetchMetadataResult, MetaStatus, Ownership } from "@/types/contracts";

/** ISBN ごとの書誌取得の結果。まだ取得中のものは持たない（取得中は結果がない）。 */
export type LookupOutcome =
  /** `fetch_metadata` が返した（オフラインのときの、取得待ちの下書きも含む）。 */
  | { status: "done"; result: FetchMetadataResult }
  /** `fetch_metadata` 自体が失敗した（通信の失敗は `done` の `offline` になるので、想定外のエラー）。 */
  | { status: "failed"; message: string };

/** 重複の理由。 */
export type RowDuplicate =
  /** 所有で登録済み（FR-SCAN-17）。 */
  | { kind: "owned"; title: string; locationPath: string | null }
  /** 同じ ISBN が、この一覧にすでにある。 */
  | { kind: "inBatch" };

export interface BatchRow {
  /** 行の識別子（一覧の中で一意。行の編集・削除・入れ替えに使う）。 */
  key: string;
  isbn13: string;
  isbn10: string | null;
  secondTier: SecondTier | null;
  /** まとめて保存の対象にするか。重複の行は、既定で対象から外す。 */
  included: boolean;
  duplicate: RowDuplicate | null;
  /** 行の編集（登録フォームの「OK」）で直した下書き。あれば、保存にはこれを使う。 */
  edited: NewBookDraft | null;
}

/** 行の状態（FR-SCAN-14）。 */
export type RowStatus = "loading" | "done" | "partial" | "failed" | "pending" | "duplicate";

export function createRow(
  key: string,
  book: { isbn13: string; isbn10: string | null; secondTier: SecondTier | null },
  duplicate: RowDuplicate | null,
): BatchRow {
  return {
    key,
    isbn13: book.isbn13,
    isbn10: book.isbn10,
    secondTier: book.secondTier,
    included: duplicate === null,
    duplicate,
    edited: null,
  };
}

/** `fetch_metadata` が失敗したときの下書き。ISBN と 2段目だけで、取得待ち（pending）として保存できる（FR-SCAN-16）。 */
export function fallbackDraft(isbn13: string, secondTier: SecondTier | null): BookDraft {
  const draft: BookDraft = {
    fields: {
      isbn13: { value: isbn13, origin: { kind: "barcode" } },
      // コアは取得状態に記録を付けない。origin に意味はない（0010 §6）
      meta_status: { value: "pending", origin: { kind: "provider", name: "bibcontainer" } },
    },
    seriesName: null,
  };
  return applySecondTier(draft, secondTier);
}

/** 行の下書き（取得結果に 2段目を重ねたもの）。取得中なら null。行を直していても、元の取得結果から作る。 */
export function rowDraft(row: BatchRow, lookup: LookupOutcome | undefined): BookDraft | null {
  if (lookup === undefined) return null;
  if (lookup.status === "failed") return fallbackDraft(row.isbn13, row.secondTier);
  return applySecondTier(lookup.result.draft, row.secondTier);
}

/** 取得を待っているか。直した行（`edited`）は、取得結果を待たない。 */
export function rowLoading(row: BatchRow, lookup: LookupOutcome | undefined): boolean {
  return row.edited === null && lookup === undefined;
}

/** 行の項目の値（項目名 → 値）。直した行はその内容、そうでなければ下書き。取得中なら null。 */
function rowValues(
  row: BatchRow,
  lookup: LookupOutcome | undefined,
): Readonly<Record<string, unknown>> | null {
  if (row.edited !== null) {
    return Object.fromEntries(row.edited.changes.map((change) => [change.field, change.value]));
  }
  const draft = rowDraft(row, lookup);
  if (draft === null) return null;
  return Object.fromEntries(
    Object.entries(draft.fields).map(([field, entry]) => [field, entry?.value]),
  );
}

function isMetaStatus(value: unknown): value is MetaStatus {
  return value === "ok" || value === "pending" || value === "partial" || value === "failed";
}

/** 行の状態。重複は取得の状態より優先して表示する。取得中は loading。 */
export function rowStatus(row: BatchRow, lookup: LookupOutcome | undefined): RowStatus {
  if (row.duplicate !== null) return "duplicate";
  const values = rowValues(row, lookup);
  if (values === null) return "loading";
  const status = values.meta_status;
  if (!isMetaStatus(status)) return "pending";
  switch (status) {
    case "ok":
      return "done";
    case "partial":
      return "partial";
    case "failed":
      return "failed";
    case "pending":
      return "pending";
  }
}

/** 一覧の 1 行に表示する内容。 */
export interface RowSummary {
  title: string | null;
  authors: string;
  publisher: string | null;
  cCode: string | null;
  listPrice: number | null;
  coverHash: string | null;
}

function textOf(value: unknown): string | null {
  return typeof value === "string" && value !== "" ? value : null;
}

function isAuthorList(value: unknown): value is Author[] {
  return Array.isArray(value);
}

export function rowSummary(row: BatchRow, lookup: LookupOutcome | undefined): RowSummary {
  const values = rowValues(row, lookup) ?? {};
  const authors = isAuthorList(values.authors)
    ? values.authors
        .map((author) => (typeof author?.name === "string" ? author.name : ""))
        .filter((name) => name !== "")
        .join("、")
    : "";
  return {
    title: textOf(values.title),
    authors,
    publisher: textOf(values.publisher),
    cCode: textOf(values.c_code),
    listPrice: typeof values.list_price === "number" ? values.list_price : null,
    // 取得して保存した書影は、下書きの cover_hash に入る
    coverHash: textOf(values.cover_hash),
  };
}

/** まとめて保存できるか。保存の対象が 1 冊以上あり、取得中の行がない（取得が終わるのを待つ）。 */
export function canSave(
  rows: readonly BatchRow[],
  lookups: Readonly<Record<string, LookupOutcome>>,
) {
  const included = rows.filter((row) => row.included);
  return included.length > 0 && included.every((row) => !rowLoading(row, lookups[row.isbn13]));
}

/** 取得を待っている、保存の対象の行の数。 */
export function loadingCount(
  rows: readonly BatchRow[],
  lookups: Readonly<Record<string, LookupOutcome>>,
): number {
  return rows.filter((row) => row.included && rowLoading(row, lookups[row.isbn13])).length;
}

// ---- 共通値 ----

/** 全冊に共通で入れる値（FR-SCAN-15）。 */
export interface BatchPresets {
  /** 保管場所の ID。 */
  locationId: string | null;
  tags: string[];
  ownership: Ownership | null;
  /** 購入日（YYYY-MM-DD）。空なら入れない。 */
  purchasedOn: string;
  purchaseStore: string;
}

export const EMPTY_PRESETS: BatchPresets = {
  locationId: null,
  tags: [],
  ownership: null,
  purchasedOn: "",
  purchaseStore: "",
};

const MANUAL = { kind: "manual" } as const;

/** 共通値を、`FieldChange`（由来は手入力）にする。入っていない項目は含めない。 */
export function presetChanges(presets: BatchPresets): FieldChange[] {
  const changes: FieldChange[] = [];
  if (presets.locationId !== null) {
    changes.push({ field: "location_id", value: presets.locationId, origin: MANUAL });
  }
  if (presets.tags.length > 0) {
    changes.push({ field: "tags", value: presets.tags, origin: MANUAL });
  }
  if (presets.ownership !== null) {
    changes.push({ field: "ownership", value: presets.ownership, origin: MANUAL });
  }
  if (presets.purchasedOn.trim() !== "") {
    changes.push({ field: "purchased_on", value: presets.purchasedOn.trim(), origin: MANUAL });
  }
  if (presets.purchaseStore.trim() !== "") {
    changes.push({ field: "purchase_store", value: presets.purchaseStore.trim(), origin: MANUAL });
  }
  return changes;
}

/** タグの入力欄の文字列を、タグの一覧にする（`,`・`、`・改行で区切る。前後の空白と重複は除く）。 */
export function parseTagInput(input: string): string[] {
  const tags: string[] = [];
  for (const part of input.split(/[,、\n]/)) {
    const tag = part.trim();
    if (tag !== "" && !tags.includes(tag)) tags.push(tag);
  }
  return tags;
}

/** 直した行に共通値を重ねる。直した内容を優先し、行にない項目だけを共通値で補う。 */
function fillPresets(draft: NewBookDraft, presets: readonly FieldChange[]): NewBookDraft {
  const present = new Set(draft.changes.map((change) => change.field));
  const missing = presets.filter((change) => !present.has(change.field));
  return missing.length === 0 ? draft : { ...draft, changes: [...draft.changes, ...missing] };
}

/**
 * 行から、`create_books_from_drafts` に渡す 1 冊分を作る。取得中の行は null。
 * - 直していない行: 取得結果の下書きに、共通値を重ねる（共通値が同じ項目の下書きの値を置き換える）。
 * - 直した行: 行の内容を優先し、行にない項目だけを共通値で補う（行ごとに直した値を共通値で消さない）。
 */
export function rowNewBookDraft(
  row: BatchRow,
  lookup: LookupOutcome | undefined,
  presets: readonly FieldChange[],
): NewBookDraft | null {
  if (row.edited !== null) return fillPresets(row.edited, presets);
  const draft = rowDraft(row, lookup);
  if (draft === null) return null;
  return toNewBookDraft(draft, presets);
}

/** 保存の対象の行すべてを、保存する形にする（取得中の行は含めない。呼ぶ前に `canSave` で確かめる）。 */
export function buildNewBookDrafts(
  rows: readonly BatchRow[],
  lookups: Readonly<Record<string, LookupOutcome>>,
  presets: BatchPresets,
): NewBookDraft[] {
  const changes = presetChanges(presets);
  return rows.flatMap((row) => {
    if (!row.included) return [];
    const draft = rowNewBookDraft(row, lookups[row.isbn13], changes);
    return draft === null ? [] : [draft];
  });
}
