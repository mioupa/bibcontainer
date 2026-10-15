// 設定の「同期」の節（FR-SYNC-01／08〜11／13、FR-CONF-01）。手順の分岐（フォルダの判定 × 手元のデータの有無）、
// 置き換え・解除・書影の整理の確認、警告、端末名の変更、状態の表示。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MOCK_FOLDER_PATH, MOCK_LIBRARY_ID, mockSync } from "@/dev/mockSync";
import { SyncSection } from "@/features/settings/sections/SyncSection";
import { useSyncStatusSubscription } from "@/features/sync/hooks";
import { ja } from "@/i18n/ja";
import { IpcError } from "@/lib/ipc";
import { useModalStore } from "@/lib/useModalGuard";
import { listenToMockSync, syncInvokeHandlers } from "@/test/syncIpc";

const isTauri = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const chooseDirectory = vi.hoisted(() => vi.fn());
vi.mock("@/lib/dialog", () => ({
  chooseDirectory,
  chooseSavePath: vi.fn(),
  chooseOpenFile: vi.fn(),
}));

const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), info: vi.fn() }));
vi.mock("sonner", () => ({ toast, Toaster: () => null }));

const CHOSEN = "/Users/me/Google Drive/マイドライブ";
const LIBRARY_FOLDER = `${CHOSEN}/bibcontainer-library`;

const at = (hour: number, minute: number) => new Date(2026, 9, 15, hour, minute).toISOString();

function Harness() {
  useSyncStatusSubscription();
  return <SyncSection />;
}

function renderSection() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
  return { client, ...view };
}

/** フォルダを選んで、判定の結果が出るまで待つ。 */
async function chooseAndInspect() {
  fireEvent.click(await screen.findByRole("button", { name: ja.sync.setup.chooseFolder }));
  return screen.findByTestId("setup-result");
}

const button = (name: string) => screen.getByRole("button", { name });

/** Tauri の中を模す: 同期のコマンドと sync-status を、モックの同期につなぐ（IPC の層を通して、状態はモックが決める）。 */
function installTauriSync() {
  isTauri.mockReturnValue(true);
  invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    const handler = syncInvokeHandlers[command as keyof typeof syncInvokeHandlers];
    if (handler === undefined) {
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      return Promise.reject({ kind: "internal", message: `未設定のコマンド: ${command}` });
    }
    return Promise.resolve(handler(args));
  });
  listen.mockImplementation(
    (name: string, callback: (event: { payload: never }) => void) =>
      listenToMockSync(name, callback as never) ?? Promise.resolve(() => {}),
  );
}

beforeEach(() => {
  installTauriSync();
  chooseDirectory.mockReset();
  chooseDirectory.mockResolvedValue(CHOSEN);
  toast.error.mockReset();
  toast.success.mockReset();
  mockSync.control.reset();
  mockSync.control.setDelays({ setupMs: 0, syncMs: 0 });
  vi.restoreAllMocks();
});

afterEach(() => {
  expect(useModalStore.getState().count).toBe(0);
});

