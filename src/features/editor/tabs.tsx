// フォームの 5 つのタブの中身（9.2）。入力欄の並べ方だけを持つ。値・検証・保存は BookEditorForm。
import { DownloadIcon } from "lucide-react";
import { useMemo } from "react";
import { useFormContext, useWatch } from "react-hook-form";
import { Button } from "@/components/ui/button";
import { AuthorsField } from "@/features/editor/AuthorsField";
import { CoverField } from "@/features/editor/CoverField";
import { useEditorContext } from "@/features/editor/editorContext";
import {
  CCodeField,
  DateField,
  ListPriceField,
  OptionSelectField,
  RatingField,
  TagsField,
  TextAreaField,
  TextField,
  type SelectOption,
} from "@/features/editor/formFields";
import type { BookFormValues } from "@/features/editor/formValues";
import { flattenLocations } from "@/features/library/trees";
import { ja } from "@/i18n/ja";
import type { Ownership, ReadStatus, WishPriority } from "@/types/contracts";

const options = <T extends string>(labels: Record<T, string>): SelectOption[] =>
  (Object.keys(labels) as T[]).map((value) => ({ value, label: labels[value] }));

const OWNERSHIP_OPTIONS = options<Ownership>(ja.labels.ownership);
const READ_STATUS_OPTIONS = options<ReadStatus>(ja.labels.readStatus);
const WISH_PRIORITY_OPTIONS = options<WishPriority>(ja.labels.wishPriority);
const PURCHASE_COND_OPTIONS: SelectOption[] = [
  { value: "new", label: ja.edit.purchaseCond.new },
  { value: "used", label: ja.edit.purchaseCond.used },
];

interface BasicTabProps {
  /** 新しい本の登録で、書誌を取得して空の項目に入れる操作（ISBN の横のボタン）。 */
  onFetch: (() => void) | null;
  fetching: boolean;
}

export function BasicTab({ onFetch, fetching }: BasicTabProps) {
  return (
    <div className="grid gap-6 p-1 md:grid-cols-[15rem_minmax(0,1fr)]">
      <CoverField />
      <div className="grid grid-cols-2 content-start gap-x-4 gap-y-4">
        <TextField name="title" label={ja.edit.fields.title} className="col-span-2" />
        <TextField name="titleKana" label={ja.edit.fields.titleKana} className="col-span-2" />
        <TextField name="subtitle" label={ja.edit.fields.subtitle} className="col-span-2" />
        <AuthorsField className="col-span-2" />
        <TextField name="publisher" label={ja.edit.fields.publisher} />
        <TextField
          name="publishedOn"
          label={ja.edit.fields.publishedOn}
          placeholder={ja.edit.datePlaceholder}
          inputMode="numeric"
        />
        <SeriesField />
        <TextField name="volume" label={ja.edit.fields.volume} placeholder={ja.edit.volumeHint} />
        <TextField
          name="isbn13"
          label={ja.edit.fields.isbn13}
          inputMode="numeric"
          trailing={
            onFetch !== null ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={fetching}
                onClick={onFetch}
              >
                <DownloadIcon />
                {fetching ? ja.edit.fetching : ja.edit.fetch}
              </Button>
            ) : undefined
          }
        />
        <CCodeField />
        <ListPriceField />
        <TextField name="pages" label={ja.edit.fields.pages} inputMode="numeric" />
        <TextField name="sizeText" label={ja.edit.fields.sizeText} placeholder="15cm" />
        <TextField name="ndc" label={ja.edit.fields.ndc} />
        <TextField name="language" label={ja.edit.fields.language} placeholder="ja" />
        <TextAreaField
          name="description"
          label={ja.edit.fields.description}
          rows={5}
          className="col-span-2"
        />
      </div>
    </div>
  );
}

/** シリーズ。名前を入力する（既存のシリーズは候補に出る）。保存のとき、名前から ID を引く（なければ作る）。 */
function SeriesField() {
  const ctx = useEditorContext();
  return (
    <>
      <TextField
        name="seriesName"
        label={ja.edit.fields.seriesName}
        list="edit-series-options"
        placeholder={ja.edit.seriesPlaceholder}
      />
      <datalist id="edit-series-options">
        {ctx.seriesSuggestions.map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
    </>
  );
}

export function ClassificationTab() {
  const ctx = useEditorContext();
  const { control } = useFormContext<BookFormValues>();
  const ownership = useWatch({ control, name: "ownership" });
  const locationOptions = useMemo<SelectOption[]>(
    () =>
      flattenLocations(ctx.locations).map((option) => ({
        value: option.id,
        label: option.name,
        depth: option.depth,
      })),
    [ctx.locations],
  );
  return (
    <div className="grid max-w-2xl grid-cols-2 gap-x-4 gap-y-4 p-1">
      <OptionSelectField
        name="locationId"
        label={ja.edit.fields.location}
        options={locationOptions}
        noneLabel={ja.edit.locationNone}
        className="col-span-2"
      />
      <TagsField className="col-span-2" />
      <OptionSelectField
        name="ownership"
        label={ja.edit.fields.ownership}
        options={OWNERSHIP_OPTIONS}
      />
      <OptionSelectField
        name="wishPriority"
        label={ja.edit.fields.wishPriority}
        options={WISH_PRIORITY_OPTIONS}
        noneLabel={ja.edit.priorityNone}
        disabled={ownership !== "wanted"}
      />
    </div>
  );
}

export function ReadingTab() {
  return (
    <div className="grid max-w-2xl grid-cols-2 gap-x-4 gap-y-4 p-1">
      <OptionSelectField
        name="readStatus"
        label={ja.edit.fields.readStatus}
        options={READ_STATUS_OPTIONS}
        className="col-span-2"
      />
      <DateField name="readStartedOn" label={ja.edit.fields.readStartedOn} />
      <DateField name="readFinishedOn" label={ja.edit.fields.readFinishedOn} />
      <RatingField className="col-span-2" />
      <TextAreaField name="review" label={ja.edit.fields.review} rows={6} className="col-span-2" />
    </div>
  );
}

export function PurchaseTab() {
  const ctx = useEditorContext();
  return (
    <div className="grid max-w-2xl grid-cols-2 gap-x-4 gap-y-4 p-1">
      <DateField name="purchasedOn" label={ja.edit.fields.purchasedOn} />
      <TextField
        name="purchaseStore"
        label={ja.edit.fields.purchaseStore}
        list="edit-store-options"
      />
      <datalist id="edit-store-options">
        {ctx.storeSuggestions.map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
      <TextField name="purchasePrice" label={ja.edit.fields.purchasePrice} inputMode="numeric" />
      <OptionSelectField
        name="purchaseCond"
        label={ja.edit.fields.purchaseCond}
        options={PURCHASE_COND_OPTIONS}
        noneLabel={ja.edit.purchaseCond.none}
      />
      <TextAreaField
        name="purchaseMemo"
        label={ja.edit.fields.purchaseMemo}
        rows={4}
        className="col-span-2"
      />
    </div>
  );
}

export function MemoTab() {
  return (
    <div className="max-w-2xl p-1">
      <TextAreaField name="memo" label={ja.edit.fields.memo} rows={12} />
    </div>
  );
}
