// 設定の「同期」の節: 設定済みのとき（FR-SYNC-08／10／11／13、FR-CONF-01）。
// 警告、状態（フォルダ、最終書き出し・取り込み、未書き出し、届いていない書影、エラー）、今すぐ同期、同期の解除、
// 端末の一覧、書影の整理。端末名の変更は、未設定のときも使えるので DeviceGroup として別にしてある。
import { RefreshCwIcon, UnlinkIcon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Group } from "@/features/settings/fields";
import { ChangeFolderButton, ChangeFolderPanel } from "@/features/sync/ChangeFolder";
import { ConfirmDialog } from "@/features/sync/ConfirmDialog";
import { DeviceList } from "@/features/sync/DeviceList";
import { skewTitle } from "@/features/sync/display";
import { formatDateTime } from "@/features/sync/format";
import { useSyncActions } from "@/features/sync/hooks";
import { Notice } from "@/features/sync/Notice";
import { useChangeFolder, type ChangeFolderControl } from "@/features/sync/useChangeFolder";
import type { SyncState, SyncStatus } from "@/features/sync/types";
import { ja } from "@/i18n/ja";
import { formatInteger } from "@/lib/format";
import { cn } from "@/lib/utils";

const STATE_TONE: Record<SyncState, string> = {
  disabled: "text-muted-foreground",
  idle: "text-success",
  syncing: "text-info",
  error: "text-destructive",
  mismatch: "text-destructive",
  updateRequired: "text-destructive",
};

/** 設定済みの同期の画面。 */
export function SyncConfigured({ status }: { status: SyncStatus }) {
  // 同期フォルダの選び直し。ボタンは、エラーの警告の近くと状態の欄の 2 か所にあり、結果は状態の欄に出す
  const changeFolder = useChangeFolder();
  return (
    <>
      <SyncWarnings status={status} changeFolder={changeFolder} />
      <StatusGroup status={status} changeFolder={changeFolder} />
      <DevicesGroup status={status} />
      <CoversGroup />
    </>
  );
}

/** 警告（ライブラリの不一致、更新が必要、時計のずれ、エラー）。 */
function SyncWarnings({
  status,
  changeFolder,
}: {
  status: SyncStatus;
  changeFolder: ChangeFolderControl;
}) {
  const w = ja.sync.warnings;
  const skew = skewTitle(status);
  return (
    <>
      {status.state === "mismatch" && (
        <Notice tone="danger" title={w.mismatch.title} testId="warning-mismatch">
          <p>{w.mismatch.description}</p>
          <div>
            <ChangeFolderButton control={changeFolder} testId="change-folder-in-mismatch" />
          </div>
        </Notice>
      )}
      {status.state === "updateRequired" && (
        <Notice tone="danger" title={w.updateRequired.title} testId="warning-update-required">
          <p>{w.updateRequired.description}</p>
        </Notice>
      )}
      {status.state === "error" && (
        <Notice tone="danger" title={w.error.title} testId="warning-error">
          {status.error !== null && <p className="text-foreground">{status.error}</p>}
          <p>{w.error.description}</p>
          <p>{ja.sync.changeFolder.hint}</p>
          <div>
            <ChangeFolderButton control={changeFolder} testId="change-folder-in-error" />
          </div>
        </Notice>
      )}
      {skew !== null && (
        <Notice tone="warning" title={skew} testId="warning-skew">
          <p>{w.skew.description}</p>
        </Notice>
      )}
    </>
  );
}

