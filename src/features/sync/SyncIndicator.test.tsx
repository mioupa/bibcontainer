// 上部バーの同期の表示（9.1）。状態ごとの見た目、押すと設定の同期の節を開く、sync-status での更新、ツールチップ。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { mockSync } from "@/dev/mockSync";
import { useSyncStatusSubscription } from "@/features/sync/hooks";
import { SyncIndicator } from "@/features/sync/SyncIndicator";
import { ja } from "@/i18n/ja";
import { useScreenStore } from "@/stores/screenStore";

// 今を 2026-10-15 13:00（ローカル時間）に固定する。時刻の表示は「今日なら時刻だけ」
const NOW = new Date(2026, 9, 15, 13, 0);
const at = (month: number, day: number, hour: number, minute: number) =>
  new Date(2026, month - 1, day, hour, minute).toISOString();

function Harness() {
  useSyncStatusSubscription();
  return <SyncIndicator />;
}

function renderIndicator() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <Harness />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

const indicator = () => screen.getByTestId("sync-status");

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
  mockSync.control.reset();
  useScreenStore.setState({ screen: "main", settingsSection: "providers" });
});

afterEach(() => {
  vi.useRealTimers();
});

/** 状態が届くまで待って、表示の種類を返す。 */
async function kindIs(kind: string) {
  await waitFor(() => expect(indicator()).toHaveAttribute("data-kind", kind));
  return indicator();
}

describe("上部バーの同期の表示: 状態ごと", () => {
  it("取得前は「同期: …」。取得できたら、状態に変わる", async () => {
    renderIndicator();
    expect(indicator()).toHaveAttribute("data-kind", "loading");
    expect(indicator()).toHaveTextContent("同期: …");
    await kindIs("disabled");
  });

  it("未設定: 「同期: 未設定」（目立たない色）", async () => {
    renderIndicator();
    const el = await kindIs("disabled");
    expect(el).toHaveTextContent("同期: 未設定");
    expect(el).toHaveAttribute("data-tone", "muted");
  });

  it("同期中: 「同期: 同期中…」（回るアイコン）", async () => {
    mockSync.control.setScenario("syncing");
    renderIndicator();
    const el = await kindIs("syncing");
    expect(el).toHaveTextContent("同期: 同期中…");
    expect(el).toHaveAttribute("data-tone", "info");
    expect(el.querySelector("svg")).toHaveClass("motion-safe:animate-spin");
  });

  it("完了: 「同期: 12:04」。最終の取り込み・書き出しの新しい方の時刻", async () => {
    mockSync.control.setScenario("idle");
    mockSync.control.patch({ lastExportAt: at(10, 15, 12, 4), lastImportAt: at(10, 15, 11, 50) });
    renderIndicator();
    const el = await kindIs("synced");
    expect(el).toHaveTextContent("同期: 12:04");
    expect(el).toHaveAttribute("data-tone", "success");

    // 取り込みの方が新しいときは、そちら
    act(() => mockSync.control.patch({ lastImportAt: at(10, 15, 12, 40) }));
    await waitFor(() => expect(indicator()).toHaveTextContent("同期: 12:40"));
  });

  it("完了: 昨日以前の時刻には、月/日を添える", async () => {
    mockSync.control.setScenario("idle");
    mockSync.control.patch({ lastExportAt: at(10, 14, 23, 5), lastImportAt: null });
    renderIndicator();
    await kindIs("synced");
    expect(indicator()).toHaveTextContent("同期: 10/14 23:05");
  });

  it("未書き出しあり: 件数を出す（注意の色）", async () => {
    mockSync.control.setScenario("pending");
    renderIndicator();
    const el = await kindIs("pending");
    expect(el).toHaveTextContent("同期: 未書き出し 7 件");
    expect(el).toHaveAttribute("data-tone", "warning");
  });

  it("エラー・不一致・更新が必要: 赤く出す", async () => {
    mockSync.control.setScenario("error");
    renderIndicator();
    let el = await kindIs("error");
    expect(el).toHaveTextContent("同期: エラー");
    expect(el).toHaveAttribute("data-tone", "danger");
    expect(el).toHaveClass("text-destructive");

    act(() => mockSync.control.setScenario("mismatch"));
    el = await kindIs("mismatch");
    expect(el).toHaveTextContent("同期: ライブラリが違います");
    expect(el).toHaveClass("text-destructive");

    act(() => mockSync.control.setScenario("updateRequired"));
    el = await kindIs("updateRequired");
    expect(el).toHaveTextContent("同期: 更新が必要");
    expect(el).toHaveClass("text-destructive");
  });

  it("時計のずれ: 完了の表示に、警告の印を添える（読み上げにも入る）", async () => {
    renderIndicator();
    await kindIs("disabled");
    // 未設定のときは出さない
    act(() =>
      mockSync.control.patch({
        clockSkew: { deviceId: "d-x", deviceName: "iMac", aheadMs: 600_000 },
      }),
    );
    expect(indicator().querySelectorAll("svg")).toHaveLength(1);

    act(() => mockSync.control.setScenario("skew"));
    await kindIs("synced");
    expect(indicator().querySelectorAll("svg")).toHaveLength(2);
    expect(indicator()).toHaveAccessibleName(new RegExp(ja.sync.indicator.skewBadge));
  });
});

