// 巻のグループの開閉（FR-LIST-08）。表・グリッドの、開閉のボタン・キー・ダブルクリックが、すべてここを通る。
// 畳むときは、選択を整える（畳んで見えなくなるメンバーの選択を、グループの選択に置き換える）。
import type { GroupedList } from "@/features/library/grouping";
import { collapseSelection } from "@/features/list/selection";
import { useGroupStore } from "@/stores/groupStore";
import { useSelectionStore } from "@/stores/selectionStore";

export function expandGroup(key: string): void {
  useGroupStore.getState().expand(key);
}

/** グループを畳む。選んでいた（focus していた）メンバーは、グループの選択（focus）に置き換える。 */
export function collapseGroup(key: string, grouped: GroupedList): void {
  const group = grouped.groups.get(key);
  if (group === undefined) return;
  const current = useSelectionStore.getState();
  const next = collapseSelection(
    current,
    key,
    group.members.map((member) => member.id),
  );
  if (next !== current) useSelectionStore.setState(next);
  useGroupStore.getState().collapse(key);
}

/** 開いていれば畳み、閉じていれば開く。開いているかは、いま表示している並びの状態（focus したメンバーのグループも開いている）。 */
export function toggleGroup(key: string, grouped: GroupedList): void {
  if (!grouped.groups.has(key)) return;
  if (grouped.expanded.has(key)) collapseGroup(key, grouped);
  else expandGroup(key);
}
