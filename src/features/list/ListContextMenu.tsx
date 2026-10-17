// 一覧の右クリックメニューの中身（表・グリッドで共通）。メニューを開く操作（ContextMenu・ContextMenuTrigger）は、
// 各一覧が持つ。ここは、開いたメニューの項目と、押したときに呼ぶ処理だけ。
//
// 操作の対象は、選択している本（`selectionStore`）。右クリックした本が選択の外なら、一覧が先にその本だけを選ぶ
// （`useListInteraction` の `handleContextMenu`）。巻のグループの行を選んでいるときは、その全巻が対象
// （`expandToBookIds` で本の ID にする）。「編集」は 1 冊の本にしかできないので、focus がグループの行なら選べない。
// 作品の手動のまとめ・外し・自動に戻す（0026）は、巻をまとめる設定がオンのときだけ出す。
import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import { expandToBookIds, type GroupedList } from "@/features/library/grouping";
import { WORK_NONE } from "@/features/library/works";
import type { BulkChange, ListAction, ListViewProps } from "@/features/list/listActions";
import { ja } from "@/i18n/ja";
import { useSelectionStore } from "@/stores/selectionStore";
import type { ListRow, Ownership, ReadStatus } from "@/types/contracts";

const READ_STATUS_CHOICES = Object.keys(ja.labels.readStatus) as ReadStatus[];
const OWNERSHIP_CHOICES = Object.keys(ja.labels.ownership) as Ownership[];
const RATING_CHOICES = [5, 4, 3, 2, 1] as const;

export interface ListContextMenuContentProps extends Pick<
  ListViewProps,
  "onOpenEdit" | "onAction" | "onBulkChange"
> {
  /** 表示している並び（選択のグループを、本の ID にするのに使う）。 */
  grouped: GroupedList;
  inTrash: boolean;
}

/** 選択（本の ID・グループの key）に、欲しい本が 1 冊でもあるか。選択が変わるたびに調べるので、並べ直さず、見つけたらやめる。 */
function hasWanted(selected: ReadonlySet<string>, grouped: GroupedList): boolean {
  for (const key of selected) {
    const group = grouped.groups.get(key);
    const wanted =
      group === undefined
        ? grouped.rowById.get(key)?.ownership === "wanted"
        : group.members.some((member) => member.ownership === "wanted");
    if (wanted) return true;
  }
  return false;
}

/**
 * 作品の操作を押せるか。対象は、選択（本の ID・グループの key）を本に広げたもの。
 * 「まとめる」は 2 冊以上、「外す」は "none" でない本が、「自動に戻す」は上書きのある本が、1 冊でもあるとき。
 */
function workActionStates(selected: ReadonlySet<string>, grouped: GroupedList) {
  const books = new Set<string>();
  let canExclude = false;
  let canReset = false;
  const visit = (row: ListRow) => {
    if (books.size < 2) books.add(row.id);
    if (row.workId !== WORK_NONE) canExclude = true;
    if (row.workId !== null) canReset = true;
  };
  for (const key of selected) {
    const group = grouped.groups.get(key);
    if (group !== undefined) group.members.forEach(visit);
    else {
      const row = grouped.rowById.get(key);
      if (row !== undefined) visit(row);
    }
  }
  return { canMerge: books.size >= 2, canExclude, canReset };
}