function StatusGroup({
  status,
  changeFolder,
}: {
  status: SyncStatus;
  changeFolder: ChangeFolderControl;
}) {
  const t = ja.sync.status;
  const actions = useSyncActions();
  const [confirmLeave, setConfirmLeave] = useState(false);
  const syncing = status.state === "syncing" || actions.syncNow.isPending;
  // 不一致・更新が必要のあいだは、同期しても止まる（解除してから選び直す、アプリを更新する）
  const canSync = status.state !== "mismatch" && status.state !== "updateRequired";

  return (
    <Group title={t.heading}>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 text-sm">
        <dt className="text-muted-foreground">{t.folder}</dt>
        <dd className="font-mono text-xs break-all" data-testid="sync-folder">
          {status.folder}
        </dd>

        <dt className="text-muted-foreground">{t.state}</dt>
        <dd className={cn("font-medium", STATE_TONE[status.state])} data-testid="sync-state">
          {ja.sync.states[status.state]}
        </dd>

        <dt className="text-muted-foreground">{t.lastExport}</dt>
        <dd data-testid="sync-last-export">{formatDateTime(status.lastExportAt) ?? t.never}</dd>

        <dt className="text-muted-foreground">{t.lastImport}</dt>
        <dd data-testid="sync-last-import">{formatDateTime(status.lastImportAt) ?? t.never}</dd>

        <dt className="text-muted-foreground">{t.pending}</dt>
        <dd data-testid="sync-pending">{t.count(formatInteger(status.pendingOps))}</dd>

        <dt className="text-muted-foreground">{t.missingCovers}</dt>
        <dd data-testid="sync-missing-covers">{t.count(formatInteger(status.missingCovers))}</dd>

        {status.error !== null && (
          <>
            <dt className="text-muted-foreground">{t.errorLabel}</dt>
            <dd className="text-destructive" data-testid="sync-error">
              {status.error}
            </dd>
          </>
        )}

        {status.libraryId !== null && (
          <>
            <dt className="text-muted-foreground">{t.libraryId}</dt>
            <dd className="font-mono text-xs break-all">{status.libraryId}</dd>
          </>
        )}
      </dl>

      <div className="flex flex-wrap gap-2">
        <Button size="sm" disabled={syncing || !canSync} onClick={() => actions.syncNow.mutate()}>
          <RefreshCwIcon className={cn(syncing && "motion-safe:animate-spin")} />
          {syncing ? ja.sync.actions.syncing : ja.sync.actions.syncNow}
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={actions.leave.isPending}
          onClick={() => setConfirmLeave(true)}
        >
          <UnlinkIcon />
          {ja.sync.actions.leave}
        </Button>
        <ChangeFolderButton control={changeFolder} testId="change-folder-button" />
      </div>

      <ChangeFolderPanel control={changeFolder} />

      <ConfirmDialog
        open={confirmLeave}
        title={ja.sync.leaveConfirm.title}
        description={ja.sync.leaveConfirm.description}
        confirmLabel={ja.sync.leaveConfirm.confirm}
        cancelLabel={ja.sync.leaveConfirm.cancel}
        onCancel={() => setConfirmLeave(false)}
        onConfirm={() => {
          setConfirmLeave(false);
          actions.leave.mutate();
        }}
      />
    </Group>
  );
}

function DevicesGroup({ status }: { status: SyncStatus }) {
  const t = ja.sync.devices;
  return (
    <Group title={t.heading} description={t.description}>
      {status.devices.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t.empty}</p>
      ) : (
        <DeviceList devices={status.devices} testId="sync-devices" />
      )}
    </Group>
  );
}

function CoversGroup() {
  const t = ja.sync.covers;
  const actions = useSyncActions();
  const [confirm, setConfirm] = useState(false);
  return (
    <Group title={t.heading} description={t.description}>
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="outline"
          size="sm"
          disabled={actions.gcCovers.isPending}
          onClick={() => setConfirm(true)}
        >
          {t.button}
        </Button>
        {actions.gcCovers.data !== undefined && (
          <p className="text-sm text-muted-foreground" data-testid="gc-result">
            {t.result(formatInteger(actions.gcCovers.data))}
          </p>
        )}
      </div>
      <ConfirmDialog
        open={confirm}
        title={t.confirm.title}
        description={t.confirm.description}
        confirmLabel={t.confirm.confirm}
        cancelLabel={t.confirm.cancel}
        onCancel={() => setConfirm(false)}
        onConfirm={() => {
          setConfirm(false);
          actions.gcCovers.mutate();
        }}
      />
    </Group>
  );
}