describe("同期の節: 未設定 — フォルダを選ぶ", () => {
  it("説明と「同期フォルダを選ぶ」ボタン。選ぶまで、手順は出さない", async () => {
    renderSection();
    expect(await screen.findByText(ja.sync.setup.intro)).toBeInTheDocument();
    expect(button(ja.sync.setup.chooseFolder)).toBeEnabled();
    expect(screen.queryByTestId("setup-result")).not.toBeInTheDocument();
    // 未設定のときも、端末名と Google Drive の注意は出る
    expect(screen.getByLabelText(ja.sync.deviceName.label)).toBeInTheDocument();
    expect(screen.getByTestId("drive-notes")).toBeInTheDocument();
    // 設定済みの内容は出ない
    expect(screen.queryByTestId("sync-folder")).not.toBeInTheDocument();
  });

  it("フォルダ選択のダイアログを開き、選んだフォルダと、実際に使うフォルダを出す", async () => {
    renderSection();
    const result = await chooseAndInspect();
    expect(chooseDirectory).toHaveBeenCalledWith({ title: ja.sync.setup.dialogTitle });
    expect(within(result).getByTestId("setup-chosen-folder")).toHaveTextContent(CHOSEN);
    expect(within(result).getByTestId("setup-library-folder")).toHaveTextContent(LIBRARY_FOLDER);
  });

  it("ダイアログをキャンセルしたら、何も変えない", async () => {
    chooseDirectory.mockResolvedValue(null);
    const inspect = vi.spyOn(mockSync.api, "inspectSyncFolder");
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.setup.chooseFolder }));
    await waitFor(() => expect(chooseDirectory).toHaveBeenCalled());
    expect(inspect).not.toHaveBeenCalled();
    expect(screen.queryByTestId("setup-result")).not.toBeInTheDocument();
  });

  it("ブラウザ（Tauri の外）では、ダイアログを使わず、モックのパスを選んだことにする", async () => {
    isTauri.mockReturnValue(false);
    renderSection();
    const result = await chooseAndInspect();
    expect(chooseDirectory).not.toHaveBeenCalled();
    expect(within(result).getByTestId("setup-chosen-folder")).toHaveTextContent(MOCK_FOLDER_PATH);
  });

  it("判定に失敗したら、理由を通知して、選ぶところに戻る", async () => {
    vi.spyOn(mockSync.api, "inspectSyncFolder").mockRejectedValue(
      new IpcError({ kind: "io", message: "フォルダを読めません" }),
    );
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.setup.chooseFolder }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        `${ja.sync.setup.inspectFailed}: フォルダを読めません`,
      ),
    );
    expect(button(ja.sync.setup.chooseFolder)).toBeEnabled();
  });

  it("「別のフォルダを選ぶ」で、選び直せる", async () => {
    renderSection();
    await chooseAndInspect();
    fireEvent.click(button(ja.sync.setup.rechoose));
    expect(screen.queryByTestId("setup-result")).not.toBeInTheDocument();
    expect(button(ja.sync.setup.chooseFolder)).toBeEnabled();
  });
});

