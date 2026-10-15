// 同期フォルダにいる端末の一覧（名前、OS、バージョン、最終書き出し。自分には印）。
// 設定の手順で「選んだフォルダが正しいか」を確かめるときと、設定済みの状態の両方で使う。
import { LaptopIcon } from "lucide-react";
import { formatDateTime, osLabel } from "@/features/sync/format";
import type { SyncDevice } from "@/features/sync/types";
import { ja } from "@/i18n/ja";

export function DeviceList({
  devices,
  testId,
}: {
  devices: readonly SyncDevice[];
  testId: string;
}) {
  const t = ja.sync.devices;
  return (
    <ul className="divide-y rounded-lg border" data-testid={testId}>
      {devices.map((device) => (
        <li
          key={device.deviceId}
          className="flex items-center gap-3 px-3 py-2 text-sm"
          data-self={device.isSelf ? "true" : undefined}
        >
          <LaptopIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          <div className="min-w-0 flex-1">
            <p className="flex flex-wrap items-center gap-x-2 font-medium">
              <span className="truncate">{device.name}</span>
              {device.isSelf && (
                <span className="rounded-full bg-primary/15 px-2 py-0.5 text-xs font-normal text-primary">
                  {t.self}
                </span>
              )}
            </p>
            <p className="text-xs text-muted-foreground">
              {osLabel(device.os)} ／ {t.appVersion(device.appVersion)} ／ {t.lastExport}:{" "}
              {formatDateTime(device.lastExportAt) ?? t.never}
            </p>
          </div>
        </li>
      ))}
    </ul>
  );
}
