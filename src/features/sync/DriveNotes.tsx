// Google Drive で同期するときの注意（8.3）。設定済みでも未設定でも、同期の節の下に出す。
import { Group } from "@/features/settings/fields";
import { ja } from "@/i18n/ja";

export function DriveNotes() {
  const t = ja.sync.drive;
  return (
    <Group title={t.heading}>
      <ul className="flex list-disc flex-col gap-1.5 pl-5 text-sm" data-testid="drive-notes">
        {t.items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
      <div className="flex flex-col gap-1.5 text-sm">
        <p className="font-medium">{t.examplesHeading}</p>
        <dl className="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1">
          {t.examples.map((example) => (
            <div key={example.os} className="contents">
              <dt className="text-muted-foreground">{example.os}</dt>
              <dd className="font-mono text-xs break-all">{example.path}</dd>
            </div>
          ))}
        </dl>
        <p className="text-xs text-muted-foreground">{t.examplesNote}</p>
      </div>
    </Group>
  );
}
