// 蔵書の一覧のグリッド（FR-LIST-07、9.5）。書影のカードを敷き詰め、行ごとに TanStack Virtual で仮想スクロールする。
//
// 設計:
// - 表（BookTable）と同じ props・同じ選択（stores/selectionStore.ts）・同じ右クリックメニュー（ListContextMenu.tsx）。
//   共通の動き（共通のキー、クリック、右クリックの対象、選択行へのスクロール、選択の整理）は useListInteraction.ts。
//   このファイルが持つのは、カードの並べ方と、矢印キーでの 2 次元の移動（gridLayout.ts の純粋関数）だけ。
// - カードの大きさは rem で決める（文字サイズの設定で、書影も一緒に大きくなる）。仮想スクロールの行の高さは px で要るので、
//   <html> の文字サイズ（useRootFontSize）に掛けて出す。カードの高さは固定（書名 2 行ぶんの高さを取る）で、行の高さも固定。
// - 列の数は、スクロール領域の幅から決める（初めは clientWidth、そのあとは ResizeObserver）。幅がまだ分からない間は、
//   行を作らない（列の数が違う並びに「スクロール待ち」が使われて、行き先がずれるのを防ぐ）。
// - 画面の外のカード（と書影の <img>）は DOM に置かない。書影は WebKit のメモリキャッシュにも残さない
//   （サムネイルは no-store。docs/decisions/0024）ので、何千冊あっても、メモリは見えている分で済む。
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { BookCover } from "@/components/BookCover";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { authorsText } from "@/features/list/cellText";
import {
  gridColumnCount,
  gridIndex,
  gridMove,
  layoutGrid,
  type GridDirection,
} from "@/features/list/gridLayout";
import { ListContextMenuContent } from "@/features/list/ListContextMenu";
import { ListEmptyState } from "@/features/list/ListEmptyState";
import type { ListViewProps } from "@/features/list/listActions";
import {
  useListInteraction,
  usePruneSelection,
  useScrollToFocus,
} from "@/features/list/useListInteraction";
import { useRootFontSize } from "@/features/list/useRootFontSize";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";
import { useSelectionStore } from "@/stores/selectionStore";
import type { ListRow } from "@/types/contracts";

// カードの寸法（rem）。書影の幅 7rem ＋ 両側の余白 0.5rem ずつ。
// 高さは、余白 1 ＋ 書影 9.8（7 × 7/5）＋ 書名 2 行 2.5 ＋ 巻・著者 1 行 1 ＋ すき間 0.625 ≒ 14.93 に、少し余裕を足した値。
const COVER_WIDTH_REM = 7;
const CARD_WIDTH_REM = 8;
const CARD_HEIGHT_REM = 15;
/** カードどうしの間隔。 */
const GAP_REM = 0.75;
/** スクロール領域の縁とカードの間の余白（上下左右）。 */
const PADDING_REM = 0.75;

/** 見えている範囲の上下に、余分に描く行数。 */
const OVERSCAN_ROWS = 3;

/** カードの 2 行目（巻・著者）の区切り。 */
const META_SEPARATOR = " · ";

const DIRECTION_BY_CODE: Partial<Record<string, GridDirection>> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
  Home: "home",
  End: "end",
  PageUp: "pageUp",
  PageDown: "pageDown",
};

/** グリッドが受け取る props（表と同じ）。 */
export type BookGridProps = ListViewProps;

