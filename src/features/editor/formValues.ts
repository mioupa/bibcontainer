// 登録・編集フォームの値（画面の入力の形）と、Op 上の値（コアに渡す形）の変換。
//
// フォームの値は、入力欄の都合で文字列が中心（金額・ページ数も文字列）。コアに渡す値は `opValue` が作る。
// 「変えた項目だけを送る」（edit）と「下書きとの差分を取る」（new）は、どちらも `opValue` の結果を比べる。
// 前後の空白、全角の数字、ISBN のハイフンなど、コアが保存前に直す書式の違いは、ここで先に直して比べる
// （直す前の値で比べると、書式の違いだけで「変えた」ことになってしまうため）。
import type { MetaField } from "@/bindings/MetaField";
import { compactIsbn, isbn10ToIsbn13 } from "@/lib/isbn";
import type { Author, BookDetail, Ownership, ReadStatus, WishPriority } from "@/types/contracts";

export interface AuthorRow {
  name: string;
  kana: string;
  role: string;
}

export interface BookFormValues {
  // 基本
  title: string;
  titleKana: string;
  subtitle: string;
  authors: AuthorRow[];
  publisher: string;
  publishedOn: string;
  seriesName: string;
  volume: string;
  isbn13: string;
  cCode: string;
  listPrice: string;
  pages: string;
  sizeText: string;
  ndc: string;
  language: string;
  description: string;
  coverHash: string | null;
  // 分類・保管
  locationId: string | null;
  tags: string[];
  ownership: Ownership;
  wishPriority: WishPriority | null;
  // 読書
  readStatus: ReadStatus;
  readStartedOn: string;
  readFinishedOn: string;
  rating: number | null;
  review: string;
  // 購入
  purchasedOn: string;
  purchaseStore: string;
  purchasePrice: string;
  purchaseCond: "new" | "used" | null;
  purchaseMemo: string;
  // メモ
  memo: string;
}

export type FormKey = keyof BookFormValues;

export type EditorTab = "basic" | "classification" | "reading" | "purchase" | "memo";
export const EDITOR_TABS: readonly EditorTab[] = [
  "basic",
  "classification",
  "reading",
  "purchase",
  "memo",
];

export interface FieldDef {
  key: FormKey;
  /** Op 上の項目名（`update_book` の `field`）。シリーズは `series_id`。 */
  op: string;
  /** 取得元の候補（`MetaMerged.fields`）の項目。候補がない項目は省く。 */
  meta?: MetaField;
  tab: EditorTab;
}

/** フォームの全項目。並びは画面の順。 */
export const FIELDS: readonly FieldDef[] = [
  { key: "title", op: "title", meta: "title", tab: "basic" },
  { key: "titleKana", op: "title_kana", meta: "title_kana", tab: "basic" },
  { key: "subtitle", op: "subtitle", meta: "subtitle", tab: "basic" },
  { key: "authors", op: "authors", meta: "authors", tab: "basic" },
  { key: "publisher", op: "publisher", meta: "publisher", tab: "basic" },
  { key: "publishedOn", op: "published_on", meta: "published_on", tab: "basic" },
  { key: "seriesName", op: "series_id", meta: "series", tab: "basic" },
  { key: "volume", op: "volume", meta: "volume", tab: "basic" },
  { key: "isbn13", op: "isbn13", tab: "basic" },
  { key: "cCode", op: "c_code", meta: "c_code", tab: "basic" },
  { key: "listPrice", op: "list_price", meta: "list_price", tab: "basic" },
  { key: "pages", op: "pages", meta: "pages", tab: "basic" },
  { key: "sizeText", op: "size_text", meta: "size_text", tab: "basic" },
  { key: "ndc", op: "ndc", meta: "ndc", tab: "basic" },
  { key: "language", op: "language", meta: "language", tab: "basic" },
  { key: "description", op: "description", meta: "description", tab: "basic" },
  { key: "coverHash", op: "cover_hash", meta: "cover", tab: "basic" },
  { key: "locationId", op: "location_id", tab: "classification" },
  { key: "tags", op: "tags", tab: "classification" },
  { key: "ownership", op: "ownership", tab: "classification" },
  { key: "wishPriority", op: "wish_priority", tab: "classification" },
  { key: "readStatus", op: "read_status", tab: "reading" },
  { key: "readStartedOn", op: "read_started_on", tab: "reading" },
  { key: "readFinishedOn", op: "read_finished_on", tab: "reading" },
  { key: "rating", op: "rating", tab: "reading" },
  { key: "review", op: "review", tab: "reading" },
  { key: "purchasedOn", op: "purchased_on", tab: "purchase" },
  { key: "purchaseStore", op: "purchase_store", tab: "purchase" },
  { key: "purchasePrice", op: "purchase_price", tab: "purchase" },
  { key: "purchaseCond", op: "purchase_cond", tab: "purchase" },
  { key: "purchaseMemo", op: "purchase_memo", tab: "purchase" },
  { key: "memo", op: "memo", tab: "memo" },
];

