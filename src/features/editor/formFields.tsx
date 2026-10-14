// フォームの入力欄（react-hook-form につないだもの）。項目の行（FieldRow）の中に入れて使う。
import { StarIcon } from "lucide-react";
import { Controller, useFormContext, useWatch } from "react-hook-form";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { todayLocal } from "@/features/editor/dates";
import { useEditorContext } from "@/features/editor/editorContext";
import { FieldRow } from "@/features/editor/FieldRow";
import { TagChipsInput } from "@/features/editor/TagChipsInput";
import {
  opValue,
  parseIntegerInput,
  type BookFormValues,
  type FormKey,
} from "@/features/editor/formValues";
import { formatListPrice } from "@/features/library/price";
import { ja } from "@/i18n/ja";
import { cCodeFullLabel } from "@/lib/cCode";
import { cn } from "@/lib/utils";

/** 値が文字列の項目。 */
export type StringKey = {
  [K in FormKey]: BookFormValues[K] extends string ? K : never;
}[FormKey];

const fieldId = (name: FormKey) => `edit-${name}`;

interface TextFieldProps {
  name: StringKey;
  label: string;
  placeholder?: string;
  inputMode?: "numeric" | "text";
  /** `<datalist>` の id（入力の候補）。 */
  list?: string;
  maxLength?: number;
  className?: string;
  hint?: React.ReactNode;
  /** 入力欄の右に置くもの（「今日」ボタンなど）。 */
  trailing?: React.ReactNode;
}

export function TextField({
  name,
  label,
  placeholder,
  inputMode,
  list,
  maxLength,
  className,
  hint,
  trailing,
}: TextFieldProps) {
  const { register, formState } = useFormContext<BookFormValues>();
  const invalid = formState.errors[name] !== undefined;
  return (
    <FieldRow name={name} label={label} htmlFor={fieldId(name)} hint={hint} className={className}>
      <div className="flex items-center gap-1.5">
        <Input
          id={fieldId(name)}
          autoComplete="off"
          spellCheck={false}
          placeholder={placeholder}
          inputMode={inputMode}
          list={list}
          maxLength={maxLength}
          aria-invalid={invalid}
          {...register(name)}
        />
        {trailing}
      </div>
    </FieldRow>
  );
}

export function TextAreaField({
  name,
  label,
  rows = 4,
  className,
}: {
  name: StringKey;
  label: string;
  rows?: number;
  className?: string;
}) {
  const { register, formState } = useFormContext<BookFormValues>();
  return (
    <FieldRow name={name} label={label} htmlFor={fieldId(name)} className={className}>
      <Textarea
        id={fieldId(name)}
        rows={rows}
        aria-invalid={formState.errors[name] !== undefined}
        {...register(name)}
      />
    </FieldRow>
  );
}

/** 日付（YYYY・YYYY-MM・YYYY-MM-DD）の欄。「今日」ボタン付き。 */
export function DateField({
  name,
  label,
  className,
}: {
  name: StringKey;
  label: string;
  className?: string;
}) {
  const { setValue } = useFormContext<BookFormValues>();
  return (
    <TextField
      name={name}
      label={label}
      placeholder={ja.edit.datePlaceholder}
      inputMode="numeric"
      className={className}
      trailing={
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setValue(name, todayLocal(), { shouldDirty: true, shouldValidate: true })}
        >
          {ja.edit.today}
        </Button>
      }
    />
  );
}

/** 本体価格。設定が「税込を併記」なら、入力した本体価格の税込価格を欄の下に出す（FR-CONF-05）。 */
export function ListPriceField({ className }: { className?: string }) {
  const ctx = useEditorContext();
  const { control } = useFormContext<BookFormValues>();
  const raw = useWatch({ control, name: "listPrice" });
  const parsed = parseIntegerInput(raw);
  const showTax = ctx.price.showTaxIncluded && typeof parsed === "number";
  return (
    <TextField
      name="listPrice"
      label={ja.edit.fields.listPrice}
      inputMode="numeric"
      className={className}
      hint={
        showTax ? (
          <p className="text-xs text-muted-foreground">{formatListPrice(parsed, ctx.price)}</p>
        ) : undefined
      }
    />
  );
}

