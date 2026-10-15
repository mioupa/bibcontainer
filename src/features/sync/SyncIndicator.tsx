// 上部バーの同期の表示（9.1）: 「同期: ✓ 12:04」。状態ごとに見た目を変え、押すと設定の同期の節を開く。
// ツールチップに詳細（フォルダ、最終書き出し・取り込み、未書き出し、届いていない書影、エラー、時計のずれ）。
import {
  CheckIcon,
  ClockAlertIcon,
  CloudAlertIcon,
  CloudIcon,
  CloudOffIcon,
  CloudUploadIcon,
  RefreshCwIcon,
} from "lucide-react";
import type { ComponentType } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { describeSync, type SyncDisplayKind, type SyncTone } from "@/features/sync/display";
import { useSyncStatus } from "@/features/sync/hooks";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";
import { useScreenStore } from "@/stores/screenStore";

const ICONS: Record<SyncDisplayKind, ComponentType<{ className?: string }>> = {
  loading: CloudIcon,
  disabled: CloudOffIcon,
  syncing: RefreshCwIcon,
  synced: CheckIcon,
  pending: CloudUploadIcon,
  error: CloudAlertIcon,
  mismatch: CloudAlertIcon,
  updateRequired: CloudAlertIcon,
};

const ICON_TONE: Record<SyncTone, string> = {
  muted: "text-muted-foreground",
  info: "text-info",
  success: "text-success",
  warning: "text-warning",
  danger: "text-destructive",
};

export function SyncIndicator() {
  const { data } = useSyncStatus();
  const showSettings = useScreenStore((s) => s.showSettings);
  const display = describeSync(data, new Date());
  const Icon = ICONS[display.kind];
  const t = ja.sync.indicator;
  const accessibleText = display.skew ? `${display.text}、${t.skewBadge}` : display.text;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          data-testid="sync-status"
          data-kind={display.kind}
          data-tone={display.tone}
          aria-label={t.openLabel(`${t.label}: ${accessibleText}`)}
          onClick={() => showSettings("sync")}
          className={cn(
            "flex h-8 items-center gap-1.5 rounded-md px-2 text-xs outline-none hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring/50",
            display.tone === "danger" ? "text-destructive" : "text-muted-foreground",
          )}
        >
          <Icon
            className={cn(
              "size-4 shrink-0",
              ICON_TONE[display.tone],
              display.kind === "syncing" && "motion-safe:animate-spin",
            )}
            aria-hidden
          />
          <span aria-hidden>
            {t.label}: {display.text}
          </span>
          {display.skew && <ClockAlertIcon className="size-4 shrink-0 text-warning" aria-hidden />}
        </button>
      </TooltipTrigger>
      <TooltipContent className="flex-col items-start gap-0.5" side="bottom">
        {display.details.map((line, index) => (
          <span key={index} className="block max-w-full break-all">
            {line}
          </span>
        ))}
        <span className="mt-1 block opacity-70">{ja.sync.tooltip.clickHint}</span>
      </TooltipContent>
    </Tooltip>
  );
}
