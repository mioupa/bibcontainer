// App の画面の切り替え。表示する画面は useScreenStore の screen で決まり、
// 登録・編集フォーム（EditorHost）は画面の外側に 1 つだけ置かれる。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type * as SyncModule from "@/features/library/sync";
import { App } from "@/App";
import { useModalStore } from "@/lib/useModalGuard";
import { useEditorStore } from "@/stores/editorStore";
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
    useEditorStore.getState().close();
    useModalStore.setState({ count: 0 });
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

// 9.5: ⌘N（手動登録）・⇧⌘N（スキャン登録）・⌘,（設定）は、どの画面でも効く。App が受ける。
describe("App の画面全体のショートカット（9.5）", () => {
  /** キーを押す。既定の動作を止められたか（App がそのキーを受けたか）を返す。 */
  const press = (init: KeyboardEventInit): boolean => {
    const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
    act(() => {
      document.body.dispatchEvent(event);
    });
    return event.defaultPrevented;
  };
  const showScreen = (screen: "main" | "scan" | "settings") => {
    act(() => {
      const store = useScreenStore.getState();
      if (screen === "main") store.showMain();
      else if (screen === "scan") store.showScan();
      else store.showSettings();
    });
  };

  beforeEach(() => {
    isTauri.mockReturnValue(false);
    useScreenStore.setState({ screen: "main", settingsSection: "providers", handoff: null });
    useEditorStore.getState().close();
    useModalStore.setState({ count: 0 });
  });

  it.each(["main", "scan", "settings"] as const)(
    "%s 画面でも、⌘N は手動登録のフォームを開く",
    (screenName) => {
      renderApp();
      showScreen(screenName);
      press({ code: "KeyN", metaKey: true });
      expect(useEditorStore.getState().request).toMatchObject({ mode: "new" });
      // 画面は替わらない
      expect(useScreenStore.getState().screen).toBe(screenName);
    },
  );

  it.each(["main", "scan", "settings"] as const)(
    "%s 画面でも、⇧⌘N はスキャン登録、⌘, は設定へ移る",
    (screenName) => {
      renderApp();
      showScreen(screenName);
      press({ code: "KeyN", metaKey: true, shiftKey: true });
      expect(useScreenStore.getState().screen).toBe("scan");
      expect(screen.getByTestId("screen-scan")).toBeInTheDocument();

      showScreen(screenName);
      press({ code: "Comma", metaKey: true });
      expect(useScreenStore.getState().screen).toBe("settings");
      expect(screen.getByTestId("screen-settings")).toBeInTheDocument();
    },
  );

  it("設定の節は、⌘, で開き直しても保たれる", () => {
    renderApp();
    act(() => useScreenStore.getState().showSettings("backup"));
    press({ code: "Comma", metaKey: true });
    expect(useScreenStore.getState().settingsSection).toBe("backup");
  });

  it("フォームやダイアログを開いている間は、どの画面でも効かない（useModalGuard の数で止める）", () => {
    renderApp();
    for (const screenName of ["main", "scan", "settings"] as const) {
      showScreen(screenName);
      act(() => useModalStore.setState({ count: 1 }));
      press({ code: "KeyN", metaKey: true });
      press({ code: "KeyN", metaKey: true, shiftKey: true });
      press({ code: "Comma", metaKey: true });
      expect(useEditorStore.getState().request).toBeNull();
      expect(useScreenStore.getState().screen).toBe(screenName);

      // 閉じれば、また効く
      act(() => useModalStore.setState({ count: 0 }));
      press({ code: "KeyN", metaKey: true });
      expect(useEditorStore.getState().request).not.toBeNull();
      useEditorStore.getState().close();
    }
  });

  it("画面ごとのショートカット（⌘F・⌘I）は App では受けない（キーを奪わない）", () => {
    renderApp();
    expect(press({ code: "KeyN", metaKey: true })).toBe(true);
    expect(press({ code: "KeyF", metaKey: true })).toBe(false);
    expect(press({ code: "KeyI", metaKey: true })).toBe(false);
  });
});
