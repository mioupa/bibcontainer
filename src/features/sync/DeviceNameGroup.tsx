// この端末の名前（FR-CONF-01）。同期フォルダの端末の一覧に出る名前で、既定はホスト名。未設定のときも変えられる。
import { useId, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Group } from "@/features/settings/fields";
import { useSyncActions } from "@/features/sync/hooks";
import type { SyncStatus } from "@/features/sync/types";
import { ja } from "@/i18n/ja";

/** 端末名の長さの上限（文字数）。Rust 側の検査と合わせる（0015 の未解決）。 */
export const DEVICE_NAME_MAX_LENGTH = 64;

export function DeviceNameGroup({ status }: { status: SyncStatus }) {
  // 外から名前が変わったら（変更の保存後、解除・参加で端末が替わったとき）、入力欄を作り直して合わせる
  return <DeviceNameForm key={`${status.deviceId}:${status.deviceName}`} status={status} />;
}

function DeviceNameForm({ status }: { status: SyncStatus }) {
  const t = ja.sync.deviceName;
  const id = useId();
  const actions = useSyncActions();
  const [name, setName] = useState(status.deviceName);
  const trimmed = name.trim();
  const canSave = trimmed !== "" && trimmed !== status.deviceName && !actions.rename.isPending;

  return (
    <Group title={t.heading}>
      <form
        className="flex flex-col gap-1.5"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSave) actions.rename.mutate(trimmed);
        }}
      >
        <label htmlFor={id} className="text-sm">
          {t.label}
        </label>
        <div className="flex gap-2">
          <Input
            id={id}
            value={name}
            maxLength={DEVICE_NAME_MAX_LENGTH}
            className="h-8 max-w-xs"
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              // 日本語入力の変換を確定する Enter では、保存しない
              if (
                event.key === "Enter" &&
                (event.nativeEvent.isComposing || event.keyCode === 229)
              ) {
                event.preventDefault();
              }
            }}
          />
          <Button type="submit" variant="outline" size="sm" disabled={!canSave}>
            {t.save}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">{t.hint}</p>
      </form>
      <p className="text-xs text-muted-foreground">
        {t.deviceId}: <span className="font-mono break-all">{status.deviceId}</span>
      </p>
    </Group>
  );
}
