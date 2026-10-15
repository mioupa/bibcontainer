// 設定済みの「同期」の節: 同期フォルダを選び直す（0016）。
// Windows のドライブ文字が変わった、Google Drive の場所を移した、などで同期フォルダが使えなくなったとき、
// 同期を解除せずに、同じライブラリのフォルダへ場所だけを付け替える。
//   フォルダを選ぶ → 中身を調べる（inspect）→ 同じライブラリなら「この場所に付け替える」（join）。
//   ライブラリがない・別のライブラリ・書き込めない・更新が必要なら、付け替えずに理由を出す。
import { FolderOpenIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DeviceList } from "@/features/sync/DeviceList";
import { Notice } from "@/features/sync/Notice";
import { planChangeFolder, type ChangeFolderPlan } from "@/features/sync/setupPlan";
import type { ChangeFolderControl } from "@/features/sync/useChangeFolder";
import { ja } from "@/i18n/ja";

export function ChangeFolderButton({
  control,
  variant = "outline",
  testId,
}: {
  control: ChangeFolderControl;
  variant?: "outline" | "default";
  testId?: string;
}) {
  return (
    <Button
      variant={variant}
      size="sm"
      disabled={control.busy}
      onClick={control.choose}
      data-testid={testId}
    >
      <FolderOpenIcon />
      {ja.sync.changeFolder.action}
    </Button>
  );
}

/** 選んだフォルダの判定と、付け替える（または付け替えない理由の）表示。選ぶまでは何も出さない。 */
export function ChangeFolderPanel({ control }: { control: ChangeFolderControl }) {
  const t = ja.sync.changeFolder;
  const { step } = control;
  if (step.kind === "idle") return null;
  if (step.kind === "inspecting") {
    return (
      <p className="text-sm text-muted-foreground" role="status" data-testid="change-folder">
        {t.inspecting}
      </p>
    );
  }
  const plan = planChangeFolder(step.inspection);
  return (
    <div
      className="flex flex-col gap-3 rounded-lg border bg-background p-3"
      data-testid="change-folder"
    >
      <h4 className="text-sm font-semibold">{t.heading}</h4>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-sm">
        <dt className="text-muted-foreground">{ja.sync.setup.chosenFolder}</dt>
        <dd className="font-mono text-xs break-all" data-testid="change-folder-chosen">
          {step.path}
        </dd>
        <dt className="text-muted-foreground">{ja.sync.setup.libraryFolder}</dt>
        <dd className="font-mono text-xs break-all" data-testid="change-folder-library">
          {step.inspection.libraryFolder}
        </dd>
      </dl>

      <PlanView plan={plan} />

      {plan.kind === "reconnect" && step.inspection.devices.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <h5 className="text-sm font-medium">{t.devicesHeading}</h5>
          <DeviceList devices={step.inspection.devices} testId="change-folder-devices" />
        </div>
      )}

      {control.applying && (
        <p className="text-sm text-muted-foreground" role="status">
          {t.working}
        </p>
      )}

      <div className="flex flex-wrap gap-2">
        {plan.kind === "reconnect" && (
          <Button size="sm" disabled={control.busy} onClick={control.apply}>
            {t.reconnect.action}
          </Button>
        )}
        <Button variant="ghost" size="sm" disabled={control.busy} onClick={control.cancel}>
          {t.cancel}
        </Button>
      </div>
    </div>
  );
}

function PlanView({ plan }: { plan: ChangeFolderPlan }) {
  const t = ja.sync.changeFolder;
  switch (plan.kind) {
    case "reconnect":
      return (
        <div data-testid="change-folder-reconnect">
          <p className="text-sm font-medium">{t.reconnect.title}</p>
          <p className="mt-1 text-xs text-muted-foreground">{t.reconnect.description}</p>
        </div>
      );
    case "empty":
      return (
        <Notice tone="warning" title={t.empty.title} testId="change-folder-empty">
          <p>{t.empty.description}</p>
        </Notice>
      );
    case "otherLibrary":
      return (
        <Notice tone="danger" title={t.otherLibrary.title} testId="change-folder-other-library">
          <p>{t.otherLibrary.description}</p>
          <p>{t.otherLibrary.hint}</p>
        </Notice>
      );
    case "notWritable":
      return (
        <Notice
          tone="warning"
          title={ja.sync.setup.notWritable.title}
          testId="change-folder-not-writable"
        >
          <p>{ja.sync.setup.notWritable.description}</p>
        </Notice>
      );
    case "updateRequired":
      return (
        <Notice
          tone="warning"
          title={ja.sync.setup.updateRequired.title}
          testId="change-folder-update-required"
        >
          <p>{ja.sync.setup.updateRequired.description}</p>
        </Notice>
      );
    case "notJoined":
      return (
        <Notice tone="warning" title={t.notJoined.title} testId="change-folder-not-joined">
          <p>{t.notJoined.description}</p>
        </Notice>
      );
  }
}
