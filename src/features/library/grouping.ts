// 巻をまとめた表示（FR-LIST-08）のための、グループ化。React に依存しない純粋関数。
//
// 絞り込み・並べ替え済みの行（`view.rows`）を受け取り、同じ作品の巻を 1 つのグループにして、表とグリッドが共通で使う
// 「並び」（`GroupedList`）を作る。
//
// 同じ作品とみなす規則（作品名の比べ方・著者の比べ方は works.ts）:
// - 作品名の比べ方（`titleKey`）が同じ本を、1 つの束にする。
// - 束の中で、著者の比べ方が 1 つでも同じ本は、同じ作品（推移的。A と B、B と C が同じなら A・B・C）。
// - 著者のない本: 束の中の「著者のある作品」がちょうど 1 つなら、そこに入れる。著者のある本が 1 冊もなければ、著者のない本どうしで 1 つの作品。
//   著者のある作品が 2 つ以上あれば、どれに入るか決められないので、単独のまま。
// - 2 冊以上ある作品だけをグループにする（いまの絞り込みの中で数える。1 冊だけなら、ふつうの行）。
//
// 本ごとの手動の指定（`ListRow.workId`。0026）は、自動の規則より先に使う。
// - `"none"` の本は、どの作品にも入れない（ふつうの行。束にも入れない）。
// - `"none"` でも null でもない本は、同じ ID どうしが 1 つの作品（書名や著者が違ってもよい。違う ID は、書名・著者が同じでも別の作品）。
//   自動の規則の束には入れない。
// - null の本は、上の自動の規則で作品に分ける（1 冊だけのものも）。その作品が、手動の作品の本と、作品名の比べ方が同じで
//   著者が 1 人でも共通するなら、その手動の作品に入る（著者のない本は「つながり」を作らないが、作品の中にいれば一緒に入る）。
//   つながる手動の作品が 2 つ以上なら、どれにも入らない（自動の作品のまま）。
// 「巻をまとめる」がオフのときは、`workId` は使わない。
//
// 2 段に分けてある。`buildWorkLayout`（行 → 作品のまとまり。展開の状態に依らない）と、`layoutEntries`（展開の状態 → 並び）。
// 展開の状態だけが変わったときは、後者だけをやり直せばよい（グループの計算は使い回す）。`groupRows` は 2 つを続けて呼ぶ。
import { SORTABLE_COLUMN_IDS, sortKeyOf, type SortContext } from "@/features/library/sort";
import {
  WORK_NONE,
  volumeLabel,
  volumeOrder,
  workInfo,
  type WorkInfo,
} from "@/features/library/works";
import { ja } from "@/i18n/ja";
import type { ListRow } from "@/types/contracts";

/** グループの key の先頭。本の ID（UUID）とは決して重ならない。 */
export const GROUP_KEY_PREFIX = "work:";
const KEY_SEPARATOR = "\u001f";

/** 手動の作品のグループの key。自動の key は作品名の比べ方（空でない）から始まるので、重ならない。 */
export function manualGroupKey(workId: string): string {
  return `${GROUP_KEY_PREFIX}${KEY_SEPARATOR}${workId}`;
}

/** グループの key か（本の ID ではないか）。 */
export function isGroupKey(key: string | null | undefined): key is string {
  return typeof key === "string" && key.startsWith(GROUP_KEY_PREFIX);
}

/** 同じ作品の巻のまとまり。 */
export interface WorkGroup {
  /**
   * 自動の作品は `work:` + 作品名の比べ方 + 区切り + 著者の比べ方のうち最小のもの（なければ空）。
   * 手動の作品は `work:` + 区切り + 手動の作品の ID（`manualGroupKey`）。
   */
  key: string;
  /** 作品名（巻の表記を除いた書名。巻の順で先頭の本のもの）。 */
  baseTitle: string;
  /** 巻の順（`volumeSort`、なければ書名から読み取った番号が小さい順。値のない本は最後。同じなら元の並び）。 */
  members: ListRow[];
  /** 巻の表示（巻次、なければ書名から読み取った番号）をもつ、最初と最後の本のもの。どれももたなければ null。 */
  firstVolume: string | null;
  lastVolume: string | null;
}