describe("上部バーの同期の表示: 操作", () => {
  it("押すと、設定の同期の節を開く", async () => {
    mockSync.control.setScenario("idle");
    renderIndicator();
    await kindIs("synced");
    fireEvent.click(indicator());
    expect(useScreenStore.getState()).toMatchObject({
      screen: "settings",
      settingsSection: "sync",
    });
  });

  it("アクセシブルな名前に、見えている文字（同期: 12:04）を含める", async () => {
    mockSync.control.setScenario("idle");
    mockSync.control.patch({ lastExportAt: at(10, 15, 12, 4), lastImportAt: null });
    renderIndicator();
    await kindIs("synced");
    expect(screen.getByRole("button", { name: /同期: 12:04/ })).toBe(indicator());
  });

  it("sync-status を受けて、表示が変わる（同期中 → 完了）", async () => {
    mockSync.control.setScenario("idle");
    renderIndicator();
    await kindIs("synced");

    act(() => mockSync.control.setScenario("syncing"));
    await kindIs("syncing");

    act(() => {
      mockSync.control.setScenario("idle");
      mockSync.control.patch({
        lastExportAt: at(10, 15, 12, 59),
        lastImportAt: at(10, 15, 12, 59),
      });
    });
    await waitFor(() => expect(indicator()).toHaveTextContent("同期: 12:59"));
  });

  it("購読は、画面を外すとやめる", async () => {
    const { unmount } = renderIndicator();
    await kindIs("disabled");
    await waitFor(() => expect(mockSync.control.listenerCount()).toBe(1));
    unmount();
    expect(mockSync.control.listenerCount()).toBe(0);
  });
});

describe("上部バーの同期の表示: ツールチップ", () => {
  it("フォーカスすると、詳細（フォルダ、最終書き出し・取り込み、エラー、時計のずれ）を出す", async () => {
    mockSync.control.setScenario("error");
    mockSync.control.patch({
      lastExportAt: at(10, 15, 12, 4),
      lastImportAt: at(10, 15, 11, 50),
      missingCovers: 2,
      clockSkew: { deviceId: "d-x", deviceName: "iMac", aheadMs: 12 * 60_000 },
    });
    renderIndicator();
    await kindIs("error");

    act(() => indicator().focus());
    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip).toHaveTextContent("最終書き出し: 2026-10-15 12:04");
    expect(tooltip).toHaveTextContent("最終取り込み: 2026-10-15 11:50");
    expect(tooltip).toHaveTextContent("未書き出し: 12 件");
    expect(tooltip).toHaveTextContent("届いていない書影: 2 件");
    expect(tooltip).toHaveTextContent("同期フォルダに書き込めませんでした");
    expect(tooltip).toHaveTextContent("端末 iMac の時計が 12 分進んでいます");
    expect(tooltip).toHaveTextContent(ja.sync.tooltip.clickHint);
  });
});
