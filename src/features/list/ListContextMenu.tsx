// 一覧の右クリックメニューの中身（表・グリッドで共通）。メニューを開く操作（ContextMenu・ContextMenuTrigger）は、
// 各一覧が持つ。ここは、開いたメニューの項目と、押したときに呼ぶ処理だけ。
//
// 操作の対象は、選択している本（`selectionStore`）。右クリックした本が選択の外なら、一覧が先にその本だけを選ぶ
// （`useListInteraction` の `handleContextMenu`）。
import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import type { BulkChange, ListAction, ListViewProps } from "@/features/list/listActions";
import { ja } from "@/i18n/ja";
import { useSelectionStore } from "@/stores/selectionStore";
import type { Ownership, ReadStatus } from "@/types/contracts";

const READ_STATUS_CHOICES = Object.keys(ja.labels.readStatus) as ReadStatus[];
const OWNERSHIP_CHOICES = Object.keys(ja.labels.ownership) as Ownership[];
const RATING_CHOICES = [5, 4, 3, 2, 1] as const;

export interface ListContextMenuContentProps extends Pick<
  ListViewProps,
  "rows" | "onOpenEdit" | "onAction" | "onBulkChange"
> {
  /** 表示している順の ID（`rows` と同じ並び）。 */
  ids: readonly string[];
  /** ID → `rows` の何番目か。 */
  indexById: ReadonlyMap<string, number>;
  inTrash: boolean;
}

/** 右クリックメニューの中身。`ContextMenu` の子として置く。開いたときにだけ描かれる。 */
export function ListContextMenuContent({
  rows,
  ids,
  indexById,
  inTrash,
  onOpenEdit,
  onAction,
  onBulkChange,
}: ListContextMenuContentProps) {
  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);

  const selectedIds = (): string[] => ids.filter((id) => selected.has(id));

  // 欲しい本（購入済みにできる本）。選択の数だけ調べる
  const wantedSelectedIds = (): string[] =>
    [...selected].filter((id) => {
      const index = indexById.get(id);
      return index !== undefined && rows[index]?.ownership === "wanted";
    });
  const anyWantedSelected = selected.size > 0 && wantedSelectedIds().length > 0;

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
      <ContextMenuItem onSelect={() => runAction("edit")}>
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