/** 一覧の 1 冊の行（カード）。`level` が 2 なら、展開したグループの中の本。 */
export interface BookEntry {
  kind: "book";
  /** 本の ID。 */
  key: string;
  row: ListRow;
  level: 1 | 2;
  /** `level` が 2 のとき、属するグループの key。 */
  groupKey: string | null;
}

/** 一覧のグループの行（カード）。 */
export interface GroupEntry {
  kind: "group";
  /** グループの key。 */
  key: string;
  group: WorkGroup;
  expanded: boolean;
}

export type ListEntry = BookEntry | GroupEntry;

/** 表・グリッドが表示する並び。選択は、この `entries` の key の上で行う。 */
export interface GroupedList {
  /** 巻をまとめる設定がオンか。オフのときは、すべて `level: 1` の本の行で、グループはない。 */
  enabled: boolean;
  /** 表示する行。グループ、展開したグループの中の本（`level: 2`）、ふつうの本の順。 */
  entries: ListEntry[];
  /** グループの key → グループ。 */
  groups: ReadonlyMap<string, WorkGroup>;
  /** グループに入っている本の ID → グループの key。 */
  groupOfBook: ReadonlyMap<string, string>;
  /** すべての本の ID（表示の順。グループの中は巻の順。展開しているかによらない）。 */
  bookIds: string[];
  /** 本の ID → 行。 */
  rowById: ReadonlyMap<string, ListRow>;
  /** 展開しているグループの key（`entries` を作ったときの状態）。 */
  expanded: ReadonlySet<string>;
}

type Unit =
  | { kind: "book"; entry: BookEntry }
  | { kind: "group"; collapsed: GroupEntry; expanded: GroupEntry; members: BookEntry[] };

/** 行 → 作品のまとまり。展開の状態に依らない（`layoutEntries` で並びにする）。 */
export interface WorkLayout {
  enabled: boolean;
  units: Unit[];
  groups: ReadonlyMap<string, WorkGroup>;
  groupOfBook: ReadonlyMap<string, string>;
  bookIds: string[];
  rowById: ReadonlyMap<string, ListRow>;
}

const EMPTY_GROUPS: ReadonlyMap<string, WorkGroup> = new Map();
const EMPTY_GROUP_OF_BOOK: ReadonlyMap<string, string> = new Map();

