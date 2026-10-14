// 一覧の行の選択。計算は src/features/list/selection.ts の純粋関数に任せ、ここは保持するだけ。
import { create } from "zustand";
import {
  EMPTY_SELECTION,
  clickRow,
  moveFocus,
  pruneSelection,
  selectAll,
  type ClickModifiers,
  type MoveTarget,
  type SelectionState,
} from "@/features/list/selection";

interface SelectionStore extends SelectionState {
  click: (ids: readonly string[], id: string, modifiers: ClickModifiers) => void;
  move: (
    ids: readonly string[],
    target: MoveTarget,
    options: { extend: boolean; pageSize: number },
  ) => void;
  selectAll: (ids: readonly string[]) => void;
  /** 選択を、表示している行だけに絞る。 */
  prune: (visible: ReadonlySet<string>) => void;
  clear: () => void;
}

const pick = (s: SelectionStore): SelectionState => ({
  selected: s.selected,
  anchor: s.anchor,
  focus: s.focus,
});

export const useSelectionStore = create<SelectionStore>()((set) => ({
  ...EMPTY_SELECTION,
  click: (ids, id, modifiers) => set((s) => clickRow(pick(s), ids, id, modifiers)),
  move: (ids, target, options) => set((s) => moveFocus(pick(s), ids, target, options)),
  selectAll: (ids) => set((s) => selectAll(pick(s), ids)),
  prune: (visible) =>
    set((s) => {
      const current = pick(s);
      const next = pruneSelection(current, visible);
      return next === current ? s : next;
    }),
  clear: () => set(EMPTY_SELECTION),
}));