const DEF_BY_KEY = new Map(FIELDS.map((def) => [def.key, def]));
const DEF_BY_OP = new Map(FIELDS.map((def) => [def.op, def]));

export function fieldDef(key: FormKey): FieldDef {
  const def = DEF_BY_KEY.get(key);
  if (!def) throw new Error(`フォームにない項目です: ${key}`);
  return def;
}

/** Op 上の項目名から、フォームの項目を引く（サーバーの検証エラーを項目に結びつける）。 */
export function fieldDefByOp(op: string): FieldDef | undefined {
  return DEF_BY_OP.get(op);
}

export function emptyFormValues(): BookFormValues {
  return {
    title: "",
    titleKana: "",
    subtitle: "",
    authors: [],
    publisher: "",
    publishedOn: "",
    seriesName: "",
    volume: "",
    isbn13: "",
    cCode: "",
    listPrice: "",
    pages: "",
    sizeText: "",
    ndc: "",
    language: "",
    description: "",
    coverHash: null,
    locationId: null,
    tags: [],
    ownership: "owned",
    wishPriority: null,
    readStatus: "unread",
    readStartedOn: "",
    readFinishedOn: "",
    rating: null,
    review: "",
    purchasedOn: "",
    purchaseStore: "",
    purchasePrice: "",
    purchaseCond: null,
    purchaseMemo: "",
    memo: "",
  };
}

// ===== 入力欄の文字列 → Op の値 =====

/** 前後の空白を除いた文字列。空なら null。 */
function nullableText(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** 複数行の文字列。末尾の空白・改行を除く。空白だけなら null。 */
function nullableMultiline(value: string): string | null {
  return value.trim() === "" ? null : value.trimEnd();
}

/** 全角を半角にして前後の空白を除く。空なら null（日付、Cコード）。 */
function nullableNfkc(value: string): string | null {
  return nullableText(value.normalize("NFKC"));
}

/** 金額・ページ数の入力。全角を半角にし、桁区切りのカンマ・空白・「円」を除く。空なら null。数字以外が残れば元の文字列。 */
export function parseIntegerInput(value: string): number | string | null {
  const compact = value.normalize("NFKC").replace(/[\s,，円]/g, "");
  if (compact === "") return null;
  return /^\d+$/.test(compact) ? Number(compact) : compact;
}

/** ISBN の入力。ハイフン・空白・全角を直す。ISBN-10 は ISBN-13 にする。空なら null。 */
export function parseIsbnInput(value: string): string | null {
  const compact = compactIsbn(value);
  if (compact === "") return null;
  return isbn10ToIsbn13(compact) ?? compact;
}

/** 空の行を除き、名前・ヨミ・役割を整えた著者の一覧。 */
export function normalizeAuthors(rows: readonly AuthorRow[]): Author[] {
  return rows
    .filter((row) => row.name.trim() !== "" || row.kana.trim() !== "" || row.role.trim() !== "")
    .map((row) => ({
      name: row.name.trim(),
      kana: nullableText(row.kana),
      role: nullableText(row.role),
    }));
}

/** タグ名の一覧。前後の空白を除き、空と重複を除く（順序は保つ）。 */
export function normalizeTags(tags: readonly string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const tag of tags) {
    const trimmed = tag.trim();
    if (trimmed !== "" && !seen.has(trimmed)) {
      seen.add(trimmed);
      result.push(trimmed);
    }
  }
  return result;
}

