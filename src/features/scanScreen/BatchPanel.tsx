// 連続登録の一覧（FR-SCAN-14／15）。共通値の入力欄、溜まった本の一覧、「まとめて保存」、ISBN 一覧の取り込み。
import { FileUpIcon, SaveIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
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
import { useCCodeTable } from "@/features/library/hooks";
import { playSound } from "@/features/scan/playSound";
import {
  buildNewBookDrafts,
  canSave,
  loadingCount,
  presetChanges,
  rowStatus,
  type BatchRow,
} from "@/features/scanScreen/batch";
import { BatchRowItem } from "@/features/scanScreen/BatchRowItem";
import { startLookup } from "@/features/scanScreen/lookup";
import { PresetsForm } from "@/features/scanScreen/PresetsForm";
import { toMetaSecondTier, withSecondTier } from "@/features/scanScreen/secondTier";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { useEditorStore } from "@/stores/editorStore";

interface BatchPanelProps {
  onImport: () => void;
}

export function BatchPanel({ onImport }: BatchPanelProps) {
  const rows = useScanSessionStore((s) => s.rows);
  const lookups = useScanSessionStore((s) => s.lookups);
  const presets = useScanSessionStore((s) => s.presets);
  const cCodes = useCCodeTable();
  const [saving, setSaving] = useState(false);
  // 一覧を空にする前の確認（誤って押して、溜めた本を失わないように）
  const [clearing, setClearing] = useState(false);
  const b = ja.scan.batch;

  // 保存の対象で、まだ取得を始めていない行（重複から対象に戻した行など）の取得を始める
  useEffect(() => {
    for (const row of rows) {
      if (row.included && row.edited === null && lookups[row.isbn13] === undefined) {
        void startLookup(row.isbn13);
      }
    }
  }, [rows, lookups]);

  const includedCount = useMemo(() => rows.filter((row) => row.included).length, [rows]);
  const waiting = loadingCount(rows, lookups);
  const savable = canSave(rows, lookups) && !saving;

  const edit = (row: BatchRow) => {
    const outcome = lookups[row.isbn13];
    useEditorStore.getState().openDraft(
      {
        isbn13: row.isbn13,
        secondTier: toMetaSecondTier(row.secondTier),
        fetched: outcome?.status === "done" ? withSecondTier(outcome.result, row.secondTier) : null,
        // 共通値は、フォームにも先に入れておく（直した行を保存するとき、共通値で消されないように）
        presets: presetChanges(presets),
        draft: row.edited,
      },
      (edited) => useScanSessionStore.getState().setEdited(row.key, edited),
    );
  };

  const saveAll = async () => {
    const store = useScanSessionStore.getState();
    const drafts = buildNewBookDrafts(store.rows, store.lookups, store.presets);
    if (drafts.length === 0) return;
    const pendingCount = store.rows.filter(
      (row) => row.included && rowStatus(row, store.lookups[row.isbn13]) === "pending",
    ).length;
    setSaving(true);
    try {
      // 1 回の呼び出しで、全冊を 1 つのトランザクションで保存する
      const ids = await ipc.createBooksFromDrafts(drafts);
      const isbns = store.rows.map((row) => row.isbn13);
      store.clearRows();
      store.clearLookups(isbns);
      playSound("success");
      toast.success(b.saved(ids.length), {
        description: pendingCount > 0 ? b.pendingSaved(pendingCount) : undefined,
      });
    } catch (error) {
      playSound("error");
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-4" data-testid="batch-panel">
      <PresetsForm />

      <section aria-label={b.heading} className="rounded-xl border bg-card">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b px-4 py-3">
          <div>
            <h3 className="text-sm font-semibold">{b.heading}</h3>
            <p className="text-xs text-muted-foreground">{b.summary(rows.length, includedCount)}</p>
          </div>
          <Button variant="outline" size="sm" onClick={onImport}>
            <FileUpIcon />
            {ja.scan.import.button}
          </Button>
        </div>

        {rows.length === 0 ? (
          <p className="px-4 py-8 text-center text-sm text-muted-foreground">{b.empty}</p>
        ) : (
          <table className="w-full text-sm">
            <thead className="sr-only">
              <tr>
                <th>{b.columns.include}</th>
                <th>{b.columns.cover}</th>
                <th>{b.columns.book}</th>
                <th>{b.columns.status}</th>
                <th>{b.columns.actions}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <BatchRowItem
                  key={row.key}
                  row={row}
                  lookup={lookups[row.isbn13]}
                  cCodes={cCodes}
                  onInclude={(included) =>
                    useScanSessionStore.getState().setIncluded(row.key, included)
                  }
                  onEdit={() => edit(row)}
                  onRemove={() => useScanSessionStore.getState().removeRow(row.key)}
                />
              ))}
            </tbody>
          </table>
        )}

        <div className="flex flex-wrap items-center justify-end gap-3 border-t px-4 py-3">
          {waiting > 0 && (
            <p className="mr-auto text-xs text-muted-foreground">{b.waiting(waiting)}</p>
          )}
          {rows.length > 0 && (
            <Button variant="ghost" size="sm" onClick={() => setClearing(true)}>
              {b.clear}
            </Button>
          )}
          <Button onClick={() => void saveAll()} disabled={!savable}>
            <SaveIcon />
            {saving ? b.saving : b.saveAll(includedCount)}
          </Button>
        </div>
      </section>

      <Dialog open={clearing} onOpenChange={setClearing}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{b.clearConfirm.title}</DialogTitle>
            <DialogDescription>{b.clearConfirm.description(rows.length)}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClearing(false)}>
              {b.clearConfirm.cancel}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const store = useScanSessionStore.getState();
                store.clearLookups(store.rows.map((row) => row.isbn13));
                store.clearRows();
                setClearing(false);
              }}
            >
              {b.clearConfirm.confirm}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
