// 書影（FR-COVER-05）: 今の書影（なければ仮の表紙）と、差し替えの操作。
//   ファイルを選ぶ／ドラッグ＆ドロップ／クリップボードから貼り付け／画像 URL／取得元の候補から選ぶ。
// ドロップ・貼り付けの受け口は、タブを替えても効くようフォーム全体にある（BookEditorForm）。
import { ClipboardPasteIcon, FolderOpenIcon, ImageIcon, Undo2Icon } from "lucide-react";
import { useState } from "react";
import { useFormContext, useWatch } from "react-hook-form";
import { BookCover } from "@/components/BookCover";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { coverCandidates } from "@/features/editor/candidates";
import { useEditorContext } from "@/features/editor/editorContext";
import { FieldRow } from "@/features/editor/FieldRow";
import type { BookFormValues } from "@/features/editor/formValues";
import { sourceLabel } from "@/features/editor/sources";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";

export function CoverField({ className }: { className?: string }) {
  const ctx = useEditorContext();
  const { control, setValue } = useFormContext<BookFormValues>();
  const coverHash = useWatch({ control, name: "coverHash" });
  const title = useWatch({ control, name: "title" });
  const authors = useWatch({ control, name: "authors" });
  const [url, setUrl] = useState("");
  const { importer, dragging } = ctx.cover;
  const candidates = coverCandidates(ctx.merged);
  const authorText = authors
    .map((author) => author.name.trim())
    .filter(Boolean)
    .join(ja.format.listSeparator);

  const importUrl = async () => {
    if (url.trim() === "") return;
    if (await importer.fromUrl(url)) setUrl("");
  };

  return (
    <FieldRow name="coverHash" label={ja.edit.fields.cover} className={className}>
      <div className="flex flex-col gap-3">
        <div
          data-testid="cover-drop-zone"
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => {
            event.preventDefault();
            void importer.fromFiles(event.dataTransfer.files);
          }}
          className={cn(
            "relative flex justify-center rounded-lg border-2 border-dashed border-transparent p-1 transition-colors",
            dragging && "border-primary bg-primary/10",
          )}
        >
          <BookCover
            id={ctx.coverId}
            title={title.trim() === "" ? ja.edit.cover.untitled : title}
            authors={authorText}
            coverHash={coverHash}
            variant="lg"
          />
          {dragging && (
            <p className="absolute inset-x-0 bottom-3 text-center text-xs font-medium text-primary">
              {ja.edit.cover.dropHere}
            </p>
          )}
        </div>

        <div className="flex flex-wrap gap-1.5">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={importer.busy}
            onClick={() => void importer.chooseFile()}
          >
            <FolderOpenIcon />
            {ja.edit.cover.chooseFile}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={importer.busy}
            onClick={() => void importer.fromClipboard()}
          >
            <ClipboardPasteIcon />
            {ja.edit.cover.paste}
          </Button>
          {candidates.length > 0 && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button type="button" variant="outline" size="sm" disabled={importer.busy}>
                  <ImageIcon />
                  {ja.edit.cover.fromProviders}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="max-w-sm">
                <DropdownMenuLabel>{ja.edit.cover.providersTitle}</DropdownMenuLabel>
                {candidates.map((candidate) => (
                  <DropdownMenuItem
                    key={candidate.url}
                    onSelect={() => void importer.fromUrl(candidate.url)}
                  >
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {sourceLabel(candidate.source)}
                    </span>
                    <span className="truncate">{candidate.url}</span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          )}
          {coverHash !== ctx.initial.coverHash && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() =>
                setValue("coverHash", ctx.initial.coverHash, {
                  shouldDirty: true,
                  shouldValidate: true,
                })
              }
            >
              <Undo2Icon />
              {ja.edit.cover.revert}
            </Button>
          )}
        </div>

        <div className="flex items-center gap-1.5">
          <Input
            aria-label={ja.edit.cover.urlLabel}
            placeholder={ja.edit.cover.urlPlaceholder}
            value={url}
            autoComplete="off"
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                event.preventDefault();
                void importUrl();
              }
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={importer.busy || url.trim() === ""}
            onClick={() => void importUrl()}
          >
            {ja.edit.cover.importUrl}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">{ja.edit.cover.hint}</p>
      </div>
    </FieldRow>
  );
}
