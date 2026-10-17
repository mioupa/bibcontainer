// バックエンドの代わりに、ダミーのデータを返す（Tauri の外で `pnpm dev` をブラウザで開いたとき用）。
// 呼び出しは src/features/library/api.ts が、Tauri の外のときだけここへ振り分ける。
// ゴミ箱への移動・復元・完全削除・1 項目の一括変更・作品のまとめ（mergeWorks）は、メモリ上の行を書き換えるだけ（再読み込みで元に戻る）。
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { MOCK_LOCATIONS, generateMockRows, normalizeSortKey } from "@/dev/mockRows";
import type { ChangeReport } from "@/bindings/ChangeReport";
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import type { MergeWorksResult } from "@/bindings/MergeWorksResult";
import type { SeriesInfo } from "@/bindings/SeriesInfo";
import type { StoreCount } from "@/bindings/StoreCount";
import type { TagInfo } from "@/bindings/TagInfo";
import { WORK_NONE } from "@/features/library/works";
import { ja } from "@/i18n/ja";
import { IpcError } from "@/lib/ipc";
import type {
  BookDetail,
  CCodeTable,
  FieldChange,
  ListRow,
  Location,
  NewBookDraft,
} from "@/types/contracts";

export const DEFAULT_MOCK_ROW_COUNT = 5000;
export const MAX_MOCK_ROW_COUNT = 100_000;

/** URL の `?mockRows=20000` で冊数を変えられる（5,000〜20,000 冊での動作確認用）。 */
export function mockRowCountFromLocation(
  search: string = globalThis.location?.search ?? "",
): number {
  const raw = new URLSearchParams(search).get("mockRows");
  const parsed = raw === null ? Number.NaN : Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 0) return DEFAULT_MOCK_ROW_COUNT;
  return Math.min(parsed, MAX_MOCK_ROW_COUNT);
}

let cachedRows: ListRow[] | null = null;
let cachedHaystacks: WeakMap<ListRow, string> | null = null;

function rows(): ListRow[] {
  cachedRows ??= generateMockRows(mockRowCountFromLocation());
  return cachedRows;
}

/** 指定した本を書き換える。行は新しいオブジェクトに置き換え（呼び出し元が持つ配列は変えない）、変えた ID を返す。 */
function updateRows(ids: readonly string[], update: (row: ListRow) => ListRow | null): string[] {
  const wanted = new Set(ids);
  const changed: string[] = [];
  const next: ListRow[] = [];
  for (const row of rows()) {
    if (!wanted.has(row.id)) {
      next.push(row);
      continue;
    }
    const updated = update(row);
    if (updated === row) next.push(row);
    else {
      changed.push(row.id);
      if (updated !== null) next.push(updated);
    }
  }
  cachedRows = next;
  return changed;
}

const camelCase = (name: string) => name.replace(/_([a-z])/g, (_, ch: string) => ch.toUpperCase());

/** 検索用に、行ごとの検索対象の文字列（書名・ヨミ・著者・出版社・シリーズ・タグ・ISBN）を正規化して持つ。 */
function haystackOf(row: ListRow): string {
  cachedHaystacks ??= new WeakMap();
  let text = cachedHaystacks.get(row);
  if (text === undefined) {
    text = normalizeSortKey(
      [
        row.title,
        row.titleKana ?? "",
        row.subtitle ?? "",
        row.authors.map((a) => `${a.name} ${a.kana ?? ""}`).join(" "),
        row.publisher ?? "",
        row.seriesName ?? "",
        row.tags.join(" "),
        row.isbn13 ?? "",
      ].join("\n"),
    );
    cachedHaystacks.set(row, text);
  }
  return text;
}

/** 一覧の行にない項目（感想・メモなど）の保存先。詳細（getBookDetail）に混ぜて返す。 */
const extras = new Map<string, Record<string, unknown>>();
let createdCount = 0;

