// バックアップ（FR-CONF-06、FR-IO-05）と、書き出し（FR-IO-01／02）。
// 保存先（フォルダ選択）、日次・週次の世代数、今すぐバックアップ、一覧と復元（確認 → restore_backup → 全件の取り直し）、
// フォルダを開く。書き出しは CSV 全件と JSON 全データ。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArchiveRestoreIcon, FolderOpenIcon, HardDriveDownloadIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import type { BackupInfo } from "@/bindings/BackupInfo";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { dateStamp } from "@/features/library/actions";
import { reloadLibrary } from "@/features/library/sync";
import { formatBackupTime, formatBytes } from "@/features/settings/backupFormat";
import { clampInteger } from "@/features/settings/clamp";
import { Group, NumberRow, Row } from "@/features/settings/fields";
import { settingsKeys, useBackupSettings } from "@/features/settings/hooks";
import { ja } from "@/i18n/ja";
import { chooseDirectory, chooseSavePath } from "@/lib/dialog";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";

const clampDailyKeep = clampInteger(1, 365, 7);
const clampWeeklyKeep = clampInteger(1, 52, 4);

export function BackupSection() {
  const t = ja.settings.backup;
  if (!inTauri()) {
    return (
      <div className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">{t.description}</p>
        <p className="text-sm">{ja.settings.browserNotice}</p>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{t.description}</p>
      <LocationGroup />
      <BackupList />
      <ExportGroup />
    </div>
  );
}

function LocationGroup() {
  const { value, isLoading, isError, save, saving } = useBackupSettings();
  const t = ja.settings.backup;

  const openFolder = useMutation({
    mutationFn: ipc.openBackupDir,
    onError: (error) => toast.error(errorMessage(error)),
  });

  if (isLoading) return <p className="text-sm text-muted-foreground">{ja.settings.loading}</p>;
  if (isError || value === undefined) {
    return <p className="text-sm text-destructive">{ja.settings.loadFailed}</p>;
  }

  const chooseFolder = async () => {
    try {
      const directory = await chooseDirectory({
        title: t.location.dialogTitle,
        defaultPath: value.directory === "" ? undefined : value.directory,
      });
      if (directory !== null) save({ ...value, directory });
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  return (
    <Group title={t.location.heading}>
      <div className="flex flex-col gap-2 text-sm">
        <span>{t.location.label}</span>
        <p
          className="rounded-md bg-muted px-3 py-2 font-mono text-xs break-all"
          data-testid="backup-directory"
        >
          {value.directory}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" disabled={saving} onClick={() => void chooseFolder()}>
            {t.location.change}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={saving}
            onClick={() => save({ ...value, directory: "" })}
          >
            {t.location.reset}
          </Button>
          <Button variant="outline" size="sm" onClick={() => openFolder.mutate()}>
            <FolderOpenIcon />
            {t.location.open}
          </Button>
        </div>
      </div>
      <NumberRow
        label={t.dailyKeep}
        value={value.dailyKeep}
        clamp={clampDailyKeep}
        onCommit={(dailyKeep) => save({ ...value, dailyKeep })}
      />
      <NumberRow
        label={t.weeklyKeep}
        value={value.weeklyKeep}
        clamp={clampWeeklyKeep}
        onCommit={(weeklyKeep) => save({ ...value, weeklyKeep })}
      />
    </Group>
  );
}

function BackupList() {
  const client = useQueryClient();
  const t = ja.settings.backup;
  const [target, setTarget] = useState<BackupInfo | null>(null);
  const backups = useQuery({ queryKey: settingsKeys.backups, queryFn: ipc.listBackups });

  const backupNow = useMutation({
    mutationFn: ipc.manualBackup,
    onSuccess: async (info) => {
      toast.success(t.nowDone(info.fileName));
      await client.invalidateQueries({ queryKey: settingsKeys.backups });
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const restore = useMutation({
    mutationFn: (path: string) => ipc.restoreBackup(path),
    onSuccess: async (report) => {
      // 復元で行が丸ごと入れ替わる。library-changed では全行を取り直せないので、ここで全部取り直す
      await reloadLibrary(client);
      toast.success(t.restore.done, {
        description: t.restore.doneDescription(report.preRestorePath) || undefined,
      });
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const confirmRestore = () => {
    const info = target;
    setTarget(null);
    if (info !== null) restore.mutate(info.path);
  };

  const list = backups.data ?? [];

  return (
    <Group title={t.list.heading}>
      <div>
        <Button size="sm" disabled={backupNow.isPending} onClick={() => backupNow.mutate()}>
          <HardDriveDownloadIcon />
          {t.now}
        </Button>
      </div>

      {backups.isLoading ? (
        <p className="text-sm text-muted-foreground">{ja.settings.loading}</p>
      ) : backups.isError ? (
        <p className="text-sm text-destructive">{ja.settings.loadFailed}</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t.list.empty}</p>
      ) : (
        <ul className="divide-y rounded-lg border" data-testid="backup-list">
          {list.map((info) => (
            <li key={info.path} className="flex items-center gap-3 px-3 py-2 text-sm">
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">{info.fileName}</p>
                <p className="text-xs text-muted-foreground">
                  {t.list.kinds[info.kind]} ／{" "}
                  {formatBackupTime(info.createdAt) ?? t.list.unknownDate} ／{" "}
                  {formatBytes(info.sizeBytes)}
                </p>
              </div>
              <Button
                variant="outline"
                size="sm"
                disabled={restore.isPending}
                aria-label={t.list.restoreOf(info.fileName)}
                onClick={() => setTarget(info)}
              >
                <ArchiveRestoreIcon />
                {t.list.restore}
              </Button>
            </li>
          ))}
        </ul>
      )}

      <Dialog open={target !== null} onOpenChange={(open) => !open && setTarget(null)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t.restore.title}</DialogTitle>
            <DialogDescription>
              {target === null ? "" : t.restore.description(target.fileName)}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setTarget(null)}>
              {t.restore.cancel}
            </Button>
            <Button variant="destructive" onClick={confirmRestore}>
              {t.restore.confirm}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Group>
  );
}

function ExportGroup() {
  const t = ja.settings.backup.export;

  const exportAll = useMutation({
    mutationFn: async (kind: "csv" | "json") => {
      const path = await chooseSavePath({
        title: ja.dialog.export.title,
        defaultPath: `${ja.dialog.export.defaultName}-${dateStamp()}.${kind}`,
        filters: [
          kind === "csv"
            ? { name: ja.dialog.export.csvFilter, extensions: ["csv"] }
            : { name: ja.dialog.export.jsonFilter, extensions: ["json"] },
        ],
      });
      if (path === null) return null;
      if (kind === "csv") return { count: await ipc.exportCsv(path) };
      await ipc.exportJson(path);
      return { count: null };
    },
    onSuccess: (done) => {
      if (done === null) return;
      toast.success(
        done.count === null ? ja.settings.toast.exported : ja.toast.exported(done.count),
      );
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  return (
    <Group title={t.heading} description={t.description}>
      <Row label={t.csv} hint={t.csvHint}>
        <Button
          variant="outline"
          size="sm"
          aria-label={t.csv}
          disabled={exportAll.isPending}
          onClick={() => exportAll.mutate("csv")}
        >
          {t.button}
        </Button>
      </Row>
      <Row label={t.json} hint={t.jsonHint}>
        <Button
          variant="outline"
          size="sm"
          aria-label={t.json}
          disabled={exportAll.isPending}
          onClick={() => exportAll.mutate("json")}
        >
          {t.button}
        </Button>
      </Row>
    </Group>
  );
}