describe("同期の節: 未設定 — 手順の分岐（フォルダの判定 × 手元のデータ）", () => {
  it("空のフォルダ: 「新しいライブラリを作る」だけ。手元のデータを書き出すと説明する", async () => {
    mockSync.control.setInspect({ kind: "empty", localHasData: true });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    renderSection();
    const result = await chooseAndInspect();
    const plan = within(result).getByTestId("plan-create");
    expect(plan).toHaveTextContent(ja.sync.setup.empty.description);
    expect(plan).toHaveTextContent("この端末の蔵書を書き出します");
    expect(within(result).queryByTestId("plan-join")).not.toBeInTheDocument();
    expect(within(result).queryByTestId("plan-choose")).not.toBeInTheDocument();
    // 端末は見つからない
    expect(within(result).queryByTestId("setup-devices")).not.toBeInTheDocument();

    fireEvent.click(within(plan).getByRole("button", { name: ja.sync.setup.empty.action }));
    await waitFor(() => expect(setup).toHaveBeenCalledWith(CHOSEN, "create"));
  });

  it("ライブラリあり・手元が空: 「このライブラリに参加する」だけ。端末の一覧で、フォルダが正しいか確かめられる", async () => {
    mockSync.control.setInspect({ kind: "library", localHasData: false });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    renderSection();
    const result = await chooseAndInspect();
    const plan = within(result).getByTestId("plan-join");
    expect(plan).toHaveTextContent(ja.sync.setup.join.description);
    expect(within(result).queryByTestId("plan-choose")).not.toBeInTheDocument();
    expect(within(result).queryByTestId("plan-create")).not.toBeInTheDocument();

    // 見つかった端末（名前、OS、最終書き出し）
    const devices = within(result).getByTestId("setup-devices");
    expect(within(devices).getByText("Windows デスクトップ")).toBeInTheDocument();
    expect(
      within(devices).getByText(/Windows ／ v0\.1\.0 ／ 最終書き出し: \d{4}-\d\d-\d\d \d\d:\d\d/),
    ).toBeInTheDocument();
    expect(within(devices).getByText("iMac")).toBeInTheDocument();
    expect(within(devices).getByText(/macOS ／/)).toBeInTheDocument();

    fireEvent.click(within(plan).getByRole("button", { name: ja.sync.setup.join.action }));
    await waitFor(() => expect(setup).toHaveBeenCalledWith(CHOSEN, "join"));
  });

  it("同じライブラリを選び直した（再接続）: 統合も置き換えも出さず、「この場所に付け替える」だけ。join で設定する", async () => {
    mockSync.control.setInspect({
      kind: "library",
      localHasData: true,
      localLibraryId: MOCK_LIBRARY_ID,
    });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    renderSection();
    const result = await chooseAndInspect();
    const plan = within(result).getByTestId("plan-reconnect");
    expect(plan).toHaveTextContent(ja.sync.setup.reconnect.description);
    for (const id of ["plan-create", "plan-join", "plan-choose", "plan-merge", "plan-replace"]) {
      expect(within(result).queryByTestId(id)).not.toBeInTheDocument();
    }
    fireEvent.click(within(plan).getByRole("button", { name: ja.sync.setup.reconnect.action }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.sync.setup.done));
    expect(setup).toHaveBeenCalledExactlyOnceWith(CHOSEN, "join");
    // 設定済みの画面に切り替わる。置き換えではないので、全部の取り直しはしない
    expect(await screen.findByTestId("sync-folder")).toHaveTextContent(LIBRARY_FOLDER);
  });

  it("手元がライブラリに属しているのに、空のフォルダ: 新しく作らせない。先に同期を解除してから選び直す", async () => {
    mockSync.control.setInspect({
      kind: "empty",
      localHasData: true,
      localLibraryId: "lib-local",
    });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    const leave = vi.spyOn(mockSync.api, "leaveSync");
    renderSection();
    const result = await chooseAndInspect();
    const notice = within(result).getByTestId("plan-leave-first");
    expect(notice).toHaveAttribute("role", "alert");
    expect(notice).toHaveTextContent(ja.sync.setup.leaveFirst.title);
    expect(notice).toHaveTextContent("手元のデータと同期フォルダの中身は消えません");
    for (const id of ["plan-create", "plan-join", "plan-choose", "plan-reconnect"]) {
      expect(within(result).queryByTestId(id)).not.toBeInTheDocument();
    }
    expect(setup).not.toHaveBeenCalled();

    // 解除（確認つき）→ 同じフォルダを調べ直す → 手元がどのライブラリにも属さないので、新しく作れる
    fireEvent.click(button(ja.sync.setup.leaveFirst.action));
    const dialog = await screen.findByRole("dialog");
    expect(leave).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: ja.sync.leaveConfirm.confirm }));
    expect(await screen.findByTestId("plan-create")).toBeInTheDocument();
    expect(leave).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("plan-leave-first")).not.toBeInTheDocument();
  });

  it("ライブラリあり・手元にデータ: 「統合する」と「置き換える」を選ばせる。それぞれの意味を説明する", async () => {
    mockSync.control.setInspect({ kind: "library", localHasData: true });
    renderSection();
    const result = await chooseAndInspect();
    const choose = within(result).getByTestId("plan-choose");
    expect(within(choose).getByTestId("plan-merge")).toHaveTextContent(
      "どちらもすべて残します。同じ本をそれぞれで別々に登録していた場合は、同じ本が 2 冊になることがあります",
    );
    expect(within(choose).getByTestId("plan-replace")).toHaveTextContent(
      "この端末の蔵書を消して（先にバックアップを取ります）、ライブラリの内容にします",
    );
    expect(within(result).queryByTestId("plan-join")).not.toBeInTheDocument();
    expect(within(result).queryByTestId("plan-create")).not.toBeInTheDocument();
  });

  it("統合: 確認なしで、joinMerge で設定する", async () => {
    mockSync.control.setInspect({ kind: "library", localHasData: true });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    renderSection();
    await chooseAndInspect();
    fireEvent.click(button(ja.sync.setup.chooseMode.merge.action));
    await waitFor(() => expect(setup).toHaveBeenCalledWith(CHOSEN, "joinMerge"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("置き換え: 確認ダイアログ（バックアップを取ること、端末 ID が新しくなること）を挟む。キャンセルなら何もしない", async () => {
    mockSync.control.setInspect({ kind: "library", localHasData: true });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    renderSection();
    await chooseAndInspect();

    fireEvent.click(button(ja.sync.setup.chooseMode.replace.action));
    let dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(ja.sync.setup.replaceConfirm.title)).toBeInTheDocument();
    expect(dialog).toHaveTextContent("バックアップとして残します");
    expect(dialog).toHaveTextContent("端末 ID は新しくなります");
    expect(useModalStore.getState().count).toBe(1);
    expect(setup).not.toHaveBeenCalled();

    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.sync.setup.replaceConfirm.cancel }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(setup).not.toHaveBeenCalled();
    // 手順は残っている
    expect(screen.getByTestId("plan-choose")).toBeInTheDocument();

    fireEvent.click(button(ja.sync.setup.chooseMode.replace.action));
    dialog = await screen.findByRole("dialog");
    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.sync.setup.replaceConfirm.confirm }),
    );
    await waitFor(() => expect(setup).toHaveBeenCalledWith(CHOSEN, "joinReplace"));
    expect(setup).toHaveBeenCalledTimes(1);
  });

  it("置き換えが終わったら、一覧などを全部取り直す（DB を作り直すため）", async () => {
    mockSync.control.setInspect({ kind: "library", localHasData: true });
    const { client } = renderSection();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    await chooseAndInspect();
    fireEvent.click(button(ja.sync.setup.chooseMode.replace.action));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.sync.setup.replaceConfirm.confirm }),
    );
    await waitFor(() => expect(invalidate).toHaveBeenCalledWith());
  });

  it("統合・参加・新規では、全部の取り直しはしない", async () => {
    mockSync.control.setInspect({ kind: "library", localHasData: true });
    const { client } = renderSection();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    await chooseAndInspect();
    fireEvent.click(button(ja.sync.setup.chooseMode.merge.action));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.sync.setup.done));
    expect(invalidate).not.toHaveBeenCalledWith();
  });

  it("別のライブラリ（AT-S8）: 警告を出し、新規・参加・統合・置き換えは出さない", async () => {
    mockSync.control.setInspect({
      kind: "otherLibrary",
      localHasData: true,
      localLibraryId: "lib-local",
    });
    const setup = vi.spyOn(mockSync.api, "setupSync");
    renderSection();
    const result = await chooseAndInspect();
    const warning = within(result).getByTestId("plan-other-library");
    expect(warning).toHaveAttribute("role", "alert");
    expect(warning).toHaveTextContent(ja.sync.setup.otherLibrary.title);
    expect(warning).toHaveTextContent("同期しません");
    expect(warning).toHaveTextContent("手元のデータと同期フォルダの中身は消えません");
    for (const id of ["plan-create", "plan-join", "plan-choose", "plan-merge", "plan-replace"]) {
      expect(within(result).queryByTestId(id)).not.toBeInTheDocument();
    }
    expect(setup).not.toHaveBeenCalled();
  });

  it("別のライブラリ: 「同期を解除してから、選び直す」で、解除（確認つき）→ 同じフォルダを調べ直す", async () => {
    mockSync.control.setInspect({
      kind: "otherLibrary",
      localHasData: true,
      localLibraryId: "lib-local",
    });
    const leave = vi.spyOn(mockSync.api, "leaveSync");
    renderSection();
    await chooseAndInspect();

    fireEvent.click(button(ja.sync.setup.otherLibrary.action));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(ja.sync.leaveConfirm.title)).toBeInTheDocument();
    expect(leave).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: ja.sync.leaveConfirm.confirm }));

    // 解除すると、判定は「ライブラリあり」になり、手元にデータがあるので統合・置き換えの選択になる
    expect(await screen.findByTestId("plan-choose")).toBeInTheDocument();
    expect(leave).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("plan-other-library")).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("setup-result")).getByTestId("setup-chosen-folder"),
    ).toHaveTextContent(CHOSEN);
  });

  it("書き込めない: 理由と対処を出す。手順のボタンは出さない", async () => {
    mockSync.control.setInspect({ kind: "notWritable" });
    renderSection();
    const result = await chooseAndInspect();
    const warning = within(result).getByTestId("plan-not-writable");
    expect(warning).toHaveTextContent(ja.sync.setup.notWritable.title);
    expect(warning).toHaveTextContent("書き込む権限がないか");
    expect(within(result).queryByTestId("plan-create")).not.toBeInTheDocument();
    expect(within(result).getByRole("button", { name: ja.sync.setup.rechoose })).toBeEnabled();
  });

  it("アプリの更新が必要: 理由と対処を出す", async () => {
    mockSync.control.setInspect({ kind: "updateRequired", localHasData: false });
    renderSection();
    const result = await chooseAndInspect();
    const warning = within(result).getByTestId("plan-update-required");
    expect(warning).toHaveTextContent(ja.sync.setup.updateRequired.title);
    expect(warning).toHaveTextContent("アプリを更新してから");
    expect(within(result).queryByTestId("plan-join")).not.toBeInTheDocument();
    // 見つかった端末は出る
    expect(within(result).getByTestId("setup-devices")).toBeInTheDocument();
  });

  it("設定が終わったら、設定済みの画面（フォルダ、状態、端末）に切り替わる", async () => {
    mockSync.control.setInspect({ kind: "empty" });
    renderSection();
    await chooseAndInspect();
    fireEvent.click(button(ja.sync.setup.empty.action));
    expect(await screen.findByTestId("sync-folder")).toHaveTextContent(LIBRARY_FOLDER);
    expect(toast.success).toHaveBeenCalledWith(ja.sync.setup.done);
    expect(screen.queryByTestId("setup-result")).not.toBeInTheDocument();
    expect(button(ja.sync.actions.syncNow)).toBeInTheDocument();
  });

  it("設定中は、ボタンを押せない。失敗したら、理由を通知して、手順に残る", async () => {
    mockSync.control.setInspect({ kind: "empty" });
    let rejectSetup: (error: unknown) => void = () => {};
    vi.spyOn(mockSync.api, "setupSync").mockReturnValue(
      new Promise((_, reject) => {
        rejectSetup = reject;
      }),
    );
    renderSection();
    await chooseAndInspect();
    fireEvent.click(button(ja.sync.setup.empty.action));

    expect(await screen.findByText(ja.sync.setup.working)).toBeInTheDocument();
    expect(button(ja.sync.setup.empty.action)).toBeDisabled();
    expect(button(ja.sync.setup.rechoose)).toBeDisabled();

    act(() => {
      rejectSetup(new IpcError({ kind: "io", message: "書き込みに失敗しました" }));
    });
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("書き込みに失敗しました"));
    expect(button(ja.sync.setup.empty.action)).toBeEnabled();
    expect(screen.queryByText(ja.sync.setup.working)).not.toBeInTheDocument();
  });
});

