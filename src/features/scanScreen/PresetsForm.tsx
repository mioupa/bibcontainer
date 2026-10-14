// 連続登録の「全冊に共通の値」の入力欄（FR-SCAN-15）。保管場所・タグ・所有状態・購入日・購入店。
import { useId, useMemo, useState } from "react";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useLocations, usePurchaseStores, useTags } from "@/features/library/hooks";
import { buildLocationPaths } from "@/features/library/trees";
import { parseTagInput } from "@/features/scanScreen/batch";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import type { Ownership } from "@/types/contracts";

/** Select は空の値を持てないので、「指定しない」にはこの値を使う。 */
const NONE = "__none__";

const OWNERSHIPS: readonly Ownership[] = ["owned", "wanted", "released"];

/**
 * 選んだあと、フォーカスを選択欄に戻さない。入力欄（コンボボックス）にフォーカスが残ると、リーダーの入力を拾わなくなる
 * （数字が選択欄の文字検索に入ってしまう）ため。
 */
const keepFocusAway = (event: Event) => event.preventDefault();

/** Enter（日本語入力の変換中を除く）で入力を終えて、フォーカスを外す（リーダーの入力を拾えるようにする）。 */
const blurOnEnter = (event: React.KeyboardEvent<HTMLInputElement>) => {
  if (event.key === "Enter" && !event.nativeEvent.isComposing) event.currentTarget.blur();
};

export function PresetsForm() {
  const presets = useScanSessionStore((s) => s.presets);
  const setPresets = useScanSessionStore((s) => s.setPresets);
  const locations = useLocations().data;
  const tags = useTags().data;
  const stores = usePurchaseStores().data;

  const paths = useMemo(() => buildLocationPaths(locations ?? []), [locations]);
  const locationOptions = useMemo(
    () =>
      (locations ?? [])
        .map((location) => ({ id: location.id, path: paths.get(location.id) ?? location.name }))
        .sort((a, b) => a.path.localeCompare(b.path, "ja")),
    [locations, paths],
  );

  // タグは、入力中の区切り文字（末尾のカンマ）を残すため、文字列を別に持つ
  const [tagText, setTagText] = useState(() => presets.tags.join(", "));
  const tagListId = useId();
  const storeListId = useId();
  const locationId = useId();
  const ownershipId = useId();
  const tagsId = useId();
  const purchasedOnId = useId();
  const purchaseStoreId = useId();
  const p = ja.scan.batch.presets;

  return (
    <section
      aria-label={p.heading}
      className="flex flex-col gap-3 rounded-xl border bg-card p-4"
      data-testid="presets"
    >
      <div>
        <h3 className="text-sm font-semibold">{p.heading}</h3>
        <p className="text-xs text-muted-foreground">{p.description}</p>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <Field label={p.location} htmlFor={locationId}>
          <Select
            value={presets.locationId ?? NONE}
            onValueChange={(value) => setPresets({ locationId: value === NONE ? null : value })}
          >
            <SelectTrigger id={locationId} className="w-full" aria-label={p.location}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent onCloseAutoFocus={keepFocusAway}>
              <SelectItem value={NONE}>{p.locationNone}</SelectItem>
              {locationOptions.map((option) => (
                <SelectItem key={option.id} value={option.id}>
                  {option.path}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <Field label={p.ownership} htmlFor={ownershipId}>
          <Select
            value={presets.ownership ?? NONE}
            onValueChange={(value) =>
              setPresets({ ownership: value === NONE ? null : (value as Ownership) })
            }
          >
            <SelectTrigger id={ownershipId} className="w-full" aria-label={p.ownership}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent onCloseAutoFocus={keepFocusAway}>
              <SelectItem value={NONE}>{p.ownershipNone}</SelectItem>
              {OWNERSHIPS.map((ownership) => (
                <SelectItem key={ownership} value={ownership}>
                  {ja.labels.ownership[ownership]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <Field label={p.tags} htmlFor={tagsId}>
          <Input
            id={tagsId}
            list={tagListId}
            value={tagText}
            placeholder={p.tagsPlaceholder}
            onKeyDown={blurOnEnter}
            onChange={(event) => {
              setTagText(event.target.value);
              setPresets({ tags: parseTagInput(event.target.value) });
            }}
          />
          <datalist id={tagListId}>
            {(tags ?? []).map((tag) => (
              <option key={tag.name} value={tag.name} />
            ))}
          </datalist>
        </Field>

        <Field label={p.purchasedOn} htmlFor={purchasedOnId}>
          <Input
            id={purchasedOnId}
            type="date"
            value={presets.purchasedOn}
            onChange={(event) => setPresets({ purchasedOn: event.target.value })}
          />
        </Field>

        <Field label={p.purchaseStore} htmlFor={purchaseStoreId}>
          <Input
            id={purchaseStoreId}
            list={storeListId}
            value={presets.purchaseStore}
            onKeyDown={blurOnEnter}
            onChange={(event) => setPresets({ purchaseStore: event.target.value })}
          />
          <datalist id={storeListId}>
            {(stores ?? []).map((store) => (
              <option key={store.name} value={store.name} />
            ))}
          </datalist>
        </Field>
      </div>
    </section>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <label htmlFor={htmlFor} className="text-xs font-medium text-muted-foreground">
        {label}
      </label>
      {children}
    </div>
  );
}
