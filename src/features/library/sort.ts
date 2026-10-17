// 一覧の並べ替え（FR-LIST-02）。複数の列の組み合わせに対応する純粋関数。
//
// - 書名は `sortTitle`、著者は `sortAuthors`（どちらも Rust が作る。ヨミがあればヨミ順になる）で比べる。
// - 文字列は UTF-16 のコード単位で比べる（Intl.Collator は 2 万行では遅いので使わない）。
//   かな・英数字は正規化済みの sortTitle で足りる。出版社など正規化のない列は、かなの五十音順に近い順になる。
// - 値のない行（null）は、昇順でも降順でも最後に置く。
// - すべての列が同じ値のときは、書名 → 巻次 → 元の並びの順で決める（シリーズが巻の順に並ぶ）。
import type { ListRow } from "@/types/contracts";

export interface SortSpec {
  /** 列 id（src/features/list/columns.tsx の ColumnId）。 */
  id: string;
  desc: boolean;
}

export interface SortContext {
  /** 保管場所 id → 「自宅 > 書斎」の形のパス。保管場所の列の並べ替えに使う。 */
  locationPaths: ReadonlyMap<string, string>;
}

/** 並べ替えのキー。null は「値なし」。 */
export type SortKey = string | number | null;
type Key = SortKey;

const READ_STATUS_ORDER: Record<ListRow["readStatus"], number> = {
  unread: 0,
  reading: 1,
  read: 2,
  paused: 3,
};
const OWNERSHIP_ORDER: Record<ListRow["ownership"], number> = {
  owned: 0,
  wanted: 1,
  released: 2,
};

function nonEmpty(value: string | null): string | null {
  return value === null || value === "" ? null : value;
}

function cCodeDigit(row: ListRow, from: number, to: number): string | null {
  return row.cCode !== null && row.cCode.length === 4 ? row.cCode.slice(from, to) : null;
}

/** 列 id ごとの、並べ替えに使うキー。キーのない列（書影など）は並べ替えできない。 */
const KEY_OF: Record<string, (row: ListRow, ctx: SortContext) => Key> = {
  title: (r) => r.sortTitle,
  volume: (r) => r.volumeSort,
  authors: (r) => nonEmpty(r.sortAuthors),
  publisher: (r) => nonEmpty(r.publisher),
  publishedOn: (r) => nonEmpty(r.publishedOn),
  cCode: (r) => r.cCode,
  location: (r, ctx) =>
    r.locationId === null ? null : (ctx.locationPaths.get(r.locationId) ?? null),
  readStatus: (r) => READ_STATUS_ORDER[r.readStatus],
  rating: (r) => r.rating,
  createdAt: (r) => r.createdAt,
  isbn13: (r) => r.isbn13,
  series: (r) => nonEmpty(r.seriesName),
  listPrice: (r) => r.listPrice,
  purchasedOn: (r) => r.purchasedOn,
  purchasePrice: (r) => r.purchasePrice,
  purchaseStore: (r) => nonEmpty(r.purchaseStore),
  ndc: (r) => r.ndc,
  format: (r) => cCodeDigit(r, 1, 2),
  audience: (r) => cCodeDigit(r, 0, 1),
  pages: (r) => r.pages,
  tags: (r) => (r.tags.length === 0 ? null : r.tags.join("/")),
  ownership: (r) => OWNERSHIP_ORDER[r.ownership],
  readFinishedOn: (r) => r.readFinishedOn,
};

/** この列 id は並べ替えできるか。 */
export function isSortable(columnId: string): boolean {
  return columnId in KEY_OF;
}

/** 並べ替えできる列の id（キーのない列は含まない）。 */
export const SORTABLE_COLUMN_IDS: readonly string[] = Object.keys(KEY_OF);

/**
 * 列のキー（並べ替えに使う値）。巻をまとめた行で「全部同じ値か」を調べるのにも使う。
 * 並べ替えできない列（書影など）は undefined。
 */
export function sortKeyOf(columnId: string, row: ListRow, ctx: SortContext): SortKey | undefined {
  return KEY_OF[columnId]?.(row, ctx);
}

/** 同じ列のキーは、どれも文字列か、どれも数値（null は呼び出し側で先に除く）。 */
function compareKeys(a: string | number, b: string | number): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** 並べ替えた新しい配列を返す。`rows` は変えない。並べ替えできない列の指定は無視する。 */
export function sortRows(
  rows: readonly ListRow[],
  sorting: readonly SortSpec[],
  ctx: SortContext,
): ListRow[] {
  const specs = sorting.filter((s) => isSortable(s.id));
  const n = rows.length;

  // 各行のキーを先に作っておく（比較のたびにプロパティを引かない）
  const keys: Key[][] = specs.map((spec) => {
    const keyOf = KEY_OF[spec.id];
    const list: Key[] = new Array<Key>(n);
    for (let i = 0; i < n; i++) list[i] = keyOf ? keyOf(rows[i] as ListRow, ctx) : null;
    return list;
  });
  const descending = specs.map((s) => s.desc);

  const order = new Array<number>(n);
  for (let i = 0; i < n; i++) order[i] = i;

  order.sort((x, y) => {
    for (let s = 0; s < keys.length; s++) {
      const column = keys[s] as Key[];
      const a = column[x] as Key;
      const b = column[y] as Key;
      if (a === b) continue;
      // null は向きによらず最後
      if (a === null) return 1;
      if (b === null) return -1;
      const c = compareKeys(a, b);
      if (c !== 0) return descending[s] ? -c : c;
    }
    // タイブレーク: 書名 → 巻次 → 元の並び
    const rx = rows[x] as ListRow;
    const ry = rows[y] as ListRow;
    if (rx.sortTitle !== ry.sortTitle) return rx.sortTitle < ry.sortTitle ? -1 : 1;
    const vx = rx.volumeSort;
    const vy = ry.volumeSort;
    if (vx !== vy) {
      if (vx === null) return 1;
      if (vy === null) return -1;
      return vx - vy;
    }
    return x - y;
  });

  const result = new Array<ListRow>(n);
  for (let i = 0; i < n; i++) result[i] = rows[order[i] as number] as ListRow;
  return result;
}
