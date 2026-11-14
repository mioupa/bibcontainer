// CSV から取り込むダイアログ（0028）。列の対応 → 確認 → 取り込み中 → 結果。
// 取り込むを押すまで DB には何も書かない。取り込み中は閉じられない。
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import type { ColumnMapping } from "@/bindings/ColumnMapping";
import type { CsvEncoding } from "@/bindings/CsvEncoding";
import type { CsvImportOptions } from "@/bindings/CsvImportOptions";
import type { CsvImportPlan } from "@/bindings/CsvImportPlan";
import type { CsvImportResult } from "@/bindings/CsvImportResult";
import type { CsvPreview } from "@/bindings/CsvPreview";
import type { ImportTarget } from "@/bindings/ImportTarget";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ja } from "@/i18n/ja";
import { chooseSavePath } from "@/lib/dialog";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { useModalGuard } from "@/lib/useModalGuard";
import { ConfirmStep } from "./ConfirmStep";
import { MappingStep } from "./MappingStep";
import { baseName, canProceed, fileName, setTarget } from "./model";

interface CsvImportDialogProps {
  /** 取り込む CSV のパス。null なら閉じている。 */
  path: string | null;
  onClose: () => void;
}

type Phase = "mapping" | "checking" | "confirm" | "running" | "result";

export function CsvImportDialog({ path, onClose }: CsvImportDialogProps) {
  useModalGuard(path !== null);
  return path === null ? null : <ImportFlow key={path} path={path} onClose={onClose} />;
}

function ImportFlow({ path, onClose }: { path: string; onClose: () => void }) {
  const t = ja.csvImport;
  const [encoding, setEncoding] = useState<CsvEncoding | null>(null);
  const [autoEncoding, setAutoEncoding] = useState<CsvEncoding | null>(null);
  const [hasHeader, setHasHeader] = useState(true);
  const [preview, setPreview] = useState<CsvPreview | null>(null);
  const [mapping, setMapping] = useState<ColumnMapping[]>([]);
  const [loading, setLoading] = useState(true);
  const [phase, setPhase] = useState<Phase>("mapping");
  const [plan, setPlan] = useState<CsvImportPlan | null>(null);
  const [result, setResult] = useState<CsvImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const requestId = useRef(0);
  const busy = phase === "running";

  // 状態の初期値は「読み込み中・エラーなし」なので、最初の読み込みでは state を同期的に触らない
  const fetchPreview = useCallback(
    (enc: CsvEncoding | null, header: boolean) => {
      const id = ++requestId.current;
      ipc
        .csvImportOpen(path, enc, header)
        .then((p) => {
          if (id !== requestId.current) return;
          setPreview(p);
          if (p.autoDetected) setAutoEncoding(p.encoding);
          setMapping(p.suggested);
          setLoading(false);
        })
        .catch((e: unknown) => {
          if (id !== requestId.current) return;
          setError(errorMessage(e));
          setLoading(false);
        });
    },
    [path],
  );

  const load = (enc: CsvEncoding | null, header: boolean) => {
    setLoading(true);
    setError(null);
    fetchPreview(enc, header);
  };

  useEffect(() => {
    fetchPreview(null, true);
    return () => {
      requestId.current += 1;
    };
  }, [fetchPreview]);

  const options: CsvImportOptions = { encoding, hasHeader, mapping };

  const check = async () => {
    setError(null);
    setPhase("checking");
    try {
      setPlan(await ipc.csvImportCheck(path, options));
      setPhase("confirm");
    } catch (e) {
      setError(errorMessage(e));
      setPhase("mapping");
    }
  };

  const run = async () => {
    setError(null);
    setPhase("running");
    try {
      setResult(await ipc.csvImportRun(path, options));
      setPhase("result");
    } catch (e) {
      setError(errorMessage(e));
      setPhase("confirm");
    }
  };

  const saveRejected = async () => {
    setError(null);
    try {
      const outPath = await chooseSavePath({
        title: t.result.saveDialogTitle,
        defaultPath: t.result.defaultRejectedName(baseName(path)),
        filters: [{ name: t.result.csvFilter, extensions: ["csv"] }],
      });
      if (outPath === null) return;
      setSaving(true);
      const count = await ipc.csvImportSaveRejected(path, options, outPath);
      toast.success(t.result.saved(count));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const changeEncoding = (next: CsvEncoding | null) => {
    setEncoding(next);
    load(next, hasHeader);
  };
  const changeHeader = (next: boolean) => {
    setHasHeader(next);
    load(encoding, next);
  };

  return (
    <Dialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogContent
        className="flex max-h-[90vh] max-w-3xl flex-col sm:max-w-3xl"
        showCloseButton={!busy}
        onEscapeKeyDown={(event) => busy && event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t.title}</DialogTitle>
          <DialogDescription className="sr-only">{t.mapping.description}</DialogDescription>
        </DialogHeader>

        {(phase === "mapping" || phase === "checking") && (
          <MappingStep
            fileName={fileName(path)}
            preview={preview}
            autoEncoding={autoEncoding}
            encoding={encoding}
            hasHeader={hasHeader}
            mapping={mapping}
            loading={loading || phase === "checking"}
            onEncoding={changeEncoding}
            onHasHeader={changeHeader}
            onTarget={(column: number, target: ImportTarget | null) =>
              setMapping((m) => setTarget(m, column, target))
            }
          />
        )}
        {phase === "confirm" && plan !== null && <ConfirmStep plan={plan} />}
        {phase === "running" && (
          <p className="text-sm" role="status">
            {t.running}
          </p>
        )}
        {phase === "result" && result !== null && <ResultView result={result} />}
        {phase === "checking" && (
          <p className="text-sm text-muted-foreground" role="status">
            {t.confirm.checking}
          </p>
        )}

        {error !== null && (
          <p className="text-sm text-destructive" role="alert">
            {error}
          </p>
        )}

        <DialogFooter>
          {phase === "mapping" || phase === "checking" ? (
            <>
              <Button variant="outline" onClick={onClose} disabled={phase === "checking"}>
                {t.close}
              </Button>
              <Button
                disabled={
                  loading || preview === null || !canProceed(mapping) || phase === "checking"
                }
                onClick={() => void check()}
              >
                {t.next}
              </Button>
            </>
          ) : phase === "confirm" ? (
            <>
              <Button variant="outline" onClick={() => setPhase("mapping")}>
                {t.back}
              </Button>
              <Button disabled={plan === null || plan.toImport === 0} onClick={() => void run()}>
                {t.confirm.run(plan?.toImport ?? 0)}
              </Button>
            </>
          ) : phase === "result" ? (
            <>
              {result !== null && result.errors > 0 && (
                <Button variant="outline" disabled={saving} onClick={() => void saveRejected()}>
                  {t.result.saveRejected}
                </Button>
              )}
              <Button onClick={onClose}>{t.close}</Button>
            </>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ResultView({ result }: { result: CsvImportResult }) {
  const t = ja.csvImport.result;
  return (
    <div className="flex flex-col gap-1 text-sm" data-testid="csv-result">
      <p className="font-medium">{t.imported(result.imported)}</p>
      {result.fetchQueued > 0 && (
        <p className="text-muted-foreground">{t.fetchNote(result.fetchQueued)}</p>
      )}
      {result.skipped > 0 && <p>{t.skipped(result.skipped)}</p>}
      {result.errors > 0 && <p>{t.errors(result.errors)}</p>}
      {result.createdLocations > 0 && <p>{t.createdLocations(result.createdLocations)}</p>}
      {result.createdSeries > 0 && <p>{t.createdSeries(result.createdSeries)}</p>}
    </div>
  );
}
