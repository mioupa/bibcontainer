// 表示する行を、巻のグループにまとめた並びにする（FR-LIST-08）。呼び出しは画面の最上位で 1 回だけにして、表とグリッドに渡す。
//
// - まとめるかは layoutStore の `groupVolumes`、開いているグループは groupStore（アプリを開いている間だけ）。
// - focus している本が、畳まれたグループのメンバーなら（検索欄の ISBN、新しく登録した本など）、そのグループは開く。
//   同じ描画で並びに入るよう、開いている状態に focus のグループを足して並びを作り（選択の整理で消されないように）、
//   あとの effect で、その開いた状態を groupStore に保存する。
// - まとめる設定をオフにしたとき、選んでいたグループは、メンバーの選択に置き換える。
import { useEffect, useLayoutEffect, useMemo } from "react";
import {
  buildWorkLayout,
  isGroupKey,
  layoutEntries,
  type GroupedList,
} from "@/features/library/grouping";
import { ungroupSelection } from "@/features/list/selection";
import { useGroupStore } from "@/stores/groupStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useSelectionStore } from "@/stores/selectionStore";
import type { ListRow } from "@/types/contracts";

export function useGroupedList(rows: readonly ListRow[]): GroupedList {
  const enabled = useLayoutStore((s) => s.groupVolumes);
  const stored = useGroupStore((s) => s.expanded);
  const focus = useSelectionStore((s) => s.focus);

  // 展開の状態に依らない部分（グループの計算）。行か設定が変わったときだけやり直す
  const layout = useMemo(() => buildWorkLayout(rows, enabled), [rows, enabled]);

  const focusGroup = focus === null ? undefined : layout.groupOfBook.get(focus);
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

  return grouped;
}
