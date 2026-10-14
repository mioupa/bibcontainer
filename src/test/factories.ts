// テスト用の ListRow を作る。
import type { ListRow } from "@/types/contracts";

let counter = 0;

export function makeRow(overrides: Partial<ListRow> = {}): ListRow {
  counter += 1;
  const title = overrides.title ?? `本${counter}`;
  return {
    id: `book-${counter}`,
    isbn13: null,
    title,
    titleKana: null,
    subtitle: null,
    volume: null,
    volumeSort: null,
    seriesId: null,
    seriesName: null,
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
    createdAt: "2026-01-01T00:00:00Z",
    sortTitle: title.toLowerCase(),
    sortAuthors: "",
    ...overrides,
  };
}
