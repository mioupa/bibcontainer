// 蔵書の一覧の表（FR-LIST-01／02、9.5）。TanStack Table（列の状態）＋ TanStack Virtual（仮想スクロール）。
//
// 設計:
// - 行は絞り込み・並べ替え済みの ListRow[] をそのまま仮想スクロールに渡す。TanStack Table の行モデルは作らない
//   （絞り込みのたびに 2 万行ぶんの行オブジェクトを作り直す無駄を避ける）。Table は列の定義・表示・幅・並べ替えの状態を受け持つ。
// - 並べ替えは features/library/sort.ts の純粋関数。Table には manualSorting を指定し、Shift+クリックの複数列の操作だけを使う。
// - 列の状態（並べ替え・表示・幅）は Zustand（stores/tableStore.ts）が持ち、Table に controlled で渡す。
// - 行の選択は stores/selectionStore.ts。キーボードの操作（矢印・Shift・Home/End・PageUp/Down・⌘A・Enter・Delete）はこのファイルで受ける。
import { useTable } from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDownIcon, ArrowUpIcon } from "lucide-react";
import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import type { PriceSettings } from "@/bindings/PriceSettings";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type { BulkChange } from "@/features/list/bulk";
import { bookColumns, tableFeaturesDef } from "@/features/list/columns";
import { Cell } from "@/features/list/cells";
import { cellTitle, type CellContext } from "@/features/list/cellText";
import { NUMERIC_COLUMN_IDS, type ColumnId } from "@/features/list/columnMeta";
import { matchShortcut } from "@/features/shortcuts/shortcuts";
import { ja } from "@/i18n/ja";
import { detectPlatform } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useSelectionStore } from "@/stores/selectionStore";
import { useTableStore } from "@/stores/tableStore";
import type { CCodeTable, ListRow, Ownership, ReadStatus } from "@/types/contracts";

export const ROW_HEIGHT = 48;
export const HEADER_HEIGHT = 32;

/**
 * 一覧から起こせる操作（右クリックメニューとキーボード）。実際の処理は呼び出し側が行う。
 * location・addTags・removeTags は、値を選ぶダイアログを開く操作。markPurchased は、欲しい本を所有にする
 * （`ids` には欲しい本だけが入る）。
 */
export type ListAction =
  | "edit"
  | "refetch"
  | "export"
  | "trash"
  | "restore"
  | "purge"
  | "location"
  | "addTags"
  | "removeTags"
  | "markPurchased";

export type { BulkChange };

const READ_STATUS_CHOICES = Object.keys(ja.labels.readStatus) as ReadStatus[];
const OWNERSHIP_CHOICES = Object.keys(ja.labels.ownership) as Ownership[];
const RATING_CHOICES = [5, 4, 3, 2, 1] as const;

export interface BookTableProps {
  /** 表示する行（絞り込み・並べ替え済み）。 */
  rows: ListRow[];
  cCodes: CCodeTable;
  locationPaths: ReadonlyMap<string, string>;
  /** 価格の表示設定（税込を併記するか）。 */
  price?: PriceSettings;
  isLoading: boolean;
  /** Enter・ダブルクリック・メニューの「編集」で、編集を開く。 */
  onOpenEdit: (id: string) => void;
  /** メニューやキーボードからの操作。ids は対象の本の ID。 */
  onAction: (action: Exclude<ListAction, "edit">, ids: string[]) => void;
  /** メニューの「一括変更」から選んだ変更（読書状況・所有状態・評価）。ids は対象の本の ID。 */
  onBulkChange: (change: BulkChange, ids: string[]) => void;
  /** ゴミ箱を開いているか。そうなら、メニューは「ゴミ箱へ移動」の代わりに「戻す」「完全に削除」を出す。 */
  inTrash?: boolean;
}

interface ColumnLayout {
  id: ColumnId;
  width: number;
}

// OS は起動中に変わらないので、1 度だけ判定する
const PLATFORM = detectPlatform();

function parseLayout(key: string): ColumnLayout[] {
  return key === ""
    ? []
    : key.split("|").map((part) => {
        const [id, width] = part.split(":");
        return { id: id as ColumnId, width: Number(width) };
      });
}