describe("同期の節: 設定済み — 状態の表示", () => {
  function configure(patch: Parameters<typeof mockSync.control.patch>[0] = {}) {
    mockSync.control.setScenario("idle");
    mockSync.control.patch({
      folder: LIBRARY_FOLDER,
      lastExportAt: at(12, 4),
      lastImportAt: at(11, 50),
      pendingOps: 0,
      missingCovers: 0,
      ...patch,
    });
  }

  it("フォルダ、状態、最終書き出し・最終取り込み、未書き出し、届いていない書影を出す", async () => {
    configure({ pendingOps: 1234, missingCovers: 5 });
    renderSection();
    expect(await screen.findByTestId("sync-folder")).toHaveTextContent(LIBRARY_FOLDER);
    expect(screen.getByTestId("sync-state")).toHaveTextContent(ja.sync.states.idle);
    expect(screen.getByTestId("sync-last-export")).toHaveTextContent("2026-10-15 12:04");
    expect(screen.getByTestId("sync-last-import")).toHaveTextContent("2026-10-15 11:50");
    expect(screen.getByTestId("sync-pending")).toHaveTextContent("1,234 件");
    expect(screen.getByTestId("sync-missing-covers")).toHaveTextContent("5 件");
    expect(screen.queryByTestId("sync-error")).not.toBeInTheDocument();
    // 設定済みなら、フォルダを選ぶ手順は出ない
    expect(
      screen.queryByRole("button", { name: ja.sync.setup.chooseFolder }),
    ).not.toBeInTheDocument();
    // Google Drive の注意は出る
    expect(screen.getByTestId("drive-notes")).toBeInTheDocument();
  });

  it("まだ書き出し・取り込みをしていなければ「まだありません」", async () => {
    configure({ lastExportAt: null, lastImportAt: null });
    renderSection();
    await screen.findByTestId("sync-folder");
    expect(screen.getByTestId("sync-last-export")).toHaveTextContent(ja.sync.status.never);
    expect(screen.getByTestId("sync-last-import")).toHaveTextContent(ja.sync.status.never);
  });

  it("端末の一覧: 名前・OS・最終書き出し。自分には「この端末」の印", async () => {
    configure();
    renderSection();
    const list = await screen.findByTestId("sync-devices");
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(3);
    const self = items.filter((item) => item.getAttribute("data-self") === "true");
    expect(self).toHaveLength(1);
    expect(self[0]).toHaveTextContent("MacBook Pro");
    expect(self[0]).toHaveTextContent(ja.sync.devices.self);
    expect(items.filter((item) => item.textContent?.includes(ja.sync.devices.self))).toHaveLength(
      1,
    );
    expect(list).toHaveTextContent("Windows デスクトップ");
  });

  it("sync-status を受けて、表示が変わる（同期中 → 完了）", async () => {
    configure();
    renderSection();
    await screen.findByTestId("sync-folder");
    act(() => mockSync.control.patch({ state: "syncing" }));
    await waitFor(() =>
      expect(screen.getByTestId("sync-state")).toHaveTextContent(ja.sync.states.syncing),
    );
    expect(screen.getByRole("button", { name: ja.sync.actions.syncing })).toBeDisabled();
    act(() => mockSync.control.patch({ state: "idle", pendingOps: 3, lastExportAt: at(12, 59) }));
    await waitFor(() => expect(screen.getByTestId("sync-pending")).toHaveTextContent("3 件"));
    expect(screen.getByTestId("sync-last-export")).toHaveTextContent("2026-10-15 12:59");
  });
});

