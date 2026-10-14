// 同期（FR-CONF-01）。同期はフェーズ 2。今は、この端末の ID と、対応予定の表示だけ。
import { useQuery } from "@tanstack/react-query";
import { Group } from "@/features/settings/fields";
import { settingsKeys } from "@/features/settings/hooks";
import { ja } from "@/i18n/ja";
import { ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";

export function SyncSection() {
  const info = useQuery({
    queryKey: settingsKeys.appInfo,
    queryFn: ipc.appInfo,
    enabled: inTauri(),
  });
  const s = ja.settings.sync;
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{s.description}</p>
      <Group title={s.deviceId}>
        <p className="font-mono text-sm break-all" data-testid="device-id">
          {info.data?.deviceId ?? s.unavailable}
        </p>
      </Group>
      <p className="rounded-lg border border-dashed px-4 py-3 text-sm">{s.phase2}</p>
    </div>
  );
}
