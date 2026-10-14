// サイドバーの絞り込み（FR-LIST-04）と件数の集計。DOM にも React にも依存しない純粋関数。
//
// - 絞り込みは「ライブラリ」「読書状況」「保管場所」「タグ」「自動分類」の 5 つのグループで、各グループは 0 か 1 つを選ぶ。
//   グループどうしは AND で掛け合わせる。
// - 件数は「そのグループ自身の選択を外して、ほかのグループの選択は掛けた」ときの冊数（ファセット件数）。
//   選んだ項目の隣にある項目に、切り替えたときの冊数が出る。
// - ゴミ箱以外のライブラリでは `deleted` の本を出さない。ゴミ箱では `deleted` の本だけを出す。
// - 検索結果（ID の集合）は、これらすべてに掛け合わせる。検索自体は Rust 側（IPC の search_books）。
import { UNSET } from "@/features/library/keys";
import { tagPrefixes, type LocationIndex } from "@/features/library/trees";
import type { ListRow, ReadStatus } from "@/types/contracts";

export type LibraryView = "all" | "owned" | "wanted" | "released" | "trash";

export const LIBRARY_VIEWS: readonly LibraryView[] = [
  "all",
  "owned",
  "wanted",
  "released",
  "trash",
];
export const READ_STATUSES: readonly ReadStatus[] = ["unread", "reading", "read", "paused"];

/** 自動分類の絞り込み。value が UNSET（空文字）のものは「その項目が空の本」。 */
export type AutoFilter =
  | { kind: "cMajor"; value: string } // 内容の大分類（Cコードの 3 桁目）
  | { kind: "cMinor"; value: string } // 内容の中分類（Cコードの 3〜4 桁目）
  | { kind: "format"; value: string } // 発行形態（Cコードの 2 桁目）
  | { kind: "publisher"; value: string }
  | { kind: "author"; value: string }
  | { kind: "year"; value: string } // 発行年（YYYY）
  | { kind: "meta"; value: "pending" | "incomplete" }; // 取得待ち / 情報不足（一部取得・取得失敗）

export interface BookFilter {
  library: LibraryView;
  readStatus: ReadStatus | null;
  /** 保管場所の id。その場所と子孫に置かれた本が対象。UNSET は「保管場所なし」。null は絞り込まない。 */
  locationId: string | null;
  /** タグのパス（"研究" なら "研究/SiC" なども対象）。UNSET は「タグなし」。null は絞り込まない。 */
  tag: string | null;
  auto: AutoFilter | null;
}

export const DEFAULT_FILTER: BookFilter = {
  library: "all",
  readStatus: null,
  locationId: null,
  tag: null,
  auto: null,
};

export interface FilterContext {
  /** 検索結果の ID の集合。null は検索していない（掛け合わせない）。 */
  searchIds: ReadonlySet<string> | null;
  locationIndex: LocationIndex;
}

export function isFilterActive(filter: BookFilter): boolean {
  return (
    filter.library !== "all" ||
    filter.readStatus !== null ||
    filter.locationId !== null ||
    filter.tag !== null ||
    filter.auto !== null
  );
}

// ===== 1 行ぶんの判定 =====

function matchesLibrary(row: ListRow, view: LibraryView): boolean {
  if (view === "trash") return row.deleted;
  if (row.deleted) return false;
  return view === "all" || row.ownership === view;
}

function matchesTag(row: ListRow, tag: string): boolean {
  if (tag === UNSET) return row.tags.length === 0;
  const prefix = `${tag}/`;
  return row.tags.some((t) => t === tag || t.startsWith(prefix));
}

/** Cコードが 4 桁でないものは「なし」として扱う。 */
function cCodeDigits(row: ListRow, from: number, to: number): string {
  const code = row.cCode;
  return code !== null && code.length === 4 ? code.slice(from, to) : UNSET;
}

function yearOf(row: ListRow): string {
  const on = row.publishedOn;
  if (on === null || on.length < 4) return UNSET;
  const year = on.slice(0, 4);
  return /^\d{4}$/.test(year) ? year : UNSET;
}

function matchesAuto(row: ListRow, auto: AutoFilter): boolean {
  switch (auto.kind) {
    case "cMajor":
      return cCodeDigits(row, 2, 3) === auto.value;
    case "cMinor":
      return cCodeDigits(row, 2, 4) === auto.value;
    case "format":
      return cCodeDigits(row, 1, 2) === auto.value;
    case "publisher":
      return (row.publisher || UNSET) === auto.value;
    case "author":
      return auto.value === UNSET
        ? row.authors.length === 0
        : row.authors.some((a) => a.name === auto.value);
    case "year":
      return yearOf(row) === auto.value;
    case "meta":
      return auto.value === "pending"
        ? row.metaStatus === "pending"
        : row.metaStatus === "partial" || row.metaStatus === "failed";
  }
}

/** 保管場所の絞り込みの判定を用意する（子孫の集合を 1 度だけ引く）。 */
function locationMatcher(
  filter: BookFilter,
  ctx: FilterContext,
): ((row: ListRow) => boolean) | null {
  const id = filter.locationId;
  if (id === null) return null;
  if (id === UNSET) return (row) => row.locationId === null;
  const set = ctx.locationIndex.descendantsOf(id);
  return (row) => row.locationId !== null && set.has(row.locationId);
}

// ===== 絞り込み =====

