// このアプリについて（FR-CONF-07、9.4、10.6）。バージョン、端末 ID、各取得元のクレジット表記、ログフォルダを開くボタン。
import { useMutation, useQuery } from "@tanstack/react-query";
import { FolderOpenIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { libraryApi } from "@/features/library/api";
import { Group } from "@/features/settings/fields";
import { settingsKeys } from "@/features/settings/hooks";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { splitUrls } from "@/lib/linkify";
import { inTauri } from "@/lib/runtime";

/** クレジット表記。文の中の https の URL は、既定のブラウザで開くリンクにする（開けるのは許可したサイトだけ。Rust 側が確かめる）。 */
function Credit({ text }: { text: string }) {
  return (
    <>
      {splitUrls(text).map((part, index) =>
        part.kind === "text" ? (
          <span key={index}>{part.text}</span>
        ) : (
          <button
            key={index}
            type="button"
            className="text-primary underline underline-offset-2 hover:opacity-80"
            onClick={() => {
              libraryApi.openLink(part.url).catch((error: unknown) => {
                toast.error(errorMessage(error));
              });
            }}
          >
            {part.url}
          </button>
        ),
      )}
    </>
  );
}

export function AboutSection() {
  const a = ja.settings.about;
  const info = useQuery({
    queryKey: settingsKeys.appInfo,
    queryFn: ipc.appInfo,
    enabled: inTauri(),
  });
  const providers = useQuery({
    queryKey: settingsKeys.providers,
    queryFn: ipc.listProviders,
    enabled: inTauri(),
  });
  const openLogs = useMutation({
    mutationFn: ipc.openLogsDir,
    onError: (error) => toast.error(errorMessage(error)),
  });

  // 取得元の一覧を読めたら、その表記を出す。読めない（ブラウザなど）ときは、同じ内容の固定の表記を出す
  const credits =
    providers.data !== undefined
      ? providers.data.map((provider) => provider.credit)
      : [...a.fallbackCredits];

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{a.description}</p>

      <Group title={ja.app.title}>
        <dl className="grid grid-cols-[8rem_1fr] gap-x-3 gap-y-1.5 text-sm">
          <dt className="text-muted-foreground">{a.version}</dt>
          <dd data-testid="about-version">{info.data?.version ?? "—"}</dd>
          <dt className="text-muted-foreground">{a.deviceId}</dt>
          <dd className="font-mono break-all">{info.data?.deviceId ?? "—"}</dd>
          <dt className="text-muted-foreground">{a.schemaVersion}</dt>
          <dd>{info.data?.schemaVersion ?? "—"}</dd>
          <dt className="text-muted-foreground">{a.dbPath}</dt>
          <dd className="font-mono text-xs break-all">{info.data?.dbPath ?? "—"}</dd>
        </dl>
      </Group>

      <Group title={a.creditsHeading} description={a.creditsIntro}>
        <ul className="flex list-disc flex-col gap-1 pl-5 text-sm" data-testid="credits">
          {credits.map((credit) => (
            <li key={credit}>
              <Credit text={credit} />
            </li>
          ))}
        </ul>
        <p className="text-sm">{a.ndlNote}</p>
      </Group>

      <Group title={a.openLogs} description={a.logsDescription}>
        <div>
          <Button
            variant="outline"
            size="sm"
            disabled={!inTauri() || openLogs.isPending}
            onClick={() => openLogs.mutate()}
          >
            <FolderOpenIcon />
            {a.openLogs}
          </Button>
        </div>
      </Group>
    </div>
  );
}
