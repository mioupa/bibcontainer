// 設定の「同期」の節: 未設定のとき（FR-SYNC-01／09／10、0013 §7）。
// フォルダを選ぶ → 中身を調べる（inspect）→ 結果と、手元のデータの有無に応じた手順を出す。
//   空                         … 新しいライブラリを作る
//   ライブラリあり・手元が空     … 参加する
//   ライブラリあり・手元にデータ … 統合する／置き換える（置き換えは確認つき）
//   手元が別のライブラリに属す   … 警告（AT-S8）。同期を解除してから選び直す
//   書き込めない・更新が必要     … 理由と対処
import { FolderOpenIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Group } from "@/features/settings/fields";
import * as api from "@/features/sync/api";
import { ConfirmDialog } from "@/features/sync/ConfirmDialog";
import { DeviceList } from "@/features/sync/DeviceList";
import { pickSyncFolder } from "@/features/sync/folderPicker";
import { useSyncActions } from "@/features/sync/hooks";
import { Notice } from "@/features/sync/Notice";
import { planSetup, type SetupPlan } from "@/features/sync/setupPlan";
import type { FolderInspection, SetupMode } from "@/features/sync/types";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";

type Step =
  | { kind: "idle" }
  | { kind: "inspecting" }
  | { kind: "ready"; path: string; inspection: FolderInspection };

export function SyncSetup() {
  const t = ja.sync.setup;
  const actions = useSyncActions();
  const [step, setStep] = useState<Step>({ kind: "idle" });
  const [confirmReplace, setConfirmReplace] = useState(false);
  const [confirmLeave, setConfirmLeave] = useState(false);
  const [leaving, setLeaving] = useState(false);

  const inspectPath = async (path: string) => {
    setStep({ kind: "inspecting" });
    try {
      const inspection = await api.inspectSyncFolder(path);
      setStep({ kind: "ready", path, inspection });
    } catch (error) {
      toast.error(`${t.inspectFailed}: ${errorMessage(error)}`);
      setStep({ kind: "idle" });
    }
  };

  const chooseFolder = async () => {
    let path: string | null;
    try {
      path = await pickSyncFolder();
    } catch (error) {
      toast.error(errorMessage(error));
      return;
    }
    if (path !== null) await inspectPath(path);
  };

  const run = (path: string, mode: SetupMode) => actions.setup.mutate({ path, mode });

  // 手元が別のライブラリに属しているとき: 同期を解除して、同じフォルダをもう一度調べる
  const leaveAndReinspect = async (path: string) => {
    setConfirmLeave(false);
    setLeaving(true);
    try {
      await actions.leave.mutateAsync();
      await inspectPath(path);
    } catch {
      // 失敗の理由は、useSyncActions が通知する
    } finally {
      setLeaving(false);
    }
  };

  const busy = actions.setup.isPending || leaving;
  const ready = step.kind === "ready" ? step : null;

  return (
    <Group title={t.heading}>
      <p className="text-sm text-muted-foreground">{t.intro}</p>

      {step.kind === "idle" && (
        <div>
          <Button onClick={() => void chooseFolder()}>
            <FolderOpenIcon />
            {t.chooseFolder}
          </Button>
        </div>
      )}

      {step.kind === "inspecting" && (
        <p className="text-sm text-muted-foreground" role="status">
          {t.inspecting}
        </p>
      )}

      {ready !== null && (
        <div className="flex flex-col gap-4" data-testid="setup-result">
          <dl className="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-sm">
            <dt className="text-muted-foreground">{t.chosenFolder}</dt>
            <dd className="font-mono text-xs break-all" data-testid="setup-chosen-folder">
              {ready.path}
            </dd>
            <dt className="text-muted-foreground">{t.libraryFolder}</dt>
            <dd className="font-mono text-xs break-all" data-testid="setup-library-folder">
              {ready.inspection.libraryFolder}
            </dd>
          </dl>

          <PlanView
            plan={planSetup(ready.inspection)}
            busy={busy}
            onRun={(mode) => run(ready.path, mode)}
            onAskReplace={() => setConfirmReplace(true)}
            onAskLeave={() => setConfirmLeave(true)}
          />

          {ready.inspection.devices.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <h4 className="text-sm font-medium">{t.devicesHeading}</h4>
              <p className="text-xs text-muted-foreground">{t.devicesHint}</p>
              <DeviceList devices={ready.inspection.devices} testId="setup-devices" />
            </div>
          )}

          {busy && (
            <p className="text-sm text-muted-foreground" role="status">
              {t.working}
            </p>
          )}

          <div>
            <Button
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => setStep({ kind: "idle" })}
            >
              {t.rechoose}
            </Button>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={confirmReplace}
        title={t.replaceConfirm.title}
        description={t.replaceConfirm.description}
        confirmLabel={t.replaceConfirm.confirm}
        cancelLabel={t.replaceConfirm.cancel}
        onCancel={() => setConfirmReplace(false)}
        onConfirm={() => {
          setConfirmReplace(false);
          if (ready !== null) run(ready.path, "joinReplace");
        }}
      />
      <ConfirmDialog
        open={confirmLeave}
        title={ja.sync.leaveConfirm.title}
        description={ja.sync.leaveConfirm.description}
        confirmLabel={ja.sync.leaveConfirm.confirm}
        cancelLabel={ja.sync.leaveConfirm.cancel}
        onCancel={() => setConfirmLeave(false)}
        onConfirm={() => {
          if (ready !== null) void leaveAndReinspect(ready.path);
        }}
      />
    </Group>
  );
}

