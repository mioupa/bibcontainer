// 表示中の絞り込み（サイドバーの選択）と検索語。保存しない（起動のたびに「すべて」から始める）。
import { create } from "zustand";
import {
  DEFAULT_FILTER,
  type AutoFilter,
  type BookFilter,
  type LibraryView,
} from "@/features/library/filter";
import type { ReadStatus } from "@/types/contracts";

interface ViewState {
  filter: BookFilter;
  searchText: string;
  setSearchText: (text: string) => void;
  setLibrary: (library: LibraryView) => void;
  /** 同じ項目をもう一度選ぶと、その絞り込みを外す（以降の toggle* も同じ）。 */
  toggleReadStatus: (status: ReadStatus) => void;
  toggleLocation: (locationId: string) => void;
  toggleTag: (path: string) => void;
  toggleAuto: (auto: AutoFilter) => void;
  clearFilters: () => void;
}

export function sameAuto(a: AutoFilter | null, b: AutoFilter | null): boolean {
  return a !== null && b !== null && a.kind === b.kind && a.value === b.value;
}

export const useViewStore = create<ViewState>()((set) => ({
  filter: DEFAULT_FILTER,
  searchText: "",
  setSearchText: (searchText) => set({ searchText }),
  setLibrary: (library) => set((s) => ({ filter: { ...s.filter, library } })),
  toggleReadStatus: (status) =>
    set((s) => ({
      filter: { ...s.filter, readStatus: s.filter.readStatus === status ? null : status },
    })),
  toggleLocation: (locationId) =>
    set((s) => ({
      filter: { ...s.filter, locationId: s.filter.locationId === locationId ? null : locationId },
    })),
  toggleTag: (path) =>
    set((s) => ({ filter: { ...s.filter, tag: s.filter.tag === path ? null : path } })),
  toggleAuto: (auto) =>
    set((s) => ({ filter: { ...s.filter, auto: sameAuto(s.filter.auto, auto) ? null : auto } })),
  clearFilters: () => set({ filter: DEFAULT_FILTER }),
}));
