// 蔵書の一覧の表（FR-LIST-01／02、9.5）。TanStack Table（列の状態）＋ TanStack Virtual（仮想スクロール）。
//
// 設計:
// - 行は絞り込み・並べ替え済みの ListRow[] をそのまま仮想スクロールに渡す。TanStack Table の行モデルは作らない
//   （絞り込みのたびに 2 万行ぶんの行オブジェクトを作り直す無駄を避ける）。Table は列の定義・表示・幅・並べ替えの状態を受け持つ。
// - 並べ替えは features/library/sort.ts の純粋関数。Table には manualSorting を指定し、Shift+クリックの複数列の操作だけを使う。
// - 列の状態（並べ替え・表示・並び順・幅）は Zustand（stores/tableStore.ts）が持ち、Table に controlled で渡す。
// - 行の選択は stores/selectionStore.ts。キーボードの操作のうち、矢印・Shift・Home/End・PageUp/Down はこのファイルで受ける。
//   Enter・Delete・⌘A・Esc、クリック、右クリックの対象の決め方、選択行へのスクロールは、グリッド（BookGrid）と共通
//   （useListInteraction.ts）。右クリックメニューの中身も共通（ListContextMenu.tsx）。
// - 巻をまとめた表示（FR-LIST-08）: `grouped`（useGroupedList）の entries（グループの行と、本の行）を並べる。選択・移動・スクロールは
//   entries の key の上で行い、グループの key は「その全巻」を表す（操作の対象にするときに本の ID にする。groupActions.ts・
//   ListContextMenu.tsx）。まとめていないときは、`rows` を 1 冊ずつの行として並べる（`grouped` がなければ、いつもこれ）。
//   まとめているときだけ、role は treegrid（行に aria-level、グループの行に aria-expanded）。
import { useTable } from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDownIcon, ArrowUpIcon, ChevronRightIcon } from "lucide-react";
import { memo, useMemo, useRef, type KeyboardEvent, type MouseEvent } from "react";
import { BookCover } from "@/components/BookCover";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { groupRows, groupSummary, type GroupEntry } from "@/features/library/grouping";
import { bookColumns, tableFeaturesDef } from "@/features/list/columns";
import { Cell } from "@/features/list/cells";
import { authorsText, cellTitle, type CellContext } from "@/features/list/cellText";
import { NUMERIC_COLUMN_IDS, resolveColumnOrder, type ColumnId } from "@/features/list/columnMeta";
import { ListContextMenuContent } from "@/features/list/ListContextMenu";
import { ListEmptyState } from "@/features/list/ListEmptyState";
import type { BulkChange, ListAction, ListViewProps } from "@/features/list/listActions";
import {
  useListInteraction,
  usePruneSelection,
  useScrollToFocus,
} from "@/features/list/useListInteraction";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";
import { useSelectionStore } from "@/stores/selectionStore";
import { useTableStore } from "@/stores/tableStore";
import type { ListRow } from "@/types/contracts";

export const ROW_HEIGHT = 48;
export const HEADER_HEIGHT = 32;

export type { BulkChange, ListAction };

/** 表が受け取る props（グリッドと同じ）。 */
export type BookTableProps = ListViewProps;

interface ColumnLayout {
  id: ColumnId;
  width: number;
}

function parseLayout(key: string): ColumnLayout[] {
  return key === ""
    ? []
    : key.split("|").map((part) => {
        const [id, width] = part.split(":");
        return { id: id as ColumnId, width: Number(width) };
      });
}

/** 巻をまとめない並び（`grouped` がないとき）の展開の状態。 */
const NO_GROUPS_EXPANDED: ReadonlySet<string> = new Set();

