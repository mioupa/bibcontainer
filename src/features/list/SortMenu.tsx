// グリッド表示の並べ替えメニュー（FR-LIST-07）。グリッドには列の見出しがないので、表の見出しのクリックの代わりに、
// 並べ替える項目（先頭の 1 つ）と向きを、ここで選ぶ。状態は表と同じ（tableStore の `sorting`）で、表に戻しても引き継がれる。
// 複数列の並べ替えは、表の見出しの Shift+クリックで作る。ここで項目を選ぶと、1 つの項目だけの並べ替えになる。
import { ArrowDownUpIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { isSortable } from "@/features/library/sort";
import { COLUMN_IDS, type ColumnId } from "@/features/list/columnMeta";
import { ja } from "@/i18n/ja";
import { useTableStore } from "@/stores/tableStore";

/** 並べ替えに選べる項目（列の順）。 */
const SORTABLE_COLUMN_IDS: readonly ColumnId[] = COLUMN_IDS.filter(isSortable);

/** 項目を選んだとき、並べ替えが空だった場合の項目（向きだけ先に選んだとき）。 */
const FALLBACK_COLUMN: ColumnId = "title";

export function SortMenu() {
  const sorting = useTableStore((s) => s.sorting);
  const setSorting = useTableStore((s) => s.setSorting);

  const primary = sorting[0];
  const column = primary?.id as ColumnId | undefined;
  const descending = primary?.desc ?? false;

  const label =
    column === undefined || !(column in ja.list.columns)
      ? ja.list.sortMenu.buttonNone
      : ja.list.sortMenu.button(ja.list.columns[column], descending);

  const chooseColumn = (id: string) => {
    // 向きは、いまの先頭の向きを引き継ぐ（なければ昇順）。2 番目以降の項目は外れる
    setSorting([{ id, desc: descending }]);
  };
  const chooseDirection = (value: string) => {
    const desc = value === "desc";
    setSorting((old) =>
      old.length === 0
        ? [{ id: FALLBACK_COLUMN, desc }]
        : old.map((spec, index) => (index === 0 ? { ...spec, desc } : spec)),
    );
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="xs">
          <ArrowDownUpIcon />
          {label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-96 w-48 overflow-y-auto">
        <DropdownMenuLabel>{ja.list.sortMenu.columnHeading}</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={primary?.id ?? ""} onValueChange={chooseColumn}>
          {SORTABLE_COLUMN_IDS.map((id) => (
            <DropdownMenuRadioItem key={id} value={id}>
              {ja.list.columns[id]}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuLabel>{ja.list.sortMenu.directionHeading}</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={primary === undefined ? "" : descending ? "desc" : "asc"}
          onValueChange={chooseDirection}
        >
          <DropdownMenuRadioItem value="asc">{ja.list.sortedAscending}</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="desc">{ja.list.sortedDescending}</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