/** 表示する行を返す（元の並びを保つ）。 */
export function filterRows(
  rows: readonly ListRow[],
  filter: BookFilter,
  ctx: FilterContext,
): ListRow[] {
  const { searchIds } = ctx;
  const inLocation = locationMatcher(filter, ctx);
  const { library, readStatus, tag, auto } = filter;
  const result: ListRow[] = [];
  for (const row of rows) {
    if (!matchesLibrary(row, library)) continue;
    if (searchIds !== null && !searchIds.has(row.id)) continue;
    if (readStatus !== null && row.readStatus !== readStatus) continue;
    if (inLocation !== null && !inLocation(row)) continue;
    if (tag !== null && !matchesTag(row, tag)) continue;
    if (auto !== null && !matchesAuto(row, auto)) continue;
    result.push(row);
  }
  return result;
}

// ===== 件数の集計 =====

export interface AutoFacets {
  cMajor: Map<string, number>;
  cMinor: Map<string, number>;
  format: Map<string, number>;
  publisher: Map<string, number>;
  author: Map<string, number>;
  year: Map<string, number>;
  metaPending: number;
  metaIncomplete: number;
}

export interface Facets {
  /** 選んでいるライブラリの冊数（検索と、ほかの絞り込みを掛ける前）。「N 冊中」の N。 */
  total: number;
  /** 表示する冊数（絞り込みと検索を掛けた後）。「M 冊を表示」の M。 */
  visible: number;
  library: Record<LibraryView, number>;
  readStatus: Record<ReadStatus, number>;
  /** 保管場所 id（なしは UNSET）ごとの、その場所に直接置かれた冊数。子孫を足すのは buildLocationTree。 */
  locations: Map<string, number>;
  /** タグのパス（階層ごと。タグなしは UNSET）ごとの冊数。 */
  tags: Map<string, number>;
  auto: AutoFacets;
}

function increment(map: Map<string, number>, key: string) {
  map.set(key, (map.get(key) ?? 0) + 1);
}

export function emptyFacets(): Facets {
  return {
    total: 0,
    visible: 0,
    library: { all: 0, owned: 0, wanted: 0, released: 0, trash: 0 },
    readStatus: { unread: 0, reading: 0, read: 0, paused: 0 },
    locations: new Map(),
    tags: new Map(),
    auto: {
      cMajor: new Map(),
      cMinor: new Map(),
      format: new Map(),
      publisher: new Map(),
      author: new Map(),
      year: new Map(),
      metaPending: 0,
      metaIncomplete: 0,
    },
  };
}

/** サイドバーに出す件数と、ステータスバーの冊数を、1 回の走査で集計する。 */
export function computeFacets(
  rows: readonly ListRow[],
  filter: BookFilter,
  ctx: FilterContext,
): Facets {
  const facets = emptyFacets();
  const { searchIds } = ctx;
  const inLocation = locationMatcher(filter, ctx);
  const { library, readStatus, tag, auto } = filter;
  const a = facets.auto;

  for (const row of rows) {
    const libOk = matchesLibrary(row, library);
    if (libOk) facets.total++;
    if (searchIds !== null && !searchIds.has(row.id)) continue;

    const readOk = readStatus === null || row.readStatus === readStatus;
    const locOk = inLocation === null || inLocation(row);
    const tagOk = tag === null || matchesTag(row, tag);
    const autoOk = auto === null || matchesAuto(row, auto);

    // ライブラリの件数: ライブラリ以外の選択を掛ける
    if (readOk && locOk && tagOk && autoOk) {
      if (row.deleted) facets.library.trash++;
      else {
        facets.library.all++;
        facets.library[row.ownership]++;
      }
    }
    if (!libOk) continue;

    // 以降はライブラリを掛けたうえで、自分のグループ以外を掛ける
    if (locOk && tagOk && autoOk) facets.readStatus[row.readStatus]++;
    if (readOk && tagOk && autoOk) increment(facets.locations, row.locationId ?? UNSET);
    if (readOk && locOk && autoOk) {
      if (row.tags.length === 0) increment(facets.tags, UNSET);
      else if (row.tags.length === 1) {
        for (const path of tagPrefixes(row.tags[0] ?? "")) increment(facets.tags, path);
      } else {
        // 同じ本の複数のタグが同じ親を共有しても、親の件数は 1 冊として数える
        const paths = new Set<string>();
        for (const t of row.tags) for (const path of tagPrefixes(t)) paths.add(path);
        for (const path of paths) increment(facets.tags, path);
      }
    }
    if (readOk && locOk && tagOk) {
      increment(a.cMajor, cCodeDigits(row, 2, 3));
      increment(a.cMinor, cCodeDigits(row, 2, 4));
      increment(a.format, cCodeDigits(row, 1, 2));
      increment(a.publisher, row.publisher || UNSET);
      increment(a.year, yearOf(row));
      if (row.authors.length === 0) increment(a.author, UNSET);
      else if (row.authors.length === 1) increment(a.author, row.authors[0]?.name ?? UNSET);
      else {
        const names = new Set<string>();
        for (const author of row.authors) names.add(author.name);
        for (const name of names) increment(a.author, name);
      }
      if (row.metaStatus === "pending") a.metaPending++;
      else if (row.metaStatus === "partial" || row.metaStatus === "failed") a.metaIncomplete++;
    }
    if (readOk && locOk && tagOk && autoOk) facets.visible++;
  }
  return facets;
}