export function BookTable({
  rows,
  grouped,
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
  const savedColumnOrder = useTableStore((s) => s.columnOrder);
  const columnSizing = useTableStore((s) => s.columnSizing);
  const setSorting = useTableStore((s) => s.setSorting);
  const setColumnVisibility = useTableStore((s) => s.setColumnVisibility);
  const setColumnOrder = useTableStore((s) => s.setColumnOrder);
  const setColumnSizing = useTableStore((s) => s.setColumnSizing);
  // 保存された並び順（空なら既定の順。載っていない列は末尾）を、全列の並びにして Table に渡す
  const columnOrder = useMemo(() => resolveColumnOrder(savedColumnOrder), [savedColumnOrder]);

  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);
  const move = useSelectionStore((s) => s.move);
  const moveTo = useSelectionStore((s) => s.moveTo);

  // 表示する並び（グループの行と本の行）。`grouped` がなければ、行を 1 冊ずつ並べる
  const list = useMemo(
    () => grouped ?? groupRows(rows, false, NO_GROUPS_EXPANDED),
    [grouped, rows],
  );
  const entries = list.entries;

  const table = useTable({
    features: tableFeaturesDef,
    columns: bookColumns,
    data: rows,
    getRowId: (row) => row.id,
    state: { sorting, columnVisibility, columnOrder, columnSizing },
    onSortingChange: setSorting,
    onColumnVisibilityChange: setColumnVisibility,
    onColumnOrderChange: setColumnOrder,
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

  // 選択・移動・スクロールの単位は、行の key（本の ID か、グループの key）
  const ids = useMemo(() => entries.map((entry) => entry.key), [entries]);
  const indexById = useMemo(() => new Map(ids.map((id, index) => [id, index])), [ids]);

  // 絞り込みや削除で消えた行を、選択から外す
  usePruneSelection(ids);

  const scrollRef = useRef<HTMLDivElement>(null);
  // useVirtualizer は関数を返すので React Compiler の自動メモ化の対象外だが、このコンポーネントは手でメモ化している
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: entries.length,
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

  // focus した行（検索欄の ISBN など、ほかの場所から選んだ本を含む）が見えるよう、その行までスクロールする
  // （規則は useScrollToFocus。0011 §16）
  useScrollToFocus(indexById, (index) => virtualizer.scrollToIndex(index, { align: "auto" }));

  const pageSize = () =>
    Math.max(
      1,
      Math.floor(((scrollRef.current?.clientHeight ?? 0) - HEADER_HEIGHT) / ROW_HEIGHT) - 1,
    );

  const {
    handleCommonKey,
    handleItemClick,
    handleContextMenu,
    expandGroup,
    collapseGroup,
    toggleGroup,
  } = useListInteraction({
    ids,
    grouped: list,
    inTrash,
    onOpenEdit,
    onAction,
  });

  /**
   * 左右の矢印（ツリーの操作）。処理したら true。
   * → : 閉じたグループなら開く。開いたグループなら、最初のメンバーへ。 ← : 開いたグループなら畳む。メンバーなら、グループの行へ。
   */
  const handleTreeKey = (code: "ArrowLeft" | "ArrowRight"): boolean => {
    const focusKey = useSelectionStore.getState().focus;
    if (focusKey === null) return false;
    const group = list.groups.get(focusKey);
    if (group !== undefined) {
      const open = list.expanded.has(focusKey);
      if (code === "ArrowRight") {
        const first = group.members[0];
        if (!open) expandGroup(focusKey);
        else if (first !== undefined) moveTo(ids, first.id, { extend: false });
      } else if (open) collapseGroup(focusKey);
      return true;
    }
    const index = indexById.get(focusKey);
    const entry = index === undefined ? undefined : entries[index];
    if (code === "ArrowLeft" && entry?.kind === "book" && entry.groupKey !== null) {
      moveTo(ids, entry.groupKey, { extend: false });
      return true;
    }
    return false;
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (handleCommonKey(event)) return;

    if (
      list.enabled &&
      !event.shiftKey &&
      (event.code === "ArrowLeft" || event.code === "ArrowRight") &&
      handleTreeKey(event.code)
    ) {
      event.preventDefault();
      scrollToFocus();
      return;
    }

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
      default:
        return;
    }
    event.preventDefault();
    scrollToFocus();
  };

  const resizingId = table.state.columnResizing?.isResizingColumn;
  const resizeOffset = table.state.columnResizing?.deltaOffset ?? 0;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild onContextMenu={handleContextMenu}>
        <div
          ref={scrollRef}
          role={list.enabled ? "treegrid" : "grid"}
          tabIndex={0}
          aria-label={ja.list.ariaLabel}
          aria-rowcount={entries.length + 1}
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
              const entry = entries[item.index];
              if (!entry) return null;
              if (entry.kind === "group") {
                return (
                  <GroupRow
                    key={entry.key}
                    entry={entry}
                    index={item.index}
                    top={item.start - HEADER_HEIGHT}
                    selected={selected.has(entry.key)}
                    focused={focus === entry.key}
                    layout={layout}
                    totalWidth={totalWidth}
                    cellContext={cellContext}
                    onClick={handleItemClick}
                    onToggle={toggleGroup}
                  />
                );
              }
              return (
                <BookRow
                  key={entry.key}
                  row={entry.row}
                  level={entry.level}
                  tree={list.enabled}
                  index={item.index}
                  top={item.start - HEADER_HEIGHT}
                  selected={selected.has(entry.key)}
                  focused={focus === entry.key}
                  layout={layout}
                  totalWidth={totalWidth}
                  cellContext={cellContext}
                  onClick={handleItemClick}
                  onDoubleClick={onOpenEdit}
                />
              );
            })}
          </div>

          {entries.length === 0 && <ListEmptyState isLoading={isLoading} />}
        </div>
      </ContextMenuTrigger>
      <ListContextMenuContent
        grouped={list}
        inTrash={inTrash}
        onOpenEdit={onOpenEdit}
        onAction={onAction}
        onBulkChange={onBulkChange}
      />
    </ContextMenu>
  );
}

