import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { BookTable, type BookTableProps } from "@/features/list/BookTable";
import { ja } from "@/i18n/ja";
import { useSelectionStore } from "@/stores/selectionStore";
import { useTableStore } from "@/stores/tableStore";
import { makeRow } from "@/test/factories";
import { stubElementSize } from "@/test/layout";
import type { ListRow } from "@/types/contracts";

stubElementSize({ width: 1400, height: 600 });

function makeRows(count: number): ListRow[] {
  return Array.from({ length: count }, (_, i) =>
    makeRow({
      id: `row-${i}`,
      title: `書名${i}`,
      sortTitle: `書名${String(i).padStart(3, "0")}`,
      publisher: i % 2 === 0 ? "岩波書店" : "講談社",
      cCode: "0193",
      authors: [{ name: `著者${i}`, kana: null, role: null }],
    }),
  );
}

function renderTable(overrides: Partial<BookTableProps> = {}) {
  const props: BookTableProps = {
    rows: makeRows(100),
    cCodes: PROVISIONAL_C_CODE_TABLE,
    locationPaths: new Map(),
    isLoading: false,
    onOpenEdit: vi.fn(),
    onAction: vi.fn(),
    onBulkChange: vi.fn(),
    ...overrides,
  };
  const view = render(<BookTable {...props} />);
  return { ...view, props, grid: screen.getByRole("grid") };
}

const rowEl = (id: string) => document.querySelector<HTMLElement>(`[data-row-id="${id}"]`);
const selectedIds = () => [...useSelectionStore.getState().selected].sort();
const press = (grid: HTMLElement, key: Parameters<typeof fireEvent.keyDown>[1]) =>
  fireEvent.keyDown(grid, key);

