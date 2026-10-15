// 同期の警告・案内の囲み（ライブラリの不一致、時計のずれ、更新が必要、エラー、設定の途中の理由）。
import { CircleAlertIcon, TriangleAlertIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export type NoticeTone = "warning" | "danger";

export function Notice({
  tone,
  title,
  children,
  testId,
}: {
  tone: NoticeTone;
  title: string;
  children?: ReactNode;
  testId?: string;
}) {
  const Icon = tone === "danger" ? CircleAlertIcon : TriangleAlertIcon;
  return (
    <div
      role="alert"
      data-testid={testId}
      data-tone={tone}
      className={cn(
        "flex gap-3 rounded-lg border px-4 py-3 text-sm",
        tone === "danger"
          ? "border-destructive/40 bg-destructive/10"
          : "border-warning/40 bg-warning/10",
      )}
    >
      <Icon
        className={cn(
          "mt-0.5 size-4 shrink-0",
          tone === "danger" ? "text-destructive" : "text-warning",
        )}
        aria-hidden
      />
      <div className="min-w-0 flex-1">
        <p className="font-medium">{title}</p>
        {children !== undefined && (
          <div className="mt-1 flex flex-col gap-2 text-xs text-muted-foreground">{children}</div>
        )}
      </div>
    </div>
  );
}