interface BookRowProps {
  row: ListRow;
  /** 1: ふつうの行。2: 展開したグループの中の本（書名を字下げする）。 */
  level: 1 | 2;
  /** 巻をまとめた表示のとき true（行に aria-level を付ける）。 */
  tree: boolean;
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
  level,
  tree,
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
      aria-level={tree ? level : undefined}
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
            // グループの中の本は、書名を字下げして、子として読めるようにする
            level === 2 && column.id === "title" && "pl-9",
          )}
        >
          <Cell id={column.id} row={row} ctx={cellContext} />
        </div>
      ))}
    </div>
  );
});

interface GroupRowProps {
  entry: GroupEntry;
  index: number;
  top: number;
  selected: boolean;
  focused: boolean;
  layout: ColumnLayout[];
  totalWidth: number;
  cellContext: CellContext;
  onClick: (key: string, event: MouseEvent) => void;
  onToggle: (key: string) => void;
}

/**
 * 巻のグループの行（FR-LIST-08）。書名の列に、開閉のボタン・作品名・冊数。書影は先頭の巻（重なった見た目）、巻は範囲（1〜7）。
 * そのほかの列は、全巻が同じ値ならその値、ばらばらなら空（groupSummary）。
 */
const GroupRow = memo(function GroupRow({
  entry,
  index,
  top,
  selected,
  focused,
  layout,
  totalWidth,
  cellContext,
  onClick,
  onToggle,
}: GroupRowProps) {
  const { group, expanded } = entry;
  const summary = useMemo(() => groupSummary(group, cellContext), [group, cellContext]);
  const { row, blank } = summary;

  const renderCell = (id: ColumnId) => {
    switch (id) {
      case "title":
        return (
          <div className="flex min-w-0 flex-1 items-center gap-1.5">
            <button
              type="button"
              // 行はコンテナがキーボードを受けるので、ボタンは Tab の順に入れない（クリックだけ）
              tabIndex={-1}
              aria-label={expanded ? ja.list.group.collapse : ja.list.group.expand}
              // 行のクリック（選択）・ダブルクリック（開閉）にしない。開閉だけして、選択は変えない
              onClick={(event) => {
                event.stopPropagation();
                onToggle(group.key);
              }}
              onDoubleClick={(event) => event.stopPropagation()}
              className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
            >
              <ChevronRightIcon
                aria-hidden
                className={cn("size-4 transition-transform", expanded && "rotate-90")}
              />
            </button>
            <span className="min-w-0 flex-1">
              <Cell id="title" row={row} ctx={cellContext} />
            </span>
            <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none text-muted-foreground tabular-nums">
              {ja.list.group.count(group.members.length)}
            </span>
          </div>
        );
      case "cover":
        // 先頭の巻の書影。後ろに 1 枚ずらした枠を置いて、重なった（複数ある）見た目にする
        return (
          <span className="relative inline-flex">
            <span
              aria-hidden
              className="absolute inset-0 translate-x-[3px] -translate-y-[3px] rounded-[3px] bg-muted ring-1 ring-foreground/10"
            />
            <BookCover
              id={row.id}
              title={row.title}
              authors={authorsText(row)}
              coverHash={row.coverHash}
              variant="sm"
            />
          </span>
        );
      default:
        return blank.has(id) ? null : <Cell id={id} row={row} ctx={cellContext} />;
    }
  };

  return (
    <div
      role="row"
      data-row-id={group.key}
      aria-rowindex={index + 2}
      aria-level={1}
      aria-expanded={expanded}
      aria-selected={selected}
      onClick={(event) => onClick(group.key, event)}
      onDoubleClick={() => onToggle(group.key)}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width: totalWidth,
        height: ROW_HEIGHT,
        transform: `translateY(${top}px)`,
      }}
      className={cn(
        "flex cursor-default items-center border-b border-border/60 bg-muted/30 text-sm select-none",
        selected ? "bg-primary/15" : "hover:bg-accent/60",
        focused && "ring-1 ring-primary/70 ring-inset",
        row.deleted && "text-muted-foreground",
      )}
    >
      {layout.map((column) => (
        <div
          key={column.id}
          role="gridcell"
          title={blank.has(column.id) ? undefined : cellTitle(column.id, row, cellContext)}
          style={{ width: column.width }}
          className={cn(
            "flex h-full min-w-0 shrink-0 items-center overflow-hidden px-2",
            NUMERIC_COLUMN_IDS.has(column.id) && "justify-end tabular-nums",
          )}
        >
          {renderCell(column.id)}
        </div>
      ))}
    </div>
  );
});