describe("BookTable", () => {
  beforeEach(() => {
    useTableStore.getState().resetColumns();
    useSelectionStore.getState().clear();
  });

  describe("表示", () => {
    it("既定の列（FR-LIST-01）の見出しを出し、選べる列は出さない", () => {
      renderTable();
      const headers = screen.getAllByRole("columnheader").map((h) => h.textContent);
      expect(headers).toEqual(
        [
          "cover",
          "title",
          "volume",
          "authors",
          "publisher",
          "publishedOn",
          "cCode",
          "location",
          "readStatus",
          "rating",
          "createdAt",
        ].map((id) => ja.list.columns[id as keyof typeof ja.list.columns]),
      );
      expect(screen.queryByText(ja.list.columns.isbn13)).not.toBeInTheDocument();
    });

    it("仮想スクロール: 100 行のうち、見える範囲とその前後だけを描く", () => {
      renderTable();
      const rendered = document.querySelectorAll("[data-row-id]").length;
      expect(rendered).toBeGreaterThan(5);
      expect(rendered).toBeLessThan(60);
      expect(rowEl("row-0")).not.toBeNull();
      expect(rowEl("row-99")).toBeNull();
    });

    it("セルに書名・出版社・Cコードの名称を出す（Cコード表は外から受け取る）", () => {
      renderTable();
      const row = rowEl("row-0")!;
      expect(within(row).getByText("書名0")).toBeInTheDocument();
      expect(within(row).getByText("岩波書店")).toBeInTheDocument();
      expect(within(row).getByText("0193")).toBeInTheDocument();
      expect(within(row).getByText("日本文学、小説・物語")).toBeInTheDocument();
    });

    it("Cコード表が空（まだ取得できていない）のあいだは、名称の代わりに「未定義（xx）」を出す", () => {
      renderTable({ cCodes: { audience: {}, format: {}, contentMajor: {}, content: {} } });
      expect(within(rowEl("row-0")!).getByText("未定義（93）")).toBeInTheDocument();
    });

    it("行がないとき、読み込み中か「該当なし」を出す", () => {
      const { rerender, props } = renderTable({ rows: [], isLoading: true });
      expect(screen.getByText(ja.list.loading)).toBeInTheDocument();
      rerender(<BookTable {...props} rows={[]} isLoading={false} />);
      expect(screen.getByText(ja.list.empty)).toBeInTheDocument();
    });
  });

  describe("列の操作（FR-LIST-01／02）", () => {
    it("見出しのクリックで並べ替える（昇順 → 降順 → 解除）。状態は Zustand に保存される", () => {
      renderTable();
      // 既定は書名の昇順
      expect(useTableStore.getState().sorting).toEqual([{ id: "title", desc: false }]);
      const title = screen.getByRole("button", { name: ja.list.columns.title });
      fireEvent.click(title);
      expect(useTableStore.getState().sorting).toEqual([{ id: "title", desc: true }]);
      fireEvent.click(title);
      expect(useTableStore.getState().sorting).toEqual([]);
      fireEvent.click(title);
      expect(useTableStore.getState().sorting).toEqual([{ id: "title", desc: false }]);
    });

    it("Shift+クリックで複数列の並べ替えになる。見出しに並びの番号が出る", () => {
      renderTable();
      fireEvent.click(screen.getByRole("button", { name: ja.list.columns.publisher }), {
        shiftKey: true,
      });
      expect(useTableStore.getState().sorting).toEqual([
        { id: "title", desc: false },
        { id: "publisher", desc: false },
      ]);
      const header = screen.getByRole("columnheader", {
        name: new RegExp(ja.list.columns.publisher),
      });
      expect(header).toHaveAttribute("aria-sort", "ascending");
      expect(within(header).getByText("2")).toBeInTheDocument();
    });

    it("書影の列は並べ替えできない", () => {
      renderTable();
      expect(screen.queryByRole("button", { name: ja.list.columns.cover })).not.toBeInTheDocument();
    });

    it("列の表示・非表示（Zustand の状態）が反映される", () => {
      renderTable();
      act(() => {
        useTableStore
          .getState()
          .setColumnVisibility((old) => ({ ...old, publisher: false, isbn13: true }));
      });
      expect(
        screen.queryByRole("columnheader", { name: new RegExp(ja.list.columns.publisher) }),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("columnheader", { name: new RegExp(ja.list.columns.isbn13) }),
      ).toBeInTheDocument();
    });

    it("列の並び順（Zustand の状態）が、見出しと行のセルに反映される（FR-LIST-01）", () => {
      renderTable();
      act(() => {
        // 既定の並びは 書影・書名・巻・著者…。著者を 3 回上へ動かして、先頭にする
        for (let i = 0; i < 3; i++) useTableStore.getState().moveColumn("authors", "up");
      });
      const names = screen.getAllByRole("columnheader").map((h) => h.textContent);
      expect(names.slice(0, 3)).toEqual([
        ja.list.columns.authors,
        ja.list.columns.cover,
        ja.list.columns.title,
      ]);
      // 行のセルも同じ順（先頭のセルが著者）
      const first = within(rowEl("row-0")!).getAllByRole("gridcell")[0]!;
      expect(first).toHaveTextContent("著者0");
    });

    it("並び順に載っていない列（保存のあとに増えた列）は、末尾に並ぶ。非表示の列は並び順があっても出ない", () => {
      renderTable();
      act(() => {
        useTableStore.getState().setColumnOrder(["rating", "title"]);
        useTableStore.getState().setColumnVisibility((old) => ({ ...old, publisher: false }));
      });
      const names = screen.getAllByRole("columnheader").map((h) => h.textContent);
      expect(names.slice(0, 2)).toEqual([ja.list.columns.rating, ja.list.columns.title]);
      expect(names).not.toContain(ja.list.columns.publisher);
      expect(names.at(-1)).toBe(ja.list.columns.createdAt);
    });

    it("列の幅の見出しをダブルクリックすると、既定の幅に戻る", () => {
      renderTable();
      act(() => {
        useTableStore.getState().setColumnSizing({ title: 500, authors: 300 });
      });
      fireEvent.doubleClick(
        screen.getByRole("separator", { name: ja.list.resizeColumn(ja.list.columns.title) }),
      );
      expect(useTableStore.getState().columnSizing).toEqual({ authors: 300 });
    });

    it("列の幅（Zustand の状態）が反映される", () => {
      renderTable();
      act(() => {
        useTableStore.getState().setColumnSizing({ title: 500 });
      });
      const header = screen.getByRole("columnheader", { name: new RegExp(ja.list.columns.title) });
      expect(header).toHaveStyle({ width: "500px" });
      expect(within(rowEl("row-0")!).getAllByRole("gridcell")[1]).toHaveStyle({ width: "500px" });
    });
  });

  describe("行の選択（9.5）", () => {
    it("クリックで選び、⌘+クリック（macOS）で足し、Shift+クリックで範囲を選ぶ", () => {
      renderTable();
      fireEvent.click(rowEl("row-2")!);
      expect(selectedIds()).toEqual(["row-2"]);
      expect(rowEl("row-2")).toHaveAttribute("aria-selected", "true");

      fireEvent.click(rowEl("row-5")!, { metaKey: true });
      expect(selectedIds()).toEqual(["row-2", "row-5"]);

      fireEvent.click(rowEl("row-8")!);
      fireEvent.click(rowEl("row-10")!, { shiftKey: true });
      expect(selectedIds()).toEqual(["row-10", "row-8", "row-9"]);
    });

    it("ダブルクリックで編集を開く", () => {
      const { props } = renderTable();
      fireEvent.doubleClick(rowEl("row-3")!);
      expect(props.onOpenEdit).toHaveBeenCalledWith("row-3");
    });

    it("矢印キーで動かし、Shift+矢印で範囲を選び、Enter で編集を開く", () => {
      const { grid, props } = renderTable();
      grid.focus();
      press(grid, { code: "ArrowDown" });
      press(grid, { code: "ArrowDown" });
      expect(useSelectionStore.getState().focus).toBe("row-1");
      expect(selectedIds()).toEqual(["row-1"]);

      press(grid, { code: "ArrowDown", shiftKey: true });
      press(grid, { code: "ArrowDown", shiftKey: true });
      expect(selectedIds()).toEqual(["row-1", "row-2", "row-3"]);

      press(grid, { code: "Enter" });
      expect(props.onOpenEdit).toHaveBeenCalledWith("row-3");

      press(grid, { code: "ArrowUp" });
      expect(selectedIds()).toEqual(["row-2"]);
    });

    it("Home / End / PageDown と、⌘A（macOS）ですべて選ぶ", () => {
      const { grid } = renderTable();
      press(grid, { code: "End" });
      expect(useSelectionStore.getState().focus).toBe("row-99");
      press(grid, { code: "Home" });
      expect(useSelectionStore.getState().focus).toBe("row-0");
      press(grid, { code: "PageDown" });
      expect(Number(useSelectionStore.getState().focus?.replace("row-", ""))).toBeGreaterThan(3);
      press(grid, { code: "KeyA", metaKey: true });
      expect(useSelectionStore.getState().selected.size).toBe(100);
      press(grid, { code: "Escape" });
      expect(useSelectionStore.getState().selected.size).toBe(0);
    });

    it("ゴミ箱へ移動（macOS は ⌘⌫）で、選んだ本の ID を渡す", () => {
      const { grid, props } = renderTable();
      press(grid, { code: "Backspace" });
      expect(props.onAction).not.toHaveBeenCalled();
      fireEvent.click(rowEl("row-1")!);
      fireEvent.click(rowEl("row-2")!, { metaKey: true });
      press(grid, { code: "Backspace", metaKey: true });
      expect(props.onAction).toHaveBeenCalledOnce();
      const [action, ids] = (props.onAction as ReturnType<typeof vi.fn>).mock.calls[0] as [
        string,
        string[],
      ];
      expect(action).toBe("trash");
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);
    });

    it("日本語入力の変換中のキーは無視する", () => {
      const { grid, props } = renderTable();
      press(grid, { code: "Enter", isComposing: true });
      press(grid, { code: "ArrowDown", isComposing: true });
      expect(props.onOpenEdit).not.toHaveBeenCalled();
      expect(useSelectionStore.getState().focus).toBeNull();
    });

    it("絞り込みで消えた行は、選択から外れる", () => {
      const rows = makeRows(20);
      const { rerender, props } = renderTable({ rows });
      fireEvent.click(rowEl("row-1")!);
      fireEvent.click(rowEl("row-2")!, { metaKey: true });
      rerender(<BookTable {...props} rows={rows.filter((r) => r.id !== "row-1")} />);
      expect(selectedIds()).toEqual(["row-2"]);
    });
  });

  // 選択行（focus）へのスクロール。jsdom にはレイアウトがないので、scrollTo を作って scrollTop を動かす
  describe("選択行へのスクロール", () => {
    function installScroll(grid: HTMLElement) {
      // jsdom の scrollTop は動かせず、scrollHeight は 0 なので（0 だと、仮想スクロールが行き先を 0 に丸める）、
      // 書き換えられる値と、十分な高さにする
      Object.defineProperty(grid, "scrollTop", { value: 0, writable: true, configurable: true });
      Object.defineProperty(grid, "scrollHeight", { value: 200 * 48, configurable: true });
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

    it("focus が変わったとき、その行までスクロールする（見えていない行へ移ったとき）", () => {
      const { grid } = renderTable();
      const scrollTo = installScroll(grid);
      focusRow("row-60");
      expect(scrollTo).toHaveBeenCalled();
      expect(grid.scrollTop).toBeGreaterThan(40 * 48);
    });

    it("行が差し替わっても（library-changed で行が更新されても）、ユーザーのスクロール位置から選択行へ戻さない", () => {
      const { grid, props, rerender } = renderTable();
      const scrollTo = installScroll(grid);
      focusRow("row-60");
      expect(grid.scrollTop).toBeGreaterThan(0);

      // ユーザーが先頭の方へスクロールした
      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();

      // 同じ ID の行が、更新された内容で差し替わる（行の配列が新しくなる）
      rerender(<BookTable {...props} rows={makeRows(100).map((row) => ({ ...row }))} />);
      rerender(<BookTable {...props} rows={makeRows(100)} />);
      expect(scrollTo).not.toHaveBeenCalled();
      expect(grid.scrollTop).toBe(0);
    });

    it("focus が変わらないまま行が増減しても、動かさない。別の行を選んだときは、また動く", () => {
      const { grid, props, rerender } = renderTable();
      const scrollTo = installScroll(grid);
      focusRow("row-60");
      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();

      rerender(<BookTable {...props} rows={makeRows(120)} />);
      expect(scrollTo).not.toHaveBeenCalled();

      focusRow("row-80");
      expect(scrollTo).toHaveBeenCalled();
    });

    it("focus した行がまだ並びにないとき（絞り込みの更新待ち）は、行が現れたときに 1 度だけスクロールする", () => {
      const { grid, props, rerender } = renderTable({ rows: makeRows(10) });
      const scrollTo = installScroll(grid);
      focusRow("row-60"); // まだ 10 行しかない
      expect(scrollTo).not.toHaveBeenCalled();

      rerender(<BookTable {...props} rows={makeRows(100)} />);
      expect(scrollTo).toHaveBeenCalled();
      expect(grid.scrollTop).toBeGreaterThan(40 * 48);

      // そのあとの差し替えでは動かない
      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();
      rerender(<BookTable {...props} rows={makeRows(100)} />);
      expect(scrollTo).not.toHaveBeenCalled();
    });

    it("開いたときに選んでいる本があれば（グリッドから切り替えたとき）、その行までスクロールする", () => {
      useSelectionStore.setState({
        selected: new Set(["row-60"]),
        anchor: "row-60",
        focus: "row-60",
      });
      // 描画の前に scrollTo を使えるよう、要素の作りに差し込む（jsdom の scrollHeight は 0 なので、十分な高さにもする）
      const scrollTo = vi.fn();
      const original = HTMLElement.prototype.scrollTo;
      const heightDescriptor = Object.getOwnPropertyDescriptor(
        HTMLElement.prototype,
        "scrollHeight",
      );
      HTMLElement.prototype.scrollTo = scrollTo;
      Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
        configurable: true,
        get: () => 200 * 48,
      });
      try {
        renderTable();
        expect(scrollTo).toHaveBeenCalled();
        const top = (scrollTo.mock.calls.at(-1)?.[0] as ScrollToOptions).top ?? 0;
        expect(top).toBeGreaterThan(40 * 48);
      } finally {
        if (original) HTMLElement.prototype.scrollTo = original;
        else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollTo;
        if (heightDescriptor)
          Object.defineProperty(HTMLElement.prototype, "scrollHeight", heightDescriptor);
        else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollHeight;
      }
    });

    it("reveal（検索欄の ISBN など）は、すでに focus している本でも、その行までスクロールする", () => {
      const { grid } = renderTable();
      const scrollTo = installScroll(grid);
      focusRow("row-60");
      grid.scrollTop = 0;
      fireEvent.scroll(grid);
      scrollTo.mockClear();

      act(() => useSelectionStore.getState().reveal("row-60"));
      expect(scrollTo).toHaveBeenCalled();
      expect(selectedIds()).toEqual(["row-60"]);
    });
  });

  describe("右クリックメニュー", () => {
    const openMenu = (id: string) => {
      fireEvent.contextMenu(rowEl(id)!);
      return screen.findByRole("menu");
    };
    const lastCall = (fn: unknown) =>
      (fn as ReturnType<typeof vi.fn>).mock.calls.at(-1) as unknown[];

    it("右クリックした行が選択の外なら、その行だけを選び、「ゴミ箱へ移動」でその ID を渡す", async () => {
      const { props } = renderTable();
      fireEvent.click(rowEl("row-1")!);
      const menu = await openMenu("row-5");
      expect(selectedIds()).toEqual(["row-5"]);
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.trash }));
      expect(lastCall(props.onAction)).toEqual(["trash", ["row-5"]]);
    });

    it("「書き出し…」は、選んだ本の ID を渡す", async () => {
      const { props } = renderTable();
      fireEvent.click(rowEl("row-1")!);
      fireEvent.click(rowEl("row-2")!, { metaKey: true });
      const menu = await openMenu("row-1");
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.export }));
      const [action, ids] = lastCall(props.onAction) as [string, string[]];
      expect(action).toBe("export");
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);
    });

    it("ゴミ箱を開いているときは、「ゴミ箱へ移動」の代わりに「ゴミ箱から戻す」と「完全に削除…」を出す", async () => {
      const { props } = renderTable({ inTrash: true });
      const menu = await openMenu("row-3");
      expect(
        within(menu).queryByRole("menuitem", { name: ja.list.contextMenu.trash }),
      ).not.toBeInTheDocument();
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.restore }));
      expect(lastCall(props.onAction)).toEqual(["restore", ["row-3"]]);

      const again = await openMenu("row-3");
      fireEvent.click(within(again).getByRole("menuitem", { name: ja.list.contextMenu.purge }));
      expect(lastCall(props.onAction)).toEqual(["purge", ["row-3"]]);
    });

    it("ゴミ箱を開いているときは、削除のキー操作でさらにゴミ箱へ移さない", () => {
      const { grid, props } = renderTable({ inTrash: true });
      fireEvent.click(rowEl("row-1")!);
      press(grid, { code: "Backspace", metaKey: true });
      expect(props.onAction).not.toHaveBeenCalled();
    });

    it("「一括変更」のサブメニューから、読書状況・所有状態を選べる", async () => {
      const { props } = renderTable();
      fireEvent.click(rowEl("row-1")!);
      fireEvent.click(rowEl("row-2")!, { metaKey: true });
      const menu = await openMenu("row-1");
      const trigger = within(menu).getByRole("menuitem", { name: ja.list.contextMenu.bulkEdit });
      fireEvent.keyDown(trigger, { key: "ArrowRight" });
      const sub = await screen.findByRole("menuitem", { name: ja.labels.readStatus.read });
      fireEvent.click(sub);
      const [change, ids] = lastCall(props.onBulkChange) as [unknown, string[]];
      expect(change).toEqual({ field: "read_status", value: "read" });
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);

      const again = await openMenu("row-1");
      fireEvent.keyDown(
        within(again).getByRole("menuitem", { name: ja.list.contextMenu.bulkEdit }),
        {
          key: "ArrowRight",
        },
      );
      fireEvent.click(await screen.findByRole("menuitem", { name: ja.labels.ownership.released }));
      expect(lastCall(props.onBulkChange)[0]).toEqual({ field: "ownership", value: "released" });
    });

    it("「一括変更」の評価は、★1〜5 と「評価なし」（null）を選べる", async () => {
      const { props } = renderTable();
      fireEvent.click(rowEl("row-1")!);
      const openBulk = async () => {
        const menu = await openMenu("row-1");
        fireEvent.keyDown(
          within(menu).getByRole("menuitem", { name: ja.list.contextMenu.bulkEdit }),
          { key: "ArrowRight" },
        );
      };
      await openBulk();
      fireEvent.click(await screen.findByRole("menuitem", { name: "★★★★" }));
      expect(lastCall(props.onBulkChange)).toEqual([{ field: "rating", value: 4 }, ["row-1"]]);

      await openBulk();
      fireEvent.click(
        await screen.findByRole("menuitem", { name: ja.list.contextMenu.ratingNone }),
      );
      expect(lastCall(props.onBulkChange)).toEqual([{ field: "rating", value: null }, ["row-1"]]);
    });

    it("保管場所・タグの一括変更は、値を選ぶダイアログを開く操作（location・addTags・removeTags）として、選んだ本の ID を渡す", async () => {
      const { props } = renderTable();
      fireEvent.click(rowEl("row-1")!);
      for (const [label, action] of [
        [ja.list.contextMenu.bulkLocation, "location"],
        [ja.list.contextMenu.bulkAddTags, "addTags"],
        [ja.list.contextMenu.bulkRemoveTags, "removeTags"],
      ] as const) {
        const menu = await openMenu("row-1");
        fireEvent.keyDown(
          within(menu).getByRole("menuitem", { name: ja.list.contextMenu.bulkEdit }),
          { key: "ArrowRight" },
        );
        fireEvent.click(await screen.findByRole("menuitem", { name: label }));
        expect(lastCall(props.onAction)).toEqual([action, ["row-1"]]);
      }
    });

    it("「書誌を再取得」は、選んだ本の ID を渡す（確認は呼び出し側）", async () => {
      const { props } = renderTable();
      fireEvent.click(rowEl("row-1")!);
      fireEvent.click(rowEl("row-2")!, { metaKey: true });
      const menu = await openMenu("row-1");
      fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.refetch }));
      const [action, ids] = lastCall(props.onAction) as [string, string[]];
      expect(action).toBe("refetch");
      expect([...ids].sort()).toEqual(["row-1", "row-2"]);
    });

    it("「購入済みにする」は、欲しい本を選んでいるときだけ出て、欲しい本の ID だけを渡す（FR-WISH-01）", async () => {
      const rows = [
        makeRow({ id: "own", ownership: "owned" }),
        makeRow({ id: "want-1", ownership: "wanted" }),
        makeRow({ id: "want-2", ownership: "wanted" }),
      ];
      const { props } = renderTable({ rows });
      fireEvent.click(rowEl("own")!);
      const first = await openMenu("own");
      expect(
        within(first).queryByRole("menuitem", { name: ja.list.contextMenu.markPurchased }),
      ).not.toBeInTheDocument();
      fireEvent.keyDown(first, { key: "Escape" });

      fireEvent.click(rowEl("own")!);
      fireEvent.click(rowEl("want-1")!, { metaKey: true });
      fireEvent.click(rowEl("want-2")!, { metaKey: true });
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
