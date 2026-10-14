// 「1 冊ずつ」と「連続」の切り替え（9.3）。選んだ方法は、端末に保存する。
import { Button } from "@/components/ui/button";
import type { ScanMode } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";

interface ModeToggleProps {
  mode: ScanMode;
  onChange: (mode: ScanMode) => void;
  /** 連続登録の一覧に溜まっている冊数（連続の横に出す）。 */
  batchCount: number;
}

export function ModeToggle({ mode, onChange, batchCount }: ModeToggleProps) {
  const m = ja.scan.screen.mode;
  const options: { value: ScanMode; label: string; hint: string }[] = [
    { value: "single", label: m.single, hint: m.singleHint },
    { value: "batch", label: m.batch, hint: m.batchHint },
  ];
  return (
    <div role="radiogroup" aria-label={m.label} className="inline-flex rounded-lg bg-muted p-[3px]">
      {options.map((option) => {
        const checked = mode === option.value;
        return (
          <Button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            title={option.hint}
            variant={checked ? "default" : "ghost"}
            size="sm"
            className={cn("min-w-20", !checked && "text-muted-foreground")}
            onClick={() => onChange(option.value)}
          >
            {option.label}
            {option.value === "batch" && batchCount > 0 && (
              <span className="ml-1 tabular-nums">（{batchCount}）</span>
            )}
          </Button>
        );
      })}
    </div>
  );
}
