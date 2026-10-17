import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { EditorHost } from "@/features/editor/EditorHost";
import { MainScreen } from "@/features/layout/MainScreen";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";
import { ja } from "@/i18n/ja";
import { chooseSavePath } from "@/lib/dialog";
import { useEditorStore } from "@/stores/editorStore";
import { useGroupStore } from "@/stores/groupStore";
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
    useLayoutStore.setState({
      sidebarOpen: true,
      detailOpen: true,
      viewMode: "table",
      groupVolumes: false,
    });
    useSettingsStore.setState({ theme: "system", scan: { ...DEFAULT_SCAN_SETTINGS } });
    useScreenStore.setState({ screen: "main", handoff: null });
    useEditorStore.getState().close();
    useGroupStore.getState().reset();
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

  describe("表示の形（表・グリッド。FR-LIST-07）", () => {
    const viewButton = (name: string) => screen.getByRole("button", { name });

    it("既定は表。表示の形のボタンは左、右に CSV の書き出しと、表では列のメニューが並ぶ", async () => {
      renderMain();
      await screen.findByRole("grid", { name: ja.list.ariaLabel });
      await rowsReady();
      expect(viewButton(ja.list.viewSwitch.table)).toHaveAttribute("aria-pressed", "true");
      expect(viewButton(ja.list.viewSwitch.grid)).toHaveAttribute("aria-pressed", "false");
      expect(screen.getByRole("button", { name: ja.list.columnPicker.button })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /^並べ替え/ })).not.toBeInTheDocument();
      expect(screen.getAllByRole("columnheader").length).toBeGreaterThan(0);
    });

    it("グリッドに切り替えると、カードのグリッドと並べ替えのメニューを出す。選んだ形は layoutStore に入る", async () => {
      renderMain();
      await screen.findByRole("grid", { name: ja.list.ariaLabel });
      await rowsReady();

      fireEvent.click(viewButton(ja.list.viewSwitch.grid));
      expect(useLayoutStore.getState().viewMode).toBe("grid");
      expect(await screen.findByRole("grid", { name: ja.list.grid.ariaLabel })).toBeInTheDocument();
      expect(screen.queryByRole("grid", { name: ja.list.ariaLabel })).not.toBeInTheDocument();
      expect(screen.queryByRole("columnheader")).not.toBeInTheDocument();
      expect(rowEl("a")).toHaveAttribute("role", "gridcell");
      expect(
        within(rowEl("a")!).getByText("吾輩は猫である", { selector: "p[title]" }),
      ).toBeVisible();
      expect(viewButton(ja.list.viewSwitch.grid)).toHaveAttribute("aria-pressed", "true");
      // 列のメニューの代わりに、並べ替えのメニュー（いまの並びは書名の昇順）
      expect(
        screen.queryByRole("button", { name: ja.list.columnPicker.button }),
      ).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "並べ替え: 書名 ↑" })).toBeInTheDocument();
      // CSV の書き出しは、どちらの形でも右に残る
      expect(
        screen.getByRole("button", { name: ja.list.exportShown.label(2) }),
      ).toBeInTheDocument();

      // 表へ戻す
      fireEvent.click(viewButton(ja.list.viewSwitch.table));
      expect(useLayoutStore.getState().viewMode).toBe("table");
      expect(await screen.findByRole("grid", { name: ja.list.ariaLabel })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: ja.list.columnPicker.button })).toBeInTheDocument();
    });

    it("保存してあった表示の形（グリッド）で開く", async () => {
      useLayoutStore.setState({ viewMode: "grid" });
      renderMain();
      expect(await screen.findByRole("grid", { name: ja.list.grid.ariaLabel })).toBeInTheDocument();
      await rowsReady();
    });

    it("形を切り替えても、選択は変わらず、選んだ本が詳細ペインに出たまま。並べ替えのメニューは表と同じ状態を使う", async () => {
      renderMain();
      await screen.findByRole("grid");
      await rowsReady();
      fireEvent.click(rowEl("b")!);
      expect(await screen.findAllByText("詳細のメモ")).not.toHaveLength(0);

      fireEvent.click(viewButton(ja.list.viewSwitch.grid));
      await screen.findByRole("grid", { name: ja.list.grid.ariaLabel });
      expect([...useSelectionStore.getState().selected]).toEqual(["b"]);
      expect(useSelectionStore.getState().focus).toBe("b");
      expect(rowEl("b")).toHaveAttribute("aria-selected", "true");
      expect(rowEl("a")).toHaveAttribute("aria-selected", "false");

      // 並べ替え（書名の降順）→ 順番が入れ替わり、表へ戻しても同じ並び
      fireEvent.pointerDown(screen.getByRole("button", { name: /^並べ替え/ }), {
        button: 0,
        ctrlKey: false,
      });
      fireEvent.click(await screen.findByRole("menuitemradio", { name: ja.list.sortedDescending }));
      expect(useTableStore.getState().sorting).toEqual([{ id: "title", desc: true }]);
      const order = () =>
        [...document.querySelectorAll("[data-row-id]")].map((el) => el.getAttribute("data-row-id"));
      await waitFor(() => expect(order()).toEqual(["b", "a"]));

      fireEvent.click(viewButton(ja.list.viewSwitch.table));
      await screen.findByRole("grid", { name: ja.list.ariaLabel });
      await waitFor(() => expect(order()).toEqual(["b", "a"]));
      expect(rowEl("b")).toHaveAttribute("aria-selected", "true");
    });

    it("グリッドでも、カードのダブルクリックで編集を開く。Enter でも開く", async () => {
      useLayoutStore.setState({ viewMode: "grid" });
      renderMain();
      await screen.findByRole("grid", { name: ja.list.grid.ariaLabel });
      await rowsReady();
      fireEvent.doubleClick(rowEl("a")!);
      expect(useEditorStore.getState().request).toMatchObject({ mode: "edit" });
      await closeEditor();

      fireEvent.click(rowEl("b")!);
      fireEvent.keyDown(screen.getByRole("grid"), { code: "Enter" });
      expect(useEditorStore.getState().request).toMatchObject({ mode: "edit" });
      await closeEditor();
    });
  });

  // 巻をまとめた表示（FR-LIST-08）。スローループ 1〜7 巻（巻次あり）と 8 巻（書名の末尾だけ）が 1 つのグループになる
  describe("巻をまとめた表示（FR-LIST-08）", () => {
    const isbn13 = (body: string) =>
      body +
      ((10 - ([...body].reduce((sum, d, i) => sum + Number(d) * (i % 2 === 0 ? 1 : 3), 0) % 10)) %
        10);
    const author = [{ name: "うちの 太郎", kana: null, role: null }];
    const volumeRows = [1, 2, 3, 4, 5, 6, 7, 8].map((n) =>
      makeRow({
        id: `v${n}`,
        title: n === 8 ? "スローループ 8" : "スローループ",
        sortTitle: "すろーるーぷ",
        volume: n === 8 ? null : String(n),
        volumeSort: n === 8 ? null : n,
        authors: author,
        isbn13: isbn13(`97848320${String(n).padStart(4, "0")}`),
      }),
    );
    const workRows = [
      makeRow({ id: "pre", title: "あの本", sortTitle: "あ" }),
      ...volumeRows,
      makeRow({ id: "post", title: "んの本", sortTitle: "ん" }),
    ];
    const MEMBER_IDS = volumeRows.map((r) => r.id);
    const byKey = (key: string) =>
      [...document.querySelectorAll<HTMLElement>("[data-row-id]")].find(
        (el) => el.dataset.rowId === key,
      ) ?? null;
    const groupRowEl = () =>
      [...document.querySelectorAll<HTMLElement>("[data-row-id]")].find((el) =>
        el.dataset.rowId?.startsWith("work:"),
      ) ?? null;
    const groupKey = () => groupRowEl()?.dataset.rowId ?? "";
    const toggleButton = () => screen.getByRole("button", { name: ja.list.group.toggle });
    const status = () => screen.getByRole("status");
    const renderGrouped = async () => {
      routeLibrary({ list_books: workRows });
      useLayoutStore.setState({ groupVolumes: true });
      renderMain();
      await screen.findByRole("treegrid");
      await waitFor(() => expect(groupRowEl()).not.toBeNull());
    };

    it("上部バーの「巻をまとめる」で、まとめる設定を切り替える（layoutStore に入る。aria-pressed）。表でもグリッドでも出る", async () => {
      renderMain();
      await screen.findByRole("grid");
      await rowsReady();
      expect(toggleButton()).toHaveAttribute("aria-pressed", "false");
      expect(toggleButton()).toHaveAttribute("title", ja.list.group.toggleHint);
      expect(useLayoutStore.getState().groupVolumes).toBe(false);

      fireEvent.click(toggleButton());
      expect(useLayoutStore.getState().groupVolumes).toBe(true);
      expect(toggleButton()).toHaveAttribute("aria-pressed", "true");
      expect(await screen.findByRole("treegrid")).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: ja.list.viewSwitch.grid }));
      expect(await screen.findByRole("grid", { name: ja.list.grid.ariaLabel })).toBeInTheDocument();
      expect(toggleButton()).toHaveAttribute("aria-pressed", "true");

      fireEvent.click(toggleButton());
      expect(useLayoutStore.getState().groupVolumes).toBe(false);
      expect(toggleButton()).toHaveAttribute("aria-pressed", "false");
    });

    it("まとめる設定がオンなら、同じ作品の巻が 1 行になる。件数（N 冊中 M 冊）は本の数のまま", async () => {
      await renderGrouped();
      expect(
        [...document.querySelectorAll("[data-row-id]")].map((el) => el.getAttribute("data-row-id")),
      ).toEqual(["pre", groupKey(), "post"]);
      expect(within(groupRowEl()!).getByText("8 冊")).toBeInTheDocument();
      expect(within(groupRowEl()!).getByText("1〜8")).toBeInTheDocument();
      expect(within(status()).getByText(ja.statusBar.shown(10, 10))).toBeInTheDocument();
    });

    it("検索欄の ISBN が、畳まれたグループの巻なら、グループを開いて、その本を選ぶ", async () => {
      await renderGrouped();
      expect(byKey("v3")).toBeNull();
      fireEvent.change(search(), { target: { value: volumeRows[2]?.isbn13 ?? "" } });
      fireEvent.keyDown(search(), { key: "Enter" });

      await waitFor(() => expect(byKey("v3")).not.toBeNull());
      expect([...useSelectionStore.getState().selected]).toEqual(["v3"]);
      expect(useSelectionStore.getState().focus).toBe("v3");
      expect(byKey("v3")).toHaveAttribute("aria-selected", "true");
      expect(groupRowEl()).toHaveAttribute("aria-expanded", "true");
      expect(useGroupStore.getState().expanded.has(groupKey())).toBe(true);
      // 詳細ペインには、その本の詳細
      expect(await screen.findAllByText("詳細のメモ")).not.toHaveLength(0);
      expect(calls("get_book_detail")).toContainEqual({ id: "v3" });
    });

    it("グループを選ぶと、ステータスバーと詳細ペインは、全巻の冊数（本の数）を出す", async () => {
      await renderGrouped();
      fireEvent.click(groupRowEl()!);
      expect(within(status()).getByText(ja.statusBar.selected(8))).toBeInTheDocument();
      expect(screen.getByText(ja.detail.multiple(8))).toBeInTheDocument();
      // 詳細は 1 冊の本のものではないので、取りにいかない
      expect(calls("get_book_detail")).toHaveLength(0);

      // ふつうの本を足すと 9 冊。グループの中の巻を足しても、重ならない
      fireEvent.click(byKey("post")!, { metaKey: true });
      expect(within(status()).getByText(ja.statusBar.selected(9))).toBeInTheDocument();
      fireEvent.click(within(groupRowEl()!).getByRole("button", { name: ja.list.group.expand }));
      fireEvent.click(byKey("v2")!, { metaKey: true });
      expect(within(status()).getByText(ja.statusBar.selected(9))).toBeInTheDocument();
    });

    it("開いたグループの巻を選ぶと、詳細ペインはその本の詳細。畳むと、focus がグループの行に移り、全巻の選択に変わる", async () => {
      await renderGrouped();
      fireEvent.click(within(groupRowEl()!).getByRole("button", { name: ja.list.group.expand }));
      fireEvent.click(byKey("v4")!);
      expect(await screen.findAllByText("詳細のメモ")).not.toHaveLength(0);
      expect(within(status()).getByText(ja.statusBar.selected(1))).toBeInTheDocument();

      fireEvent.click(within(groupRowEl()!).getByRole("button", { name: ja.list.group.collapse }));
      expect(byKey("v4")).toBeNull();
      expect(useSelectionStore.getState().focus).toBe(groupKey());
      expect([...useSelectionStore.getState().selected]).toEqual([groupKey()]);
      expect(groupRowEl()).toHaveAttribute("aria-selected", "true");
      expect(groupRowEl()).toHaveAttribute("aria-expanded", "false");
      expect(within(status()).getByText(ja.statusBar.selected(8))).toBeInTheDocument();
      expect(screen.getByText(ja.detail.multiple(8))).toBeInTheDocument();
    });

    it("「表示中の N 冊を CSV に書き出す」は、畳まれたグループの巻も含めて、表示の順に、全巻の ID を渡す", async () => {
      vi.mocked(chooseSavePath).mockResolvedValue("/tmp/shown.csv");
      routeLibrary({ list_books: workRows, export_csv: 10 });
      useLayoutStore.setState({ groupVolumes: true });
      renderMain();
      await screen.findByRole("treegrid");
      await waitFor(() => expect(groupRowEl()).not.toBeNull());

      fireEvent.click(screen.getByRole("button", { name: ja.list.exportShown.label(10) }));
      await waitFor(() => expect(calls("export_csv")).toHaveLength(1));
      expect(calls("export_csv")[0]).toEqual({
        path: "/tmp/shown.csv",
        ids: ["pre", ...MEMBER_IDS, "post"],
      });
    });

    it("グループを選んでゴミ箱へ移動すると、全巻の ID で呼ぶ", async () => {
      routeLibrary({
        list_books: workRows,
        trash_books: { requested: 8, trashed: 8 },
      });
      useLayoutStore.setState({ groupVolumes: true });
      renderMain();
      const grid = await screen.findByRole("treegrid");
      await waitFor(() => expect(groupRowEl()).not.toBeNull());
      fireEvent.click(groupRowEl()!);
      fireEvent.keyDown(grid, { code: "Backspace", metaKey: true });
      await waitFor(() => expect(calls("trash_books")).toHaveLength(1));
      expect(calls("trash_books")[0]).toEqual({ ids: MEMBER_IDS });
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
