// 一覧の行の選択。計算は src/features/list/selection.ts の純粋関数に任せ、ここは保持するだけ。
import { create } from "zustand";
import {
  EMPTY_SELECTION,
  clickRow,
  moveFocus,
  moveFocusTo,
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
  /** focus を `id` の行へ動かす（グリッドの上下左右など、行き先が決まっているとき）。`extend`（Shift）なら範囲で選ぶ。 */
  moveTo: (ids: readonly string[], id: string, options: { extend: boolean }) => void;
  selectAll: (ids: readonly string[]) => void;
  /**
   * 1 冊だけを選び、一覧をその本までスクロールさせる（検索欄の ISBN など、ほかの場所から本へ移るとき）。
   * すでに focus している本でも、一覧は動く（`revealTick` が変わる）。
   */
  reveal: (id: string) => void;
  /** `reveal` のたびに増える。一覧は、focus が変わったときと、この値が変わったときだけスクロールする。 */
  revealTick: number;
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
  revealTick: 0,
  click: (ids, id, modifiers) => set((s) => clickRow(pick(s), ids, id, modifiers)),
  move: (ids, target, options) => set((s) => moveFocus(pick(s), ids, target, options)),
  moveTo: (ids, id, options) => set((s) => moveFocusTo(pick(s), ids, id, options)),
  selectAll: (ids) => set((s) => selectAll(pick(s), ids)),
  reveal: (id) =>
    set((s) => ({
      selected: new Set([id]),
      anchor: id,
      focus: id,
      revealTick: s.revealTick + 1,
    })),
  prune: (visible) =>
    set((s) => {
      const current = pick(s);
      const next = pruneSelection(current, visible);
      return next === current ? s : next;
    }),
  clear: () => set(EMPTY_SELECTION),
}));
