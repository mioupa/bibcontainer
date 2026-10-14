// サイドバーの管理ダイアログ: 保管場所の作成・名前の変更・移動・削除、タグの名前の変更・色の設定。
// 処理はコア（Rust）が検証する。子や本がある保管場所の削除は `conflict` で返るので、理由を表示する。
// 変更は Rust が出す `library-changed`（others）で、サイドバーの一覧に反映される。
import { useMemo, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { libraryApi } from "@/features/library/api";
import { buildLocationIndex, flattenLocations } from "@/features/library/trees";
import { validateTagName, type TagNameError } from "@/features/sidebar/tagName";
import { ja } from "@/i18n/ja";
import { COVER_ACCENTS } from "@/lib/coverColor";
import { errorMessage } from "@/lib/errors";
import { IpcError } from "@/lib/ipc";
import { useModalGuard } from "@/lib/useModalGuard";
import { cn } from "@/lib/utils";
import { useViewStore } from "@/stores/viewStore";
import { renamedTagPath } from "@/features/sidebar/tagName";
import type { Location } from "@/types/contracts";

export type ManageDialog =
  | { kind: "createLocation"; parentId: string | null }
  | { kind: "renameLocation"; id: string; name: string }
  | { kind: "moveLocation"; id: string; name: string }
  | { kind: "deleteLocation"; id: string; name: string }
  | { kind: "renameTag"; name: string; hasChildren: boolean }
  | { kind: "tagColor"; name: string; color: string | null };

interface ManageDialogsProps {
  dialog: ManageDialog | null;
  locations: readonly Location[];
  onClose: () => void;
}

export function ManageDialogs({ dialog, locations, onClose }: ManageDialogsProps) {
  useModalGuard(dialog !== null);
  return (
    <Dialog open={dialog !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md">
        {dialog !== null && <ManageForm dialog={dialog} locations={locations} onClose={onClose} />}
      </DialogContent>
    </Dialog>
  );
}

const ROOT = "__root__";

/** 実行中の状態とエラー表示をまとめた、保存の共通部分。成功したらダイアログを閉じる。 */
function useSubmit(onClose: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (task: () => Promise<void>, onError?: (error: unknown) => string | null) => {
    setBusy(true);
    setError(null);
    try {
      await task();
      onClose();
    } catch (failure) {
      setError(onError?.(failure) ?? errorMessage(failure));
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, setError, run };
}

function ManageForm({
  dialog,
  locations,
  onClose,
}: Required<ManageDialogsProps> & { dialog: ManageDialog }) {
  switch (dialog.kind) {
    case "createLocation":
    case "renameLocation":
      return <LocationNameForm dialog={dialog} locations={locations} onClose={onClose} />;
    case "moveLocation":
      return <LocationMoveForm dialog={dialog} locations={locations} onClose={onClose} />;
    case "deleteLocation":
      return <LocationDeleteForm dialog={dialog} onClose={onClose} />;
    case "renameTag":
      return <TagRenameForm dialog={dialog} onClose={onClose} />;
    case "tagColor":
      return <TagColorForm dialog={dialog} onClose={onClose} />;
  }
}

function FormFooter({
  busy,
  disabled,
  confirm,
  destructive,
  onClose,
}: {
  busy: boolean;
  disabled?: boolean;
  confirm: string;
  destructive?: boolean;
  onClose: () => void;
}) {
  return (
    <DialogFooter>
      <Button type="button" variant="outline" onClick={onClose} disabled={busy}>
        {ja.sidebar.manage.cancel}
      </Button>
      <Button
        type="submit"
        variant={destructive ? "destructive" : "default"}
        disabled={busy || disabled}
      >
        {confirm}
      </Button>
    </DialogFooter>
  );
}

function ErrorLine({ error }: { error: string | null }) {
  return error === null ? null : (
    <p role="alert" className="text-sm text-destructive">
      {error}
    </p>
  );
}

// ===== 保管場所 =====

function LocationNameForm({
  dialog,
  locations,
  onClose,
}: {
  dialog: Extract<ManageDialog, { kind: "createLocation" | "renameLocation" }>;
  locations: readonly Location[];
  onClose: () => void;
}) {
  const text = ja.sidebar.manage.locations;
  const creating = dialog.kind === "createLocation";
  const [name, setName] = useState(creating ? "" : dialog.name);
  const { busy, error, setError, run } = useSubmit(onClose);
  const parentName =
    creating && dialog.parentId !== null
      ? (locations.find((location) => location.id === dialog.parentId)?.name ?? "")
      : null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === "") {
      setError(text.emptyName);
      return;
    }
    void run(async () => {
      if (dialog.kind === "createLocation") {
        await libraryApi.createLocation(trimmed, dialog.parentId);
        toast.success(text.created(trimmed));
      } else {
        await libraryApi.renameLocation(dialog.id, trimmed);
        toast.success(text.renamed);
      }
    });
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <DialogHeader>
        <DialogTitle>{creating ? text.createTitle : text.renameTitle}</DialogTitle>
        <DialogDescription>
          {creating
            ? parentName === null
              ? text.createRoot
              : text.createChild(parentName)
            : dialog.kind === "renameLocation"
              ? text.renameDescription(dialog.name)
              : ""}
        </DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-2">
        <Label htmlFor="manage-location-name">{text.nameLabel}</Label>
        <Input
          id="manage-location-name"
          value={name}
          autoFocus
          autoComplete="off"
          placeholder={text.namePlaceholder}
          onChange={(event) => setName(event.target.value)}
        />
      </div>
      <ErrorLine error={error} />
      <FormFooter
        busy={busy}
        confirm={creating ? text.create : text.renameConfirm}
        onClose={onClose}
      />
    </form>
  );
}

function LocationMoveForm({
  dialog,
  locations,
  onClose,
}: {
  dialog: Extract<ManageDialog, { kind: "moveLocation" }>;
  locations: readonly Location[];
  onClose: () => void;
}) {
  const text = ja.sidebar.manage.locations;
  const [target, setTarget] = useState<string | null>(null);
  const { busy, error, run } = useSubmit(onClose);
  // 自分と、自分の子孫の下へは移せない
  const options = useMemo(
    () => flattenLocations(locations, buildLocationIndex(locations).descendantsOf(dialog.id)),
    [locations, dialog.id],
  );

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (target === null) return;
    void run(async () => {
      await libraryApi.moveLocation(dialog.id, target === ROOT ? null : target);
      toast.success(text.moved);
    });
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <DialogHeader>
        <DialogTitle>{text.moveTitle}</DialogTitle>
        <DialogDescription>{text.moveDescription(dialog.name)}</DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-2">
        <Label htmlFor="manage-location-target">{text.moveTarget}</Label>
        <Select value={target ?? undefined} onValueChange={setTarget}>
          <SelectTrigger id="manage-location-target" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent className="max-h-72">
            <SelectItem value={ROOT}>{text.moveRoot}</SelectItem>
            {options.map((option) => (
              <SelectItem key={option.id} value={option.id}>
                <span style={{ paddingLeft: `${option.depth * 0.75}rem` }}>{option.name}</span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <ErrorLine error={error} />
      <FormFooter
        busy={busy}
        disabled={target === null}
        confirm={text.moveConfirm}
        onClose={onClose}
      />
    </form>
  );
}

function LocationDeleteForm({
  dialog,
  onClose,
}: {
  dialog: Extract<ManageDialog, { kind: "deleteLocation" }>;
  onClose: () => void;
}) {
  const text = ja.sidebar.manage.locations;
  const { busy, error, run } = useSubmit(onClose);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    void run(
      async () => {
        await libraryApi.deleteLocation(dialog.id);
        // 絞り込み中の保管場所を消したときは、絞り込みを外す
        const view = useViewStore.getState();
        if (view.filter.locationId === dialog.id) view.toggleLocation(dialog.id);
        toast.success(text.deleted);
      },
      // 子や本がある保管場所は、コアが conflict で断る。何をすればよいかまで案内する
      (failure) =>
        failure instanceof IpcError && failure.kind === "conflict"
          ? `${text.deleteBlocked}（${failure.message}）`
          : null,
    );
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <DialogHeader>
        <DialogTitle>{text.deleteTitle}</DialogTitle>
        <DialogDescription>{text.deleteDescription(dialog.name)}</DialogDescription>
      </DialogHeader>
      <ErrorLine error={error} />
      <FormFooter busy={busy} destructive confirm={text.deleteConfirm} onClose={onClose} />
    </form>
  );
}

// ===== タグ =====

const TAG_ERRORS: Record<TagNameError, string> = {
  empty: ja.sidebar.manage.tags.emptyName,
  invalid: ja.sidebar.manage.tags.invalidName,
  same: ja.sidebar.manage.tags.sameName,
};

function TagRenameForm({
  dialog,
  onClose,
}: {
  dialog: Extract<ManageDialog, { kind: "renameTag" }>;
  onClose: () => void;
}) {
  const text = ja.sidebar.manage.tags;
  const [name, setName] = useState(dialog.name);
  const { busy, error, setError, run } = useSubmit(onClose);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const result = validateTagName(dialog.name, name);
    if (!result.ok) {
      setError(TAG_ERRORS[result.error]);
      return;
    }
    void run(async () => {
      const changed = await libraryApi.renameTag(dialog.name, result.name);
      // 絞り込み中のタグを改名したときは、新しい名前で絞り込み続ける
      const view = useViewStore.getState();
      const next = renamedTagPath(view.filter.tag, dialog.name, result.name);
      if (next !== view.filter.tag) {
        useViewStore.setState({ filter: { ...view.filter, tag: next } });
      }
      toast.success(text.renamed(changed.length));
    });
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <DialogHeader>
        <DialogTitle>{text.renameTitle}</DialogTitle>
        <DialogDescription>
          {text.renameDescription(dialog.name, dialog.hasChildren)}
        </DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-2">
        <Label htmlFor="manage-tag-name">{text.nameLabel}</Label>
        <Input
          id="manage-tag-name"
          value={name}
          autoFocus
          autoComplete="off"
          onChange={(event) => setName(event.target.value)}
        />
      </div>
      <ErrorLine error={error} />
      <FormFooter busy={busy} confirm={text.renameConfirm} onClose={onClose} />
    </form>
  );
}

function TagColorForm({
  dialog,
  onClose,
}: {
  dialog: Extract<ManageDialog, { kind: "tagColor" }>;
  onClose: () => void;
}) {
  const text = ja.sidebar.manage.tags;
  const { busy, error, run } = useSubmit(onClose);

  const choose = (color: string | null) => {
    void run(async () => {
      await libraryApi.setTagColor(dialog.name, color);
      toast.success(text.colorSet);
    });
  };

  return (
    <div className="flex flex-col gap-4">
      <DialogHeader>
        <DialogTitle>{text.colorTitle}</DialogTitle>
        <DialogDescription>{text.colorDescription(dialog.name)}</DialogDescription>
      </DialogHeader>
      <div className="flex flex-wrap gap-2" role="group" aria-label={text.colorTitle}>
        {COVER_ACCENTS.map((accent) => (
          <button
            key={accent}
            type="button"
            aria-label={accent}
            aria-pressed={dialog.color === accent}
            disabled={busy}
            onClick={() => choose(accent)}
            className={cn(
              "size-7 rounded-full ring-offset-2 ring-offset-popover transition",
              dialog.color === accent ? "ring-2 ring-foreground" : "hover:ring-2 hover:ring-border",
            )}
            style={{ backgroundColor: `var(--ctp-${accent})` }}
          />
        ))}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={() => choose(null)}
        >
          {text.colorNone}
        </Button>
      </div>
      <ErrorLine error={error} />
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose} disabled={busy}>
          {ja.sidebar.manage.cancel}
        </Button>
      </DialogFooter>
    </div>
  );
}
