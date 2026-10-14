import { useQuery } from "@tanstack/react-query";
import { ErrorScreen } from "@/ErrorScreen";
import { ja } from "@/i18n/ja";
import { IpcError, ipc } from "@/lib/ipc";

export function App() {
  const appInfo = useQuery({ queryKey: ["app_info"], queryFn: ipc.appInfo });

  if (appInfo.isError) {
    const error = appInfo.error;
    return error instanceof IpcError ? (
      <ErrorScreen kind={error.kind} message={error.message} />
    ) : (
      <ErrorScreen kind="internal" message={error.message} />
    );
  }

  const rows =
    appInfo.data &&
    ([
      [ja.appInfo.name, appInfo.data.name],
      [ja.appInfo.version, appInfo.data.version],
      [ja.appInfo.deviceId, appInfo.data.deviceId],
      [ja.appInfo.schemaVersion, String(appInfo.data.schemaVersion)],
      [ja.appInfo.dbPath, appInfo.data.dbPath],
      [ja.appInfo.hlcLast, appInfo.data.hlcLast ?? ja.appInfo.none],
      [ja.appInfo.pendingOps, String(appInfo.data.pendingOps)],
    ] as const);

  return (
    <main className="mx-auto flex min-h-screen max-w-3xl flex-col justify-center gap-6 p-8 text-slate-800">
      <header>
        <h1 className="text-3xl font-semibold tracking-tight">{ja.app.title}</h1>
        <p className="text-slate-500">{ja.app.tagline}</p>
      </header>

      <section
        aria-labelledby="app-info-heading"
        className="rounded-lg border border-slate-200 p-4"
      >
        <h2 id="app-info-heading" className="mb-3 text-sm font-medium text-slate-500">
          {ja.appInfo.heading}
        </h2>
        {rows ? (
          <dl className="grid grid-cols-[12rem_1fr] gap-y-1">
            {rows.map(([label, value]) => (
              <div key={label} className="contents">
                <dt className="text-slate-500">{label}</dt>
                <dd className="font-mono text-sm break-all">{value}</dd>
              </div>
            ))}
          </dl>
        ) : (
          <p>{ja.appInfo.loading}</p>
        )}
      </section>
    </main>
  );
}
