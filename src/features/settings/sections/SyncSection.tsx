// 同期（FR-CONF-01、FR-SYNC-01／08〜11／13）。未設定のときはフォルダの選択と設定の手順、設定済みのときは状態・端末・
// 解除など。端末名の変更と Google Drive の注意は、どちらのときも出す。部品は src/features/sync/ にある（0015）。
import { usingMockSync } from "@/features/sync/api";
import { DeviceNameGroup } from "@/features/sync/DeviceNameGroup";
import { DriveNotes } from "@/features/sync/DriveNotes";
import { useSyncStatus } from "@/features/sync/hooks";
import { SyncConfigured } from "@/features/sync/SyncConfigured";
import { SyncSetup } from "@/features/sync/SyncSetup";
import { SyncDevPanel } from "@/dev/SyncDevPanel";
import { ja } from "@/i18n/ja";

export function SyncSection() {
  const t = ja.sync.section;
  const status = useSyncStatus();

  if (status.isLoading) return <p className="text-sm text-muted-foreground">{t.loading}</p>;
  if (status.data === undefined) return <p className="text-sm text-destructive">{t.loadFailed}</p>;

  const data = status.data;
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{t.description}</p>
      {data.state === "disabled" ? <SyncSetup /> : <SyncConfigured status={data} />}
      <DeviceNameGroup status={data} />
      <DriveNotes />
      {/* 開発中（pnpm dev・pnpm tauri dev）だけ、モックの状態を切り替えるパネルを出す */}
      {import.meta.env.DEV && import.meta.env.MODE !== "test" && usingMockSync() && (
        <SyncDevPanel />
      )}
    </div>
  );
}