export function BookTable({
  rows,
  cCodes,
  locationPaths,
  price,
  isLoading,
  onOpenEdit,
  onAction,
  onBulkChange,
  inTrash = false,
}: BookTableProps) {
  const sorting = useTableStore((s) => s.sorting);
  const columnVisibility = useTableStore((s) => s.columnVisibility);
  const columnSizing = useTableStore((s) => s.columnSizing);
  const setSorting = useTableStore((s) => s.setSorting);
  const setColumnVisibility = useTableStore((s) => s.setColumnVisibility);
  const setColumnSizing = useTableStore((s) => s.setColumnSizing);

  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);
  const click = useSelectionStore((s) => s.click);
  const move = useSelectionStore((s) => s.move);
  const selectAll = useSelectionStore((s) => s.selectAll);
  const prune = useSelectionStore((s) => s.prune);
  const clear = useSelectionStore((s) => s.clear);

  const table = useTable({
    features: tableFeaturesDef,
    columns: bookColumns,
    data: rows,
    getRowId: (row) => row.id,
    state: { sorting, columnVisibility, columnSizing },
    onSortingChange: setSorting,
    onColumnVisibilityChange: setColumnVisibility,
    onColumnSizingChange: setColumnSizing,
    manualSorting: true,
    enableMultiSort: true,
    // 幅の変更は、ドラッグをやめたときに確定する（途中の移動のたびに保存しない）
    columnResizeMode: "onEnd",
  });

  const headers = table.getHeaderGroups()[0]?.headers ?? [];
  const layoutKey = table
    .getVisibleLeafColumns()
    .map((column) => `${column.id}:${column.getSize()}`)
    .join("|");
  const layout = useMemo(() => parseLayout(layoutKey), [layoutKey]);
  const totalWidth = layout.reduce((sum, column) => sum + column.width, 0);

  const cellContext = useMemo<CellContext>(
    () => ({ cCodes, locationPaths, price }),
    [cCodes, locationPaths, price],
  );

  const ids = useMemo(() => rows.map((row) => row.id), [rows]);
  const indexById = useMemo(() => new Map(ids.map((id, index) => [id, index])), [ids]);

  // 絞り込みや削除で消えた行を、選択から外す
  useEffect(() => {
    prune(new Set(ids));
  }, [ids, prune]);

  const scrollRef = useRef<HTMLDivElement>(null);
  // useVirtualizer は関数を返すので React Compiler の自動メモ化の対象外だが、このコンポーネントは手でメモ化している
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    // ヘッダー（sticky）が先頭にあるぶん、行の位置をずらす
    scrollMargin: HEADER_HEIGHT,
    scrollPaddingStart: HEADER_HEIGHT,
    getItemKey: (index) => ids[index] ?? index,
  });

  const scrollToFocus = () => {
    const focusId = useSelectionStore.getState().focus;
    const index = focusId === null ? undefined : indexById.get(focusId);
    if (index !== undefined) virtualizer.scrollToIndex(index, { align: "auto" });
  };

  // 検索欄の ISBN などで、ほかの場所から選んだ本が見えるよう、focus した行までスクロールする
  // （見えている行なら動かない）
  useEffect(() => {
    if (focus === null) return;
    const index = indexById.get(focus);
    if (index !== undefined) virtualizer.scrollToIndex(index, { align: "auto" });
    // virtualizer は毎回同じ参照ではないので、focus と並びが変わったときだけ動かす
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus, indexById]);

  const pageSize = () =>
    Math.max(
      1,
      Math.floor(((scrollRef.current?.clientHeight ?? 0) - HEADER_HEIGHT) / ROW_HEIGHT) - 1,
    );

  const selectedIds = (): string[] => ids.filter((id) => selected.has(id));

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.nativeEvent.isComposing) return;
    const shortcut = matchShortcut(event.nativeEvent, PLATFORM);
    if (shortcut === "edit") {
      if (focus !== null) {
        event.preventDefault();
        onOpenEdit(focus);
      }
      return;
    }
    if (shortcut === "trash") {
      // ゴミ箱の中では、さらにゴミ箱へ移すことはない（完全に削除は、確認のあるメニューから）
      const targets = inTrash ? [] : selectedIds();
      if (targets.length > 0) {
        event.preventDefault();
        onAction("trash", targets);
      }
      return;
    }

    const mod = PLATFORM === "mac" ? event.metaKey : event.ctrlKey;
    if (mod && event.code === "KeyA") {
      event.preventDefault();
      selectAll(ids);
      return;
    }
    if (event.altKey || event.ctrlKey || event.metaKey) return;

    const options = { extend: event.shiftKey, pageSize: pageSize() };
    switch (event.code) {
      case "ArrowDown":
        move(ids, "down", options);
        break;
      case "ArrowUp":
        move(ids, "up", options);
        break;
      case "Home":
        move(ids, "first", options);
        break;
      case "End":
        move(ids, "last", options);
        break;
      case "PageDown":
        move(ids, "pageDown", options);
        break;
      case "PageUp":
        move(ids, "pageUp", options);
        break;
      case "Escape":
        clear();
        return;
      default:
        return;
    }
    event.preventDefault();
    scrollToFocus();
  };

  const handleRowClick = useCallback(
    (id: string, event: MouseEvent) => {
      click(ids, id, {
        toggle: PLATFORM === "mac" ? event.metaKey : event.ctrlKey,
        range: event.shiftKey,
      });
    },
    [click, ids],
  );

  // 右クリックした行が選択の外なら、その行だけを選ぶ。行の外ならメニューを出さない
  const handleContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    const rowElement = (event.target as HTMLElement).closest<HTMLElement>("[data-row-id]");
    const id = rowElement?.dataset.rowId;
    if (!id) {
      event.preventDefault();
      return;
    }
    if (!useSelectionStore.getState().selected.has(id)) {
      click(ids, id, { toggle: false, range: false });
    }
  };

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

  const resizingId = table.state.columnResizing?.isResizingColumn;
  const resizeOffset = table.state.columnResizing?.deltaOffset ?? 0;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild onContextMenu={handleContextMenu}>
        <div
          ref={scrollRef}
          role="grid"
          tabIndex={0}
          aria-label={ja.list.ariaLabel}
          aria-rowcount={rows.length + 1}
          aria-colcount={layout.length}
          aria-multiselectable
          onKeyDown={handleKeyDown}
          className="relative min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-ring/60 focus-visible:ring-inset"
        >
          {/* ヘッダー（縦にスクロールしても上に張りつく） */}
          <div
            role="row"
            style={{ width: totalWidth, height: HEADER_HEIGHT }}
            className="sticky top-0 z-10 flex border-b bg-muted text-xs font-medium text-muted-foreground"
          >
            {headers.map((header) => {
              const id = header.column.id as ColumnId;
              const sorted = header.column.getIsSorted();
              const sortable = header.column.getCanSort();
              const name = ja.list.columns[id];
              const label = (
                <>
                  <span className="truncate">{name}</span>
                  {sorted === "asc" && <ArrowUpIcon className="size-3 shrink-0" aria-hidden />}
                  {sorted === "desc" && <ArrowDownIcon className="size-3 shrink-0" aria-hidden />}
                  {sorted && sorting.length > 1 && (
                    <span className="shrink-0 text-[10px] tabular-nums">
                      {header.column.getSortIndex() + 1}
                    </span>
                  )}
                </>
              );
              return (
                <div
                  key={header.id}
                  role="columnheader"
                  aria-sort={
                    sorted === "asc" ? "ascending" : sorted === "desc" ? "descending" : "none"
                  }
                  style={{ width: header.getSize() }}
                  className="relative flex shrink-0 items-center"
                >
                  {sortable ? (
                    <button
                      type="button"
                      title={ja.list.sortHint}
                      onClick={header.column.getToggleSortingHandler()}
                      className={cn(
                        "flex h-full min-w-0 flex-1 items-center gap-1 px-2 hover:text-foreground focus-visible:outline-none",
                        NUMERIC_COLUMN_IDS.has(id) && "justify-end",
                      )}
                    >
                      {label}
                    </button>
                  ) : (
                    <div className="flex h-full min-w-0 flex-1 items-center gap-1 px-2">
                      {label}
                    </div>
                  )}
                  {header.column.getCanResize() && (
                    <div
                      role="separator"
                      aria-orientation="vertical"
                      aria-label={ja.list.resizeColumn(name)}
                      onMouseDown={header.getResizeHandler()}
                      onTouchStart={header.getResizeHandler()}
                      onDoubleClick={() => header.column.resetSize()}
                      style={
                        resizingId === header.column.id
                          ? { transform: `translateX(${resizeOffset}px)` }
                          : undefined
                      }
                      className={cn(
                        "absolute top-0 right-0 z-20 h-full w-2 cursor-col-resize touch-none select-none",
                        "after:absolute after:top-1.5 after:right-0.5 after:h-[calc(100%-12px)] after:w-px after:bg-border",
                        resizingId === header.column.id && "after:w-0.5 after:bg-primary",
                      )}
                    />
                  )}
                </div>
              );
            })}
          </div>

          {/* 行（見えている範囲だけを描く） */}
          <div
            style={{ height: virtualizer.getTotalSize(), width: totalWidth }}
            className="relative"
          >
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index];
              if (!row) return null;
              return (
                <BookRow
                  key={row.id}
                  row={row}
                  index={item.index}
                  top={item.start - HEADER_HEIGHT}
                  selected={selected.has(row.id)}
                  focused={focus === row.id}
                  layout={layout}
                  totalWidth={totalWidth}
                  cellContext={cellContext}
                  onClick={handleRowClick}
                  onDoubleClick={onOpenEdit}
                />
              );
            })}
          </div>

          {rows.length === 0 && (
            <div className="pointer-events-none absolute inset-x-0 top-24 flex flex-col items-center gap-1 text-sm text-muted-foreground">
              {isLoading ? (
                <p>{ja.list.loading}</p>
              ) : (
                <>
                  <p>{ja.list.empty}</p>
                  <p className="text-xs">{ja.list.emptyHint}</p>
                </>
              )}
            </div>
          )}
        </div>
      </ContextMenuTrigger>
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
    </ContextMenu>
  );
}

