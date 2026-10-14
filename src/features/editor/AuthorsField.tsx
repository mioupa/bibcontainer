// 著者（名前・ヨミ・役割の複数行）。空の行は保存のとき無視される。
import { PlusIcon, Trash2Icon } from "lucide-react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { FieldRow } from "@/features/editor/FieldRow";
import type { BookFormValues } from "@/features/editor/formValues";
import { ja } from "@/i18n/ja";

export function AuthorsField({ className }: { className?: string }) {
  const { register, control, formState } = useFormContext<BookFormValues>();
  const { fields, append, remove } = useFieldArray({ control, name: "authors" });

  return (
    <FieldRow name="authors" label={ja.edit.fields.authors} className={className}>
      <div className="flex flex-col gap-2">
        {fields.map((field, index) => (
          <div
            key={field.id}
            className="grid grid-cols-[minmax(0,2fr)_minmax(0,2fr)_minmax(0,1fr)_auto] items-center gap-1.5"
          >
            <Input
              aria-label={ja.edit.authors.name(index + 1)}
              placeholder={ja.edit.authors.namePlaceholder}
              autoComplete="off"
              aria-invalid={formState.errors.authors?.[index]?.name !== undefined}
              {...register(`authors.${index}.name`)}
            />
            <Input
              aria-label={ja.edit.authors.kana(index + 1)}
              placeholder={ja.edit.authors.kanaPlaceholder}
              autoComplete="off"
              {...register(`authors.${index}.kana`)}
            />
            <Input
              aria-label={ja.edit.authors.role(index + 1)}
              placeholder={ja.edit.authors.rolePlaceholder}
              list="edit-author-roles"
              autoComplete="off"
              {...register(`authors.${index}.role`)}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={ja.edit.authors.remove(index + 1)}
              onClick={() => remove(index)}
            >
              <Trash2Icon />
            </Button>
          </div>
        ))}
        <datalist id="edit-author-roles">
          {ja.edit.authors.roleOptions.map((role) => (
            <option key={role} value={role} />
          ))}
        </datalist>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="self-start"
          onClick={() => append({ name: "", kana: "", role: "" })}
        >
          <PlusIcon />
          {ja.edit.authors.add}
        </Button>
      </div>
    </FieldRow>
  );
}