/** Cコード。数字と名称を並べて出す（9.2）。名称は Cコード表から。 */
export function CCodeField({ className }: { className?: string }) {
  const ctx = useEditorContext();
  const { control } = useFormContext<BookFormValues>();
  const raw = useWatch({ control, name: "cCode" });
  const code = opValue("cCode", { ...ctx.initial, cCode: raw });
  const label = typeof code === "string" ? cCodeFullLabel(ctx.cCodes, code) : null;
  return (
    <TextField
      name="cCode"
      label={ja.edit.fields.cCode}
      inputMode="numeric"
      maxLength={8}
      className={className}
      hint={
        label !== null ? (
          <p className="text-xs break-words text-muted-foreground" data-testid="c-code-label">
            {label}
          </p>
        ) : undefined
      }
    />
  );
}

const NONE = "__none__";

export interface SelectOption {
  value: string;
  label: string;
  /** ツリーの深さ（インデントに使う）。 */
  depth?: number;
}

/** 選択肢から 1 つ選ぶ欄。`noneLabel` を渡すと、「未選択（null）」を選べる。 */
export function OptionSelectField({
  name,
  label,
  options,
  noneLabel,
  disabled,
  className,
}: {
  name: "locationId" | "ownership" | "wishPriority" | "readStatus" | "purchaseCond";
  label: string;
  options: readonly SelectOption[];
  noneLabel?: string;
  disabled?: boolean;
  className?: string;
}) {
  const { control } = useFormContext<BookFormValues>();
  return (
    <FieldRow name={name} label={label} htmlFor={fieldId(name)} className={className}>
      <Controller
        control={control}
        name={name}
        render={({ field }) => (
          <Select
            value={field.value ?? NONE}
            onValueChange={(value) => field.onChange(value === NONE ? null : value)}
            disabled={disabled}
          >
            <SelectTrigger id={fieldId(name)} className="w-full" onBlur={field.onBlur}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="max-h-72">
              {noneLabel !== undefined && <SelectItem value={NONE}>{noneLabel}</SelectItem>}
              {options.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  <span style={{ paddingLeft: `${(option.depth ?? 0) * 0.75}rem` }}>
                    {option.label}
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      />
    </FieldRow>
  );
}

/** 評価（★1〜5）。同じ星をもう一度押すと評価なしに戻る。 */
export function RatingField({ className }: { className?: string }) {
  const { control } = useFormContext<BookFormValues>();
  return (
    <FieldRow name="rating" label={ja.edit.fields.rating} className={className}>
      <Controller
        control={control}
        name="rating"
        render={({ field }) => (
          <div className="flex items-center gap-1" role="group" aria-label={ja.edit.fields.rating}>
            {[1, 2, 3, 4, 5].map((star) => (
              <Button
                key={star}
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={ja.edit.ratingStar(star)}
                aria-pressed={field.value === star}
                onClick={() => field.onChange(field.value === star ? null : star)}
              >
                <StarIcon
                  className={cn(
                    "size-5",
                    field.value !== null && star <= field.value
                      ? "fill-warning text-warning"
                      : "text-muted-foreground",
                  )}
                />
              </Button>
            ))}
            {field.value !== null && (
              <Button type="button" variant="ghost" size="xs" onClick={() => field.onChange(null)}>
                {ja.edit.ratingClear}
              </Button>
            )}
          </div>
        )}
      />
    </FieldRow>
  );
}

/** タグ。候補（`list_tags`）付きの複数入力。Enter かカンマで足し、× で外す。 */
export function TagsField({ className }: { className?: string }) {
  const ctx = useEditorContext();
  const { control } = useFormContext<BookFormValues>();
  return (
    <FieldRow
      name="tags"
      label={ja.edit.fields.tags}
      htmlFor={fieldId("tags")}
      className={className}
      hint={<p className="text-xs text-muted-foreground">{ja.edit.tagsHint}</p>}
    >
      <Controller
        control={control}
        name="tags"
        render={({ field }) => (
          <TagChipsInput
            id={fieldId("tags")}
            value={field.value}
            onChange={field.onChange}
            onBlur={field.onBlur}
            suggestions={ctx.tagSuggestions}
          />
        )}
      />
    </FieldRow>
  );
}