/** 本の行を、巻のグループにまとめる（`enabled` が false なら、まとめない）。展開の状態は、あとで `layoutEntries` に渡す。 */
export function buildWorkLayout(rows: readonly ListRow[], enabled: boolean): WorkLayout {
  const n = rows.length;
  const units: Unit[] = [];
  const bookIds: string[] = [];
  const rowById = new Map<string, ListRow>();

  if (!enabled) {
    for (const row of rows) {
      units.push({ kind: "book", entry: plainEntry(row) });
      bookIds.push(row.id);
      rowById.set(row.id, row);
    }
    return {
      enabled,
      units,
      groups: EMPTY_GROUPS,
      groupOfBook: EMPTY_GROUP_OF_BOOK,
      bookIds,
      rowById,
    };
  }

  // 1. 手動の指定のない本は、作品名の比べ方ごとの束にする（行の番号）。手動の作品の本は、作品の ID ごとに集める。
  //    "none" の本は、どちらにも入れない
  const infos: WorkInfo[] = new Array<WorkInfo>(n);
  const buckets = new Map<string, number[]>();
  let manualRows: Map<string, number[]> | null = null;
  for (let i = 0; i < n; i++) {
    const row = rows[i] as ListRow;
    const workId = row.workId;
    if (workId === WORK_NONE) continue;
    const info = workInfo(row);
    infos[i] = info;
    if (workId !== null) {
      manualRows ??= new Map();
      const members = manualRows.get(workId);
      if (members === undefined) manualRows.set(workId, [i]);
      else members.push(i);
      continue;
    }
    if (info.titleKey === "") continue;
    const bucket = buckets.get(info.titleKey);
    if (bucket === undefined) buckets.set(info.titleKey, [i]);
    else bucket.push(i);
  }

  // 手動の作品の本の、作品名の比べ方 → 作品の ID → 著者の比べ方（自動の作品が、どの手動の作品とつながるかを調べる）。
  // 作品名か著者のない本は、つながりを作らない
  const manualIndex = manualRows === null ? null : indexManualWorks(manualRows, infos);

  // 2. 束の中を、著者でつないで作品に分ける。2 冊以上の作品がグループ。手動の作品があるときは、1 冊だけの作品も調べ、
  //    つながる手動の作品がちょうど 1 つなら、その作品に入れる
  const groupOfRow = new Int32Array(n).fill(-1);
  const memberIndexes: number[][] = [];
  const groupKeys: string[] = [];
  for (const [titleKey, bucket] of buckets) {
    const linked = manualIndex?.get(titleKey);
    if (bucket.length < 2 && linked === undefined) continue;
    for (const work of splitWorks(bucket, infos)) {
      if (linked !== undefined) {
        const workId = soleLinkedWork(work, infos, linked);
        if (workId !== null) {
          const members = (manualRows as Map<string, number[]>).get(workId);
          if (members !== undefined) for (const index of work) members.push(index);
          continue;
        }
      }
      if (work.length < 2) continue;
      const groupIndex = memberIndexes.length;
      let smallestAuthor = "";
      for (const index of work) {
        groupOfRow[index] = groupIndex;
        for (const key of (infos[index] as WorkInfo).authorKeys) {
          if (smallestAuthor === "" || key < smallestAuthor) smallestAuthor = key;
        }
      }
      memberIndexes.push(work);
      groupKeys.push(`${GROUP_KEY_PREFIX}${titleKey}${KEY_SEPARATOR}${smallestAuthor}`);
    }
  }
  // 手動の作品（自動の作品が入ったものを含む）。表示中に 2 冊以上あるものだけがグループ
  if (manualRows !== null) {
    for (const [workId, members] of manualRows) {
      if (members.length < 2) continue;
      const groupIndex = memberIndexes.length;
      for (const index of members) groupOfRow[index] = groupIndex;
      memberIndexes.push(members);
      groupKeys.push(manualGroupKey(workId));
    }
  }

  // 3. グループの本を巻の順に並べ、グループを作る
  const workGroups: WorkGroup[] = memberIndexes.map((indexes, groupIndex) => {
    const order = new Map<number, number>();
    for (const index of indexes) {
      const value = volumeOrder(rows[index] as ListRow, infos[index]);
      order.set(index, value ?? Number.POSITIVE_INFINITY);
    }
    indexes.sort((a, b) => {
      const x = order.get(a) as number;
      const y = order.get(b) as number;
      if (x !== y) return x < y ? -1 : 1;
      return a - b;
    });
    const members = indexes.map((index) => rows[index] as ListRow);
    let firstVolume: string | null = null;
    let lastVolume: string | null = null;
    for (const index of indexes) {
      const label = volumeLabel(rows[index] as ListRow, infos[index]);
      if (label === null) continue;
      firstVolume ??= label;
      lastVolume = label;
    }
    return {
      key: groupKeys[groupIndex] as string,
      baseTitle: (infos[indexes[0] as number] as WorkInfo).baseTitle,
      members,
      firstVolume,
      lastVolume,
    };
  });

  // 4. 表示の順に、ふつうの本とグループ（最初の 1 冊のあった位置）を並べる
  const groups = new Map<string, WorkGroup>();
  const groupOfBook = new Map<string, string>();
  const emitted = new Uint8Array(workGroups.length);
  for (let i = 0; i < n; i++) {
    const row = rows[i] as ListRow;
    rowById.set(row.id, row);
    const groupIndex = groupOfRow[i] as number;
    if (groupIndex < 0) {
      units.push({ kind: "book", entry: plainEntry(row) });
      bookIds.push(row.id);
      continue;
    }
    if (emitted[groupIndex] === 1) continue;
    emitted[groupIndex] = 1;
    const group = workGroups[groupIndex] as WorkGroup;
    groups.set(group.key, group);
    const members = group.members.map((member): BookEntry => ({
      kind: "book",
      key: member.id,
      row: member,
      level: 2,
      groupKey: group.key,
    }));
    for (const member of group.members) {
      groupOfBook.set(member.id, group.key);
      bookIds.push(member.id);
    }
    units.push({
      kind: "group",
      collapsed: { kind: "group", key: group.key, group, expanded: false },
      expanded: { kind: "group", key: group.key, group, expanded: true },
      members,
    });
  }

  return { enabled, units, groups, groupOfBook, bookIds, rowById };
}