/** 調べた結果に応じた手順（上の分岐）。 */
function PlanView({
  plan,
  busy,
  onRun,
  onAskReplace,
  onAskLeave,
}: {
  plan: SetupPlan;
  busy: boolean;
  onRun: (mode: SetupMode) => void;
  onAskReplace: () => void;
  onAskLeave: () => void;
}) {
  const t = ja.sync.setup;
  switch (plan.kind) {
    case "create":
      return (
        <StepCard title={t.empty.title} description={t.empty.description} testId="plan-create">
          <Button disabled={busy} onClick={() => onRun("create")}>
            {t.empty.action}
          </Button>
        </StepCard>
      );
    case "join":
      return (
        <StepCard title={t.join.title} description={t.join.description} testId="plan-join">
          <Button disabled={busy} onClick={() => onRun("join")}>
            {t.join.action}
          </Button>
        </StepCard>
      );
    case "choose":
      return (
        <div className="flex flex-col gap-3" data-testid="plan-choose">
          <div>
            <p className="text-sm font-medium">{t.chooseMode.title}</p>
            <p className="text-xs text-muted-foreground">{t.chooseMode.description}</p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <StepCard
              title={t.chooseMode.merge.title}
              description={t.chooseMode.merge.description}
              testId="plan-merge"
            >
              <Button disabled={busy} onClick={() => onRun("joinMerge")}>
                {t.chooseMode.merge.action}
              </Button>
            </StepCard>
            <StepCard
              title={t.chooseMode.replace.title}
              description={t.chooseMode.replace.description}
              testId="plan-replace"
            >
              <Button variant="outline" disabled={busy} onClick={onAskReplace}>
                {t.chooseMode.replace.action}
              </Button>
            </StepCard>
          </div>
        </div>
      );
    case "otherLibrary":
      return (
        <Notice tone="danger" title={t.otherLibrary.title} testId="plan-other-library">
          <p>{t.otherLibrary.description}</p>
          <p>{t.otherLibrary.hint}</p>
          <div>
            <Button variant="outline" size="sm" disabled={busy} onClick={onAskLeave}>
              {t.otherLibrary.action}
            </Button>
          </div>
        </Notice>
      );
    case "notWritable":
      return (
        <Notice tone="warning" title={t.notWritable.title} testId="plan-not-writable">
          <p>{t.notWritable.description}</p>
        </Notice>
      );
    case "updateRequired":
      return (
        <Notice tone="warning" title={t.updateRequired.title} testId="plan-update-required">
          <p>{t.updateRequired.description}</p>
        </Notice>
      );
  }
}

function StepCard({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="flex flex-col justify-between gap-3 rounded-lg border bg-background p-3"
      data-testid={testId}
    >
      <div>
        <h4 className="text-sm font-semibold">{title}</h4>
        <p className="mt-1 text-xs text-muted-foreground">{description}</p>
      </div>
      <div>{children}</div>
    </div>
  );
}
