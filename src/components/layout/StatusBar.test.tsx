import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { StatusBar } from "@/components/layout/StatusBar";
import { libraryApi } from "@/features/library/api";
import { ja } from "@/i18n/ja";
import { useRefetchJobStore } from "@/stores/refetchJobStore";
import { makeJob } from "@/test/refetchJob";

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
vi.mock("@/features/library/api", () => ({ libraryApi: { cancelRefetch: vi.fn() } }));

const api = vi.mocked(libraryApi);
const renderBar = () =>
  render(<StatusBar total={500} shown={500} selectedCount={0} mock={false} />);

describe("ステータスバーの再取得の表示（0027 §6）", () => {
  beforeEach(() => {
    useRefetchJobStore.getState().set(null);
    api.cancelRefetch.mockReset().mockResolvedValue(null);
  });

  it("作業がなければ、何も出さない", () => {
    renderBar();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: ja.statusBar.refetch.cancel })).toBeNull();
  });

  it("終わった作業も出さない", () => {
    useRefetchJobStore.getState().set(makeJob({ state: "finished", done: 100, remaining: 0 }));
    renderBar();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });

  it("動いている間は、冊数・残りの時間・進み具合の棒・「やめる」を出す", () => {
    useRefetchJobStore
      .getState()
      .set(makeJob({ done: 37, total: 412, remaining: 375, elapsedMs: 74_000 }));
    renderBar();
    expect(screen.getByText("書誌を再取得中 37 / 412 冊・残り約 13 分")).toBeInTheDocument();
    const bar = screen.getByRole("progressbar", { name: ja.statusBar.refetch.barLabel });
    expect(bar).toHaveAttribute("aria-valuenow", "37");
    expect(bar).toHaveAttribute("aria-valuemax", "412");
    expect((bar.firstElementChild as HTMLElement).style.width).toMatch(/^8\.98/);
  });

  it("済んだ冊数が 0 なら「残りを計算中」、1 分未満なら「残り 1 分未満」", () => {
    useRefetchJobStore.getState().set(makeJob({ done: 0, total: 50, remaining: 50 }));
    const { unmount } = renderBar();
    expect(screen.getByText(/残りを計算中/)).toBeInTheDocument();
    unmount();
    useRefetchJobStore
      .getState()
      .set(makeJob({ done: 40, total: 50, remaining: 10, elapsedMs: 40_000 }));
    renderBar();
    expect(screen.getByText(/残り 1 分未満/)).toBeInTheDocument();
  });

  it("「やめる」で cancel_refetch を呼ぶ", () => {
    useRefetchJobStore.getState().set(makeJob({ done: 10, remaining: 90, elapsedMs: 1000 }));
    renderBar();
    fireEvent.click(screen.getByRole("button", { name: ja.statusBar.refetch.cancel }));
    expect(api.cancelRefetch).toHaveBeenCalledOnce();
  });
});