function plainEntry(row: ListRow): BookEntry {
  return { kind: "book", key: row.id, row, level: 1, groupKey: null };
}

/** 1 つの束（作品名が同じ本の行の番号）を、著者でつないで作品に分ける。 */
function splitWorks(bucket: readonly number[], infos: readonly WorkInfo[]): number[][] {
  const size = bucket.length;
  // union-find（束の中の通し番号で）
  const parent = Array.from({ length: size }, (_, k) => k);
  const find = (x: number): number => {
    let root = x;
    while (parent[root] !== root) root = parent[root] as number;
    while (parent[x] !== root) {
      const next = parent[x] as number;
      parent[x] = root;
      x = next;
    }
    return root;
  };

  const firstOfAuthor = new Map<string, number>();
  const authorless: number[] = [];
  const isAuthorless = new Uint8Array(size);
  for (let k = 0; k < size; k++) {
    const keys = (infos[bucket[k] as number] as WorkInfo).authorKeys;
    if (keys.length === 0) {
      authorless.push(k);
      isAuthorless[k] = 1;
      continue;
    }
    for (const key of keys) {
      const first = firstOfAuthor.get(key);
      if (first === undefined) firstOfAuthor.set(key, k);
      else parent[find(k)] = find(first);
    }
  }

  const byRoot = new Map<number, number[]>();
  for (let k = 0; k < size; k++) {
    if (isAuthorless[k] === 1) continue;
    const root = find(k);
    const list = byRoot.get(root);
    if (list === undefined) byRoot.set(root, [bucket[k] as number]);
    else list.push(bucket[k] as number);
  }
  const works = [...byRoot.values()];
  const authorlessRows = authorless.map((k) => bucket[k] as number);
  if (works.length === 1) (works[0] as number[]).push(...authorlessRows);
  else if (works.length === 0) works.push(authorlessRows);
  return works;
}

/** 手動の作品の本から、「作品名の比べ方 → 手動の作品の ID → その作品の著者の比べ方」を作る。作品名か著者のない本は入れない。 */
function indexManualWorks(
  manualRows: ReadonlyMap<string, readonly number[]>,
  infos: readonly WorkInfo[],
): Map<string, Map<string, Set<string>>> {
  const index = new Map<string, Map<string, Set<string>>>();
  for (const [workId, members] of manualRows) {
    for (const row of members) {
      const info = infos[row] as WorkInfo;
      if (info.titleKey === "" || info.authorKeys.length === 0) continue;
      let works = index.get(info.titleKey);
      if (works === undefined) {
        works = new Map<string, Set<string>>();
        index.set(info.titleKey, works);
      }
      let authors = works.get(workId);
      if (authors === undefined) {
        authors = new Set<string>();
        works.set(workId, authors);
      }
      for (const key of info.authorKeys) authors.add(key);
    }
  }
  return index;
}

/**
 * 自動の作品（行の番号。作品名の比べ方は全員同じ）とつながる手動の作品の ID。つながるのは、作品の中の著者のある本が、
 * 手動の作品の本と著者を 1 人でも共有するとき。ちょうど 1 つならその ID、0 または 2 つ以上なら null。
 */
function soleLinkedWork(
  work: readonly number[],
  infos: readonly WorkInfo[],
  linked: ReadonlyMap<string, ReadonlySet<string>>,
): string | null {
  let found: string | null = null;
  for (const index of work) {
    const keys = (infos[index] as WorkInfo).authorKeys;
    if (keys.length === 0) continue;
    for (const [workId, authors] of linked) {
      if (workId === found || !keys.some((key) => authors.has(key))) continue;
      if (found !== null) return null;
      found = workId;
    }
  }
  return found;
}

