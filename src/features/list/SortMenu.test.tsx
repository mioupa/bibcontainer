import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { isSortable } from "@/features/library/sort";
import { COLUMN_IDS } from "@/features/list/columnMeta";
import { SortMenu } from "@/features/list/SortMenu";
import { ja } from "@/i18n/ja";
import { useTableStore } from "@/stores/tableStore";

const trigger = (name: string | RegExp = /^並べ替え/) => screen.getByRole("button", { name });
const sorting = () => useTableStore.getState().sorting;

async function openMenu() {
  fireEvent.pointerDown(trigger(), { button: 0, ctrlKey: false });
  return await screen.findByRole("menu");
}
const radio = (menu: HTMLElement, name: string) =>
  within(menu).getByRole("menuitemradio", { name });

describe("グリッドの並べ替えメニュー（FR-LIST-07）", () => {
  beforeEach(() => {
    useTableStore.getState().resetColumns();
  });

  it("引き金のボタンに、いまの先頭の項目と向きを出す", () => {
    render(<SortMenu />);
    expect(trigger()).toHaveTextContent(ja.list.sortMenu.button(ja.list.columns.title, false));
    expect(trigger()).toHaveTextContent("並べ替え: 書名 ↑");

    // 状態（表と共通の tableStore）が変わったら、ラベルも変わる
    act(() => useTableStore.getState().setSorting([{ id: "publisher", desc: true }]));
    expect(trigger(/並べ替え: 出版社 ↓/)).toBeInTheDocument();
  });

  it("並べ替えが空のときは「なし」", () => {
    useTableStore.getState().setSorting([]);
    render(<SortMenu />);
    expect(trigger()).toHaveTextContent(ja.list.sortMenu.buttonNone);
  });

  it("並べ替えられる列（書影を除く）を、列の順に出す。いまの項目と向きにチェックが付く", async () => {
    render(<SortMenu />);
    const menu = await openMenu();
    const sortable = COLUMN_IDS.filter(isSortable).map((id) => ja.list.columns[id]);
    expect(sortable).not.toContain(ja.list.columns.cover);
    const radios = within(menu)
      .getAllByRole("menuitemradio")
      .map((item) => item.textContent);
    // 項目の一覧のあとに、向きの 2 つ
    expect(radios).toEqual([...sortable, ja.list.sortedAscending, ja.list.sortedDescending]);

    expect(radio(menu, ja.list.columns.title)).toHaveAttribute("aria-checked", "true");
    expect(radio(menu, ja.list.columns.authors)).toHaveAttribute("aria-checked", "false");
    expect(radio(menu, ja.list.sortedAscending)).toHaveAttribute("aria-checked", "true");
    expect(radio(menu, ja.list.sortedDescending)).toHaveAttribute("aria-checked", "false");
  });

  it("項目を選ぶと、その項目だけの並べ替えになる。向きはいまの先頭の向きを引き継ぐ", async () => {
    useTableStore.getState().setSorting([
      { id: "title", desc: true },
      { id: "publisher", desc: false },
    ]);
    render(<SortMenu />);
    const menu = await openMenu();
    fireEvent.click(radio(menu, ja.list.columns.authors));
    expect(sorting()).toEqual([{ id: "authors", desc: true }]);
  });

  it("並べ替えが空のときに項目を選ぶと、昇順で並べる", async () => {
    useTableStore.getState().setSorting([]);
    render(<SortMenu />);
    const menu = await openMenu();
    fireEvent.click(radio(menu, ja.list.columns.createdAt));
    expect(sorting()).toEqual([{ id: "createdAt", desc: false }]);
  });

  it("向きを選ぶと、先頭の項目の向きだけが変わる（2 番目以降は残る）", async () => {
    useTableStore.getState().setSorting([
      { id: "publisher", desc: false },
      { id: "title", desc: false },
    ]);
    render(<SortMenu />);
    const menu = await openMenu();
    fireEvent.click(radio(menu, ja.list.sortedDescending));
    expect(sorting()).toEqual([
      { id: "publisher", desc: true },
      { id: "title", desc: false },
    ]);
    expect(trigger()).toHaveTextContent("並べ替え: 出版社 ↓");
  });

  it("並べ替えが空のときに向きを選ぶと、書名で並べる", async () => {
    useTableStore.getState().setSorting([]);
    render(<SortMenu />);
    const menu = await openMenu();
    fireEvent.click(radio(menu, ja.list.sortedDescending));
    expect(sorting()).toEqual([{ id: "title", desc: true }]);
  });
});
