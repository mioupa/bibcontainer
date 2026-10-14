// 下部のステータスバー（9.1）:「N 冊中 M 冊を表示」。
import { ja } from "@/i18n/ja";

interface StatusBarProps {
  /** 選んでいるライブラリの冊数（N）。 */
  total: number;
  /** 表示している冊数（M）。 */
  shown: number;
  selectedCount: number;
  /** モックのデータで動いているか。 */
  mock: boolean;
}

export function StatusBar({ total, shown, selectedCount, mock }: StatusBarProps) {
  return (
    <footer
      role="status"
      className="flex h-7 shrink-0 items-center gap-4 border-t bg-card px-3 text-xs text-muted-foreground"
    >
      <span>{ja.statusBar.shown(total, shown)}</span>
      {selectedCount > 0 && <span>{ja.statusBar.selected(selectedCount)}</span>}
      {mock && <span className="ml-auto text-warning">{ja.statusBar.mockData}</span>}
    </footer>
  );
}
