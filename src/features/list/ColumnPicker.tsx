// 列の表示・非表示を選ぶメニュー（FR-LIST-01）。
import { Columns3Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  DEFAULT_COLUMN_IDS,
  FIXED_COLUMN_IDS,
  OPTIONAL_COLUMN_IDS,
  type ColumnId,
} from "@/features/list/columnMeta";
import { ja } from "@/i18n/ja";
import { useTableStore } from "@/stores/tableStore";

export function ColumnPicker() {
  const columnVisibility = useTableStore((s) => s.columnVisibility);
  const setColumnVisibility = useTableStore((s) => s.setColumnVisibility);
  const resetColumns = useTableStore((s) => s.resetColumns);

  // 明示的に false の列だけが隠れている（記録のない列は表示）
  const isVisible = (id: ColumnId) => columnVisibility[id] !== false;

  const item = (id: ColumnId) => (
    <DropdownMenuCheckboxItem
      key={id}
      checked={isVisible(id)}
      disabled={FIXED_COLUMN_IDS.includes(id)}
      // 選んだあともメニューを開いたままにして、続けて切り替えられるようにする
      onSelect={(event) => event.preventDefault()}
      onCheckedChange={(checked) => setColumnVisibility((old) => ({ ...old, [id]: checked }))}
    >
      {ja.list.columns[id]}
    </DropdownMenuCheckboxItem>
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="xs">
          <Columns3Icon />
          {ja.list.columnPicker.button}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-96 w-52">
        <DropdownMenuLabel>{ja.list.columnPicker.heading}</DropdownMenuLabel>
        {DEFAULT_COLUMN_IDS.map(item)}
        <DropdownMenuSeparator />
        {OPTIONAL_COLUMN_IDS.map(item)}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={resetColumns}>{ja.list.columnPicker.reset}</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
