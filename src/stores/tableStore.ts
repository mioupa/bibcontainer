// 一覧の列の状態（表示・非表示、並び順、幅、並べ替え）。端末ごとに保存する（FR-LIST-01）。
// 保存先は src/lib/persist.ts（今は localStorage。後で Rust の local_settings に差し替える）。
import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
  COLUMN_IDS,
  FIXED_COLUMN_IDS,
  defaultColumnVisibility,
  MAX_COLUMN_SIZE,
  MIN_COLUMN_SIZE,
  moveColumnId,
  sanitizeColumnOrder,
  type ColumnId,
} from "@/features/list/columnMeta";
import type { SortSpec } from "@/features/library/sort";
import { isSortable } from "@/features/library/sort";
import { STORAGE_KEYS, settingsStorage } from "@/lib/persist";

/** TanStack Table の Updater と同じ形（値か、古い値から新しい値を作る関数）。 */
export type Updater<T> = T | ((old: T) => T);

export function applyUpdater<T>(updater: Updater<T>, old: T): T {
  return typeof updater === "function" ? (updater as (old: T) => T)(old) : updater;
}

const DEFAULT_SORTING: SortSpec[] = [{ id: "title", desc: false }];

interface TableState {
  sorting: SortSpec[];
  columnVisibility: Record<string, boolean>;
  /** 列の並び順。空なら既定の順（Rust の `list.columnOrder` と同じ意味）。載っていない列は末尾に並ぶ。 */
  columnOrder: string[];
  columnSizing: Record<string, number>;
  setSorting: (updater: Updater<SortSpec[]>) => void;
  setColumnVisibility: (updater: Updater<Record<string, boolean>>) => void;
  setColumnOrder: (updater: Updater<string[]>) => void;
  /** 列を 1 つ上（左）・下（右）へ動かす。 */
  moveColumn: (id: ColumnId, direction: "up" | "down") => void;
  setColumnSizing: (updater: Updater<Record<string, number>>) => void;
  /** 列の状態を既定に戻す。 */
  resetColumns: () => void;
}

const knownColumn = (id: string) => (COLUMN_IDS as readonly string[]).includes(id);

function sanitizeSorting(sorting: SortSpec[]): SortSpec[] {
  return sorting
    .filter((s) => knownColumn(s.id) && isSortable(s.id))
    .map((s) => ({ id: s.id, desc: s.desc }));
}

function sanitizeVisibility(visibility: Record<string, boolean>): Record<string, boolean> {
  const result: Record<string, boolean> = {};
  for (const [id, visible] of Object.entries(visibility)) {
    if (knownColumn(id) && !(FIXED_COLUMN_IDS as readonly string[]).includes(id))
      result[id] = Boolean(visible);
  }
  return result;
}

function sanitizeSizing(sizing: Record<string, number>): Record<string, number> {
  const result: Record<string, number> = {};
  for (const [id, size] of Object.entries(sizing)) {
    if (knownColumn(id) && Number.isFinite(size)) {
      result[id] = Math.min(MAX_COLUMN_SIZE, Math.max(MIN_COLUMN_SIZE, Math.round(size)));
    }
  }
  return result;
}

export const useTableStore = create<TableState>()(
  persist(
    (set) => ({
      sorting: DEFAULT_SORTING,
      columnVisibility: defaultColumnVisibility(),
      columnOrder: [],
      columnSizing: {},
      setSorting: (updater) =>
        set((s) => ({ sorting: sanitizeSorting(applyUpdater(updater, s.sorting)) })),
      setColumnVisibility: (updater) =>
        set((s) => ({
          columnVisibility: sanitizeVisibility(applyUpdater(updater, s.columnVisibility)),
        })),
      setColumnOrder: (updater) =>
        set((s) => ({ columnOrder: sanitizeColumnOrder(applyUpdater(updater, s.columnOrder)) })),
      moveColumn: (id, direction) =>
        set((s) => ({ columnOrder: moveColumnId(s.columnOrder, id, direction) })),
      setColumnSizing: (updater) =>
        set((s) => ({ columnSizing: sanitizeSizing(applyUpdater(updater, s.columnSizing)) })),
      resetColumns: () =>
        set({
          sorting: DEFAULT_SORTING,
          columnVisibility: defaultColumnVisibility(),
          columnOrder: [],
          columnSizing: {},
        }),
    }),
    {
      name: STORAGE_KEYS.table,
      version: 1,
      storage: settingsStorage,
      partialize: (s) => ({
        sorting: s.sorting,
        columnVisibility: s.columnVisibility,
        columnOrder: s.columnOrder,
        columnSizing: s.columnSizing,
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<TableState, "sorting" | "columnVisibility" | "columnOrder" | "columnSizing">
        >;
        return {
          ...current,
          sorting: Array.isArray(saved.sorting) ? sanitizeSorting(saved.sorting) : current.sorting,
          columnVisibility: {
            ...current.columnVisibility,
            ...sanitizeVisibility(saved.columnVisibility ?? {}),
          },
          columnOrder: Array.isArray(saved.columnOrder)
            ? sanitizeColumnOrder(saved.columnOrder.filter((id) => typeof id === "string"))
            : [],
          columnSizing: sanitizeSizing(saved.columnSizing ?? {}),
        };
      },
    },
  ),
);
