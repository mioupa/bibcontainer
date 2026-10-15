import { useQuery } from "@tanstack/react-query";
import { ErrorScreen } from "@/ErrorScreen";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { EditorHost } from "@/features/editor/EditorHost";
import { MainScreen } from "@/features/layout/MainScreen";
import { useCoverArrivalSync } from "@/features/library/coverArrivals";
import { useLibraryChangeSync } from "@/features/library/sync";
import { useSoundUnlock } from "@/features/scan/useSoundUnlock";
import { ScanScreen } from "@/features/scanScreen/ScanScreen";
import { SettingsScreen } from "@/features/settings/SettingsScreen";
import { useGlobalShortcuts } from "@/features/shortcuts/useGlobalShortcuts";
import { useSyncStatusSubscription } from "@/features/sync/hooks";
import { ja } from "@/i18n/ja";
import { IpcError, ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";
import { useThemeSync } from "@/lib/useTheme";
import { useScreenStore } from "@/stores/screenStore";

/** 画面の切り替え。表示する画面は useScreenStore の screen で決まる（画面を切り替えるのは、ストアの show* だけ）。 */
function Screens() {
  const screen = useScreenStore((s) => s.screen);
  switch (screen) {
    case "main":
      return <MainScreen />;
    case "scan":
      return <ScanScreen />;
    case "settings":
      return <SettingsScreen />;
  }
}

/**
 * DB が使えるときのアプリ本体。画面（メイン・スキャン登録・設定）と、画面の外側に 1 つだけ置くもの
 * （登録・編集フォーム）。トーストは、エラー画面でも出せるよう、App が持つ。
 * 画面全体のショートカット（⌘N・⇧⌘N・⌘,）も、ここで受ける（どの画面でも効く）。
 * `library-changed` の購読は、どの画面を出していても続ける（スキャン登録や設定の画面で本を変えたあと、
 * メイン画面に戻ったときに一覧が古くならないように）。`sync-status`（同期の状態）と `covers-arrived`（同期で届いた書影）の購読も同じ。
 */
function AppShell() {
  useLibraryChangeSync();
  useCoverArrivalSync();
  // 同期の状態（上部バーの表示、設定の同期の節）。`sync-status` を、どの画面でも受け続ける
  useSyncStatusSubscription();
  // ⌘N（手動登録）・⇧⌘N（スキャン登録）・⌘,（設定）は、どの画面でも効く（9.5）
  useGlobalShortcuts();
  return (
    <>
      <Screens />
      <EditorHost />
    </>
  );
}

/**
 * アプリの入口。Tauri の中では、まず DB の状態を確かめ、開けなければエラー画面を出す。
 * Tauri の外（`pnpm dev` をブラウザで開いたとき）は IPC がないので、確認を飛ばしてモックのデータで動かす。
 */
function Gate() {
  const tauri = inTauri();
  const appInfo = useQuery({ queryKey: ["app_info"], queryFn: ipc.appInfo, enabled: tauri });

  if (!tauri) return <AppShell />;

  if (appInfo.isError) {
    const error = appInfo.error;
    return error instanceof IpcError ? (
      <ErrorScreen kind={error.kind} message={error.message} />
    ) : (
      <ErrorScreen kind="internal" message={error.message} />
    );
  }
  if (!appInfo.data) {
    return <p className="p-8 text-muted-foreground">{ja.app.loading}</p>;
  }
  return <AppShell />;
}

export function App() {
  useThemeSync();
  // 通知音の AudioContext を、最初のキー入力・クリックで作って再開しておく（最初の 1 回から鳴らすため）
  useSoundUnlock();
  return (
    <TooltipProvider delayDuration={400}>
      <Gate />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