export function BookGrid({
  rows,
  isLoading,
  onOpenEdit,
  onAction,
  onBulkChange,
  inTrash = false,
}: BookGridProps) {
  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);
  const moveTo = useSelectionStore((s) => s.moveTo);

  const ids = useMemo(() => rows.map((row) => row.id), [rows]);
  const indexById = useMemo(() => new Map(ids.map((id, index) => [id, index])), [ids]);

  // 絞り込みや削除で消えた行を、選択から外す
  usePruneSelection(ids);

  // ---- 寸法（rem → px）と列の数 ----
  const rootPx = useRootFontSize();
  const cardWidthPx = CARD_WIDTH_REM * rootPx;
  const rowHeightPx = CARD_HEIGHT_REM * rootPx;
  const gapPx = GAP_REM * rootPx;
  const paddingPx = PADDING_REM * rootPx;

  const scrollRef = useRef<HTMLDivElement>(null);
  // スクロール領域の幅（px）。測るまでは null
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (element === null) return;
    const update = () => setWidth(element.clientWidth);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const columns = width === null ? null : gridColumnCount(width, cardWidthPx, gapPx, paddingPx);
  const grid = useMemo(() => (columns === null ? [] : layoutGrid(ids, columns)), [ids, columns]);
  const positions = useMemo(() => gridIndex(grid), [grid]);
  // 本の ID → 何行目か（選択行へのスクロールの行き先）
  const rowOfId = useMemo(
    () => new Map([...positions].map(([id, position]) => [id, position.row])),
    [positions],
  );

  // useVirtualizer は関数を返すので React Compiler の自動メモ化の対象外だが、このコンポーネントは手でメモ化している
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: grid.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeightPx,
    gap: gapPx,
    // 上下の余白。「見えるようにスクロール」でも、この余白が見える位置まで動かす
    paddingStart: paddingPx,
    paddingEnd: paddingPx,
    scrollPaddingStart: paddingPx,
    scrollPaddingEnd: paddingPx,
    overscan: OVERSCAN_ROWS,
  });

  // 行の高さは estimateSize が返す。文字サイズが変わったら、測り直させる
  useEffect(() => {
    virtualizer.measure();
    // virtualizer は毎回同じ参照ではないので、行の高さが変わったときだけ動かす
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rowHeightPx]);

  const scrollToFocus = () => {
    const focusId = useSelectionStore.getState().focus;
    const row = focusId === null ? undefined : rowOfId.get(focusId);
    if (row !== undefined) virtualizer.scrollToIndex(row, { align: "auto" });
  };

  // focus した本が見えるよう、その本のある行までスクロールする（規則は useScrollToFocus。0011 §16）
  useScrollToFocus(rowOfId, (row) => virtualizer.scrollToIndex(row, { align: "auto" }));

  /** PageUp・PageDown で動く行数（見えている行の数。1 以上）。 */
  const pageRows = () =>
    Math.max(
      1,
      Math.floor(((scrollRef.current?.clientHeight ?? 0) - 2 * paddingPx) / (rowHeightPx + gapPx)),
    );

  const { handleCommonKey, handleItemClick, handleContextMenu } = useListInteraction({
    ids,
    inTrash,
    onOpenEdit,
    onAction,
  });

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (handleCommonKey(event)) return;

    const direction = DIRECTION_BY_CODE[event.code];
    if (direction === undefined) return;
    event.preventDefault();
    const target = gridMove(
      grid,
      useSelectionStore.getState().focus,
      direction,
      pageRows(),
      positions,
    );
    if (target !== null) moveTo(ids, target, { extend: event.shiftKey });
    scrollToFocus();
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild onContextMenu={handleContextMenu}>
        <div
          ref={scrollRef}
          role="grid"
          tabIndex={0}
          aria-label={ja.list.grid.ariaLabel}
          aria-rowcount={grid.length}
          aria-colcount={columns ?? 1}
          aria-multiselectable
          onKeyDown={handleKeyDown}
          className="relative min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-ring/60 focus-visible:ring-inset"
        >
          {/* 行（見えている範囲だけを描く） */}
          <div style={{ height: virtualizer.getTotalSize() }} className="relative">
            {virtualizer.getVirtualItems().map((item) => {
              const gridRow = grid[item.index];
              if (!gridRow) return null;
              return (
                <div
                  key={item.index}
                  role="row"
                  aria-rowindex={item.index + 1}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: item.size,
                    transform: `translateY(${item.start}px)`,
                    paddingLeft: paddingPx,
                    columnGap: gapPx,
                  }}
                  className="flex"
                >
                  {gridRow.keys.map((id) => {
                    const row = rows[indexById.get(id) ?? -1];
                    if (!row) return null;
                    return (
                      <BookCard
                        key={id}
                        row={row}
                        selected={selected.has(id)}
                        focused={focus === id}
                        onClick={handleItemClick}
                        onDoubleClick={onOpenEdit}
                      />
                    );
                  })}
                </div>
              );
            })}
          </div>

          {rows.length === 0 && <ListEmptyState isLoading={isLoading} />}
        </div>
      </ContextMenuTrigger>
      <ListContextMenuContent
        rows={rows}
        ids={ids}
        indexById={indexById}
        inTrash={inTrash}
        onOpenEdit={onOpenEdit}
        onAction={onAction}
        onBulkChange={onBulkChange}
      />
    </ContextMenu>
  );
}

interface BookCardProps {
  row: ListRow;
  selected: boolean;
  focused: boolean;
  onClick: (id: string, event: MouseEvent) => void;
  onDoubleClick: (id: string) => void;
}

/** 1 枚のカード（書影・書名・巻と著者）。選択・focus・行の中身が変わらないかぎり、再描画しない。 */
const BookCard = memo(function BookCard({
  row,
  selected,
  focused,
  onClick,
  onDoubleClick,
}: BookCardProps) {
  const authors = authorsText(row);
  // 巻と著者を、あるものだけ「 · 」でつなぐ
  const meta = [row.volume, authors]
    .filter((part) => part !== null && part !== "")
    .join(META_SEPARATOR);
  return (
    <div
      role="gridcell"
      data-row-id={row.id}
      aria-selected={selected}
      onClick={(event) => onClick(row.id, event)}
      onDoubleClick={() => onDoubleClick(row.id)}
      style={{ width: `${CARD_WIDTH_REM}rem`, height: `${CARD_HEIGHT_REM}rem` }}
      className={cn(
        "flex shrink-0 cursor-default flex-col overflow-hidden rounded-lg p-2 select-none",
        selected ? "bg-primary/15" : "hover:bg-accent/60",
        focused && "ring-1 ring-primary/70 ring-inset",
        row.deleted && "text-muted-foreground",
      )}
    >
      <div style={{ width: `${COVER_WIDTH_REM}rem` }}>
        <BookCover
          id={row.id}
          title={row.title}
          authors={authors}
          coverHash={row.coverHash}
          variant="grid"
        />
      </div>
      <p
        title={row.title}
        className="mt-2 line-clamp-2 min-h-10 text-sm leading-5 font-medium break-words"
      >
        {row.title}
      </p>
      {meta !== "" && (
        <p title={meta} className="mt-0.5 truncate text-xs text-muted-foreground">
          {meta}
        </p>
      )}
    </div>
  );
});
