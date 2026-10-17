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
  type PendingWorkSelection,
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
  /**
   * 作品の操作のあとに選ぶ行（0026）。行の更新が届いてから、`useGroupedList` が `settleWork` で選ぶ。
   * 利用者が選び直したら（click・move・moveTo・selectAll・reveal・clear）取り消す。更新が届かないままでも、期限で捨てる。
   */
  pendingWork: PendingWorkSelection | null;
  setPendingWork: (pending: PendingWorkSelection | null) => void;
  /** 保留中の選択を済ませる。`next` があればそれを選び（一覧はその行までスクロールする）、null なら選ばずに捨てる。 */
  settleWork: (next: SelectionState | null) => void;
  /** 選択を、表示している行だけに絞る。 */
  prune: (visible: ReadonlySet<string>) => void;
  clear: () => void;
}

const pick = (s: SelectionStore): SelectionState => ({
  selected: s.selected,
  anchor: s.anchor,
  focus: s.focus,
});

/** 利用者が選び直したときの新しい状態。更新待ちの、作品の操作のあとの選択（`pendingWork`）は取り消す。 */
const chosen = (next: SelectionState) => ({ ...next, pendingWork: null });

export const useSelectionStore = create<SelectionStore>()((set) => ({
  ...EMPTY_SELECTION,
  revealTick: 0,
  click: (ids, id, modifiers) => set((s) => chosen(clickRow(pick(s), ids, id, modifiers))),
  move: (ids, target, options) => set((s) => chosen(moveFocus(pick(s), ids, target, options))),
  moveTo: (ids, id, options) => set((s) => chosen(moveFocusTo(pick(s), ids, id, options))),
  selectAll: (ids) => set((s) => chosen(selectAll(pick(s), ids))),
  reveal: (id) =>
    set((s) => ({
      ...chosen({ selected: new Set([id]), anchor: id, focus: id }),
      revealTick: s.revealTick + 1,
    })),
  pendingWork: null,
  setPendingWork: (pending) => set({ pendingWork: pending }),
  settleWork: (next) =>
    set((s) =>
      next === null
        ? { pendingWork: null }
        : { ...next, pendingWork: null, revealTick: s.revealTick + 1 },
    ),
  prune: (visible) =>
    set((s) => {
      const current = pick(s);
      const next = pruneSelection(current, visible);
      return next === current ? s : next;
    }),
  clear: () => set(chosen(EMPTY_SELECTION)),
}));
