import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "@/App";
import type { AppInfo } from "@/bindings/AppInfo";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";
import { ja } from "@/i18n/ja";
import { useLayoutStore } from "@/stores/layoutStore";
import { useScreenStore } from "@/stores/screenStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";
import { useViewStore } from "@/stores/viewStore";
import { makeRow } from "@/test/factories";
import { stubElementSize } from "@/test/layout";

// Tauri の invoke と isTauri、イベントの購読だけを差し替え、ipc.ts のエラー変換も含めて通す
const invoke = vi.hoisted(() => vi.fn());
const isTauri = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

stubElementSize({ width: 1400, height: 700 });

const info: AppInfo = {
  name: "bibcontainer",
  version: "9.9.9",
  deviceId: "d-0123456789ab",
  schemaVersion: 1,
  dbPath: "/data/library.sqlite3",
  hlcLast: null,
  pendingOps: 3,
};

type Handler = (args: Record<string, unknown> | undefined) => unknown;

/** Tauri の invoke が失敗するときの拒否の値は、Error ではなく `{ kind, message }` のオブジェクト。 */
function rejectWith(payload: { kind: string; message: string } | string): Promise<never> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
  return Promise.reject(payload);
}

/** コマンドごとの返答を決める。決めていないコマンドは、内部エラーで失敗する。 */
function routeInvoke(handlers: Record<string, unknown>) {
  invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (!(command in handlers)) {
      return rejectWith({ kind: "internal", message: `未設定のコマンド: ${command}` });
    }
    const handler = handlers[command];
    return Promise.resolve(typeof handler === "function" ? (handler as Handler)(args) : handler);
  });
}

const EMPTY_C_CODE_TABLE = { audience: [], format: [], contentMajor: [], content: [] };

/** `library-changed` を受け取る関数（App が購読したもの）。 */
let emitLibraryChanged: (payload: { books: string[]; others: boolean }) => void;

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

function resetStores() {
  useViewStore.getState().clearFilters();
  useViewStore.getState().setSearchText("");
  useSelectionStore.getState().clear();
  useTableStore.getState().resetColumns();
  useLayoutStore.setState({ sidebarOpen: true, detailOpen: true });
  useSettingsStore.setState({ theme: "system", scan: { ...DEFAULT_SCAN_SETTINGS } });
  // 前のテストが別の画面（設定など）に移っていても、メイン画面から始める
  useScreenStore.setState({ screen: "main", handoff: null });
  document.body.querySelectorAll("[data-sonner-toast]").forEach((el) => el.remove());
}

