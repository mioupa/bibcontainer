import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { EditorHost } from "@/features/editor/EditorHost";
import { MainScreen } from "@/features/layout/MainScreen";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";
import { ja } from "@/i18n/ja";
import { chooseSavePath } from "@/lib/dialog";
import { useEditorStore } from "@/stores/editorStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useScreenStore } from "@/stores/screenStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";
import { useViewStore } from "@/stores/viewStore";
import { makeDetail } from "@/test/detail";
import { makeRow } from "@/test/factories";
import { stubElementSize } from "@/test/layout";

const invoke = vi.hoisted(() => vi.fn());
const isTauri = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
);
vi.mock("sonner", () => ({ toast }));
vi.mock("@/lib/dialog", () => ({ chooseSavePath: vi.fn() }));

stubElementSize({ width: 1400, height: 700 });

type Handler = (args: Record<string, unknown> | undefined) => unknown;

function routeInvoke(handlers: Record<string, unknown>) {
  invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (!(command in handlers)) {
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      return Promise.reject({ kind: "internal", message: `未設定のコマンド: ${command}` });
    }
    const handler = handlers[command];
    return Promise.resolve(typeof handler === "function" ? (handler as Handler)(args) : handler);
  });
}

const rows = [
  makeRow({ id: "a", title: "吾輩は猫である", sortTitle: "a", isbn13: "9784003101117" }),
  makeRow({ id: "b", title: "坊っちゃん", sortTitle: "b", ownership: "wanted" }),
];

function routeLibrary(extra: Record<string, unknown> = {}) {
  routeInvoke({
    list_books: rows,
    list_locations: [],
    list_tags: [],
    list_series: [],
    purchase_store_suggestions: [],
    c_code_table: { audience: [], format: [], contentMajor: [], content: [] },
    get_price_settings: { showTaxIncluded: false, taxRatePercent: 10 },
    search_books: [],
    find_by_isbn: [],
    // 詳細ペインの開閉は、端末ごとの設定として保存される
    set_list_settings: (args: Record<string, unknown> | undefined) => args?.settings,
    get_book_detail: (args: Record<string, unknown> | undefined) =>
      makeDetail({
        id: String(args?.id),
        title: String(args?.id) === "a" ? "吾輩は猫である" : "坊っちゃん",
        memo: "詳細のメモ",
      }),
    ...extra,
  });
}

function renderMain() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <MainScreen />
        <EditorHost />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

const calls = (command: string) =>
  invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args as unknown);
const search = () => screen.getByRole("searchbox", { name: ja.topBar.searchLabel });
const rowEl = (id: string) => document.querySelector<HTMLElement>(`[data-row-id="${id}"]`);
/** 一覧の行が描かれる（list_books が届く）のを待つ。 */
const rowsReady = () => waitFor(() => expect(rowEl("a")).not.toBeNull());
const closeEditor = async () => {
  act(() => useEditorStore.getState().close());
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
};
const press = (init: KeyboardEventInit, target: EventTarget = document.body) =>
  act(() => {
    target.dispatchEvent(
      new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }),
    );
  });

