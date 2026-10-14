import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BookDraft } from "@/bindings/BookDraft";
import type { FetchMetadataResult } from "@/bindings/FetchMetadataResult";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import { EditorHost } from "@/features/editor/EditorHost";
import { ja } from "@/i18n/ja";
import { useEditorStore } from "@/stores/editorStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { makeDetail } from "@/test/detail";
import { makeRow } from "@/test/factories";

// Tauri の invoke と isTauri だけを差し替え、ipc.ts のエラー変換も含めて通す
const invoke = vi.hoisted(() => vi.fn());
const isTauri = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
);
vi.mock("sonner", () => ({ toast }));

type Handler = (args: Record<string, unknown> | undefined) => unknown;

function rejectWith(payload: { kind: string; message: string }): Promise<never> {
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

const MANUAL = { kind: "manual" } as const;
const EMPTY_C_CODES = { audience: [], format: [], contentMajor: [], content: [] };

const metadata = (isbn: string, offline = false): FetchMetadataResult => ({
  merged: {
    isbn13: isbn,
    fields: [
      {
        field: "publisher",
        chosen: { source: "openbd", value: { type: "text", value: "岩波書店" } },
        others: [{ source: "ndl", value: { type: "text", value: "岩波書店（NDL）" } }],
      },
    ],
  },
  draft: {
    fields: {
      isbn13: { value: isbn, origin: { kind: "barcode" } },
      title: { value: "取得した書名", origin: { kind: "provider", name: "openbd" } },
      publisher: { value: "岩波書店", origin: { kind: "provider", name: "openbd" } },
      meta_status: {
        value: offline ? "pending" : "ok",
        origin: { kind: "provider", name: "bibcontainer" },
      },
    },
    seriesName: null,
  } satisfies BookDraft,
  reports: [],
  offline,
  coverHash: null,
});

/** 共通の、読み取り系コマンドの返答。 */
function baseHandlers(extra: Record<string, unknown> = {}) {
  return {
    list_books: [],
    list_series: [],
    list_locations: [
      { id: "loc-1", name: "自宅", parentId: null, sortOrder: 1, path: "自宅", depth: 0 },
      { id: "loc-2", name: "書斎", parentId: "loc-1", sortOrder: 1, path: "自宅 > 書斎", depth: 1 },
    ],
    list_tags: [{ name: "小説", count: 3, color: null, id: null }],
    purchase_store_suggestions: [{ name: "紀伊國屋", count: 3 }],
    c_code_table: EMPTY_C_CODES,
    get_price_settings: { showTaxIncluded: false, taxRatePercent: 10 },
    find_by_isbn: [],
    search_books: [],
    ...extra,
  };
}

function renderHost() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <EditorHost />
    </QueryClientProvider>,
  );
}

// タブの内容（tabpanel）も、タブの名前で名づけられているので、入力欄だけに絞る
const input = (label: string | RegExp) =>
  screen.getByLabelText<HTMLInputElement>(label, { selector: "input, textarea" });
const change = (label: string | RegExp, value: string) =>
  fireEvent.change(input(label), { target: { value } });
const save = (name: string = ja.edit.actions.save) =>
  fireEvent.click(screen.getByRole("button", { name }));
const calls = (command: string) =>
  invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args as unknown);
const openTab = (tab: keyof typeof ja.edit.tabs) =>
  fireEvent.mouseDown(screen.getByRole("tab", { name: new RegExp(ja.edit.tabs[tab]) }));

