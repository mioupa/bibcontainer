// 表示する行を、巻のグループにまとめた並びにする（FR-LIST-08）。呼び出しは画面の最上位で 1 回だけにして、表とグリッドに渡す。
//
// - まとめるかは layoutStore の `groupVolumes`、開いているグループは groupStore（アプリを開いている間だけ）。
// - focus している本が、畳まれたグループのメンバーなら（検索欄の ISBN、新しく登録した本など）、そのグループは開く。
//   同じ描画で並びに入るよう、開いている状態に focus のグループを足して並びを作り（選択の整理で消されないように）、
//   あとの effect で、その開いた状態を groupStore に保存する。
// - まとめる設定をオフにしたとき、選んでいたグループは、メンバーの選択に置き換える。
// - 作品の操作（まとめる・外す・自動に戻す。0026）のあと、行の更新が届いたら、保留中の選択（selectionStore の `pendingWork`）を済ませる。
//   更新が届くまでの間、対象の本への focus では、グループを開かない（まとめた作品を、畳んだまま選ぶため）。
import { useEffect, useLayoutEffect, useMemo } from "react";
import {
  buildWorkLayout,
  isGroupKey,
  layoutEntries,
  type GroupedList,
} from "@/features/library/grouping";
import { resolvePendingWork, ungroupSelection } from "@/features/list/selection";
import { useGroupStore } from "@/stores/groupStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useSelectionStore } from "@/stores/selectionStore";
import type { ListRow } from "@/types/contracts";

export function useGroupedList(rows: readonly ListRow[]): GroupedList {
  const enabled = useLayoutStore((s) => s.groupVolumes);
  const stored = useGroupStore((s) => s.expanded);
  const focus = useSelectionStore((s) => s.focus);
  const pendingWork = useSelectionStore((s) => s.pendingWork);

  // 展開の状態に依らない部分（グループの計算）。行か設定が変わったときだけやり直す
  const layout = useMemo(() => buildWorkLayout(rows, enabled), [rows, enabled]);

  // 作品の操作の対象の本に（操作の前から）focus があるときは、行の更新が届くまでの間、そのグループを開かない。
  // 操作のあとに選ぶ行（`pendingWork`）が決まるまでに、できた作品を開いてしまわないため
  const focusGroup =
    focus === null || pendingWork?.bookIds.includes(focus) === true
      ? undefined
      : layout.groupOfBook.get(focus);
  const expanded = useMemo<ReadonlySet<string>>(
    () =>
      focusGroup === undefined || stored.has(focusGroup) ? stored : new Set(stored).add(focusGroup),
    [stored, focusGroup],
  );
  const grouped = useMemo(() => layoutEntries(layout, expanded), [layout, expanded]);

  // focus のグループを開いた状態を保存する（保存するまでの間も、上の `expanded` で開いて見える）
  useEffect(() => {
    if (focusGroup !== undefined) useGroupStore.getState().expand(focusGroup);
  }, [focusGroup]);

  // オフにしたら、グループの選択をメンバーの選択に置き換える。一覧の選択の整理（usePruneSelection。子の effect）が、
  // 並びにないグループの key を消してしまう前に済ませるため、layout effect で行う
  useLayoutEffect(() => {
    if (enabled) return;
    const { selected, anchor, focus: current } = useSelectionStore.getState();
    let stale = isGroupKey(anchor) || isGroupKey(current);
    if (!stale) for (const key of selected) if (isGroupKey(key)) stale = true;
    if (!stale) return;
    const groups = buildWorkLayout(rows, true).groups;
    const state = useSelectionStore.getState();
    const next = ungroupSelection(state, (key) =>
      groups.get(key)?.members.map((member) => member.id),
    );
    if (next !== state) useSelectionStore.setState(next);
  }, [enabled, rows]);

  // 作品の操作のあとに選ぶ行。操作は済んでいても、行の更新は遅れて届くので、対象の本の行が更新されるまで待つ。
  // 選ぶのは一覧の選択の整理（usePruneSelection）より先に済ませたいので、layout effect で行う
  useLayoutEffect(() => {
    if (pendingWork === null) return;
    const next = resolvePendingWork(
      pendingWork,
      layout.groups,
      (id) => layout.rowById.get(id)?.workId,
      Date.now(),
    );
    if (next !== "wait") useSelectionStore.getState().settleWork(next);
  }, [pendingWork, layout]);

  return grouped;
}