describe("メイン画面", () => {
  beforeEach(() => {
    invoke.mockReset();
    for (const fn of [toast.success, toast.info, toast.error]) fn.mockReset();
    isTauri.mockReturnValue(true);
    useViewStore.getState().clearFilters();
    useViewStore.getState().setSearchText("");
    useSelectionStore.getState().clear();
    useTableStore.getState().resetColumns();
    useLayoutStore.setState({ sidebarOpen: true, detailOpen: true });
    useSettingsStore.setState({ theme: "system", scan: { ...DEFAULT_SCAN_SETTINGS } });
    useScreenStore.setState({ screen: "main", handoff: null });
    useEditorStore.getState().close();
    routeLibrary();
  });

  describe("上部バー", () => {
    it("「手動登録」でフォームを新規で開く。「スキャン登録」「設定」で画面を切り替える", async () => {
      renderMain();
      await screen.findByRole("grid");

      fireEvent.click(screen.getByRole("button", { name: new RegExp(ja.topBar.newBook) }));
      expect(useEditorStore.getState().request).toMatchObject({ mode: "new", source: {} });
      await closeEditor();

      fireEvent.click(screen.getByRole("button", { name: new RegExp(ja.topBar.scanRegister) }));
      expect(useScreenStore.getState().screen).toBe("scan");
      expect(useScreenStore.getState().handoff).toBeNull();

      useScreenStore.setState({ screen: "main" });
      fireEvent.click(screen.getByRole("button", { name: ja.topBar.settings }));
      expect(useScreenStore.getState().screen).toBe("settings");
    });
  });

  describe("検索欄の ISBN（FR-SCAN-09）", () => {
    it("詳細ペインが閉じていれば、登録済みの本へ移るときに開く（FR-SCAN-09）", async () => {
      useLayoutStore.setState({ detailOpen: false });
      renderMain();
      await screen.findByRole("grid");
      expect(screen.queryByLabelText(ja.detail.ariaLabel)).not.toBeInTheDocument();

      fireEvent.change(search(), { target: { value: "9784003101117" } });
      fireEvent.keyDown(search(), { key: "Enter" });

      await waitFor(() => expect(useLayoutStore.getState().detailOpen).toBe(true));
      expect(await screen.findAllByText("詳細のメモ")).not.toHaveLength(0);
      expect(useSelectionStore.getState().focus).toBe("a");
    });

    it("未登録の ISBN では、詳細ペインを開かない（開くのは本へ移るときだけ）", async () => {
      useLayoutStore.setState({ detailOpen: false });
      renderMain();
      await screen.findByRole("grid");
      fireEvent.change(search(), { target: { value: "9784140819005" } });
      fireEvent.keyDown(search(), { key: "Enter" });
      await screen.findByText(ja.list.isbnSearch.registerTitle);
      expect(useLayoutStore.getState().detailOpen).toBe(false);
    });

    it("登録済みの ISBN を Enter で確定すると、その本を選んで詳細を出す（ハイフン・ISBN-10 でも）", async () => {
      renderMain();
      await screen.findByRole("grid");
      useViewStore.getState().toggleReadStatus("read"); // 絞り込み中でも見つかる
      fireEvent.change(search(), { target: { value: "4-00-310111-1" } });
      fireEvent.keyDown(search(), { key: "Enter" });

      await waitFor(() => expect([...useSelectionStore.getState().selected]).toEqual(["a"]));
      expect(useSelectionStore.getState().focus).toBe("a");
      expect(useViewStore.getState().filter.readStatus).toBeNull();
      expect(useViewStore.getState().searchText).toBe("");
      // 詳細ペインに、get_book_detail の内容が出る
      expect(await screen.findAllByText("詳細のメモ")).not.toHaveLength(0);
      expect(calls("get_book_detail")).toContainEqual({ id: "a" });
    });

    it("未登録の ISBN は「登録しますか？」と確認し、「登録する」でスキャン登録に ISBN-13 を引き渡す", async () => {
      renderMain();
      await screen.findByRole("grid");
      fireEvent.change(search(), { target: { value: "978-4-14-081900-5" } });
      fireEvent.keyDown(search(), { key: "Enter" });

      expect(await screen.findByText(ja.list.isbnSearch.registerTitle)).toBeInTheDocument();
      expect(
        screen.getByText(ja.list.isbnSearch.registerDescription("9784140819005")),
      ).toBeInTheDocument();
      expect(useScreenStore.getState().screen).toBe("main");

      fireEvent.click(screen.getByRole("button", { name: ja.list.isbnSearch.register }));
      expect(useScreenStore.getState().screen).toBe("scan");
      expect(useScreenStore.getState().handoff).toBe("9784140819005");
    });

    it("確認をキャンセルすると、画面は替えない", async () => {
      renderMain();
      await screen.findByRole("grid");
      fireEvent.change(search(), { target: { value: "9784140819005" } });
      fireEvent.keyDown(search(), { key: "Enter" });
      fireEvent.click(await screen.findByRole("button", { name: ja.list.isbnSearch.cancel }));
      await waitFor(() =>
        expect(screen.queryByText(ja.list.isbnSearch.registerTitle)).not.toBeInTheDocument(),
      );
      expect(useScreenStore.getState().screen).toBe("main");
    });

    // 日本語入力がオンのままリーダーで読むと、数字は変換中の全角の数字になり、Enter は変換の確定に使われる。
    // それでも、Enter 1 回で確定する（Chromium: key "Process"・isComposing → compositionend）
    it("日本語入力がオンのままスキャンしても（変換中の全角の数字 → Enter で変換を確定）、Enter 1 回でその本へ移る", async () => {
      useLayoutStore.setState({ detailOpen: false });
      renderMain();
      await screen.findByRole("grid");
      fireEvent.compositionStart(search());
      fireEvent.change(search(), { target: { value: "９７８４００３１０１１１７" } });
      fireEvent.keyDown(search(), {
        key: "Process",
        code: "Enter",
        keyCode: 229,
        isComposing: true,
      });
      fireEvent.compositionEnd(search(), { data: "９７８４００３１０１１１７" });

      await waitFor(() => expect([...useSelectionStore.getState().selected]).toEqual(["a"]));
      expect(useViewStore.getState().searchText).toBe("");
      expect(useLayoutStore.getState().detailOpen).toBe(true);
    });

    it("日本語入力がオンのままスキャンした、未登録の ISBN は、Enter 1 回で「登録しますか？」を出す", async () => {
      renderMain();
      await screen.findByRole("grid");
      fireEvent.compositionStart(search());
      fireEvent.change(search(), { target: { value: "９７８４１４０８１９００５" } });
      // Safari（WKWebView）の順序: compositionend → 確定の Enter（keyCode 229・isComposing false）
      fireEvent.compositionEnd(search(), { data: "９７８４１４０８１９００５" });
      fireEvent.keyDown(search(), {
        key: "Enter",
        code: "Enter",
        keyCode: 229,
        isComposing: false,
      });

      expect(await screen.findByText(ja.list.isbnSearch.registerTitle)).toBeInTheDocument();
      expect(
        screen.getByText(ja.list.isbnSearch.registerDescription("9784140819005")),
      ).toBeInTheDocument();
    });

    it("日本語を変換して Enter で確定しただけ（ISBN の形でない）なら、何も起きない", async () => {
      renderMain();
      await screen.findByRole("grid");
      fireEvent.compositionStart(search());
      fireEvent.change(search(), { target: { value: "吾輩は猫である" } });
      fireEvent.keyDown(search(), {
        key: "Process",
        code: "Enter",
        keyCode: 229,
        isComposing: true,
      });
      fireEvent.compositionEnd(search(), { data: "吾輩は猫である" });
      await new Promise((resolve) => setTimeout(resolve, 30));
      expect(screen.queryByText(ja.list.isbnSearch.registerTitle)).not.toBeInTheDocument();
      expect(useSelectionStore.getState().selected.size).toBe(0);
      // 検索語は、そのまま残る
      expect(useViewStore.getState().searchText).toBe("吾輩は猫である");
    });

    it("ISBN でない検索語の Enter では、何もしない。日本語入力の変換中の Enter も無視する", async () => {
      renderMain();
      await screen.findByRole("grid");
      fireEvent.change(search(), { target: { value: "吾輩" } });
      fireEvent.keyDown(search(), { key: "Enter" });
      fireEvent.change(search(), { target: { value: "9784140819005" } });
      fireEvent.keyDown(search(), { key: "Enter", isComposing: true });
      expect(screen.queryByText(ja.list.isbnSearch.registerTitle)).not.toBeInTheDocument();
      expect(useSelectionStore.getState().selected.size).toBe(0);
    });
  });

  describe("ショートカット（9.5）", () => {
    // ⌘N・⇧⌘N・⌘, は、どの画面でも効くので App が受ける（AppScreens.test.tsx）。メイン画面だけのものはここ
    it("⌘I は詳細ペインの切替、⌘F は検索欄へ", async () => {
      renderMain();
      await screen.findByRole("grid");

      press({ code: "KeyI", metaKey: true });
      expect(useLayoutStore.getState().detailOpen).toBe(false);
      press({ code: "KeyI", metaKey: true });
      expect(useLayoutStore.getState().detailOpen).toBe(true);

      press({ code: "KeyF", metaKey: true });
      expect(document.activeElement).toBe(search());
    });

    it("一覧で Enter を押すと、選んだ本の編集フォームを開く", async () => {
      renderMain();
      const grid = await screen.findByRole("grid");
      await rowsReady();
      fireEvent.click(rowEl("a")!);
      fireEvent.keyDown(grid, { code: "Enter" });
      expect(useEditorStore.getState().request).toEqual({ mode: "edit", bookId: "a" });
    });

    it("フォームを開いている間は、一覧向け・画面全体のショートカットとスキャンの検出を止める", async () => {
      renderMain();
      await screen.findByRole("grid");
      const first = useEditorStore.getState();
      act(() => first.openEdit("a"));
      await screen.findByRole("dialog");
      const request = useEditorStore.getState().request;

      press({ code: "KeyI", metaKey: true }); // 詳細ペインの切替
      expect(useLayoutStore.getState().detailOpen).toBe(true);
      press({ code: "KeyF", metaKey: true }); // 検索欄へ
      // （ダイアログの外は aria-hidden になるので、検索欄は role では探せない）
      expect(document.activeElement).not.toBe(document.querySelector('input[type="search"]'));
      expect(useScreenStore.getState().screen).toBe("main");
      expect(useEditorStore.getState().request).toBe(request);

      // メイン画面のスキャン検出（FR-SCAN-10）も止まる
      const showScan = vi.fn();
      useScreenStore.setState({ showScan });
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
      const enter = new KeyboardEvent("keydown", { code: "Enter", key: "Enter", bubbles: true });
      Object.defineProperty(enter, "timeStamp", { value: at + 5 });
      act(() => {
        document.body.dispatchEvent(enter);
      });
      expect(showScan).not.toHaveBeenCalled();
    });
  });

  describe("表示中の本の書き出し（FR-IO-01）", () => {
    it("「表示中の N 冊を CSV に書き出す」で、一覧に出ている本（絞り込み結果）の ID だけを書き出す", async () => {
      vi.mocked(chooseSavePath).mockResolvedValue("/tmp/shown.csv");
      routeLibrary({ export_csv: 2 });
      renderMain();
      await screen.findByRole("grid");
      await rowsReady();

      fireEvent.click(screen.getByRole("button", { name: ja.list.exportShown.label(2) }));
      await waitFor(() => expect(calls("export_csv")).toHaveLength(1));
      expect(calls("export_csv")[0]).toEqual({ path: "/tmp/shown.csv", ids: ["a", "b"] });
      expect(toast.success).toHaveBeenCalledWith(ja.toast.exported(2));

      // 絞り込む（欲しい本だけ）と、その 1 冊だけ。件数もボタンに出る
      act(() => useViewStore.getState().setLibrary("wanted"));
      fireEvent.click(await screen.findByRole("button", { name: ja.list.exportShown.label(1) }));
      await waitFor(() => expect(calls("export_csv")).toHaveLength(2));
      expect(calls("export_csv")[1]).toEqual({ path: "/tmp/shown.csv", ids: ["b"] });
    });

    it("保存先の選択をやめたら、何も書き出さない。表示中の本がないときは押せない", async () => {
      vi.mocked(chooseSavePath).mockResolvedValue(null);
      renderMain();
      await screen.findByRole("grid");
      await rowsReady();
      fireEvent.click(screen.getByRole("button", { name: ja.list.exportShown.label(2) }));
      await waitFor(() => expect(chooseSavePath).toHaveBeenCalled());
      expect(calls("export_csv")).toHaveLength(0);

      act(() => useViewStore.getState().setLibrary("trash"));
      expect(
        await screen.findByRole("button", { name: ja.list.exportShown.label(0) }),
      ).toBeDisabled();
    });
  });

  describe("詳細ペイン", () => {
    it("「編集」でフォームを開き、「購入済みにする」（欲しい本）で所有に変える", async () => {
      routeLibrary({ update_books: [{ id: "b", written: ["ownership"], skippedManual: [] }] });
      renderMain();
      await screen.findByRole("grid");
      await rowsReady();
      fireEvent.click(rowEl("b")!);

      fireEvent.click(await screen.findByRole("button", { name: ja.detail.markPurchased }));
      await waitFor(() => expect(calls("update_books")).toHaveLength(1));
      expect(calls("update_books")[0]).toEqual({
        ids: ["b"],
        changes: [{ field: "ownership", value: "owned", origin: { kind: "manual" } }],
      });
      expect(toast.success).toHaveBeenCalledWith(ja.toast.purchased(1));

      fireEvent.click(screen.getByRole("button", { name: ja.detail.edit }));
      expect(useEditorStore.getState().request).toEqual({ mode: "edit", bookId: "b" });
    });

    it("「書誌を再取得」は確認を出し、既定では手で直した項目を上書きしない（AT-16）", async () => {
      routeLibrary({
        refetch_books: {
          requested: 1,
          updated: 1,
          unchanged: 0,
          offline: 0,
          skipped: 0,
          failed: 0,
          coversSaved: 0,
          noActiveProviders: false,
          items: [],
        },
      });
      renderMain();
      await screen.findByRole("grid");
      await rowsReady();
      fireEvent.click(rowEl("a")!);

      fireEvent.click(await screen.findByRole("button", { name: ja.detail.refetch }));
      const checkbox = await screen.findByRole("checkbox", {
        name: ja.list.refetchDialog.overrideLabel,
      });
      expect(checkbox).not.toBeChecked();
      expect(calls("refetch_books")).toHaveLength(0);

      fireEvent.click(screen.getByRole("button", { name: ja.list.refetchDialog.confirm }));
      await waitFor(() => expect(calls("refetch_books")).toHaveLength(1));
      expect(calls("refetch_books")[0]).toEqual({ ids: ["a"], overrideManual: false });
    });
  });
});