describe("同期の節: 警告（FR-SYNC-10／11、更新が必要、エラー）", () => {
  it("問題がなければ、警告は出ない", async () => {
    mockSync.control.setScenario("idle");
    renderSection();
    await screen.findByTestId("sync-folder");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("ライブラリの不一致: 警告と、解除してから選び直す案内。今すぐ同期は押せない", async () => {
    mockSync.control.setScenario("mismatch");
    renderSection();
    const warning = await screen.findByTestId("warning-mismatch");
    expect(warning).toHaveAttribute("role", "alert");
    expect(warning).toHaveTextContent(ja.sync.warnings.mismatch.title);
    expect(warning).toHaveTextContent("「同期を解除」してから選び直してください");
    expect(screen.getByTestId("sync-state")).toHaveTextContent("一致しません");
    expect(button(ja.sync.actions.syncNow)).toBeDisabled();
    // 解除はできる
    expect(button(ja.sync.actions.leave)).toBeEnabled();
  });

  it("アプリの更新が必要: 警告。今すぐ同期は押せない", async () => {
    mockSync.control.setScenario("updateRequired");
    renderSection();
    const warning = await screen.findByTestId("warning-update-required");
    expect(warning).toHaveTextContent(ja.sync.warnings.updateRequired.title);
    expect(warning).toHaveTextContent("アプリを更新するまで、同期は止まります");
    expect(button(ja.sync.actions.syncNow)).toBeDisabled();
  });

  it("時計のずれ: 「端末 ○○ の時計が N 分進んでいます」", async () => {
    mockSync.control.setScenario("skew");
    renderSection();
    const warning = await screen.findByTestId("warning-skew");
    expect(warning).toHaveTextContent("端末 Windows デスクトップ の時計が 12 分進んでいます");
    expect(warning).toHaveTextContent("日付と時刻を、自動設定にして合わせてください");
    // 同期は止めない
    expect(button(ja.sync.actions.syncNow)).toBeEnabled();
  });

  it("エラー: 直近のエラーを、警告と状態の欄に出す。今すぐ同期でやり直せる", async () => {
    mockSync.control.setScenario("error");
    renderSection();
    const warning = await screen.findByTestId("warning-error");
    expect(warning).toHaveTextContent("同期フォルダに書き込めませんでした");
    expect(screen.getByTestId("sync-error")).toHaveTextContent(
      "同期フォルダに書き込めませんでした",
    );
    expect(button(ja.sync.actions.syncNow)).toBeEnabled();
  });

  it("警告は、状態が戻ると消える", async () => {
    mockSync.control.setScenario("skew");
    renderSection();
    await screen.findByTestId("warning-skew");
    act(() => mockSync.control.patch({ clockSkew: null }));
    await waitFor(() => expect(screen.queryByTestId("warning-skew")).not.toBeInTheDocument());
  });
});

describe("同期の節: 今すぐ同期・同期の解除", () => {
  it("今すぐ同期: syncNow を呼び、終わったら通知する。同期中は押せない", async () => {
    mockSync.control.setScenario("pending");
    const syncNow = vi.spyOn(mockSync.api, "syncNow");
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.actions.syncNow }));
    await waitFor(() => expect(syncNow).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.sync.actions.syncNowDone));
    // 同期が終わって、未書き出しがなくなった
    await waitFor(() => expect(screen.getByTestId("sync-pending")).toHaveTextContent("0 件"));
  });

  it("今すぐ同期が失敗したら、理由を通知する", async () => {
    mockSync.control.setScenario("idle");
    vi.spyOn(mockSync.api, "syncNow").mockRejectedValue(
      new IpcError({ kind: "io", message: "同期フォルダが見つかりません" }),
    );
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.actions.syncNow }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("同期フォルダが見つかりません"));
  });

  it("同期を解除: 確認ダイアログ（手元のデータと同期フォルダの中身は消えない）。キャンセルなら何もしない", async () => {
    mockSync.control.setScenario("idle");
    const leave = vi.spyOn(mockSync.api, "leaveSync");
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.actions.leave }));

    let dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(ja.sync.leaveConfirm.title)).toBeInTheDocument();
    expect(dialog).toHaveTextContent("この端末の蔵書と、同期フォルダの中身は、消えません");
    expect(useModalStore.getState().count).toBe(1);

    fireEvent.click(within(dialog).getByRole("button", { name: ja.sync.leaveConfirm.cancel }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(leave).not.toHaveBeenCalled();
    expect(screen.getByTestId("sync-folder")).toBeInTheDocument();

    fireEvent.click(button(ja.sync.actions.leave));
    dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: ja.sync.leaveConfirm.confirm }));
    await waitFor(() => expect(leave).toHaveBeenCalledTimes(1));
    // 未設定の画面に戻る
    expect(
      await screen.findByRole("button", { name: ja.sync.setup.chooseFolder }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("sync-folder")).not.toBeInTheDocument();
    expect(toast.success).toHaveBeenCalledWith(ja.sync.leaveConfirm.done);
  });
});

