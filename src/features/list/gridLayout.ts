// グリッド表示（FR-LIST-07）の並べ方と、キーボードの移動。React にも DOM にも依存しない純粋関数。
//
// グリッドは、本の ID（キー）の並びを、1 行に `columns` 個ずつの行に区切って仮想スクロールに渡す。
// 矢印キーの移動は「いまの行の何列目か」で決める。行をまたぐ左右はキーの並びどおり、上下は同じ列へ動く
// （行き先の行が短ければ、その行の最後のキー）。
// 巻をまとめた表示（FR-LIST-08）では、展開したグループの本に「band」の印（グループの key）を付ける。band の本は、
// 新しい行から始まって band の行に並び、band が終わったら、次のキーは新しい行から始まる。移動は band の行も同じ規則。

/** グリッドの 1 行。 */
export interface GridRow {
  /** この行に並ぶ本のキー（左から順）。 */
  keys: string[];
  /** 展開した巻のグループの本を並べる行か（FR-LIST-08）。そうでない行には付けない。 */
  band?: boolean;
  /** `band` の行の、グループの key。続く band の行が、同じグループのものかを見分ける。 */
  bandId?: string;
}

/** 並べるキーと、band の印（展開したグループの本なら、そのグループの key。そうでなければ付けない）。 */
export interface GridItem {
  key: string;
  band?: string | null;
}

/** キーの位置（何行目の何列目か。どちらも 0 始まり）。 */
export interface GridPosition {
  row: number;
  col: number;
}

export type GridDirection =
  "left" | "right" | "up" | "down" | "home" | "end" | "pageUp" | "pageDown";

/**
 * 1 行に並べられるカードの数（1 以上）。両端の余白（`padding`）を除いた幅に、カード（`cardWidth`）と
 * 間隔（`gap`）が何組入るか。数はどれも同じ単位（px）で渡す。
 */
export function gridColumnCount(
  containerWidth: number,
  cardWidth: number,
  gap: number,
  padding: number,
): number {
  if (![containerWidth, cardWidth, gap, padding].every(Number.isFinite) || cardWidth <= 0) return 1;
  const usable = containerWidth - 2 * padding;
  // n 個並べたときの幅は n × cardWidth + (n − 1) × gap。これが usable に収まる最大の n
  return Math.max(1, Math.floor((usable + gap) / (cardWidth + gap)));
}

/**
 * キーの並びを、1 行に `columns` 個ずつの行に区切る（順序は変えない）。`columns` は 1 以上に直す。
 * 要素は、キーの文字列か、band の印つきの `GridItem`。band の印が前のキーと変わるところ（始まりと終わり）では、行がまだ空いていても
 * 新しい行にする。band の印のあるキーだけの行は、`band: true`・`bandId` を付ける。
 */
export function layoutGrid(items: readonly (string | GridItem)[], columns: number): GridRow[] {
  const width = Number.isFinite(columns) ? Math.max(1, Math.floor(columns)) : 1;
  const rows: GridRow[] = [];
  let current: GridRow | null = null;
  let currentBand: string | null = null;
  for (const item of items) {
    const key = typeof item === "string" ? item : item.key;
    const band = typeof item === "string" ? null : (item.band ?? null);
    if (current === null || current.keys.length >= width || band !== currentBand) {
      current = band === null ? { keys: [] } : { keys: [], band: true, bandId: band };
      currentBand = band;
      rows.push(current);
    }
    current.keys.push(key);
  }
  return rows;
}

/** キー → 位置の対応。 */
export function gridIndex(rows: readonly GridRow[]): Map<string, GridPosition> {
  const index = new Map<string, GridPosition>();
  rows.forEach((row, rowIndex) => {
    row.keys.forEach((key, col) => index.set(key, { row: rowIndex, col }));
  });
  return index;
}

/**
 * 矢印キーなどで動かした先のキー。何も動かせないとき（端）は、いまのキーのまま。キーが 1 つもなければ null。
 *
 * - `left` / `right`: キーの並びで 1 つ前・後ろ（行をまたぐ）。
 * - `up` / `down`: 前・次の行の、同じ列のキー。その行が短ければ、その行の最後のキー。
 * - `home` / `end`: 最初・最後のキー。
 * - `pageUp` / `pageDown`: `pageRows` 行ぶん動く（先頭・末尾の行で止める。列は上下と同じ）。
 * - `focusKey` が null（まだどれも選んでいない）、または並びにないときは、`home` / `end` 以外はどれも最初のキー。
 *
 * `index` は `gridIndex(rows)` の結果。毎回作り直したくないときに渡す（なければこの中で探す）。
 */
export function gridMove(
  rows: readonly GridRow[],
  focusKey: string | null,
  direction: GridDirection,
  pageRows: number,
  index?: ReadonlyMap<string, GridPosition>,
): string | null {
  const first = rows[0]?.keys[0];
  if (first === undefined) return null;
  if (direction === "home") return first;
  if (direction === "end") {
    const lastRow = rows[rows.length - 1];
    return lastRow?.keys[lastRow.keys.length - 1] ?? first;
  }

  const position = focusKey === null ? undefined : locate(rows, focusKey, index);
  if (focusKey === null || position === undefined) return first;

  const pages = Math.max(1, Math.floor(Number.isFinite(pageRows) ? pageRows : 1));
  switch (direction) {
    case "left":
    case "right": {
      const step = direction === "left" ? -1 : 1;
      const row = rows[position.row];
      if (row === undefined) return focusKey;
      const col = position.col + step;
      if (col >= 0 && col < row.keys.length) return row.keys[col] ?? focusKey;
      // 行の端を越えたら、前の行の最後・次の行の最初へ
      const nextRow = rows[position.row + step];
      if (nextRow === undefined) return focusKey;
      return (step < 0 ? nextRow.keys[nextRow.keys.length - 1] : nextRow.keys[0]) ?? focusKey;
    }
    case "up":
      return inRow(rows, position.row - 1, position.col, focusKey);
    case "down":
      return inRow(rows, position.row + 1, position.col, focusKey);
    case "pageUp":
      return inRow(rows, position.row - pages, position.col, focusKey);
    case "pageDown":
      return inRow(rows, position.row + pages, position.col, focusKey);
  }
}

function locate(
  rows: readonly GridRow[],
  key: string,
  index: ReadonlyMap<string, GridPosition> | undefined,
): GridPosition | undefined {
  if (index !== undefined) return index.get(key);
  for (let row = 0; row < rows.length; row++) {
    const col = rows[row]?.keys.indexOf(key) ?? -1;
    if (col >= 0) return { row, col };
  }
  return undefined;
}

/** `rowIndex` 行目（範囲の外なら先頭・末尾の行）の `col` 列目のキー。その行が短ければ最後のキー。 */
function inRow(rows: readonly GridRow[], rowIndex: number, col: number, fallback: string): string {
  const row = rows[Math.min(rows.length - 1, Math.max(0, rowIndex))];
  if (row === undefined || row.keys.length === 0) return fallback;
  return row.keys[Math.min(col, row.keys.length - 1)] ?? fallback;
}
