import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { BookGrid, type BookGridProps } from "@/features/list/BookGrid";
import { ja } from "@/i18n/ja";
import { useSelectionStore } from "@/stores/selectionStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { makeRow } from "@/test/factories";
import { stubElementSize } from "@/test/layout";
import type { ListRow } from "@/types/contracts";

// 幅 1400px・高さ 600px。文字サイズ 16px のとき、カードは幅 128px・高さ 240px・間隔 12px・余白 12px なので、
// 列は floor((1400 − 24 + 12) / 140) = 9、行は 252px ごと（上下の余白 12px）、PageDown は floor((600 − 24) / 252) = 2 行
stubElementSize({ width: 1400, height: 600 });
const COLUMNS = 9;
const ROW_PX = 252;

const HASH = "a1b2c3d4e5f60718293a4b5c6d7e8f901234567890abcdef1234567890abcdef";

function makeRows(
  count: number,
  overrides: (i: number) => Partial<ListRow> = () => ({}),
): ListRow[] {
  return Array.from({ length: count }, (_, i) =>
    makeRow({
      id: `row-${i}`,
      title: `書名${i}`,
      sortTitle: `書名${String(i).padStart(3, "0")}`,
      authors: [{ name: `著者${i}`, kana: null, role: null }],
      ...overrides(i),
    }),
  );
}

function renderGrid(overrides: Partial<BookGridProps> = {}) {
  const props: BookGridProps = {
    rows: makeRows(500),
    cCodes: PROVISIONAL_C_CODE_TABLE,
    locationPaths: new Map(),
    isLoading: false,
    onOpenEdit: vi.fn(),
    onAction: vi.fn(),
    onBulkChange: vi.fn(),
    ...overrides,
  };
  const view = render(<BookGrid {...props} />);
  return { ...view, props, grid: screen.getByRole("grid") };
}

const cardEl = (id: string) => document.querySelector<HTMLElement>(`[data-row-id="${id}"]`);
const cards = () => [...document.querySelectorAll<HTMLElement>("[data-row-id]")];
const selectedIds = () => [...useSelectionStore.getState().selected].sort();
const focusId = () => useSelectionStore.getState().focus;
const press = (grid: HTMLElement, key: Parameters<typeof fireEvent.keyDown>[1]) =>
  fireEvent.keyDown(grid, key);
const click = (id: string, init: Parameters<typeof fireEvent.click>[1] = {}) =>
  fireEvent.click(cardEl(id)!, init);