/** 右クリックメニューの中身。`ContextMenu` の子として置く。開いたときにだけ描かれる。 */
export function ListContextMenuContent({
  grouped,
  inTrash,
  onOpenEdit,
  onAction,
  onBulkChange,
}: ListContextMenuContentProps) {
  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);

  /** 選んでいる本の ID（表示の順。グループは全巻）。 */
  const selectedIds = (): string[] => expandToBookIds(selected, grouped);

  // 欲しい本（購入済みにできる本）
  const wantedSelectedIds = (): string[] =>
    selectedIds().filter((id) => grouped.rowById.get(id)?.ownership === "wanted");
  const anyWantedSelected = hasWanted(selected, grouped);
  const works = grouped.enabled ? workActionStates(selected, grouped) : null;
  // focus がグループの行なら、編集できない
  const focusIsGroup = focus !== null && grouped.groups.has(focus);

  const runBulkChange = (change: BulkChange) => {
    const targets = selectedIds();
    if (targets.length > 0) onBulkChange(change, targets);
  };

  const runAction = (action: ListAction) => {
    const targets = selectedIds();
    if (action === "edit") {
      const target = focus ?? targets[0];
      if (target !== undefined) onOpenEdit(target);
      return;
    }
    if (action === "markPurchased") {
      const wanted = wantedSelectedIds();
      if (wanted.length > 0) onAction(action, wanted);
      return;
    }
    if (targets.length > 0) onAction(action, targets);
  };

  return (
    <ContextMenuContent className="w-52">
      <ContextMenuItem disabled={focusIsGroup} onSelect={() => runAction("edit")}>
        {ja.list.contextMenu.edit}
      </ContextMenuItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger>{ja.list.contextMenu.bulkEdit}</ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuLabel>{ja.list.contextMenu.bulkReadStatus}</ContextMenuLabel>
          {READ_STATUS_CHOICES.map((status) => (
            <ContextMenuItem
              key={status}
              onSelect={() => runBulkChange({ field: "read_status", value: status })}
            >
              {ja.labels.readStatus[status]}
            </ContextMenuItem>
          ))}
          <ContextMenuSeparator />
          <ContextMenuLabel>{ja.list.contextMenu.bulkOwnership}</ContextMenuLabel>
          {OWNERSHIP_CHOICES.map((ownership) => (
            <ContextMenuItem
              key={ownership}
              onSelect={() => runBulkChange({ field: "ownership", value: ownership })}
            >
              {ja.labels.ownership[ownership]}
            </ContextMenuItem>
          ))}
          <ContextMenuSeparator />
          <ContextMenuLabel>{ja.list.contextMenu.bulkRating}</ContextMenuLabel>
          {RATING_CHOICES.map((rating) => (
            <ContextMenuItem
              key={rating}
              onSelect={() => runBulkChange({ field: "rating", value: rating })}
            >
              {"★".repeat(rating)}
            </ContextMenuItem>
          ))}
          <ContextMenuItem onSelect={() => runBulkChange({ field: "rating", value: null })}>
            {ja.list.contextMenu.ratingNone}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => runAction("location")}>
            {ja.list.contextMenu.bulkLocation}
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => runAction("addTags")}>
            {ja.list.contextMenu.bulkAddTags}
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => runAction("removeTags")}>
            {ja.list.contextMenu.bulkRemoveTags}
          </ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      {anyWantedSelected && (
        <ContextMenuItem onSelect={() => runAction("markPurchased")}>
          {ja.list.contextMenu.markPurchased}
        </ContextMenuItem>
      )}
      <ContextMenuItem onSelect={() => runAction("refetch")}>
        {ja.list.contextMenu.refetch}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => runAction("export")}>
        {ja.list.contextMenu.export}
      </ContextMenuItem>
      {works !== null && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem disabled={!works.canMerge} onSelect={() => runAction("mergeWorks")}>
            {ja.list.contextMenu.mergeWorks}
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!works.canExclude}
            onSelect={() => runAction("excludeFromWorks")}
          >
            {ja.list.contextMenu.excludeFromWorks}
          </ContextMenuItem>
          <ContextMenuItem disabled={!works.canReset} onSelect={() => runAction("resetWorks")}>
            {ja.list.contextMenu.resetWorks}
          </ContextMenuItem>
        </>
      )}
      <ContextMenuSeparator />
      {inTrash ? (
        <>
          <ContextMenuItem onSelect={() => runAction("restore")}>
            {ja.list.contextMenu.restore}
          </ContextMenuItem>
          <ContextMenuItem variant="destructive" onSelect={() => runAction("purge")}>
            {ja.list.contextMenu.purge}
          </ContextMenuItem>
        </>
      ) : (
        <ContextMenuItem variant="destructive" onSelect={() => runAction("trash")}>
          {ja.list.contextMenu.trash}
        </ContextMenuItem>
      )}
    </ContextMenuContent>
  );
}
