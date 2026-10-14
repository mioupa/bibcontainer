import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { ColumnPicker } from "@/features/list/ColumnPicker";
import { ja } from "@/i18n/ja";
import { useTableStore } from "@/stores/tableStore";

function openPicker() {
  fireEvent.click(screen.getByRole("button", { name: ja.list.columnPicker.button }));
  return screen.getByRole("list", { name: ja.list.columnPicker.heading });
}

const names = (list: HTMLElement) =>
  within(list)
    .getAllByRole("listitem")
    .map((item) => item.textContent);

describe("列のメニュー（FR-LIST-01）", () => {
  beforeEach(() => {
    useTableStore.getState().resetColumns();
  });

  it("全列を今の並び順で出す。既定では、表示する列のあとに選べる列が続く", () => {
    render(<ColumnPicker />);
    const list = openPicker();
    const shown = names(list);
    expect(shown.slice(0, 3)).toEqual([
      ja.list.columns.cover,
      ja.list.columns.title,
      ja.list.columns.volume,
    ]);
    expect(shown).toHaveLength(Object.keys(ja.list.columns).length);
    expect(within(list).getByRole("checkbox", { name: ja.list.columns.isbn13 })).not.toBeChecked();
    expect(within(list).getByRole("checkbox", { name: ja.list.columns.title })).toBeDisabled();
  });

  it("下・上の矢印で並び順を変える。順は端末の設定（tableStore の columnOrder）に保存される", () => {
    render(<ColumnPicker />);
    const list = openPicker();

    fireEvent.click(
      within(list).getByRole("button", {
        name: ja.list.columnPicker.moveDown(ja.list.columns.cover),
      }),
    );
    expect(names(list).slice(0, 3)).toEqual([
      ja.list.columns.title,
      ja.list.columns.cover,
      ja.list.columns.volume,
    ]);
    expect(useTableStore.getState().columnOrder.slice(0, 3)).toEqual(["title", "cover", "volume"]);

    fireEvent.click(
      within(list).getByRole("button", {
        name: ja.list.columnPicker.moveUp(ja.list.columns.volume),
      }),
    );
    expect(useTableStore.getState().columnOrder.slice(0, 3)).toEqual(["title", "volume", "cover"]);
  });

  it("先頭の列は上へ、末尾の列は下へ動かせない", () => {
    render(<ColumnPicker />);
    const list = openPicker();
    expect(
      within(list).getByRole("button", {
        name: ja.list.columnPicker.moveUp(ja.list.columns.cover),
      }),
    ).toBeDisabled();
    const last = within(list).getAllByRole("listitem").at(-1)!;
    expect(within(last).getByRole("button", { name: /を下（右）へ動かす$/ })).toBeDisabled();
  });

  it("チェックで表示・非表示を切り替えても、並び順は変わらない。「既定に戻す」で並び順も戻る", () => {
    render(<ColumnPicker />);
    const list = openPicker();
    fireEvent.click(
      within(list).getByRole("button", {
        name: ja.list.columnPicker.moveDown(ja.list.columns.cover),
      }),
    );
    fireEvent.click(within(list).getByRole("checkbox", { name: ja.list.columns.isbn13 }));
    expect(useTableStore.getState().columnVisibility.isbn13).toBe(true);
    expect(useTableStore.getState().columnOrder.slice(0, 2)).toEqual(["title", "cover"]);

    fireEvent.click(screen.getByRole("button", { name: ja.list.columnPicker.reset }));
    expect(useTableStore.getState().columnOrder).toEqual([]);
    expect(useTableStore.getState().columnVisibility.isbn13).toBe(false);
    expect(names(list).slice(0, 2)).toEqual([ja.list.columns.cover, ja.list.columns.title]);
  });
});
