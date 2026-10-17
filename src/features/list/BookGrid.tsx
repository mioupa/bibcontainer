// 蔵書の一覧のグリッド（FR-LIST-07、9.5）。書影のカードを敷き詰め、行ごとに TanStack Virtual で仮想スクロールする。
//
// 設計:
// - 表（BookTable）と同じ props・同じ選択（stores/selectionStore.ts）・同じ右クリックメニュー（ListContextMenu.tsx）。
//   共通の動き（共通のキー、クリック、右クリックの対象、選択行へのスクロール、選択の整理）は useListInteraction.ts。
//   このファイルが持つのは、カードの並べ方と、矢印キーでの 2 次元の移動（gridLayout.ts の純粋関数）だけ。
// - 巻をまとめた表示（FR-LIST-08）: `grouped` の entries（グループのカードと、本のカード）を並べる。選択・移動・スクロールは
//   entries の key の上で行う（グループの key は「その全巻」。表と同じ）。展開したグループの本は、新しい行から、全幅の淡い背景（band）
//   の行に並べる。矢印は 2 次元の移動のまま（開閉はしない）。開閉は、カードのバッジ・Enter・ダブルクリック。
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
import { ChevronRightIcon } from "lucide-react";
import { BookCover } from "@/components/BookCover";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { groupRows, type GroupEntry } from "@/features/library/grouping";
import { authorsText } from "@/features/list/cellText";
import {
  gridColumnCount,
  gridIndex,
  gridMove,
  layoutGrid,
  type GridDirection,
  type GridItem,
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

/** 巻をまとめない並び（`grouped` がないとき）の展開の状態。 */
const NO_GROUPS_EXPANDED: ReadonlySet<string> = new Set();

export function BookGrid({
  rows,
  grouped,
  isLoading,
  onOpenEdit,
  onAction,
  onBulkChange,
  inTrash = false,
}: BookGridProps) {
  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);
  const moveTo = useSelectionStore((s) => s.moveTo);

  // 表示する並び（グループのカードと本のカード）。`grouped` がなければ、行を 1 冊ずつ並べる
  const list = useMemo(
    () => grouped ?? groupRows(rows, false, NO_GROUPS_EXPANDED),
    [grouped, rows],
  );
  const entries = list.entries;
  // 選択・移動・スクロールの単位は、カードの key（本の ID か、グループの key）
  const ids = useMemo(() => entries.map((entry) => entry.key), [entries]);
  const entryByKey = useMemo(() => new Map(entries.map((entry) => [entry.key, entry])), [entries]);

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
  // 展開したグループの本は、グループごとの band に入れる（新しい行から始め、終わったら次の行へ）
  const items = useMemo<(string | GridItem)[]>(
    () =>
      entries.map((entry) =>
        entry.kind === "book" && entry.groupKey !== null
          ? { key: entry.key, band: entry.groupKey }
          : entry.key,
      ),
    [entries],
  );
  const grid = useMemo(
    () => (columns === null ? [] : layoutGrid(items, columns)),
    [items, columns],
  );
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

  const { handleCommonKey, handleItemClick, handleContextMenu, toggleGroup } = useListInteraction({
    ids,
    grouped: list,
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
              // band（展開したグループの本）の行: 全幅の背景。続く band の行とは、行の間隔ぶんも塗って 1 つの帯にする
              const band = gridRow.band === true;
              const startsBand = band && grid[item.index - 1]?.bandId !== gridRow.bandId;
              const endsBand = band && grid[item.index + 1]?.bandId !== gridRow.bandId;
              return (
                <div
                  key={item.index}
                  role="row"
                  aria-rowindex={item.index + 1}
                  data-band={band ? "true" : undefined}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: band ? paddingPx / 2 : 0,
                    width: band ? `calc(100% - ${paddingPx}px)` : "100%",
                    height: item.size + (band && !endsBand ? gapPx : 0),
                    transform: `translateY(${item.start}px)`,
                    paddingLeft: band ? paddingPx / 2 : paddingPx,
                    columnGap: gapPx,
                  }}
                  className={cn(
                    "flex",
                    band && "bg-muted/60",
                    startsBand && "rounded-t-xl",
                    endsBand && "rounded-b-xl",
                  )}
                >
                  {gridRow.keys.map((key) => {
                    const entry = entryByKey.get(key);
                    if (!entry) return null;
                    if (entry.kind === "group") {
                      return (
                        <GroupCard
                          key={key}
                          entry={entry}
                          selected={selected.has(key)}
                          focused={focus === key}
                          onClick={handleItemClick}
                          onToggle={toggleGroup}
                        />
                      );
                    }
                    return (
                      <BookCard
                        key={key}
                        row={entry.row}
                        selected={selected.has(key)}
                        focused={focus === key}
                        onClick={handleItemClick}
                        onDoubleClick={onOpenEdit}
                      />
                    );
                  })}
                </div>
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

interface GroupCardProps {
  entry: GroupEntry;
  selected: boolean;
  focused: boolean;
  onClick: (key: string, event: MouseEvent) => void;
  onToggle: (key: string) => void;
}

/**
 * 巻のグループのカード（FR-LIST-08）。先頭の巻の書影を、後ろに 2 枚ずらした枠を置いて重ねた見た目にし、書影の角の冊数のバッジが
 * 開閉のボタン。書名は作品名、2 行目は「7 冊 · 1〜7」。
 */
const GroupCard = memo(function GroupCard({
  entry,
  selected,
  focused,
  onClick,
  onToggle,
}: GroupCardProps) {
  const { group, expanded } = entry;
  const first = group.members[0];
  if (!first) return null;
  const meta = ja.list.group.gridMeta(group.members.length, group.firstVolume, group.lastVolume);
  return (
    <div
      role="gridcell"
      data-row-id={group.key}
      aria-selected={selected}
      aria-expanded={expanded}
      onClick={(event) => onClick(group.key, event)}
      onDoubleClick={() => onToggle(group.key)}
      style={{ width: `${CARD_WIDTH_REM}rem`, height: `${CARD_HEIGHT_REM}rem` }}
      className={cn(
        "flex shrink-0 cursor-default flex-col overflow-hidden rounded-lg p-2 select-none",
        selected ? "bg-primary/15" : "hover:bg-accent/60",
        focused && "ring-1 ring-primary/70 ring-inset",
        first.deleted && "text-muted-foreground",
      )}
    >
      <div style={{ width: `${COVER_WIDTH_REM}rem` }} className="relative">
        {[2, 1].map((offset) => (
          <span
            key={offset}
            aria-hidden
            style={{ transform: `translate(${offset * 3}px, ${-offset * 3}px)` }}
            className="absolute inset-0 rounded-md bg-muted ring-1 ring-foreground/10"
          />
        ))}
        <BookCover
          id={first.id}
          title={group.baseTitle}
          authors={authorsText(first)}
          coverHash={first.coverHash}
          variant="grid"
        />
        <button
          type="button"
          tabIndex={-1}
          aria-label={expanded ? ja.list.group.collapse : ja.list.group.expand}
          aria-expanded={expanded}
          // カードのクリック（選択）・ダブルクリック（開閉）にしない。開閉だけして、選択は変えない
          onClick={(event) => {
            event.stopPropagation();
            onToggle(group.key);
          }}
          onDoubleClick={(event) => event.stopPropagation()}
          className="absolute right-1 bottom-1 flex items-center gap-0.5 rounded-full bg-background/90 py-0.5 pr-2 pl-1 text-[11px] leading-none font-medium tabular-nums shadow ring-1 ring-foreground/15 hover:bg-background"
        >
          <ChevronRightIcon
            aria-hidden
            className={cn("size-3 transition-transform", expanded && "rotate-90")}
          />
          {group.members.length}
        </button>
      </div>
      <p
        title={group.baseTitle}
        className="mt-2 line-clamp-2 min-h-10 text-sm leading-5 font-medium break-words"
      >
        {group.baseTitle}
      </p>
      <p title={meta} className="mt-0.5 truncate text-xs text-muted-foreground">
        {meta}
      </p>
    </div>
  );
});