describe("登録・編集フォーム", () => {
  beforeEach(() => {
    invoke.mockReset();
    toast.success.mockReset();
    toast.info.mockReset();
    toast.error.mockReset();
    isTauri.mockReturnValue(true);
    useEditorStore.getState().close();
    useSelectionStore.getState().clear();
  });

  describe("edit", () => {
    const detail = makeDetail({
      id: "book-1",
      title: "吾輩は猫である",
      isbn13: "9784003101117",
      publisher: "岩波書店",
      publishedOn: "1905",
      cCode: "0193",
      listPrice: 700,
      rating: 3,
      fieldSources: { title: "openbd", publisher: "openbd", c_code: "barcode" },
    });

    function routeEdit(extra: Record<string, unknown> = {}) {
      routeInvoke(
        baseHandlers({
          get_book_detail: detail,
          fetch_metadata: metadata("9784003101117"),
          update_book: { id: "book-1", written: ["title"], skippedManual: [] },
          ...extra,
        }),
      );
    }

    async function openEdit() {
      renderHost();
      act(() => useEditorStore.getState().openEdit("book-1"));
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
    }

    it("本の詳細から値を入れ、Cコードは数字と名称を並べて出し、項目の横に取得元を出す", async () => {
      routeEdit({
        c_code_table: {
          audience: [{ code: "0", name: "一般" }],
          format: [{ code: "1", name: "文庫" }],
          contentMajor: [{ code: "9", name: "文学" }],
          content: [{ code: "93", name: "日本文学、小説・物語" }],
        },
      });
      await openEdit();

      expect(input(ja.edit.fields.title).value).toBe("吾輩は猫である");
      expect(input(ja.edit.fields.publisher).value).toBe("岩波書店");
      expect(input(ja.edit.fields.listPrice).value).toBe("700");
      expect(input(ja.edit.fields.isbn13).value).toBe("9784003101117");
      await waitFor(() =>
        expect(screen.getByTestId("c-code-label")).toHaveTextContent(
          /^0193\s一般・文庫・日本文学、小説・物語$/,
        ),
      );
      // 書名の取得元は openBD、Cコードはバーコード
      const titleRow = document.querySelector('[data-field="title"]') as HTMLElement;
      expect(within(titleRow).getByText(ja.edit.sources.openbd)).toBeInTheDocument();
      const cCodeRow = document.querySelector('[data-field="cCode"]') as HTMLElement;
      expect(within(cCodeRow).getByText(ja.edit.sources.barcode)).toBeInTheDocument();
    });

    it("書名を直すと取得元が「手入力」になり、変えた項目だけを origin: manual で update_book に送る", async () => {
      routeEdit();
      await openEdit();

      change(ja.edit.fields.title, "我輩は猫である");
      const titleRow = document.querySelector('[data-field="title"]') as HTMLElement;
      await waitFor(() =>
        expect(within(titleRow).getByText(ja.edit.sources.manual)).toBeInTheDocument(),
      );
      save();

      await waitFor(() => expect(calls("update_book")).toHaveLength(1));
      expect(calls("update_book")[0]).toEqual({
        id: "book-1",
        changes: [{ field: "title", value: "我輩は猫である", origin: MANUAL }],
      });
      await waitFor(() => expect(useEditorStore.getState().request).toBeNull());
      expect(toast.success).toHaveBeenCalledWith(ja.edit.toast.saved);
    });

    it("何も変えずに保存しても、update_book を呼ばず、閉じる", async () => {
      routeEdit();
      await openEdit();
      save();
      await waitFor(() => expect(useEditorStore.getState().request).toBeNull());
      expect(calls("update_book")).toHaveLength(0);
      expect(toast.info).toHaveBeenCalledWith(ja.edit.toast.noChanges);
    });

    it("読書・購入・メモのタブの項目も、変えた分だけ送る（評価は ★、金額は整数）", async () => {
      routeEdit();
      await openEdit();
      openTab("reading");
      fireEvent.click(screen.getByRole("button", { name: ja.edit.ratingStar(5) }));
      openTab("purchase");
      change(ja.edit.fields.purchasePrice, "1,430円");
      change(ja.edit.fields.purchaseStore, "紀伊國屋");
      openTab("memo");
      change(ja.edit.fields.memo, "貸出中");
      save();

      await waitFor(() => expect(calls("update_book")).toHaveLength(1));
      const sent = calls("update_book")[0] as { changes: { field: string; value: unknown }[] };
      expect(sent.changes.map((c) => [c.field, c.value])).toEqual([
        ["rating", 5],
        ["purchase_store", "紀伊國屋"],
        ["purchase_price", 1430],
        ["memo", "貸出中"],
      ]);
    });

    it("他の取得元の値を選ぶと、その取得元の由来で送り、手修正の上書きを許す（FR-META-05）", async () => {
      routeEdit();
      await openEdit();
      // 候補（fetch_metadata の merged）が届くのを待つ
      const trigger = await screen.findByRole("button", {
        name: ja.edit.source.otherValues(ja.edit.fields.publisher),
      });
      fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });
      fireEvent.click(await screen.findByRole("menuitem", { name: /岩波書店（NDL）/ }));
      expect(input(ja.edit.fields.publisher).value).toBe("岩波書店（NDL）");
      save();

      await waitFor(() => expect(calls("update_book")).toHaveLength(1));
      expect(calls("update_book")[0]).toEqual({
        id: "book-1",
        changes: [
          {
            field: "publisher",
            value: "岩波書店（NDL）",
            origin: { kind: "provider", name: "ndl" },
          },
        ],
        options: { overrideManual: true },
      });
    });

    it("検証エラー（Cコードが 3 桁）は項目に出し、そのタブを開いて、保存しない", async () => {
      routeEdit();
      await openEdit();
      openTab("memo");
      openTab("basic");
      change(ja.edit.fields.cCode, "193");
      save();
      expect(await screen.findByText(ja.edit.errors.cCode)).toBeInTheDocument();
      expect(calls("update_book")).toHaveLength(0);
      expect(useEditorStore.getState().request).not.toBeNull();
    });

    it("サーバーの検証エラー（validation）を、その項目に表示する", async () => {
      routeEdit({
        update_book: () =>
          rejectWith({
            kind: "validation",
            message:
              "入力が不正です（published_on）: YYYY、YYYY-MM、YYYY-MM-DD のどれかで指定してください",
          }),
      });
      await openEdit();
      change(ja.edit.fields.publishedOn, "2026-10");
      save();
      await waitFor(() => expect(calls("update_book")).toHaveLength(1));
      const row = document.querySelector('[data-field="publishedOn"]') as HTMLElement;
      expect(await within(row).findByRole("alert")).toHaveTextContent(
        "YYYY、YYYY-MM、YYYY-MM-DD のどれかで指定してください",
      );
      // 保存に失敗したので、フォームは開いたまま
      expect(useEditorStore.getState().request).not.toBeNull();
    });

    it("シリーズを入れて保存すると、find_or_create_series で ID にして series_id を送る", async () => {
      routeEdit({ find_or_create_series: "series-1" });
      await openEdit();
      change(ja.edit.fields.seriesName, "猫シリーズ");
      save();
      await waitFor(() => expect(calls("update_book")).toHaveLength(1));
      expect(calls("find_or_create_series")).toEqual([{ name: "猫シリーズ" }]);
      expect(calls("update_book")[0]).toEqual({
        id: "book-1",
        changes: [{ field: "series_id", value: "series-1", origin: MANUAL }],
      });
    });

    it("画像 URL を取り込むと、書影のハッシュを cover_hash として保存する（FR-COVER-05）", async () => {
      routeEdit({ import_cover_url: "a".repeat(64) });
      await openEdit();
      change(ja.edit.cover.urlLabel, "https://example.com/c.jpg");
      fireEvent.click(screen.getByRole("button", { name: ja.edit.cover.importUrl }));
      await waitFor(() => expect(calls("import_cover_url")).toHaveLength(1));
      expect(calls("import_cover_url")[0]).toEqual({ url: "https://example.com/c.jpg" });
      await screen.findByRole("button", { name: new RegExp(ja.edit.cover.revert) });
      save();
      await waitFor(() => expect(calls("update_book")).toHaveLength(1));
      expect(calls("update_book")[0]).toEqual({
        id: "book-1",
        changes: [{ field: "cover_hash", value: "a".repeat(64), origin: MANUAL }],
      });
    });

    it("変更があるとき、キャンセルは破棄の確認を出す。変更がなければ、そのまま閉じる", async () => {
      routeEdit();
      await openEdit();
      fireEvent.click(screen.getByRole("button", { name: ja.edit.actions.cancel }));
      await waitFor(() => expect(useEditorStore.getState().request).toBeNull());

      act(() => useEditorStore.getState().openEdit("book-1"));
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      change(ja.edit.fields.title, "変えた");
      fireEvent.click(screen.getByRole("button", { name: ja.edit.actions.cancel }));
      expect(await screen.findByText(ja.edit.discard.title)).toBeInTheDocument();
      expect(useEditorStore.getState().request).not.toBeNull();
      fireEvent.click(screen.getByRole("button", { name: ja.edit.discard.discard }));
      await waitFor(() => expect(useEditorStore.getState().request).toBeNull());
      expect(calls("update_book")).toHaveLength(0);
    });
  });

  describe("new", () => {
    function openNew(source: Parameters<ReturnType<typeof useEditorStore.getState>["openNew"]>[0]) {
      renderHost();
      act(() => useEditorStore.getState().openNew(source));
    }

    it("手動登録: 書名を入れて保存すると、manual の由来で create_books_from_drafts に送る。ISBN がなければ取得待ちにしない", async () => {
      routeInvoke(baseHandlers({ create_books_from_drafts: ["new-1"] }));
      openNew({});
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      change(ja.edit.fields.title, "手で入れた本");
      change(ja.edit.fields.publisher, "自費出版");
      save(ja.edit.actions.create);

      await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));
      expect(calls("create_books_from_drafts")[0]).toEqual({
        drafts: [
          {
            changes: [
              { field: "title", value: "手で入れた本", origin: MANUAL },
              { field: "publisher", value: "自費出版", origin: MANUAL },
            ],
            seriesName: null,
          },
        ],
      });
      await waitFor(() => expect(useEditorStore.getState().request).toBeNull());
      // 登録した本を選ぶ
      expect([...useSelectionStore.getState().selected]).toEqual(["new-1"]);
    });

    it("書名が空で ISBN もないときは、検証エラーで保存しない", async () => {
      routeInvoke(baseHandlers());
      openNew({});
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      save(ja.edit.actions.create);
      expect(await screen.findByText(ja.edit.errors.titleRequired)).toBeInTheDocument();
      expect(calls("create_books_from_drafts")).toHaveLength(0);
    });

    it("取得結果の下書き・先に入れる値・フォームの内容の優先順位で、NewBookDraft を作る", async () => {
      routeInvoke(baseHandlers({ create_books_from_drafts: ["new-1"] }));
      openNew({
        isbn13: "9784003101117",
        fetched: metadata("9784003101117"),
        presets: [
          { field: "location_id", value: "loc-2", origin: MANUAL },
          { field: "publisher", value: "共通の出版社", origin: MANUAL },
        ],
      });
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      // 取得した書名が入っている。先に入れた値（出版社）は、取得結果より優先
      expect(input(ja.edit.fields.title).value).toBe("取得した書名");
      expect(input(ja.edit.fields.publisher).value).toBe("共通の出版社");
      change(ja.edit.fields.title, "直した書名");
      save(ja.edit.actions.create);

      await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));
      const { drafts } = calls("create_books_from_drafts")[0] as { drafts: NewBookDraft[] };
      const byField = Object.fromEntries(drafts[0]?.changes.map((c) => [c.field, c]) ?? []);
      expect(byField.title).toEqual({ field: "title", value: "直した書名", origin: MANUAL });
      expect(byField.publisher).toEqual({
        field: "publisher",
        value: "共通の出版社",
        origin: MANUAL,
      });
      expect(byField.location_id?.value).toBe("loc-2");
      expect(byField.isbn13?.origin).toEqual({ kind: "barcode" });
      expect(byField.meta_status?.value).toBe("ok");
    });

    it("オフライン（offline）で取得できなかった本は、取得待ち（pending）で保存する（AT-11）", async () => {
      routeInvoke(baseHandlers({ create_books_from_drafts: ["new-1"] }));
      openNew({ isbn13: "9784003101117", fetched: metadata("9784003101117", true) });
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      save(ja.edit.actions.create);
      await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));
      const { drafts } = calls("create_books_from_drafts")[0] as { drafts: NewBookDraft[] };
      expect(drafts[0]?.changes.find((c) => c.field === "meta_status")?.value).toBe("pending");
    });

    it("同じ ISBN の本があれば警告し、確認のチェックを入れるまで保存できない（FR-BOOK-10）", async () => {
      routeInvoke(
        baseHandlers({
          create_books_from_drafts: ["new-2"],
          find_by_isbn: [
            {
              id: "old-1",
              title: "すでにある本",
              ownership: "owned",
              locationId: "loc-2",
              locationPath: "自宅 > 書斎",
              deleted: false,
            },
          ],
        }),
      );
      openNew({ isbn13: "9784003101117", fetched: metadata("9784003101117") });
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });

      expect(await screen.findByText(ja.edit.duplicate.title)).toBeInTheDocument();
      expect(screen.getByText(/すでにある本（所有 ／ 自宅 > 書斎）/)).toBeInTheDocument();
      const submit = screen.getByRole("button", { name: ja.edit.actions.create });
      expect(submit).toBeDisabled();

      fireEvent.click(screen.getByRole("checkbox", { name: ja.edit.duplicate.confirm }));
      await waitFor(() => expect(submit).toBeEnabled());
      fireEvent.click(submit);
      await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));
    });

    it("手動登録で、書名・著者が似た本があれば候補に出す（FR-SCAN-19）", async () => {
      routeInvoke(
        baseHandlers({
          search_books: ["old-1"],
          list_books: [makeRow({ id: "old-1", title: "吾輩は猫である", publisher: "岩波書店" })],
        }),
      );
      openNew({});
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      change(ja.edit.fields.title, "吾輩は猫");
      expect(
        await screen.findByText(ja.edit.similar.title, {}, { timeout: 3000 }),
      ).toBeInTheDocument();
      expect(screen.getByText(/吾輩は猫である ／ 岩波書店/)).toBeInTheDocument();
    });

    it("ISBN の横の「書誌を取得」で、まだ直していない項目に取得した値を入れる", async () => {
      routeInvoke(baseHandlers({ fetch_metadata: metadata("9784003101117") }));
      openNew({});
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      change(ja.edit.fields.publisher, "自分で入れた出版社");
      change(ja.edit.fields.isbn13, "978-4-00-310111-7");
      fireEvent.click(screen.getByRole("button", { name: ja.edit.fetch }));
      await waitFor(() => expect(input(ja.edit.fields.title).value).toBe("取得した書名"));
      // 直した項目は、そのまま
      expect(input(ja.edit.fields.publisher).value).toBe("自分で入れた出版社");
      expect(calls("fetch_metadata")).toEqual([{ isbn13: "9784003101117" }]);
    });
  });

  describe("draft", () => {
    it("保存せず、onDone に直した下書きを渡して閉じる", async () => {
      routeInvoke(baseHandlers());
      const onDone = vi.fn();
      renderHost();
      act(() =>
        useEditorStore
          .getState()
          .openDraft({ isbn13: "9784003101117", fetched: metadata("9784003101117") }, onDone),
      );
      await screen.findByLabelText(ja.edit.fields.title, { selector: "input" });
      change(ja.edit.fields.title, "一覧で直した");
      save(ja.edit.actions.apply);

      await waitFor(() => expect(onDone).toHaveBeenCalledOnce());
      const draft = onDone.mock.calls[0]?.[0] as NewBookDraft;
      expect(draft.changes.find((c) => c.field === "title")).toEqual({
        field: "title",
        value: "一覧で直した",
        origin: MANUAL,
      });
      expect(calls("create_books_from_drafts")).toHaveLength(0);
      await waitFor(() => expect(useEditorStore.getState().request).toBeNull());
    });
  });
});
