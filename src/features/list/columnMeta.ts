// 一覧の列の定義のうち、描画に依存しない部分（FR-LIST-01）。ストアの既定値とサイドバー・メニューが使う。

/** 既定で表示する列（FR-LIST-01）。 */
export const DEFAULT_COLUMN_IDS = [
  "cover",
  "title",
  "volume",
  "authors",
  "publisher",
  "publishedOn",
  "cCode",
  "location",
  "readStatus",
  "rating",
  "createdAt",
] as const;

/** 選べる列（FR-LIST-01）。 */
export const OPTIONAL_COLUMN_IDS = [
  "isbn13",
  "series",
  "listPrice",
  "purchasedOn",
  "purchasePrice",
  "purchaseStore",
  "ndc",
  "format",
  "audience",
  "pages",
  "tags",
  "ownership",
  "readFinishedOn",
] as const;

export const COLUMN_IDS = [...DEFAULT_COLUMN_IDS, ...OPTIONAL_COLUMN_IDS] as const;

export type ColumnId = (typeof COLUMN_IDS)[number];

/** 隠せない列。書名がないと、どの本か分からなくなる。 */
export const FIXED_COLUMN_IDS: readonly ColumnId[] = ["title"];

/** 既定の列幅（px）。 */
export const DEFAULT_COLUMN_SIZE: Record<ColumnId, number> = {
  cover: 56,
  title: 320,
  volume: 64,
  authors: 180,
  publisher: 150,
  publishedOn: 110,
  cCode: 220,
  location: 200,
  readStatus: 90,
  rating: 100,
  createdAt: 110,
  isbn13: 140,
  series: 160,
  listPrice: 100,
  purchasedOn: 110,
  purchasePrice: 100,
  purchaseStore: 120,
  ndc: 80,
  format: 100,
  audience: 130,
  pages: 80,
  tags: 200,
  ownership: 90,
  readFinishedOn: 110,
};

/** 数値として右寄せにする列。 */
export const NUMERIC_COLUMN_IDS: ReadonlySet<ColumnId> = new Set<ColumnId>([
  "volume",
  "listPrice",
  "purchasePrice",
  "pages",
]);

export const MIN_COLUMN_SIZE = 40;
export const MAX_COLUMN_SIZE = 800;

/** 列の表示・非表示の既定値（TanStack Table の columnVisibility の形。選べる列は隠しておく）。 */
export function defaultColumnVisibility(): Record<string, boolean> {
  const visibility: Record<string, boolean> = {};
  for (const id of OPTIONAL_COLUMN_IDS) visibility[id] = false;
  return visibility;
}

const isColumnId = (id: string): id is ColumnId => (COLUMN_IDS as readonly string[]).includes(id);

/**
 * 保存された列の並び順（空なら既定の順）から、全列の並びを作る（FR-LIST-01）。
 * 知らない列と重複は捨てる。載っていない列（あとから増えた列など）は、既定の順のまま末尾に足す。
 */
export function resolveColumnOrder(saved: readonly string[]): ColumnId[] {
  const order = new Set<ColumnId>();
  for (const id of saved) {
    if (isColumnId(id)) order.add(id);
  }
  for (const id of COLUMN_IDS) order.add(id);
  return [...order];
}

/** 保存する並び順を整える（知らない列と重複を捨てる。載っていない列はそのまま。表示するときに `resolveColumnOrder` が末尾に足す）。 */
export function sanitizeColumnOrder(order: readonly string[]): ColumnId[] {
  return [...new Set(order.filter(isColumnId))];
}

/** `id` の列を 1 つ上（左）・下（右）へ動かした、全列の並び。端の列を端の外へは動かせない（変わらない）。 */
export function moveColumnId(
  order: readonly string[],
  id: ColumnId,
  direction: "up" | "down",
): ColumnId[] {
  const next = resolveColumnOrder(order);
  const from = next.indexOf(id);
  const to = direction === "up" ? from - 1 : from + 1;
  const other = next[to];
  if (from < 0 || other === undefined) return next;
  next[to] = id;
  next[from] = other;
  return next;
}