describe("同期の節: 端末名（FR-CONF-01）", () => {
  const nameInput = () => screen.getByLabelText(ja.sync.deviceName.label);

  it("現在の端末名と端末 ID を出す。変えるまで、変更ボタンは押せない", async () => {
    mockSync.control.setScenario("idle");
    renderSection();
    await screen.findByTestId("sync-folder");
    expect(nameInput()).toHaveValue("MacBook Pro");
    expect(screen.getByText("d-0123456789ab")).toBeInTheDocument();
    expect(button(ja.sync.deviceName.save)).toBeDisabled();
  });

  it("名前を変えて保存すると setDeviceName を呼び、端末の一覧の自分の名前も変わる", async () => {
    mockSync.control.setScenario("idle");
    const rename = vi.spyOn(mockSync.api, "setDeviceName");
    renderSection();
    await screen.findByTestId("sync-folder");

    fireEvent.change(nameInput(), { target: { value: "  書斎の Mac  " } });
    expect(button(ja.sync.deviceName.save)).toBeEnabled();
    fireEvent.click(button(ja.sync.deviceName.save));

    await waitFor(() => expect(rename).toHaveBeenCalledWith("書斎の Mac"));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.sync.deviceName.saved));
    const self = within(screen.getByTestId("sync-devices"))
      .getAllByRole("listitem")
      .find((item) => item.getAttribute("data-self") === "true");
    expect(self).toHaveTextContent("書斎の Mac");
    expect(nameInput()).toHaveValue("書斎の Mac");
    expect(button(ja.sync.deviceName.save)).toBeDisabled();
  });

  it("Enter でも保存できる。空の名前や、変わっていない名前は保存できない", async () => {
    mockSync.control.setScenario("idle");
    const rename = vi.spyOn(mockSync.api, "setDeviceName");
    renderSection();
    await screen.findByTestId("sync-folder");

    fireEvent.change(nameInput(), { target: { value: "   " } });
    expect(button(ja.sync.deviceName.save)).toBeDisabled();
    fireEvent.submit(nameInput().closest("form")!);
    fireEvent.change(nameInput(), { target: { value: "MacBook Pro " } });
    expect(button(ja.sync.deviceName.save)).toBeDisabled();
    expect(rename).not.toHaveBeenCalled();

    fireEvent.change(nameInput(), { target: { value: "MacBook Air" } });
    fireEvent.submit(nameInput().closest("form")!);
    await waitFor(() => expect(rename).toHaveBeenCalledWith("MacBook Air"));
  });

  it("保存に失敗したら、理由を通知する（入力は残る）", async () => {
    mockSync.control.setScenario("idle");
    vi.spyOn(mockSync.api, "setDeviceName").mockRejectedValue(
      new IpcError({ kind: "validation", message: "端末名は 64 文字までです" }),
    );
    renderSection();
    await screen.findByTestId("sync-folder");
    fireEvent.change(nameInput(), { target: { value: "新しい名前" } });
    fireEvent.click(button(ja.sync.deviceName.save));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("端末名は 64 文字までです"));
    expect(nameInput()).toHaveValue("新しい名前");
  });

  it("未設定のときも、名前を変えられる", async () => {
    const rename = vi.spyOn(mockSync.api, "setDeviceName");
    renderSection();
    await screen.findByRole("button", { name: ja.sync.setup.chooseFolder });
    fireEvent.change(nameInput(), { target: { value: "実験用 PC" } });
    fireEvent.click(button(ja.sync.deviceName.save));
    await waitFor(() => expect(rename).toHaveBeenCalledWith("実験用 PC"));
    await waitFor(() => expect(nameInput()).toHaveValue("実験用 PC"));
  });
});

