// スキャン（FR-CONF-03）。末尾キー、先頭・末尾の余分な文字数、ペアリングのタイムアウト、キー間隔の閾値、
// 音の種類ごとの有効・音量、ミュート、試聴。設定は端末ごとの設定ストア（Tauri の中では Rust の local_settings）に保存する。
import { Volume2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  SOUND_KINDS,
  clampIdleCommitMs,
  clampKeyIntervalMs,
  clampStripChars,
  clampTimeoutSec,
  type ScanTerminator,
} from "@/features/scan/config";
import { playSound } from "@/features/scan/playSound";
import { Group, NumberRow, Row } from "@/features/settings/fields";
import { ja } from "@/i18n/ja";
import { useSettingsStore } from "@/stores/settingsStore";

export function ScanSection() {
  const scan = useSettingsStore((s) => s.scan);
  const sound = useSettingsStore((s) => s.sound);
  const setScan = useSettingsStore((s) => s.setScan);
  const setSound = useSettingsStore((s) => s.setSound);
  const t = ja.settings.scan;
  const sd = ja.settings.sound;

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{t.description}</p>

      <Group title={ja.settings.sections.scan}>
        <Row label={t.terminator.label}>
          <Select
            value={scan.terminator}
            onValueChange={(value) => setScan({ terminator: value as ScanTerminator })}
          >
            <SelectTrigger className="w-56" aria-label={t.terminator.label}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="enter">{t.terminator.enter}</SelectItem>
              <SelectItem value="tab">{t.terminator.tab}</SelectItem>
              <SelectItem value="none">{t.terminator.none}</SelectItem>
            </SelectContent>
          </Select>
        </Row>
        <NumberRow
          label={t.timeoutSec}
          value={scan.timeoutSec}
          clamp={clampTimeoutSec}
          onCommit={(timeoutSec) => setScan({ timeoutSec })}
        />
        <NumberRow
          label={t.keyIntervalMs}
          value={scan.keyIntervalMs}
          clamp={clampKeyIntervalMs}
          onCommit={(keyIntervalMs) => setScan({ keyIntervalMs })}
        />
        <NumberRow
          label={t.idleCommitMs}
          value={scan.idleCommitMs}
          clamp={clampIdleCommitMs}
          onCommit={(idleCommitMs) => setScan({ idleCommitMs })}
        />
        <NumberRow
          label={t.stripLeading}
          value={scan.stripLeading}
          clamp={clampStripChars}
          onCommit={(stripLeading) => setScan({ stripLeading })}
        />
        <NumberRow
          label={t.stripTrailing}
          value={scan.stripTrailing}
          clamp={clampStripChars}
          onCommit={(stripTrailing) => setScan({ stripTrailing })}
        />
      </Group>

      <Group title={t.soundsHeading}>
        <Row label={sd.muted}>
          <Checkbox
            checked={sound.muted}
            onCheckedChange={(checked) => setSound({ muted: checked === true })}
            aria-label={sd.muted}
          />
        </Row>
        {SOUND_KINDS.map((kind) => {
          const name = sd.kinds[kind];
          const setting = sound.kinds[kind];
          return (
            <div
              key={kind}
              className="flex items-center gap-3 text-sm"
              data-testid={`sound-${kind}`}
            >
              <Checkbox
                checked={setting.enabled}
                onCheckedChange={(checked) =>
                  setSound({ kinds: { [kind]: { enabled: checked === true } } })
                }
                aria-label={sd.enabled(name)}
              />
              <span className="min-w-0 flex-1">{name}</span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={setting.volume}
                aria-label={sd.volumeOf(name)}
                onChange={(event) =>
                  setSound({ kinds: { [kind]: { volume: Number(event.target.value) } } })
                }
                className="w-40 accent-primary"
              />
              <Button variant="outline" size="sm" onClick={() => playSound(kind)}>
                <Volume2Icon />
                {sd.play}
              </Button>
            </div>
          );
        })}
      </Group>
    </div>
  );
}
