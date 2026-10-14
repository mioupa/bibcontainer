// 一覧の列を、今の並び順で一覧にして、表示・非表示（チェック）と並び順（上・下へ動かす矢印）を変える（FR-LIST-01）。
// メイン画面の「列」メニュー（ColumnPicker）と、設定の「表示」が使う。どちらも同じ設定（stores/tableStore.ts）を変える。
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react";
import { useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { FIXED_COLUMN_IDS, resolveColumnOrder } from "@/features/list/columnMeta";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";
import { useTableStore } from "@/stores/tableStore";

export function ColumnList({ className }: { className?: string }) {
  const columnVisibility = useTableStore((s) => s.columnVisibility);
  const savedOrder = useTableStore((s) => s.columnOrder);
  const setColumnVisibility = useTableStore((s) => s.setColumnVisibility);
  const moveColumn = useTableStore((s) => s.moveColumn);
  const order = useMemo(() => resolveColumnOrder(savedOrder), [savedOrder]);

  return (
    <ul
      aria-label={ja.list.columnPicker.heading}
      className={cn("flex flex-col gap-0.5", className)}
    >
      {order.map((id, index) => {
        const name = ja.list.columns[id];
        return (
          <li key={id} data-column-id={id} className="flex items-center gap-1">
            {/* 明示的に false の列だけが隠れている（記録のない列は表示） */}
            <label className="flex min-w-0 flex-1 items-center gap-2 py-1 text-sm">
              <Checkbox
                checked={columnVisibility[id] !== false}
                disabled={FIXED_COLUMN_IDS.includes(id)}
                onCheckedChange={(checked) =>
                  setColumnVisibility((old) => ({ ...old, [id]: checked === true }))
                }
              />
              <span className="truncate">{name}</span>
            </label>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={ja.list.columnPicker.moveUp(name)}
              disabled={index === 0}
              onClick={() => moveColumn(id, "up")}
            >
              <ChevronUpIcon />
            </Button>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={ja.list.columnPicker.moveDown(name)}
              disabled={index === order.length - 1}
              onClick={() => moveColumn(id, "down")}
            >
              <ChevronDownIcon />
            </Button>
          </li>
        );
      })}
    </ul>
  );
}
