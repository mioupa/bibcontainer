// App の画面の切り替え。表示する画面は useScreenStore の screen で決まり、
// 登録・編集フォーム（EditorHost）は画面の外側に 1 つだけ置かれる。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type * as SyncModule from "@/features/library/sync";
import { App } from "@/App";
import { useScreenStore } from "@/stores/screenStore";

const isTauri = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

const sync = vi.hoisted(() => ({ mounted: 0, unmounted: 0 }));
vi.mock("@/features/library/sync", async (importOriginal) => {
  const actual = await importOriginal<typeof SyncModule>();
  const { useEffect } = await import("react");
  return {
    ...actual,
    useLibraryChangeSync: () => {
      useEffect(() => {
        sync.mounted += 1;
        return () => {
          sync.unmounted += 1;
        };
      }, []);
    },
  };
});
vi.mock("@/features/layout/MainScreen", () => ({
  MainScreen: () => <div data-testid="screen-main">メイン画面</div>,
}));
vi.mock("@/features/scanScreen/ScanScreen", () => ({
  ScanScreen: () => <div data-testid="screen-scan">スキャン登録画面</div>,
}));
vi.mock("@/features/settings/SettingsScreen", () => ({
  SettingsScreen: () => <div data-testid="screen-settings">設定画面</div>,
}));
vi.mock("@/features/editor/EditorHost", () => ({
  EditorHost: () => <div data-testid="editor-host" />,
}));

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("App の画面の切り替え", () => {
  beforeEach(() => {
    // Tauri の外（モックで動く）なら、DB の確認を飛ばして、すぐ画面を出す
    isTauri.mockReturnValue(false);
    sync.mounted = 0;
    sync.unmounted = 0;
    useScreenStore.setState({ screen: "main", settingsSection: "providers", handoff: null });
  });

  it("起動時はメイン画面", () => {
    renderApp();
    expect(screen.getByTestId("screen-main")).toBeInTheDocument();
    expect(screen.queryByTestId("screen-scan")).not.toBeInTheDocument();
    expect(screen.queryByTestId("screen-settings")).not.toBeInTheDocument();
  });

  it("screen に応じて、メイン・スキャン登録・設定のどれか 1 つだけを出す", () => {
    renderApp();

    act(() => useScreenStore.getState().showScan());
    expect(screen.getByTestId("screen-scan")).toBeInTheDocument();
    expect(screen.queryByTestId("screen-main")).not.toBeInTheDocument();

    act(() => useScreenStore.getState().showSettings("backup"));
    expect(screen.getByTestId("screen-settings")).toBeInTheDocument();
    expect(screen.queryByTestId("screen-scan")).not.toBeInTheDocument();

    act(() => useScreenStore.getState().showMain());
    expect(screen.getByTestId("screen-main")).toBeInTheDocument();
    expect(screen.queryByTestId("screen-settings")).not.toBeInTheDocument();
  });

  it("登録・編集フォームは、どの画面でも 1 つだけ（画面の外側に置く）", () => {
    renderApp();
    for (const show of [
      () => useScreenStore.getState().showMain(),
      () => useScreenStore.getState().showScan(),
      () => useScreenStore.getState().showSettings(),
    ]) {
      act(show);
      expect(screen.getAllByTestId("editor-host")).toHaveLength(1);
    }
  });

  it("一覧の更新通知（library-changed）の購読は、画面を切り替えても続く（作り直さない）", () => {
    renderApp();
    expect(sync.mounted).toBe(1);
    act(() => useScreenStore.getState().showScan());
    act(() => useScreenStore.getState().showSettings());
    act(() => useScreenStore.getState().showMain());
    expect(sync.mounted).toBe(1);
    expect(sync.unmounted).toBe(0);
  });
});