describe("App（Tauri の中）", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    listen.mockImplementation((_name: string, callback: (event: { payload: never }) => void) => {
      emitLibraryChanged = (payload) => callback({ payload: payload as never });
      return Promise.resolve(() => {});
    });
    isTauri.mockReturnValue(true);
    resetStores();
  });

  it("DB の状態を確かめているあいだは読み込み中を出し、確認できたらメイン画面を出す", async () => {
    routeInvoke({
      app_info: info,
      list_books: [],
      list_locations: [],
      c_code_table: EMPTY_C_CODE_TABLE,
    });

    renderApp();

    expect(screen.getByText(ja.app.loading)).toBeInTheDocument();
    expect(await screen.findByRole("grid", { name: ja.list.ariaLabel })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("app_info", undefined);
  });

  describe("蔵書のデータ（IPC）", () => {
    const rowA = makeRow({ id: "a", title: "吾輩は猫である", sortTitle: "a" });
    const rowB = makeRow({ id: "b", title: "坊っちゃん", sortTitle: "b" });

    function routeLibrary(extra: Record<string, unknown> = {}) {
      routeInvoke({
        app_info: info,
        list_books: [rowA, rowB],
        list_locations: [],
        c_code_table: EMPTY_C_CODE_TABLE,
        search_books: [],
        ...extra,
      });
    }

    it("list_books の行を一覧に出し、モックの表示は出さない", async () => {
      routeLibrary();
      renderApp();

      expect(await screen.findByText("吾輩は猫である")).toBeInTheDocument();
      expect(screen.getByText("坊っちゃん")).toBeInTheDocument();
      expect(await screen.findByRole("status")).toHaveTextContent("2 冊中 2 冊を表示");
      expect(screen.getByRole("status")).not.toHaveTextContent(ja.statusBar.mockData);
    });

    it("library-changed を受けると、変わった本の行だけ get_rows で取り直して一覧を更新する", async () => {
      routeLibrary({
        get_rows: (args: { ids: string[] } | undefined) => {
          expect(args).toEqual({ ids: ["a"] });
          return [{ ...rowA, title: "吾輩は猫である（改）" }];
        },
      });
      renderApp();
      await screen.findByText("吾輩は猫である");
      await waitFor(() =>
        expect(listen).toHaveBeenCalledWith("library-changed", expect.any(Function)),
      );

      act(() => emitLibraryChanged({ books: ["a"], others: false }));

      expect(await screen.findByText("吾輩は猫である（改）")).toBeInTheDocument();
      expect(screen.queryByText("吾輩は猫である")).not.toBeInTheDocument();
      expect(screen.getByText("坊っちゃん")).toBeInTheDocument();
      // 一覧の全件は取り直さない
      expect(invoke.mock.calls.filter(([command]) => command === "list_books")).toHaveLength(1);
    });

    it("library-changed の others が true なら、保管場所も取り直す", async () => {
      routeLibrary();
      renderApp();
      await screen.findByText("吾輩は猫である");
      await waitFor(() => expect(listen).toHaveBeenCalled());
      const count = () => invoke.mock.calls.filter(([c]) => c === "list_locations").length;
      expect(count()).toBe(1);

      act(() => emitLibraryChanged({ books: [], others: true }));

      await waitFor(() => expect(count()).toBe(2));
    });

    it("選んだ本を ⌘⌫ でゴミ箱へ移すと trash_books を呼び、戻ってきた通知で一覧から消える", async () => {
      routeLibrary({
        trash_books: ["a"],
        get_rows: [],
      });
      renderApp();
      const row = (await screen.findByText("吾輩は猫である")).closest<HTMLElement>(
        "[data-row-id]",
      )!;
      fireEvent.click(row);
      fireEvent.keyDown(screen.getByRole("grid"), { code: "Backspace", metaKey: true });

      await waitFor(() => expect(invoke).toHaveBeenCalledWith("trash_books", { ids: ["a"] }));
      // Rust は、変えたあとに library-changed を出す。ゴミ箱の本は一覧（「すべて」）に出ない
      routeLibrary({
        trash_books: ["a"],
        get_rows: [{ ...rowA, deleted: true }],
      });
      act(() => emitLibraryChanged({ books: ["a"], others: true }));
      await waitFor(() => expect(screen.queryByText("吾輩は猫である")).not.toBeInTheDocument());
      expect(screen.getByText("坊っちゃん")).toBeInTheDocument();
    });

    it("操作に失敗したら、Rust が用意した理由を通知する", async () => {
      routeLibrary({
        trash_books: () => rejectWith({ kind: "conflict", message: "ゴミ箱へ移せませんでした" }),
      });
      renderApp();
      const row = (await screen.findByText("吾輩は猫である")).closest<HTMLElement>(
        "[data-row-id]",
      )!;
      fireEvent.click(row);
      fireEvent.keyDown(screen.getByRole("grid"), { code: "Backspace", metaKey: true });
      expect(await screen.findByText("ゴミ箱へ移せませんでした")).toBeInTheDocument();
    });
  });

  it("DB のスキーマ版が新しすぎるときは、エラー画面に原因と案内を出す", async () => {
    routeInvoke({
      app_info: () => rejectWith({ kind: "schema_too_new", message: "スキーマ版 v9 は新しすぎる" }),
    });

    renderApp();

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(ja.errorScreen.title)).toBeInTheDocument();
    expect(within(alert).getByText(ja.errorScreen.hints.schema_too_new)).toBeInTheDocument();
    expect(within(alert).getByText("スキーマ版 v9 は新しすぎる")).toBeInTheDocument();
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();
  });

  it("IpcError 以外で失敗したときも、内部エラーとして原因を出す", async () => {
    routeInvoke({ app_info: () => rejectWith("boom") });

    renderApp();

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(ja.errorScreen.hints.internal)).toBeInTheDocument();
    expect(within(alert).getByText("boom")).toBeInTheDocument();
  });
});

