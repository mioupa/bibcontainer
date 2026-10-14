// フォームの 1 項目の行: ラベル、取得元（小さく）、他の取得元の値のメニュー（FR-META-05）、入力欄、ヒント、エラー。
import { CheckIcon, ChevronsUpDownIcon } from "lucide-react";
import type { ReactNode } from "react";
import { useFormContext, useWatch } from "react-hook-form";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Label } from "@/components/ui/label";
import { candidatesFor, showCandidates } from "@/features/editor/candidates";
import { useEditorContext } from "@/features/editor/editorContext";
import {
  fieldDef,
  opValue,
  sameOpValue,
  type BookFormValues,
  type FormKey,
} from "@/features/editor/formValues";
import { errorText } from "@/features/editor/fieldErrors";
import { effectiveSource, sourceLabel } from "@/features/editor/sources";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";

interface FieldRowProps {
  name: FormKey;
  label: string;
  htmlFor?: string;
  hint?: ReactNode;
  className?: string;
  children: ReactNode;
}

export function FieldRow({ name, label, htmlFor, hint, className, children }: FieldRowProps) {
  const ctx = useEditorContext();
  const { control, formState } = useFormContext<BookFormValues>();
  const current = useWatch({ control, name });
  const withCurrent = { ...ctx.initial, [name]: current };
  const currentOp = opValue(name, withCurrent);
  const changed = !sameOpValue(opValue(name, ctx.initial), currentOp);
  const def = fieldDef(name);
  // 取得元の記録（field_sources）があるのは、外部から取得する書誌の項目（候補がある項目と ISBN）だけ。
  // 読書・購入・保管場所・メモなど、利用者が持つ項目には出さない（0006 §1）
  const tracked = def.meta !== undefined || name === "isbn13";
  const source = tracked
    ? effectiveSource({
        initialSource: ctx.initialSources[def.op] ?? null,
        changed,
        pick: ctx.picks[name],
        currentValue: currentOp,
      })
    : null;
  const candidates = candidatesFor(ctx.merged, name);
  const error = errorText(formState.errors[name]);

  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5", className)} data-field={name}>
      <div className="flex min-h-6 items-center gap-1.5">
        <Label htmlFor={htmlFor}>{label}</Label>
        {source !== null && (
          <span
            className="rounded bg-muted px-1.5 py-0.5 text-[10px] leading-none text-muted-foreground"
            title={ja.edit.source.title}
          >
            {sourceLabel(source)}
          </span>
        )}
        {showCandidates(candidates, currentOp) && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={ja.edit.source.otherValues(label)}
                title={ja.edit.source.otherValuesHint}
              >
                <ChevronsUpDownIcon />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="max-w-md">
              <DropdownMenuLabel>{ja.edit.source.menuTitle}</DropdownMenuLabel>
              {candidates.map((candidate) => (
                <DropdownMenuItem
                  key={`${candidate.source}:${candidate.label}`}
                  onSelect={() => ctx.pickCandidate(name, candidate)}
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <CheckIcon
                      className={cn(
                        "size-3.5 shrink-0",
                        sameOpValue(candidate.opValue, currentOp) ? "opacity-100" : "opacity-0",
                      )}
                      aria-hidden
                    />
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {sourceLabel(candidate.source)}
                    </span>
                    <span className="truncate">{candidate.label}</span>
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
      {children}
      {hint}
      {error !== undefined && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
