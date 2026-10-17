// 一覧の表示の形（表・グリッド。FR-LIST-07）を切り替える、アイコンのボタン 2 つ。選んだ形は端末ごとに保存する（layoutStore）。
import { LayoutGridIcon, Rows3Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ja } from "@/i18n/ja";
import { useLayoutStore } from "@/stores/layoutStore";

const OPTIONS = [
  {
    mode: "table",
    Icon: Rows3Icon,
    label: ja.list.viewSwitch.table,
    hint: ja.list.viewSwitch.tableHint,
  },
  {
    mode: "grid",
    Icon: LayoutGridIcon,
    label: ja.list.viewSwitch.grid,
    hint: ja.list.viewSwitch.gridHint,
  },
] as const;

export function ViewSwitch() {
  const viewMode = useLayoutStore((s) => s.viewMode);
  const setViewMode = useLayoutStore((s) => s.setViewMode);

  return (
    <div
      role="group"
      aria-label={ja.list.viewSwitch.ariaLabel}
      className="inline-flex items-center gap-0.5 rounded-md border p-0.5"
    >
      {OPTIONS.map(({ mode, Icon, label, hint }) => {
        const active = viewMode === mode;
        return (
          <Button
            key={mode}
            variant={active ? "secondary" : "ghost"}
            size="icon-xs"
            aria-label={label}
            aria-pressed={active}
            title={hint}
            onClick={() => setViewMode(mode)}
          >
            <Icon />
          </Button>
        );
      })}
    </div>
  );
}
