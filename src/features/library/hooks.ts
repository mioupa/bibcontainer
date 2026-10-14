// バックエンド呼び出しのフック（TanStack Query）。取得は libraryApi（IPC。Tauri の外ではモック）の裏に隠れている。
// データが変わったら、Rust が出す `library-changed` を受けて、変わった行だけ取り直す（sync.ts）。
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { libraryApi } from "@/features/library/api";
import { DEFAULT_PRICE_SETTINGS } from "@/features/library/price";
import type { CCodeTable, ListRow, Location } from "@/types/contracts";

// useQuery の data が未取得のときに渡す空の値。参照を固定して、メモ化が毎回やり直しにならないようにする。
export const EMPTY_ROWS: readonly ListRow[] = [];
export const EMPTY_LOCATIONS: readonly Location[] = [];
export const EMPTY_C_CODE_TABLE: CCodeTable = {
  audience: {},
  format: {},
  contentMajor: {},
  content: {},
};

export const libraryKeys = {
  books: ["books"] as const,
  locations: ["locations"] as const,
  tags: ["tags"] as const,
  series: ["series"] as const,
  cCodeTable: ["c_code_table"] as const,
  /** 1 冊の詳細。`library-changed` のあと、まとめて取り直せるよう `bookDetails` を先頭に共通で持つ。 */
  bookDetails: ["book_detail"] as const,
  bookDetail: (id: string) => ["book_detail", id] as const,
  purchaseStores: ["purchase_stores"] as const,
  priceSettings: ["price_settings"] as const,
  /** 検索結果の ID。`library-changed` のあと、まとめて取り直せるよう `searches` を先頭に共通で持つ。 */
  searches: ["search"] as const,
  search: (query: string) => ["search", query] as const,
};

/** 全書籍の一覧用の行。起動時に 1 度取り、絞り込み・並べ替え・集計はフロントで行う（0005 §4）。 */
export function useBooks() {
  return useQuery({
    queryKey: libraryKeys.books,
    queryFn: libraryApi.listBooks,
    staleTime: Infinity,
  });
}

export function useLocations() {
  return useQuery({
    queryKey: libraryKeys.locations,
    queryFn: libraryApi.listLocations,
    staleTime: Infinity,
  });
}

/** タグの一覧（名前・件数・色）。件数と色は変わりうるので、`library-changed`（others）で取り直す。 */
export function useTags() {
  return useQuery({
    queryKey: libraryKeys.tags,
    queryFn: libraryApi.listTags,
    staleTime: Infinity,
  });
}

/** シリーズの一覧（名前・冊数）。 */
export function useSeries() {
  return useQuery({
    queryKey: libraryKeys.series,
    queryFn: libraryApi.listSeries,
    staleTime: Infinity,
  });
}

/** 1 冊の全項目（詳細ペインとフォーム）。`id` が null のあいだは取得しない。 */
export function useBookDetail(id: string | null) {
  return useQuery({
    queryKey: libraryKeys.bookDetail(id ?? ""),
    queryFn: () => libraryApi.getBookDetail(id ?? ""),
    enabled: id !== null,
    staleTime: Infinity,
  });
}

/** 購入店の候補（過去の入力から。使用回数の多い順）。 */
export function usePurchaseStores() {
  return useQuery({
    queryKey: libraryKeys.purchaseStores,
    queryFn: libraryApi.purchaseStores,
    staleTime: Infinity,
  });
}

/**
 * 価格の表示設定（税込を併記するか。FR-CONF-05）。設定画面で変えられるので、画面を開くたびに取り直す
 * （staleTime 0）。取得できるまでは既定（税込を併記しない）。
 */
export function usePriceSettings() {
  const query = useQuery({
    queryKey: libraryKeys.priceSettings,
    queryFn: libraryApi.getPriceSettings,
    staleTime: 0,
  });
  return query.data ?? DEFAULT_PRICE_SETTINGS;
}

/** Cコード表。取得できるまでは空の表（名称は出ず、コードだけが表示される）。 */
export function useCCodeTable(): CCodeTable {
  const query = useQuery({
    queryKey: libraryKeys.cCodeTable,
    queryFn: libraryApi.cCodeTable,
    staleTime: Infinity,
  });
  return query.data ?? EMPTY_C_CODE_TABLE;
}

/**
 * 検索語に一致した本の ID の集合。検索語が空なら null（絞り込まない）。
 * 新しい結果が届くまでは、前の結果を出し続ける（入力のたびに一覧がちらつかない）。
 */
export function useSearchIds(query: string): ReadonlySet<string> | null {
  const trimmed = query.trim();
  const result = useQuery({
    queryKey: libraryKeys.search(trimmed),
    queryFn: () => libraryApi.searchBooks(trimmed),
    enabled: trimmed !== "",
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
  const ids = trimmed === "" ? undefined : result.data;
  return useMemo(() => (ids === undefined ? null : new Set(ids)), [ids]);
}
