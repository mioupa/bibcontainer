// 列の表示・非表示と並び順を選ぶメニュー（FR-LIST-01）。矢印で並び順を変えるので、メニュー（項目を 1 つずつ選ぶ形）ではなく、
// 中のボタンへ Tab で移れるポップオーバーにしている。
import { Columns3Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ColumnList } from "@/features/list/ColumnList";
import { ja } from "@/i18n/ja";
import { useTableStore } from "@/stores/tableStore";

export function ColumnPicker() {
  const resetColumns = useTableStore((s) => s.resetColumns);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="xs">
          <Columns3Icon />
          {ja.list.columnPicker.button}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 gap-2 p-3">
        <p className="text-xs text-muted-foreground">{ja.list.columnPicker.hint}</p>
        <ColumnList className="max-h-80 overflow-y-auto" />
        <Button variant="outline" size="xs" onClick={resetColumns}>
          {ja.list.columnPicker.reset}
        </Button>
      </PopoverContent>
    </Popover>
  );
}
