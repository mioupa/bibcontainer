// 一括変更のうち、値を選ぶ・入力するもの（保管場所の変更、タグの追加・削除。FR-BOOK-08）のダイアログ。
// 読書状況・所有状態・評価は、右クリックメニューから直接変える（BookTable）。
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { TagChipsInput } from "@/features/editor/TagChipsInput";
import { flattenLocations } from "@/features/library/trees";
import { tagsChange, type BulkChange } from "@/features/list/bulk";
import { ja } from "@/i18n/ja";
import { useModalGuard } from "@/lib/useModalGuard";
import type { Location } from "@/types/contracts";

export type BulkDialogState = {
  kind: "location" | "addTags" | "removeTags";
  /** 変更する本の ID。 */
  ids: string[];
} | null;

interface BulkEditDialogProps {
  state: BulkDialogState;
  locations: readonly Location[];
  /** タグの入力の候補（`list_tags`）。 */
  tagSuggestions: readonly string[];
  /** 選んだ本に付いているタグ（「タグを外す」で選べるもの）。 */
  currentTags: readonly string[];
  onCancel: () => void;
  onApply: (change: BulkChange, ids: string[]) => void;
}

export function BulkEditDialog({ state, onCancel, ...props }: BulkEditDialogProps) {
  useModalGuard(state !== null);
  return (
    <Dialog open={state !== null} onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="max-w-md">
        {state !== null && <BulkForm state={state} onCancel={onCancel} {...props} />}
      </DialogContent>
    </Dialog>
  );
}

const NONE = "__none__";

function BulkForm({
  state,
  locations,
  tagSuggestions,
  currentTags,
  onCancel,
  onApply,
}: BulkEditDialogProps & { state: NonNullable<BulkDialogState> }) {
  const { kind, ids } = state;
  const text = ja.list.bulkDialog[kind];
  const [location, setLocation] = useState<string | null>(null);
  const [added, setAdded] = useState<string[]>([]);
  const [removed, setRemoved] = useState<string[]>([]);
  const options = useMemo(() => flattenLocations(locations), [locations]);

  const change: BulkChange | null =
    kind === "location"
      ? location === null
        ? null
        : { field: "location_id", value: location === NONE ? null : location }
      : kind === "addTags"
        ? added.length > 0
          ? tagsChange(added)
          : null
        : removed.length > 0
          ? tagsChange([], removed)
          : null;

  return (
    <>
      <DialogHeader>
        <DialogTitle>{text.title}</DialogTitle>
        <DialogDescription>{text.description(ids.length)}</DialogDescription>
      </DialogHeader>

      {kind === "location" && (
        <div className="flex flex-col gap-2">
          <Label htmlFor="bulk-location">{ja.list.bulkDialog.location.label}</Label>
          <Select value={location ?? undefined} onValueChange={setLocation}>
            <SelectTrigger id="bulk-location" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="max-h-72">
              <SelectItem value={NONE}>{ja.list.bulkDialog.location.none}</SelectItem>
              {options.map((option) => (
                <SelectItem key={option.id} value={option.id}>
                  <span style={{ paddingLeft: `${option.depth * 0.75}rem` }}>{option.name}</span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {kind === "addTags" && (
        <div className="flex flex-col gap-2">
          <Label htmlFor="bulk-add-tags">{ja.list.bulkDialog.addTags.label}</Label>
          <TagChipsInput
            id="bulk-add-tags"
            value={added}
            onChange={setAdded}
            suggestions={tagSuggestions}
          />
        </div>
      )}

      {kind === "removeTags" &&
        (currentTags.length === 0 ? (
          <p className="text-sm text-muted-foreground">{ja.list.bulkDialog.removeTags.empty}</p>
        ) : (
          <fieldset className="flex max-h-64 flex-col gap-1.5 overflow-y-auto">
            <legend className="mb-1 text-sm font-medium">
              {ja.list.bulkDialog.removeTags.label}
            </legend>
            {currentTags.map((tag, index) => (
              <div key={tag} className="flex items-center gap-2">
                <Checkbox
                  id={`bulk-remove-${index}`}
                  checked={removed.includes(tag)}
                  onCheckedChange={(value) =>
                    setRemoved((current) =>
                      value === true ? [...current, tag] : current.filter((t) => t !== tag),
                    )
                  }
                />
                <Label htmlFor={`bulk-remove-${index}`}>{tag}</Label>
              </div>
            ))}
          </fieldset>
        ))}

      <DialogFooter>
        <Button variant="outline" onClick={onCancel}>
          {ja.list.bulkDialog.cancel}
        </Button>
        <Button disabled={change === null} onClick={() => change && onApply(change, ids)}>
          {text.apply}
        </Button>
      </DialogFooter>
    </>
  );
}
