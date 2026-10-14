// 設定画面の部品（行のレイアウト、数の入力、節の見出し）。
import { useId } from "react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

/** ラベルと入力を左右に並べる 1 行。 */
export function Row({
  label,
  htmlFor,
  hint,
  children,
  className,
}: {
  label: string;
  htmlFor?: string;
  hint?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex items-center justify-between gap-4 text-sm", className)}>
      <div className="min-w-0 flex-1">
        <label htmlFor={htmlFor}>{label}</label>
        {hint !== undefined && <p className="text-xs text-muted-foreground">{hint}</p>}
      </div>
      {children}
    </div>
  );
}

/** 数の入力。入力中は範囲に収めず、欄を離れたとき（または Enter）に範囲に収めて確定する。 */
export function NumberRow({
  label,
  value,
  clamp,
  onCommit,
  disabled,
}: {
  label: string;
  value: number;
  clamp: (value: number) => number;
  onCommit: (value: number) => void;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <Row label={label} htmlFor={id}>
      <Input
        id={id}
        // 値が外から変わったら（範囲に収められたときなど）、入力欄を作り直して合わせる
        key={value}
        type="number"
        defaultValue={value}
        disabled={disabled}
        className="h-8 w-24 text-right"
        onBlur={(event) => {
          const next = clamp(Number(event.currentTarget.value));
          event.currentTarget.value = String(next);
          if (next !== value) onCommit(next);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.nativeEvent.isComposing) event.currentTarget.blur();
        }}
      />
    </Row>
  );
}

/** 節の中のまとまり（見出しと説明つき）。 */
export function Group({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        {description !== undefined && (
          <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      {children}
    </section>
  );
}
