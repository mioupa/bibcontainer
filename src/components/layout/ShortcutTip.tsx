import type { ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { shortcutLabel, type ShortcutId } from "@/features/shortcuts/shortcuts";
import { detectPlatform } from "@/lib/platform";

const PLATFORM = detectPlatform();

/** ボタンなどにツールチップを付ける。ショートカットがあれば、名前のあとに併記する。 */
export function ShortcutTip({
  label,
  shortcut,
  children,
}: {
  label: string;
  shortcut?: ShortcutId;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent>
        {label}
        {shortcut && <span className="ml-1 opacity-70">{shortcutLabel(shortcut, PLATFORM)}</span>}
      </TooltipContent>
    </Tooltip>
  );
}
