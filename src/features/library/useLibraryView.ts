// 一覧に出す行と、サイドバー・ステータスバーの件数を求める。
// 取得（TanStack Query）→ 絞り込み → 並べ替えの流れを 1 か所にまとめる。呼び出しは画面の最上位で 1 回だけにする。
import { useMemo } from "react";
import {
  EMPTY_LOCATIONS,
  EMPTY_ROWS,
  useBooks,
  useLocations,
  useSearchIds,
} from "@/features/library/hooks";
import { computeFacets, filterRows, type FilterContext } from "@/features/library/filter";
import { sortRows } from "@/features/library/sort";
import { buildLocationIndex, buildLocationPaths, collectTagPaths } from "@/features/library/trees";
import { useTableStore } from "@/stores/tableStore";
import { useViewStore } from "@/stores/viewStore";

export function useLibraryView() {
  const books = useBooks();
  const locationsQuery = useLocations();
  const allRows = books.data ?? EMPTY_ROWS;
  const locations = locationsQuery.data ?? EMPTY_LOCATIONS;

  const filter = useViewStore((s) => s.filter);
  const searchText = useViewStore((s) => s.searchText);
  const sorting = useTableStore((s) => s.sorting);
  const searchIds = useSearchIds(searchText);

  const locationIndex = useMemo(() => buildLocationIndex(locations), [locations]);
  const locationPaths = useMemo(() => buildLocationPaths(locations), [locations]);
  const context = useMemo<FilterContext>(
    () => ({ searchIds, locationIndex }),
    [searchIds, locationIndex],
  );

  const filtered = useMemo(() => filterRows(allRows, filter, context), [allRows, filter, context]);
  const rows = useMemo(
    () => sortRows(filtered, sorting, { locationPaths }),
    [filtered, sorting, locationPaths],
  );
  const facets = useMemo(() => computeFacets(allRows, filter, context), [allRows, filter, context]);
  const allTagPaths = useMemo(() => collectTagPaths(allRows), [allRows]);

  return {
    /** 表示する行（絞り込みと並べ替えを済ませたもの）。 */
    rows,
    facets,
    locations,
    locationPaths,
    allTagPaths,
    isLoading: books.isPending,
    error: books.error,
  };
}

export type LibraryViewModel = ReturnType<typeof useLibraryView>;