/** 作品のまとまりと展開の状態から、表示する並びを作る。展開したグループは、グループの行のあとに本の行（`level: 2`）が続く。 */
export function layoutEntries(layout: WorkLayout, expanded: ReadonlySet<string>): GroupedList {
  const entries: ListEntry[] = [];
  for (const unit of layout.units) {
    if (unit.kind === "book") entries.push(unit.entry);
    else if (expanded.has(unit.collapsed.key)) {
      entries.push(unit.expanded);
      for (const member of unit.members) entries.push(member);
    } else entries.push(unit.collapsed);
  }
  return {
    enabled: layout.enabled,
    entries,
    groups: layout.groups,
    groupOfBook: layout.groupOfBook,
    bookIds: layout.bookIds,
    rowById: layout.rowById,
    expanded,
  };
}

/** 行をグループにまとめて、並びを作る（`buildWorkLayout` → `layoutEntries`）。 */
export function groupRows(
  rows: readonly ListRow[],
  enabled: boolean,
  expanded: ReadonlySet<string>,
): GroupedList {
  return layoutEntries(buildWorkLayout(rows, enabled), expanded);
}

/**
 * 選択の key（グループの key か、本の ID）を、本の ID にする。グループの key は、その全員の ID。本の ID はそのまま。
 * 重なりは除き、`bookIds` の順（表示の順）に並べる。並びにない ID（消えた本）は含めない。
 */
export function expandToBookIds(keys: Iterable<string>, grouped: GroupedList): string[] {
  const wanted = new Set<string>();
  for (const key of keys) {
    const group = grouped.groups.get(key);
    if (group === undefined) wanted.add(key);
    else for (const member of group.members) wanted.add(member.id);
  }
  if (wanted.size === 0) return [];
  return grouped.bookIds.filter((id) => wanted.has(id));
}

/** 選択の key が指す本の冊数（グループは中の冊数。重なりは数えない）。 */
export function countBooks(keys: Iterable<string>, grouped: GroupedList): number {
  return expandToBookIds(keys, grouped).length;
}

/** グループの行に出す値: 代表の行（先頭の本をもとに、書名・巻・シリーズを置き換えたもの）と、値を出さない列。 */
export interface GroupSummary {
  row: ListRow;
  /** 列 id のうち、全員が同じ値ではないので、空にする列。 */
  blank: ReadonlySet<string>;
}

/**
 * グループの行の、各列の値を決める。全員が同じ値の列は、その値（先頭の本の値）。ばらばらの列は空（`blank`）。
 * 同じかどうかは、列の並べ替えのキー（`sortKeyOf`）で比べる。並べ替えのキーのない列（書影）は、先頭の本の値。
 * 書名（作品名）と巻（範囲）は、必ず出す。
 */
export function groupSummary(group: WorkGroup, ctx: SortContext): GroupSummary {
  const members = group.members;
  const first = members[0] as ListRow;
  const blank = new Set<string>();
  for (const id of SORTABLE_COLUMN_IDS) {
    const expected = sortKeyOf(id, first, ctx);
    for (let i = 1; i < members.length; i++) {
      if (sortKeyOf(id, members[i] as ListRow, ctx) !== expected) {
        blank.add(id);
        break;
      }
    }
  }
  blank.delete("title");
  blank.delete("volume");
  // シリーズ名は、全員が同じ（空でない）ときだけ、書名の 2 行目に出す
  const sameSeries = !blank.has("series") && first.seriesName !== null && first.seriesName !== "";
  const row: ListRow = {
    ...first,
    title: group.baseTitle,
    subtitle: null,
    volume: ja.list.group.volumeRange(group.firstVolume, group.lastVolume) || null,
    seriesName: sameSeries ? first.seriesName : null,
    deleted: members.every((member) => member.deleted),
  };
  return { row, blank };
}