function detailOf(row: ListRow): BookDetail {
  return {
    id: row.id,
    isbn13: row.isbn13,
    title: row.title,
    titleKana: row.titleKana,
    subtitle: row.subtitle,
    seriesId: row.seriesId,
    volume: row.volume,
    volumeSort: row.volumeSort,
    workId: row.workId,
    authors: row.authors,
    publisher: row.publisher,
    publishedOn: row.publishedOn,
    cCode: row.cCode,
    ndc: row.ndc,
    listPrice: row.listPrice,
    refPriceIncl: null,
    pages: row.pages,
    sizeText: null,
    language: null,
    description: null,
    coverHash: row.coverHash,
    ownership: row.ownership,
    wishPriority: row.wishPriority,
    locationId: row.locationId,
    tags: row.tags,
    readStatus: row.readStatus,
    readStartedOn: null,
    readFinishedOn: row.readFinishedOn,
    rating: row.rating,
    review: null,
    purchasedOn: row.purchasedOn,
    purchaseStore: row.purchaseStore,
    purchasePrice: row.purchasePrice,
    purchaseCond: null,
    purchaseMemo: null,
    memo: null,
    metaStatus: row.metaStatus,
    deleted: row.deleted,
    purged: false,
    createdAt: row.createdAt,
    fieldSources: {},
    manualFields: [],
    ...extras.get(row.id),
    extra: {},
  };
}

/** `FieldChange` を行に当てる。行にある項目だけを書き、ない項目は `extras` に残す。書いた項目名を返す。 */
function applyChanges(row: ListRow, changes: readonly FieldChange[]): [ListRow, string[]] {
  const next: Record<string, unknown> = { ...row };
  const written: string[] = [];
  const rest: Record<string, unknown> = { ...extras.get(row.id) };
  for (const change of changes) {
    const key = camelCase(change.field);
    if (key === "seriesId") continue;
    if (key in row) next[key] = change.value;
    else rest[key] = change.value;
    written.push(change.field);
  }
  extras.set(row.id, rest);
  const updated = next as unknown as ListRow;
  updated.sortTitle = normalizeSortKey(updated.titleKana ?? updated.title);
  return [updated, written];
}

