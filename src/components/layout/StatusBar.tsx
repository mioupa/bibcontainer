// 下部のステータスバー（9.1）:「N 冊中 M 冊を表示」。書誌の再取得が動いている間は、その進み具合と「やめる」（0027）。
import { Button } from "@/components/ui/button";
import { describeRemaining } from "@/features/library/refetch";
import { libraryApi } from "@/features/library/api";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { useRefetchJobStore } from "@/stores/refetchJobStore";
import { toast } from "sonner";

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
  const job = useRefetchJobStore((state) => state.job);
  const text = ja.statusBar.refetch;
  const cancel = () => {
    libraryApi.cancelRefetch().catch((error: unknown) => toast.error(errorMessage(error)));
  };
  return (
    <footer
      role="status"
      className="flex h-7 shrink-0 items-center gap-4 border-t bg-card px-3 text-xs text-muted-foreground"
    >
      <span>{ja.statusBar.shown(total, shown)}</span>
      {selectedCount > 0 && <span>{ja.statusBar.selected(selectedCount)}</span>}
      {job?.state === "running" && (
        <div className="ml-auto flex items-center gap-2">
          <span>
            {text.progress(job.done, job.total)}・{describeRemaining(job)}
          </span>
          <div
            role="progressbar"
            aria-label={text.barLabel}
            aria-valuemin={0}
            aria-valuemax={job.total}
            aria-valuenow={job.done}
            className="h-1 w-32 overflow-hidden rounded-full bg-muted"
          >
            <div
              className="h-full bg-primary"
              style={{
                width: `${job.total === 0 ? 0 : Math.min(100, (job.done / job.total) * 100)}%`,
              }}
            />
          </div>
          <Button variant="ghost" size="xs" onClick={cancel}>
            {text.cancel}
          </Button>
        </div>
      )}
      {mock && (
        <span className={job?.state === "running" ? "text-warning" : "ml-auto text-warning"}>
          {ja.statusBar.mockData}
        </span>
      )}
    </footer>
  );
}
