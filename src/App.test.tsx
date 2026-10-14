import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "@/App";
import type { AppInfo } from "@/bindings/AppInfo";
import { ja } from "@/i18n/ja";

// Tauri の invoke だけを差し替え、ipc.ts のエラー変換も含めて通す
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const info: AppInfo = {
  name: "bibcontainer",
  version: "9.9.9",
  deviceId: "d-0123456789ab",
  schemaVersion: 1,
  dbPath: "/data/library.sqlite3",
  hlcLast: null,
  pendingOps: 3,
};

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe("App", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("バックエンドから取得したアプリと DB の状態を一覧で表示する", async () => {
    invoke.mockResolvedValue(info);

    renderApp();

    expect(screen.getByText(ja.appInfo.loading)).toBeInTheDocument();
    expect(await screen.findByText("9.9.9")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: ja.app.title })).toBeInTheDocument();
    expect(screen.getByText("d-0123456789ab")).toBeInTheDocument();
    expect(screen.getByText("/data/library.sqlite3")).toBeInTheDocument();
    // HLC の前回値が null のときは「なし」と表示する
    const hlcLabel = screen.getByText(ja.appInfo.hlcLast);
    expect(within(hlcLabel.parentElement!).getByText(ja.appInfo.none)).toBeInTheDocument();
    const pendingLabel = screen.getByText(ja.appInfo.pendingOps);
    expect(within(pendingLabel.parentElement!).getByText("3")).toBeInTheDocument();
  });

  it("DB のスキーマ版が新しすぎるときは、エラー画面に原因と案内を出す", async () => {
    invoke.mockRejectedValue({ kind: "schema_too_new", message: "スキーマ版 v9 は新しすぎる" });

    renderApp();

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(ja.errorScreen.title)).toBeInTheDocument();
    expect(within(alert).getByText(ja.errorScreen.hints.schema_too_new)).toBeInTheDocument();
    expect(within(alert).getByText("スキーマ版 v9 は新しすぎる")).toBeInTheDocument();
  });

  it("IpcError 以外で失敗したときも、内部エラーとして原因を出す", async () => {
    invoke.mockRejectedValue("boom");

    renderApp();

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(ja.errorScreen.hints.internal)).toBeInTheDocument();
    expect(within(alert).getByText("boom")).toBeInTheDocument();
  });
});