export const mockBackend = {
  getBookDetail: (id: string): Promise<BookDetail> => {
    const row = rows().find((candidate) => candidate.id === id);
    return row
      ? Promise.resolve(detailOf(row))
      : Promise.reject(new Error(`本が見つかりません: ${id}`));
  },
  updateBook: (id: string, changes: FieldChange[]): Promise<ChangeReport> => {
    const written: string[] = [];
    updateRows([id], (row) => {
      const [updated, fields] = applyChanges(row, changes);
      written.push(...fields);
      return updated;
    });
    return Promise.resolve({ id, written, skippedManual: [] });
  },
  modifyTags: (ids: string[], add: string[], remove: string[]): Promise<string[]> =>
    Promise.resolve(
      updateRows(ids, (row) => {
        const tags = [...row.tags.filter((tag) => !remove.includes(tag))];
        for (const tag of add) if (!tags.includes(tag)) tags.push(tag);
        return tags.length === row.tags.length && tags.every((tag, i) => tag === row.tags[i])
          ? row
          : { ...row, tags };
      }),
    ),
  createBooksFromDrafts: (drafts: NewBookDraft[]): Promise<string[]> => {
    const created: ListRow[] = drafts.map((draft) => {
      createdCount += 1;
      const id = `mock-new-${Date.now()}-${createdCount}`;
      const blank: ListRow = {
        id,
        isbn13: null,
        title: "",
        titleKana: null,
        subtitle: null,
        volume: null,
        volumeSort: null,
        seriesId: null,
        seriesName: draft.seriesName?.name ?? null,
        workId: null,
        authors: [],
        publisher: null,
        publishedOn: null,
        cCode: null,
        ndc: null,
        listPrice: null,
        pages: null,
        ownership: "owned",
        wishPriority: null,
        locationId: null,
        tags: [],
        readStatus: "unread",
        readFinishedOn: null,
        rating: null,
        purchasedOn: null,
        purchasePrice: null,
        purchaseStore: null,
        coverHash: null,
        metaStatus: "ok",
        deleted: false,
        createdAt: new Date().toISOString(),
        sortTitle: "",
        sortAuthors: "",
      };
      return applyChanges(blank, draft.changes)[0];
    });
    cachedRows = [...rows(), ...created];
    return Promise.resolve(created.map((row) => row.id));
  },
  findByIsbn: (isbn: string): Promise<IsbnMatch[]> =>
    Promise.resolve(
      rows()
        .filter((row) => row.isbn13 === isbn)
        .map((row) => ({
          id: row.id,
          title: row.title,
          ownership: row.ownership,
          locationId: row.locationId,
          locationPath: null,
          deleted: row.deleted,
        })),
    ),
  purchaseStores: (): Promise<StoreCount[]> => {
    const counts = new Map<string, number>();
    for (const row of rows()) {
      if (row.purchaseStore)
        counts.set(row.purchaseStore, (counts.get(row.purchaseStore) ?? 0) + 1);
    }
    return Promise.resolve(
      [...counts].map(([name, count]) => ({ name, count })).sort((a, b) => b.count - a.count),
    );
  },
  listBooks: (): Promise<ListRow[]> => Promise.resolve([...rows()]),
  getRows: (ids: string[]): Promise<ListRow[]> => {
    const byId = new Map(rows().map((row) => [row.id, row]));
    return Promise.resolve(ids.flatMap((id) => byId.get(id) ?? []));
  },
  listLocations: (): Promise<Location[]> => Promise.resolve([...MOCK_LOCATIONS]),
  listTags: (): Promise<TagInfo[]> => Promise.resolve([]),
  listSeries: (): Promise<SeriesInfo[]> => Promise.resolve([]),
  trashBooks: (ids: string[]): Promise<string[]> =>
    Promise.resolve(updateRows(ids, (row) => (row.deleted ? row : { ...row, deleted: true }))),
  restoreBooks: (ids: string[]): Promise<string[]> =>
    Promise.resolve(updateRows(ids, (row) => (row.deleted ? { ...row, deleted: false } : row))),
  purgeBooks: (ids: string[]): Promise<string[]> => Promise.resolve(updateRows(ids, () => null)),
  /** `read_status` など Op 上の項目名（snake_case）を、行の項目名（camelCase）に直して書く。 */
  setBooksField: (ids: string[], field: string, value: unknown): Promise<string[]> => {
    const key = camelCase(field);
    return Promise.resolve(
      updateRows(ids, (row) =>
        key in row && (row as Record<string, unknown>)[key] !== value
          ? { ...row, [key]: value }
          : row,
      ),
    );
  },
  /**
   * 選んだ本を 1 つの手動の作品にまとめる（Rust の `merge_works` と同じ規則。0026 §3）。対象は重複を除く。2 冊未満は検証のエラー。
   * 対象が入っている手動の作品があれば、その全巻（ゴミ箱の本を含む）も同じ作品にし、作品の ID は、冊数がいちばん多いもの
   * （同数なら文字列として小さいもの）。なければ新しい ID。
   */
  mergeWorks: (ids: string[]): Promise<MergeWorksResult> => {
    const byId = new Map(rows().map((row) => [row.id, row]));
    const targets = [...new Set(ids)].filter((id) => byId.has(id));
    if (targets.length < 2) {
      return Promise.reject(new IpcError({ kind: "validation", message: ja.toast.worksNeedTwo }));
    }
    const existing = new Set<string>();
    for (const id of targets) {
      const workId = (byId.get(id) as ListRow).workId;
      if (workId !== null && workId !== WORK_NONE) existing.add(workId);
    }
    const counts = new Map<string, number>();
    const rest: string[] = [];
    const targetSet = new Set(targets);
    for (const row of rows()) {
      if (row.workId === null || !existing.has(row.workId)) continue;
      counts.set(row.workId, (counts.get(row.workId) ?? 0) + 1);
      if (!targetSet.has(row.id)) rest.push(row.id);
    }
    let chosen: string | null = null;
    for (const [workId, count] of counts) {
      const best = chosen === null ? 0 : (counts.get(chosen) as number);
      if (chosen === null || count > best || (count === best && workId < chosen)) chosen = workId;
    }
    if (chosen === null) {
      createdCount += 1;
      chosen = `mock-work-${Date.now()}-${createdCount}`;
    }
    const workId = chosen;
    const changedIds = new Set(
      updateRows([...targets, ...rest], (row) =>
        row.workId === workId ? row : { ...row, workId },
      ),
    );
    // 選んだ本を渡した順に、続けて、同じ作品の残りを ID 順に（Rust と同じ並び）
    const changed = [...targets, ...rest.sort()].filter((id) => changedIds.has(id));
    return Promise.resolve({ workId, changed });
  },
  cCodeTable: (): Promise<CCodeTable> => Promise.resolve(PROVISIONAL_C_CODE_TABLE),
  /** 空白で区切った語をすべて含む本の ID を返す。本物の検索は Rust の FTS5（IPC の search_books）。 */
  searchBooks: (query: string): Promise<string[]> => {
    const terms = normalizeSortKey(query.trim()).split(/\s+/).filter(Boolean);
    const ids: string[] = [];
    for (const row of rows()) {
      const text = haystackOf(row);
      if (terms.every((term) => text.includes(term))) ids.push(row.id);
    }
    return Promise.resolve(ids);
  },
};
