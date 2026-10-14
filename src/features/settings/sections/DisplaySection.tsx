// 表示（FR-CONF-04）。テーマ（OS に連動／ライト／ダーク）、フォント、文字サイズ、一覧の列（表示と並び順）。
// 一覧の列は、メイン画面の「列」メニューと同じ設定（tableStore）を使う。
import { useId } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Group, NumberRow, Row } from "@/features/settings/fields";
import { ColumnList } from "@/features/list/ColumnList";
import { ja } from "@/i18n/ja";
import type { ThemePreference } from "@/lib/theme";
import { clampFontSize, useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";

export function DisplaySection() {
  const theme = useSettingsStore((s) => s.theme);
  const fontFamily = useSettingsStore((s) => s.fontFamily);
  const fontSize = useSettingsStore((s) => s.fontSize);
  const setTheme = useSettingsStore((s) => s.setTheme);
  const setFontFamily = useSettingsStore((s) => s.setFontFamily);
  const setFontSize = useSettingsStore((s) => s.setFontSize);
  const resetColumns = useTableStore((s) => s.resetColumns);
  const fontId = useId();
  const d = ja.settings.display;

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{d.description}</p>

      <Group title={ja.settings.sections.display}>
        <Row label={ja.settings.theme.label}>
          <Select value={theme} onValueChange={(value) => setTheme(value as ThemePreference)}>
            <SelectTrigger className="w-48" aria-label={ja.settings.theme.label}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">{ja.settings.theme.system}</SelectItem>
              <SelectItem value="light">{ja.settings.theme.light}</SelectItem>
              <SelectItem value="dark">{ja.settings.theme.dark}</SelectItem>
            </SelectContent>
          </Select>
        </Row>

        <Row label={d.fontFamily.label} htmlFor={fontId} hint={d.fontFamily.description}>
          <div className="flex items-center gap-2">
            <Input
              id={fontId}
              // 保存された値が変わったら、入力欄を作り直して合わせる
              key={fontFamily ?? ""}
              defaultValue={fontFamily ?? ""}
              placeholder={d.fontFamily.placeholder}
              className="h-8 w-56"
              onBlur={(event) => {
                const name = event.currentTarget.value.trim();
                const next = name === "" ? null : name;
                if (next !== fontFamily) setFontFamily(next);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                  event.currentTarget.blur();
                }
              }}
            />
            <Button
              variant="outline"
              size="sm"
              disabled={fontFamily === null}
              onClick={() => setFontFamily(null)}
            >
              {d.fontFamily.reset}
            </Button>
          </div>
        </Row>

        <NumberRow
          label={d.fontSize}
          value={fontSize}
          clamp={clampFontSize}
          onCommit={setFontSize}
        />
      </Group>

      <Group title={d.columns.heading} description={d.columns.description}>
        <ColumnList className="max-w-sm" />
        <div>
          <Button variant="outline" size="sm" onClick={resetColumns}>
            {d.columns.reset}
          </Button>
        </div>
      </Group>
    </div>
  );
}