describe("App（ブラウザ。バックエンドなしでモックのデータで動く）", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReturnValue(false);
    resetStores();
    document.documentElement.className = "";
  });

  it("IPC を呼ばずに、3 ペインのメイン画面を出す", async () => {
    renderApp();

    expect(await screen.findByRole("grid", { name: ja.list.ariaLabel })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
    // 上部バー
    expect(screen.getByRole("searchbox", { name: ja.topBar.searchLabel })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: new RegExp(ja.topBar.scanRegister) }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: new RegExp(ja.topBar.newBook) })).toBeInTheDocument();
    expect(screen.getByTestId("sync-status")).toHaveTextContent(ja.topBar.sync);
    // サイドバーと詳細ペイン
    expect(screen.getByRole("navigation", { name: ja.sidebar.ariaLabel })).toBeInTheDocument();
    expect(screen.getByLabelText(ja.detail.ariaLabel)).toBeInTheDocument();
  });

  it("ステータスバーに「N 冊中 M 冊を表示」を出し、サイドバーで絞り込むと M が減る", async () => {
    renderApp();
    const status = await screen.findByRole("status");
    const shown = () => /([\d,]+) 冊中 ([\d,]+) 冊を表示/.exec(status.textContent ?? "");
    await waitFor(() => expect(shown()).not.toBeNull());
    const [, total, visible] = shown()!;
    expect(total).toBe(visible);
    expect(Number(total!.replace(/,/g, ""))).toBeGreaterThan(4000);

    fireEvent.click(
      screen.getByRole("button", { name: new RegExp(`^${ja.sidebar.library.wanted}\\s*\\d+$`) }),
    );
    await waitFor(() => expect(shown()![2]).not.toBe(visible));
    expect(Number(shown()![2]!.replace(/,/g, ""))).toBeLessThan(Number(total!.replace(/,/g, "")));
    expect(status).toHaveTextContent(ja.statusBar.mockData);
  });

  it("検索欄に入力すると、一覧が絞り込まれる（検索はモック）", async () => {
    renderApp();
    const status = await screen.findByRole("status");
    await waitFor(() => expect(status.textContent).toMatch(/冊中/));
    const before = status.textContent;

    fireEvent.change(screen.getByRole("searchbox", { name: ja.topBar.searchLabel }), {
      target: { value: "存在しない語zzzzqqqq" },
    });
    await waitFor(() => expect(status).toHaveTextContent(/ 0 冊を表示/));
    expect(status.textContent).not.toBe(before);
    expect(await screen.findByText(ja.list.empty)).toBeInTheDocument();
  });

  it("ショートカット: ⌘F で検索欄へ、⌘I で詳細ペインを切り替える", async () => {
    renderApp();
    await screen.findByRole("grid");
    const search = screen.getByRole("searchbox", { name: ja.topBar.searchLabel });
    expect(search).not.toHaveFocus();
    fireEvent.keyDown(window, { code: "KeyF", metaKey: true });
    expect(search).toHaveFocus();

    expect(screen.getByLabelText(ja.detail.ariaLabel)).toBeInTheDocument();
    fireEvent.keyDown(window, { code: "KeyI", metaKey: true });
    expect(screen.queryByLabelText(ja.detail.ariaLabel)).not.toBeInTheDocument();
    fireEvent.keyDown(window, { code: "KeyI", metaKey: true });
    expect(screen.getByLabelText(ja.detail.ariaLabel)).toBeInTheDocument();
  });

  it("サイドバーと詳細ペインを、ボタンで折りたためる", async () => {
    renderApp();
    await screen.findByRole("grid");
    fireEvent.click(screen.getByRole("button", { name: ja.topBar.toggleSidebar }));
    expect(
      screen.queryByRole("navigation", { name: ja.sidebar.ariaLabel }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: ja.topBar.toggleDetail }));
    expect(screen.queryByLabelText(ja.detail.ariaLabel)).not.toBeInTheDocument();
    // 一覧は残る
    expect(screen.getByRole("grid")).toBeInTheDocument();
  });

  it("行を選ぶと、詳細ペインにその本が出る", async () => {
    renderApp();
    await screen.findByRole("grid");
    await waitFor(() => expect(document.querySelector("[data-row-id]")).not.toBeNull());
    const firstRow = document.querySelector<HTMLElement>("[data-row-id]")!;
    expect(screen.getByText(ja.detail.empty)).toBeInTheDocument();
    fireEvent.click(firstRow);
    const title = within(firstRow).getAllByRole("gridcell")[1]!.textContent;
    const pane = screen.getByLabelText(ja.detail.ariaLabel);
    await waitFor(() =>
      expect(within(pane).getByRole("heading", { level: 2 })).toBeInTheDocument(),
    );
    expect(within(pane).getByRole("heading", { level: 2 }).textContent).toContain(
      title.slice(0, 2),
    );
  });

  it("設定: 設定画面に切り替わり、テーマを「ダーク」にすると html に dark クラスが付く（FR-CONF-04）", async () => {
    renderApp();
    await screen.findByRole("grid");
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: ja.topBar.settings }));
    const settings = await screen.findByTestId("settings-screen");
    // 画面が切り替わる（メイン画面の一覧は消える）
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();

    fireEvent.click(within(settings).getByRole("button", { name: ja.settings.sections.display }));
    fireEvent.click(within(settings).getByRole("combobox", { name: ja.settings.theme.label }));
    fireEvent.click(await screen.findByRole("option", { name: ja.settings.theme.dark }));
    await waitFor(() => expect(document.documentElement.classList.contains("dark")).toBe(true));
    expect(useSettingsStore.getState().theme).toBe("dark");

    act(() => useSettingsStore.getState().setTheme("light"));
    await waitFor(() => expect(document.documentElement.classList.contains("dark")).toBe(false));

    // メイン画面へ戻る
    fireEvent.click(screen.getByRole("button", { name: ja.settings.back }));
    expect(await screen.findByRole("grid")).toBeInTheDocument();
    expect(screen.queryByTestId("settings-screen")).not.toBeInTheDocument();
  });

  it("スキャン登録: 上部のボタンでスキャン登録画面に切り替わり、メイン画面へ戻れる", async () => {
    renderApp();
    await screen.findByRole("grid");

    fireEvent.click(screen.getByRole("button", { name: new RegExp(ja.topBar.scanRegister) }));
    expect(await screen.findByTestId("scan-screen")).toBeInTheDocument();
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: ja.scan.screen.back }));
    expect(await screen.findByRole("grid")).toBeInTheDocument();
  });

  it("メイン画面でバーコードリーダーの入力（キー間隔が短い数字列＋Enter）を拾い、スキャン登録に引き渡す（FR-SCAN-10）", async () => {
    // スキャン登録画面は別の担当が作る。ここでは、引き渡し（showScan の呼び出し）だけを確かめる
    const original = useScreenStore.getState().showScan;
    const showScan = vi.fn();
    useScreenStore.setState({ showScan });
    renderApp();
    await screen.findByRole("grid");
    let at = 1000;
    for (const digit of "9784003101117") {
      const event = new KeyboardEvent("keydown", {
        code: `Digit${digit}`,
        key: digit,
        bubbles: true,
        cancelable: true,
      });
      Object.defineProperty(event, "timeStamp", { value: (at += 5) });
      act(() => {
        document.body.dispatchEvent(event);
      });
    }
    const enter = new KeyboardEvent("keydown", {
      code: "Enter",
      key: "Enter",
      bubbles: true,
      cancelable: true,
    });
    Object.defineProperty(enter, "timeStamp", { value: at + 5 });
    act(() => {
      document.body.dispatchEvent(enter);
    });
    await waitFor(() => expect(showScan).toHaveBeenCalledExactlyOnceWith("9784003101117"));
    useScreenStore.setState({ showScan: original });
  });
});