/** フォームの 1 項目を、コアに渡す値（Op の値）にする。シリーズは名前（ID に直すのは保存のとき）。 */
export function opValue(key: FormKey, values: BookFormValues): unknown {
  switch (key) {
    case "title":
      return values.title.trim();
    case "titleKana":
    case "subtitle":
    case "publisher":
    case "volume":
    case "ndc":
    case "sizeText":
    case "language":
    case "purchaseStore":
    case "seriesName":
      return nullableText(values[key]);
    case "description":
    case "review":
    case "purchaseMemo":
    case "memo":
      return nullableMultiline(values[key]);
    case "publishedOn":
    case "readStartedOn":
    case "readFinishedOn":
    case "purchasedOn":
    case "cCode":
      return nullableNfkc(values[key]);
    case "isbn13":
      return parseIsbnInput(values.isbn13);
    case "listPrice":
    case "pages":
    case "purchasePrice":
      return parseIntegerInput(values[key]);
    case "authors":
      return normalizeAuthors(values.authors);
    case "tags":
      return normalizeTags(values.tags);
    case "coverHash":
    case "locationId":
    case "ownership":
    case "wishPriority":
    case "readStatus":
    case "rating":
    case "purchaseCond":
      return values[key];
  }
}

/** 2 つの Op の値が同じか。 */
export function sameOpValue(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** 指定した項目の値が、2 つのフォームの値で違うか（書式の違いは無視する）。 */
export function isFieldChanged(key: FormKey, initial: BookFormValues, current: BookFormValues) {
  return !sameOpValue(opValue(key, initial), opValue(key, current));
}

// ===== Op の値 → 入力欄の値 =====

const asString = (value: unknown): string => (typeof value === "string" ? value : "");
const asNumberText = (value: unknown): string =>
  typeof value === "number" && Number.isFinite(value) ? String(value) : "";

function authorRows(value: unknown): AuthorRow[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item: unknown) => {
    if (typeof item !== "object" || item === null) return [];
    const author = item as Partial<Record<"name" | "kana" | "role", unknown>>;
    return [
      { name: asString(author.name), kana: asString(author.kana), role: asString(author.role) },
    ];
  });
}

/**
 * Op の値（`FieldChange.value`）を、フォームの項目の値にして `values` に入れる。
 * 型の合わない値（壊れた下書きなど）は、その項目を空にする。シリーズ（`series_id`）はここでは扱わない。
 */
export function applyOpValue(values: BookFormValues, op: string, value: unknown): void {
  const def = fieldDefByOp(op);
  if (!def || def.key === "seriesName") return;
  switch (def.key) {
    case "title":
    case "titleKana":
    case "subtitle":
    case "publisher":
    case "volume":
    case "ndc":
    case "sizeText":
    case "language":
    case "purchaseStore":
    case "description":
    case "review":
    case "purchaseMemo":
    case "memo":
    case "publishedOn":
    case "readStartedOn":
    case "readFinishedOn":
    case "purchasedOn":
    case "cCode":
    case "isbn13":
      values[def.key] = asString(value);
      break;
    case "listPrice":
    case "pages":
    case "purchasePrice":
      values[def.key] = asNumberText(value);
      break;
    case "authors":
      values.authors = authorRows(value);
      break;
    case "tags":
      values.tags = Array.isArray(value)
        ? value.filter((tag): tag is string => typeof tag === "string")
        : [];
      break;
    case "coverHash":
      values.coverHash = typeof value === "string" ? value : null;
      break;
    case "locationId":
      values.locationId = typeof value === "string" ? value : null;
      break;
    case "ownership":
      if (value === "owned" || value === "wanted" || value === "released") values.ownership = value;
      break;
    case "readStatus":
      if (value === "unread" || value === "reading" || value === "read" || value === "paused") {
        values.readStatus = value;
      }
      break;
    case "wishPriority":
      values.wishPriority = value === "high" || value === "mid" || value === "low" ? value : null;
      break;
    case "purchaseCond":
      values.purchaseCond = value === "new" || value === "used" ? value : null;
      break;
    case "rating":
      values.rating = typeof value === "number" && value >= 1 && value <= 5 ? value : null;
      break;
  }
}

/** 登録済みの本（`get_book_detail`）から、編集フォームの初期値を作る。 */
export function valuesFromDetail(detail: BookDetail, seriesName: string | null): BookFormValues {
  const values = emptyFormValues();
  values.seriesName = seriesName ?? "";
  for (const def of FIELDS) {
    if (def.key === "seriesName") continue;
    applyOpValue(values, def.op, (detail as unknown as Record<string, unknown>)[def.key]);
  }
  return values;
}