describe("BookGrid", () => {
  beforeEach(() => {
    useSelectionStore.getState().clear();
    document.documentElement.style.removeProperty("font-size");
  });

  describe("表示", () => {
    it("カードに、書影・書名・巻と著者（「 · 」でつなぐ）を出す。空の部分は出さない", () => {
      renderGrid({
        rows: makeRows(3, (i) => ({
          volume: i === 0 ? "3" : null,
          authors: i === 2 ? [] : [{ name: `著者${i}`, kana: null, role: null }],
          coverHash: i === 0 ? HASH : null,
        })),
      });
      const first = within(cardEl("row-0")!);
      expect(first.getByText("書名0")).toHaveAttribute("title", "書名0");
      expect(first.getByText("3 · 著者0")).toBeInTheDocument();
      // 書影は、グリッド用の大きさ（320px）のサムネイル
      expect(cardEl("row-0")!.querySelector("img")).toHaveAttribute(
        "src",
        `cover://localhost/${HASH}?size=320`,
      );
      // 巻がなければ著者だけ。どちらもなければ、その行は出さない
      expect(within(cardEl("row-1")!).getByTitle("著者1")).toBeInTheDocument();
      expect(cardEl("row-2")!.querySelector("p.truncate")).toBeNull();
      // 書影がなければ、書名と著者の仮の表紙
      expect(within(cardEl("row-1")!).getByRole("img", { name: "書名1" })).toBeInTheDocument();
    });

    it("グリッドの役割（grid・row・gridcell）と、複数選択・行数を持つ", () => {
      const { grid } = renderGrid();
      expect(grid).toHaveAttribute("aria-label", ja.list.grid.ariaLabel);
      expect(grid).toHaveAttribute("aria-multiselectable", "true");
      expect(grid).toHaveAttribute("tabindex", "0");
      expect(grid).toHaveAttribute("aria-rowcount", String(Math.ceil(500 / COLUMNS)));
      const first = screen.getAllByRole("row")[0]!;
      expect(within(first).getAllByRole("gridcell")).toHaveLength(COLUMNS);
      expect(cardEl("row-0")).toHaveAttribute("role", "gridcell");
      expect(cardEl("row-0")).toHaveAttribute("aria-selected", "false");
    });

    it("幅から列の数を決め、幅が変わったら並べ直す（ResizeObserver）", () => {
      const callbacks: (() => void)[] = [];
      const original = globalThis.ResizeObserver;
      globalThis.ResizeObserver = class {
        constructor(callback: ResizeObserverCallback) {
          callbacks.push(() => callback([], this));
        }
        observe() {}
        unobserve() {}
        disconnect() {}
      };
      try {
        const { grid } = renderGrid({ rows: makeRows(30) });
        expect(within(screen.getAllByRole("row")[0]!).getAllByRole("gridcell")).toHaveLength(
          COLUMNS,
        );
        // 幅 500px: floor((500 − 24 + 12) / 140) = 3 列
        Object.defineProperty(grid, "clientWidth", { value: 500, configurable: true });
        act(() => callbacks.forEach((callback) => callback()));
        expect(within(screen.getAllByRole("row")[0]!).getAllByRole("gridcell")).toHaveLength(3);
        expect(grid).toHaveAttribute("aria-rowcount", "10");
        // 狭すぎても 1 列
        Object.defineProperty(grid, "clientWidth", { value: 50, configurable: true });
        act(() => callbacks.forEach((callback) => callback()));
        expect(within(screen.getAllByRole("row")[0]!).getAllByRole("gridcell")).toHaveLength(1);
      } finally {
        globalThis.ResizeObserver = original;
      }
    });

    it("仮想スクロール: 500 冊のうち、見える範囲とその前後の行のカード（と書影）だけを描く", () => {
      renderGrid({ rows: makeRows(500, () => ({ coverHash: HASH })) });
      const rendered = cards().length;
      expect(rendered).toBeGreaterThanOrEqual(COLUMNS);
      expect(rendered).toBeLessThan(500 / 4);
      expect(cardEl("row-0")).not.toBeNull();
      expect(cardEl("row-499")).toBeNull();
      expect(document.querySelectorAll("img")).toHaveLength(rendered);
    });

    it("カードの大きさは rem で決める。<html> の文字サイズに掛けて、行の高さにする", () => {
      document.documentElement.style.fontSize = "20px";
      renderGrid({ rows: makeRows(30) });
      // 15rem × 20px
      expect(screen.getAllByRole("row")[0]).toHaveStyle({ height: "300px" });
      expect(cardEl("row-0")!.style.width).toBe("8rem");
      expect(cardEl("row-0")!.style.height).toBe("15rem");
    });

    it("文字サイズの設定が変わると、行の高さを測り直す", async () => {
      renderGrid({ rows: makeRows(30) });
      expect(screen.getAllByRole("row")[0]).toHaveStyle({ height: `${ROW_PX - 12}px` });
      expect(screen.getAllByRole("row")[1]).toHaveStyle({
        transform: `translateY(${12 + ROW_PX}px)`,
      });
      // <html> への反映（アプリの最上位の effect）のあとに、読み直す
      await act(async () => {
        document.documentElement.style.fontSize = "20px";
        useSettingsStore.setState({ fontSize: 20 });
        await Promise.resolve();
      });
      expect(screen.getAllByRole("row")[0]).toHaveStyle({ height: "300px" });
      // 20 × 0.75 = 15px の間隔と余白
      expect(screen.getAllByRole("row")[1]).toHaveStyle({
        transform: `translateY(${15 + 300 + 15}px)`,
      });
      useSettingsStore.setState({ fontSize: 16 });
    });

    it("行がないとき、読み込み中か「該当なし」を出す", () => {
      const { rerender, props } = renderGrid({ rows: [], isLoading: true });
      expect(screen.getByText(ja.list.loading)).toBeInTheDocument();
      expect(cards()).toHaveLength(0);
      rerender(<BookGrid {...props} rows={[]} isLoading={false} />);
      expect(screen.getByText(ja.list.empty)).toBeInTheDocument();
      expect(screen.getByText(ja.list.emptyHint)).toBeInTheDocument();
    });
  });

  describe("カードの選択（9.5）", () => {
    it("クリックで選び、⌘+クリック（macOS）で足し、Shift+クリックで範囲を選ぶ", () => {
      renderGrid();
      click("row-2");
      expect(selectedIds()).toEqual(["row-2"]);
      expect(cardEl("row-2")).toHaveAttribute("aria-selected", "true");

      click("row-5", { metaKey: true });
      expect(selectedIds()).toEqual(["row-2", "row-5"]);

      click("row-8");
      click("row-10", { shiftKey: true });
      expect(selectedIds()).toEqual(["row-10", "row-8", "row-9"]);
      expect(cardEl("row-9")).toHaveAttribute("aria-selected", "true");
      expect(cardEl("row-2")).toHaveAttribute("aria-selected", "false");
    });

    it("⌘+クリックで、選んだカードを外せる", () => {
      renderGrid();
      click("row-2");
      click("row-3", { metaKey: true });
      click("row-2", { metaKey: true });
      expect(selectedIds()).toEqual(["row-3"]);
    });

    it("ダブルクリックで編集を開く", () => {
      const { props } = renderGrid();
      fireEvent.doubleClick(cardEl("row-3")!);
      expect(props.onOpenEdit).toHaveBeenCalledWith("row-3");
    });

    it("絞り込みで消えた本は、選択から外れる", () => {
      const rows = makeRows(40);
      const { rerender, props } = renderGrid({ rows });
      click("row-1");
      click("row-2", { metaKey: true });
      rerender(<BookGrid {...props} rows={rows.filter((r) => r.id !== "row-1")} />);
      expect(selectedIds()).toEqual(["row-2"]);
    });

    it("選んだカードに focus の輪を付ける", () => {
      renderGrid();
      click("row-4");
      expect(cardEl("row-4")).toHaveClass("ring-primary/70");
      expect(cardEl("row-5")).not.toHaveClass("ring-primary/70");
    });
  });

  describe("キーボード", () => {
    it("矢印の左右は並びの前後（行をまたぐ）、上下は同じ列の前後の行へ動く", () => {
      const { grid } = renderGrid();
      press(grid, { code: "ArrowRight" }); // 何も選んでいなければ、最初の本
      expect(focusId()).toBe("row-0");
      press(grid, { code: "ArrowRight" });
      press(grid, { code: "ArrowRight" });
      expect(focusId()).toBe("row-2");
      expect(selectedIds()).toEqual(["row-2"]);

      press(grid, { code: "ArrowDown" });
      expect(focusId()).toBe(`row-${2 + COLUMNS}`);
      press(grid, { code: "ArrowUp" });
      expect(focusId()).toBe("row-2");

      // 行の端を越える
      click(`row-${COLUMNS - 1}`);
      press(grid, { code: "ArrowRight" });
      expect(focusId()).toBe(`row-${COLUMNS}`);
      press(grid, { code: "ArrowLeft" });
      expect(focusId()).toBe(`row-${COLUMNS - 1}`);

      // 先頭の左・上、で止まる
      click("row-0");
      press(grid, { code: "ArrowLeft" });
      press(grid, { code: "ArrowUp" });
      expect(focusId()).toBe("row-0");
      expect(selectedIds()).toEqual(["row-0"]);
    });

    it("最後の行が短いとき、上下の移動は、その行の最後の本で止める", () => {
      // 20 冊: 9 + 9 + 2
      const { grid } = renderGrid({ rows: makeRows(20) });
      click("row-3");
      press(grid, { code: "ArrowDown" });
      expect(focusId()).toBe("row-12");
      press(grid, { code: "ArrowDown" }); // 3 列目は最後の行にない → その行の最後の本
      expect(focusId()).toBe("row-19");
      press(grid, { code: "ArrowDown" });
      expect(focusId()).toBe("row-19");
      press(grid, { code: "ArrowRight" });
      expect(focusId()).toBe("row-19");
      press(grid, { code: "ArrowUp" }); // 列は覚えない（最後の行の 2 列目の上）
      expect(focusId()).toBe("row-10");
    });

    it("Shift+矢印は、起点を動かさずに範囲を選ぶ（左右は 1 冊ずつ、上下は 1 行ぶん）", () => {
      const { grid } = renderGrid();
      click("row-10");
      press(grid, { code: "ArrowRight", shiftKey: true });
      press(grid, { code: "ArrowRight", shiftKey: true });
      expect(selectedIds()).toEqual(["row-10", "row-11", "row-12"]);
      expect(useSelectionStore.getState().anchor).toBe("row-10");

      press(grid, { code: "ArrowDown", shiftKey: true });
      expect(focusId()).toBe(`row-${12 + COLUMNS}`);
      expect(useSelectionStore.getState().selected.size).toBe(COLUMNS + 3);

      press(grid, { code: "ArrowUp", shiftKey: true });
      press(grid, { code: "ArrowUp", shiftKey: true });
      expect(focusId()).toBe(`row-${12 - COLUMNS}`);
      // 起点（row-10）から row-3 まで
      expect(selectedIds()).toEqual(
        ["row-3", "row-4", "row-5", "row-6", "row-7", "row-8", "row-9", "row-10"].sort(),
      );
    });

    it("Home / End / PageDown / PageUp と、⌘A（macOS）ですべて選ぶ。Esc で外す", () => {
      const { grid } = renderGrid();
      press(grid, { code: "End" });
      expect(focusId()).toBe("row-499");
      press(grid, { code: "Home" });
      expect(focusId()).toBe("row-0");
      // 高さ 600px の見える行は 2 行ぶん
      press(grid, { code: "PageDown" });
      expect(focusId()).toBe(`row-${2 * COLUMNS}`);
      press(grid, { code: "PageUp" });
      expect(focusId()).toBe("row-0");
      press(grid, { code: "KeyA", metaKey: true });
      expect(useSelectionStore.getState().selected.size).toBe(500);
      press(grid, { code: "Escape" });
      expect(useSelectionStore.getState().selected.size).toBe(0);
    });

    it("Shift+End / Shift+Home で、起点から端までを範囲で選ぶ", () => {
      const { grid } = renderGrid({ rows: makeRows(30) });
      click("row-20");
      press(grid, { code: "End", shiftKey: true });
      expect(selectedIds()).toHaveLength(10);
      press(grid, { code: "Home", shiftKey: true });
      expect(useSelectionStore.getState().selected.size).toBe(21);
    });

    it("Enter で、focus している本の編集を開く", () => {
      const { grid, props } = renderGrid();
      press(grid, { code: "Enter" });
      expect(props.onOpenEdit).not.toHaveBeenCalled();
      press(grid, { code: "ArrowDown" });
      press(grid, { code: "ArrowDown" });
      press(grid, { code: "Enter" });
      expect(props.onOpenEdit).toHaveBeenCalledWith(`row-${COLUMNS}`);
    });

    it("ゴミ箱へ移動（macOS は ⌘⌫）で、選んだ本の ID を渡す。ゴミ箱の中では何もしない", () => {
      const { grid, props, rerender } = renderGrid();
      press(grid, { code: "Backspace" });
      expect(props.onAction).not.toHaveBeenCalled();
      click("row-1");
      click("row-2", { metaKey: true });
      press(grid, { code: "Backspace", metaKey: true });
      expect(props.onAction).toHaveBeenCalledOnce();
      const [action, ids] = (props.onAction as ReturnType<typeof vi.fn>).mock.calls[0] as [
        string,
        string[],
      ];
      expect(action).toBe("trash");
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);

      (props.onAction as ReturnType<typeof vi.fn>).mockClear();
      rerender(<BookGrid {...props} inTrash />);
      press(grid, { code: "Backspace", metaKey: true });
      expect(props.onAction).not.toHaveBeenCalled();
    });

    it("日本語入力の変換中のキーは無視する", () => {
      const { grid, props } = renderGrid();
      press(grid, { code: "Enter", isComposing: true });
      press(grid, { code: "ArrowDown", isComposing: true });
      expect(props.onOpenEdit).not.toHaveBeenCalled();
      expect(focusId()).toBeNull();
    });

    it("修飾キーつきの矢印は、動かさない", () => {
      const { grid } = renderGrid();
      click("row-3");
      press(grid, { code: "ArrowDown", metaKey: true });
      press(grid, { code: "ArrowRight", altKey: true });
      expect(focusId()).toBe("row-3");
    });
  });

  // 選んだ本のある行へのスクロール。jsdom にはレイアウトがないので、scrollTo を作って scrollTop を動かす
  describe("選んだ本へのスクロール", () => {
    function installScroll(grid: HTMLElement) {
      // jsdom の scrollTop は動かせず、scrollHeight は 0 なので、書き換えられる値と、十分な高さにする
      Object.defineProperty(grid, "scrollTop", { value: 0, writable: true, configurable: true });
      Object.defineProperty(grid, "scrollHeight", { value: 120 * ROW_PX, configurable: true });
      const scrollTo = vi.fn((options: ScrollToOptions) => {
        grid.scrollTop = options.top ?? 0;
        grid.dispatchEvent(new Event("scroll"));
      });
      Object.defineProperty(grid, "scrollTo", { value: scrollTo, configurable: true });
      return scrollTo;
    }
    const focusRow = (id: string) =>
      act(() => {
        useSelectionStore.setState({ selected: new Set([id]), anchor: id, focus: id });
      });
    const rowOf = (index: number) => Math.floor(index / COLUMNS);

    it("focus が変わったとき、その本のある行までスクロールする（見えていない行へ移ったとき）", () => {
      const { grid } = renderGrid();
      const scrollTo = installScroll(grid);
      focusRow("row-300");
      expect(scrollTo).toHaveBeenCalled();
      expect(grid.scrollTop).toBeGreaterThan((rowOf(300) - 3) * ROW_PX);
      expect(cardEl("row-300")).not.toBeNull();
    });

    it("見えている本なら、スクロールの位置を動かさない", () => {
      const { grid } = renderGrid();
      installScroll(grid);
      focusRow("row-5");
      expect(grid.scrollTop).toBe(0);
    });

    it("キーで動かした先が見えていなければ、その行までスクロールする", () => {
      const { grid } = renderGrid();
      const scrollTo = installScroll(grid);
      press(grid, { code: "End" });
      expect(focusId()).toBe("row-499");
      expect(scrollTo).toHaveBeenCalled();
      expect(grid.scrollTop).toBeGreaterThan(50 * ROW_PX);
      expect(cardEl("row-499")).not.toBeNull();
    });

    it("行が差し替わっても（library-changed で更新されても）、ユーザーのスクロール位置から選択行へ戻さない", () => {
      const { grid, props, rerender } = renderGrid();
      const scrollTo = installScroll(grid);
      focusRow("row-300");
      expect(grid.scrollTop).toBeGreaterThan(0);

      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();

      rerender(<BookGrid {...props} rows={makeRows(500).map((row) => ({ ...row }))} />);
      rerender(<BookGrid {...props} rows={makeRows(500)} />);
      expect(scrollTo).not.toHaveBeenCalled();
      expect(grid.scrollTop).toBe(0);
    });

    it("focus が変わらないまま本が増減しても、動かさない。別の本を選んだときは、また動く", () => {
      const { grid, props, rerender } = renderGrid();
      const scrollTo = installScroll(grid);
      focusRow("row-300");
      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();

      rerender(<BookGrid {...props} rows={makeRows(520)} />);
      expect(scrollTo).not.toHaveBeenCalled();

      focusRow("row-400");
      expect(scrollTo).toHaveBeenCalled();
    });

    it("focus した本がまだ並びにないとき（絞り込みの更新待ち）は、現れたときに 1 度だけスクロールする", () => {
      const { grid, props, rerender } = renderGrid({ rows: makeRows(10) });
      const scrollTo = installScroll(grid);
      focusRow("row-300"); // まだ 10 冊しかない
      expect(scrollTo).not.toHaveBeenCalled();

      rerender(<BookGrid {...props} rows={makeRows(500)} />);
      expect(scrollTo).toHaveBeenCalled();
      expect(grid.scrollTop).toBeGreaterThan((rowOf(300) - 3) * ROW_PX);

      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();
      rerender(<BookGrid {...props} rows={makeRows(500)} />);
      expect(scrollTo).not.toHaveBeenCalled();
    });

    it("reveal（検索欄の ISBN など）は、すでに focus している本でも、その行までスクロールする", () => {
      const { grid } = renderGrid();
      const scrollTo = installScroll(grid);
      focusRow("row-300");
      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();

      act(() => useSelectionStore.getState().reveal("row-300"));
      expect(scrollTo).toHaveBeenCalled();
      expect(selectedIds()).toEqual(["row-300"]);
    });

    it("開いたときに選んでいる本があれば（表から切り替えたとき）、その本までスクロールする", () => {
      useSelectionStore.setState({
        selected: new Set(["row-300"]),
        anchor: "row-300",
        focus: "row-300",
      });
      // 描画の前に scrollTo を使えるよう、scroll 要素の作りに差し込む
      const scrollTo = vi.fn();
      const original = HTMLElement.prototype.scrollTo;
      HTMLElement.prototype.scrollTo = scrollTo;
      const heightDescriptor = Object.getOwnPropertyDescriptor(
        HTMLElement.prototype,
        "scrollHeight",
      );
      Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
        configurable: true,
        get: () => 120 * ROW_PX,
      });
      try {
        renderGrid();
        expect(scrollTo).toHaveBeenCalled();
        const top = (scrollTo.mock.calls.at(-1)?.[0] as ScrollToOptions).top ?? 0;
        expect(top).toBeGreaterThan((rowOf(300) - 3) * ROW_PX);
      } finally {
        if (original) HTMLElement.prototype.scrollTo = original;
        else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollTo;
        if (heightDescriptor)
          Object.defineProperty(HTMLElement.prototype, "scrollHeight", heightDescriptor);
        else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollHeight;
      }
    });
  });

  describe("右クリックメニュー（表と同じ）", () => {
    const openMenu = (id: string) => {
      fireEvent.contextMenu(cardEl(id)!);
      return screen.findByRole("menu");
    };
    const lastCall = (fn: unknown) =>
      (fn as ReturnType<typeof vi.fn>).mock.calls.at(-1) as unknown[];

    afterEach(() => {
      // 開いたままのメニューを閉じる
      fireEvent.keyDown(document.body, { key: "Escape" });
    });

    it("表と同じ項目を出す（編集・一括変更・書誌を再取得・CSV に書き出し・ゴミ箱へ移動）", async () => {
      renderGrid();
      click("row-1");
      const menu = await openMenu("row-1");
      const names = within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent);
      expect(names).toEqual([
        ja.list.contextMenu.edit,
        ja.list.contextMenu.bulkEdit,
        ja.list.contextMenu.refetch,
        ja.list.contextMenu.export,
        ja.list.contextMenu.trash,
      ]);
    });

    it("右クリックしたカードが選択の外なら、そのカードだけを選び、「ゴミ箱へ移動」でその ID を渡す", async () => {
      const { props } = renderGrid();
      click("row-1");
      const menu = await openMenu("row-5");
      expect(selectedIds()).toEqual(["row-5"]);
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.trash }));
      expect(lastCall(props.onAction)).toEqual(["trash", ["row-5"]]);
    });

    it("選択の中のカードを右クリックしたときは、選択を変えず、選んだ本すべてを対象にする", async () => {
      const { props } = renderGrid();
      click("row-1");
      click("row-2", { metaKey: true });
      const menu = await openMenu("row-1");
      expect(selectedIds()).toEqual(["row-1", "row-2"]);
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.export }));
      const [action, ids] = lastCall(props.onAction) as [string, string[]];
      expect(action).toBe("export");
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);
    });

    it("カードの外（余白）の右クリックでは、メニューを出さない", () => {
      const { grid } = renderGrid();
      fireEvent.contextMenu(grid);
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    });

    it("ゴミ箱を開いているときは、「ゴミ箱へ移動」の代わりに「ゴミ箱から戻す」と「完全に削除…」を出す", async () => {
      const { props } = renderGrid({ inTrash: true });
      const menu = await openMenu("row-3");
      expect(
        within(menu).queryByRole("menuitem", { name: ja.list.contextMenu.trash }),
      ).not.toBeInTheDocument();
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.restore }));
      expect(lastCall(props.onAction)).toEqual(["restore", ["row-3"]]);
    });

    it("「編集」は、focus している本の編集を開く", async () => {
      const { props } = renderGrid();
      const menu = await openMenu("row-4");
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.edit }));
      expect(props.onOpenEdit).toHaveBeenCalledWith("row-4");
    });

    it("「一括変更」のサブメニューから、読書状況を選べる", async () => {
      const { props } = renderGrid();
      click("row-1");
      click("row-2", { metaKey: true });
      const menu = await openMenu("row-1");
      fireEvent.keyDown(
        within(menu).getByRole("menuitem", { name: ja.list.contextMenu.bulkEdit }),
        {
          key: "ArrowRight",
        },
      );
      fireEvent.click(await screen.findByRole("menuitem", { name: ja.labels.readStatus.read }));
      const [change, ids] = lastCall(props.onBulkChange) as [unknown, string[]];
      expect(change).toEqual({ field: "read_status", value: "read" });
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);
    });

    it("「購入済みにする」は、欲しい本を選んでいるときだけ出て、欲しい本の ID だけを渡す", async () => {
      const rows = [
        makeRow({ id: "own", ownership: "owned" }),
        makeRow({ id: "want-1", ownership: "wanted" }),
        makeRow({ id: "want-2", ownership: "wanted" }),
      ];
      const { props } = renderGrid({ rows });
      click("own");
      const first = await openMenu("own");
      expect(
        within(first).queryByRole("menuitem", { name: ja.list.contextMenu.markPurchased }),
      ).not.toBeInTheDocument();
      fireEvent.keyDown(first, { key: "Escape" });

      click("own");
      click("want-1", { metaKey: true });
      click("want-2", { metaKey: true });
      const menu = await openMenu("want-1");
      fireEvent.click(
        within(menu).getByRole("menuitem", { name: ja.list.contextMenu.markPurchased }),
      );
      const [action, ids] = lastCall(props.onAction) as [string, string[]];
      expect(action).toBe("markPurchased");
      expect([...ids].sort()).toEqual(["want-1", "want-2"]);
    });
  });
});