describe("同期の節: 使われていない書影の整理（FR-SYNC-13）", () => {
  it("確認ダイアログ（30 日より前のもの、戻せない）→ gcCovers → 結果の件数を出す", async () => {
    mockSync.control.setScenario("idle");
    mockSync.control.setGarbageCovers(3);
    const gc = vi.spyOn(mockSync.api, "gcCovers");
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.covers.button }));

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("30 日より前のもの");
    expect(dialog).toHaveTextContent("消した書影は戻せません");
    expect(gc).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: ja.sync.covers.confirm.confirm }));

    await waitFor(() => expect(gc).toHaveBeenCalledTimes(1));
    expect(await screen.findByTestId("gc-result")).toHaveTextContent("3 件の書影を整理しました");
    expect(toast.success).toHaveBeenCalledWith("3 件の書影を整理しました");

    // もう一度: 整理するものがなければ、そう出す
    fireEvent.click(button(ja.sync.covers.button));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: ja.sync.covers.confirm.confirm,
      }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("gc-result")).toHaveTextContent("整理する書影はありませんでした"),
    );
  });

  it("確認でキャンセルしたら、何もしない", async () => {
    mockSync.control.setScenario("idle");
    const gc = vi.spyOn(mockSync.api, "gcCovers");
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.covers.button }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: ja.sync.covers.confirm.cancel }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(gc).not.toHaveBeenCalled();
    expect(screen.queryByTestId("gc-result")).not.toBeInTheDocument();
  });

  it("失敗したら、理由を通知する", async () => {
    mockSync.control.setScenario("idle");
    vi.spyOn(mockSync.api, "gcCovers").mockRejectedValue(
      new IpcError({ kind: "io", message: "同期フォルダを読めません" }),
    );
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: ja.sync.covers.button }));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: ja.sync.covers.confirm.confirm,
      }),
    );
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("同期フォルダを読めません"));
    expect(screen.queryByTestId("gc-result")).not.toBeInTheDocument();
  });
});

describe("同期の節: Google Drive の注意（8.3）", () => {
  it("オフラインで使用可能にする・ミラーリングモード、反映に数秒〜数分、Mac と Windows の場所の例", async () => {
    renderSection();
    const notes = await screen.findByTestId("drive-notes");
    expect(notes).toHaveTextContent(
      "「オフラインで使用可能」にするか、ミラーリングモードを使ってください",
    );
    expect(notes).toHaveTextContent("数秒〜数分");
    expect(notes).toHaveTextContent("DB（library.sqlite3）は同期フォルダに置きません");
    expect(screen.getByText(/GoogleDrive-/)).toBeInTheDocument();
    expect(screen.getByText(/G:\\マイドライブ/)).toBeInTheDocument();
    expect(screen.getByText("macOS")).toBeInTheDocument();
    expect(screen.getByText("Windows")).toBeInTheDocument();
  });
});