interface BookRowProps {
  row: ListRow;
  index: number;
  top: number;
  selected: boolean;
  focused: boolean;
  layout: ColumnLayout[];
  totalWidth: number;
  cellContext: CellContext;
  onClick: (id: string, event: MouseEvent) => void;
  onDoubleClick: (id: string) => void;
}

/** 1 行。選択・focus・列の配置・セルの文脈が変わらないかぎり、再描画しない。 */
const BookRow = memo(function BookRow({
  row,
  index,
  top,
  selected,
  focused,
  layout,
  totalWidth,
  cellContext,
  onClick,
  onDoubleClick,
}: BookRowProps) {
  return (
    <div
      role="row"
      data-row-id={row.id}
      aria-rowindex={index + 2}
      aria-selected={selected}
      onClick={(event) => onClick(row.id, event)}
      onDoubleClick={() => onDoubleClick(row.id)}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width: totalWidth,
        height: ROW_HEIGHT,
        transform: `translateY(${top}px)`,
      }}
      className={cn(
        "flex cursor-default items-center border-b border-border/60 text-sm select-none",
        selected ? "bg-primary/15" : "hover:bg-accent/60",
        focused && "ring-1 ring-primary/70 ring-inset",
        row.deleted && "text-muted-foreground",
      )}
    >
      {layout.map((column) => (
        <div
          key={column.id}
          role="gridcell"
          title={cellTitle(column.id, row, cellContext)}
          style={{ width: column.width }}
          className={cn(
            "flex h-full min-w-0 shrink-0 items-center overflow-hidden px-2",
            NUMERIC_COLUMN_IDS.has(column.id) && "justify-end tabular-nums",
          )}
        >
          <Cell id={column.id} row={row} ctx={cellContext} />
        </div>
      ))}
    </div>
  );
});
