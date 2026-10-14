// スキャン登録画面の流れ（AT-05〜09・11〜13）。キー入力 → classify_code（モック）→ ペアリング → 確定 → フォーム／一覧。
// Tauri の invoke だけを差し替え、ipc.ts のエラー変換や、画面・ストアまで通す。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ScanCode } from "@/bindings/ScanCode";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";
import { resetLookupsForTest } from "@/features/scanScreen/lookup";
import { ScanScreen } from "@/features/scanScreen/ScanScreen";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ISBN_A, ISBN_B, makeResult } from "@/features/scanScreen/testData";
import { ja } from "@/i18n/ja";
import { useEditorStore } from "@/stores/editorStore";
import { useScreenStore } from "@/stores/screenStore";
import { useSettingsStore } from "@/stores/settingsStore";

const invoke = vi.hoisted(() => vi.fn());
const isTauri = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

const playSound = vi.hoisted(() => vi.fn<(kind: string) => void>());
vi.mock("@/features/scan/playSound", () => ({ playSound }));

const chooseOpenFile = vi.hoisted(() => vi.fn());
vi.mock("@/lib/dialog", () => ({
  chooseOpenFile,
  chooseSavePath: vi.fn(),
  chooseDirectory: vi.fn(),
}));

const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), info: vi.fn() }));
vi.mock("sonner", () => ({ toast, Toaster: () => null }));

type Handler = (args: Record<string, unknown> | undefined) => unknown;

const SECOND_RAW = "1920193007008";

/** classify_code の代わり。この画面のテストで使う数字列だけを判定する。 */
function classify(raw: string): ScanCode {
  if (raw.startsWith("978")) return { kind: "isbn13", raw, isbn13: raw, isbn10: null };
  if (raw.startsWith("192")) {
    const price = Number(raw.slice(7, 12));
    return {
      kind: "secondTier",
      raw,
      cCode: raw.slice(3, 7),
      listPrice: price === 0 ? null : price,
    };
  }
  if (raw.startsWith("491")) return { kind: "magazine", raw };
  return { kind: "invalid", raw, reason: "format" };
}

let handlers: Record<string, unknown>;

function route(extra: Record<string, unknown> = {}) {
  handlers = {
    classify_code: (args: { raw: string } | undefined) => classify(args?.raw ?? ""),
    find_by_isbn: [],
    fetch_metadata: (args: { isbn13: string } | undefined) =>
      makeResult(args?.isbn13 ?? "", { title: "こころ" }),
    c_code_table: {
      audience: [],
      format: [],
      contentMajor: [],
      content: [{ code: "93", name: "日本文学、小説・物語" }],
    },
    list_locations: [
      { id: "loc1", name: "自宅", parentId: null, sortOrder: 0, path: "自宅", depth: 0 },
    ],
    list_tags: [],
    purchase_store_suggestions: [],
    ...extra,
  };
}

function rejectWith(payload: { kind: string; message: string }): Promise<never> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
  return Promise.reject(payload);
}

function installInvoke() {
  invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (!(command in handlers)) {
      return rejectWith({ kind: "internal", message: `未設定のコマンド: ${command}` });
    }
    const handler = handlers[command];
    return Promise.resolve(typeof handler === "function" ? (handler as Handler)(args) : handler);
  });
}

const calls = (command: string) => invoke.mock.calls.filter(([name]) => name === command);

function renderScreen() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ScanScreen />
    </QueryClientProvider>,
  );
}

interface KeyInit {
  key?: string;
  isComposing?: boolean;
}

/** keydown を発火して、既定の動作が止められたかを返す。 */
function press(code: string, init: KeyInit = {}, target: EventTarget = document.body): boolean {
  const event = new KeyboardEvent("keydown", {
    code,
    key: init.key ?? code,
    isComposing: init.isComposing ?? false,
    bubbles: true,
    cancelable: true,
  });
  act(() => {
    target.dispatchEvent(event);
  });
  return event.defaultPrevented;
}

/** リーダーの入力（数字列＋Enter）。ime が true なら、日本語入力がオンのときのキー（key が Process）。 */
function scan(digits: string, options: { ime?: boolean } = {}): boolean[] {
  const prevented: boolean[] = [];
  for (const digit of digits) {
    prevented.push(
      press(`Digit${digit}`, options.ime ? { key: "Process", isComposing: true } : { key: digit }),
    );
  }
  press("Enter", options.ime ? { key: "Process", isComposing: true } : {});
  return prevented;
}

const state = () => screen.getByRole("status", { name: ja.scan.screen.stateLabel });

async function flush() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  invoke.mockReset();
  playSound.mockReset();
  chooseOpenFile.mockReset();
  toast.error.mockReset();
  toast.success.mockReset();
  toast.info.mockReset();
  isTauri.mockReturnValue(true);
  useScanSessionStore.getState().reset();
  useScanSessionStore.setState({ mode: "single" });
  resetLookupsForTest();
  useEditorStore.setState({ request: null });
  useScreenStore.setState({ screen: "scan", handoff: null });
  useSettingsStore.setState({ scan: { ...DEFAULT_SCAN_SETTINGS } });
  route();
  installInvoke();
});

describe("読み取りの状態の表示（9.3）", () => {
  it("待機中 → ISBN 保留中（残り N 秒）→ 2段目 保留中（逆順）", async () => {
    renderScreen();
    expect(state()).toHaveTextContent(ja.scan.state.idle);

    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    expect(
      screen.getByRole("button", { name: new RegExp(ja.scan.screen.commit.button) }),
    ).toBeEnabled();

    // 確定して待機中に戻す
    fireEvent.click(screen.getByRole("button", { name: new RegExp(ja.scan.screen.commit.button) }));
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.idle));
  });

  it("2段目を先に読むと、2段目 保留中になり、Cコードと価格を表示する", async () => {
    renderScreen();
    scan(SECOND_RAW);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.secondHeld(10)));
    // 直前に読んだ本の欄に、Cコードの名称と価格を出す
    expect(await screen.findByText(/0193\s日本文学/)).toBeInTheDocument();
    expect(screen.getByText("700円")).toBeInTheDocument();
  });

  it("メイン画面へ戻るボタンで、メイン画面に戻る", () => {
    renderScreen();
    fireEvent.click(screen.getByRole("button", { name: ja.scan.screen.back }));
    expect(useScreenStore.getState().screen).toBe("main");
  });

  it("ブラウザ（Tauri の外）では、使えない旨を出す", () => {
    isTauri.mockReturnValue(false);
    renderScreen();
    expect(screen.getByText(ja.scan.screen.browserNotice)).toBeInTheDocument();
  });
});

describe("1 冊ずつ登録（FR-SCAN-13）", () => {
  it("AT-05: ISBN → 2段目で 1 冊として確定し、Cコードと価格の入った下書きでフォームを開く", async () => {
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(calls("fetch_metadata")).toHaveLength(1));
    // 書誌取得は、2段目を待たずに始まる（2段目は付けない）
    expect(calls("fetch_metadata")[0]![1]).toEqual({ isbn13: ISBN_A, secondTier: undefined });

    scan(SECOND_RAW);
    await waitFor(() => expect(useEditorStore.getState().request).not.toBeNull());

    const request = useEditorStore.getState().request;
    expect(request?.mode).toBe("new");
    if (request?.mode !== "new") return;
    expect(request.source.isbn13).toBe(ISBN_A);
    expect(request.source.secondTier).toEqual({ cCode: "0193", listPrice: 700 });
    expect(request.source.fetched?.draft.fields.c_code).toEqual({
      value: "0193",
      origin: { kind: "barcode" },
    });
    expect(request.source.fetched?.draft.fields.list_price?.value).toBe(700);
    expect(request.source.fetched?.draft.fields.title?.value).toBe("こころ");
    expect(state()).toHaveTextContent(ja.scan.state.idle);
    expect(playSound).toHaveBeenCalledWith("success");
    // 取得は 1 回だけ
    expect(calls("fetch_metadata")).toHaveLength(1);
    expect(calls("find_by_isbn")[0]![1]).toEqual({ isbn: ISBN_A });
  });

  it("AT-06: 2段目 → ISBN でも、同じ結果になる", async () => {
    renderScreen();
    scan(SECOND_RAW);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.secondHeld(10)));
    scan(ISBN_A);

    await waitFor(() => expect(useEditorStore.getState().request).not.toBeNull());
    const request = useEditorStore.getState().request;
    if (request?.mode !== "new") throw new Error("new ではありません");
    expect(request.source.isbn13).toBe(ISBN_A);
    expect(request.source.secondTier).toEqual({ cCode: "0193", listPrice: 700 });
    expect(request.source.fetched?.draft.fields.c_code?.value).toBe("0193");
    // 直前に読んだ本として、2段目の値を出している
    expect(screen.getAllByText(/0193/).length).toBeGreaterThan(0);
  });

  it("AT-07: ISBN → 別の ISBN で 2 冊として扱われ、1 冊目は 2段目なし", async () => {
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(ISBN_B);

    await waitFor(() => expect(useEditorStore.getState().request).not.toBeNull());
    const first = useEditorStore.getState().request;
    if (first?.mode !== "new") throw new Error("new ではありません");
    expect(first.source.isbn13).toBe(ISBN_A);
    expect(first.source.secondTier).toBeUndefined();
    // 2 冊目は保留中のまま
    expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10));

    // 1 冊目のフォームを閉じたあと、2 冊目を確定キーで確定すると、2 冊目のフォームが開く
    act(() => useEditorStore.getState().close());
    await flush();
    press("Space");
    await waitFor(() => expect(useEditorStore.getState().request).not.toBeNull());
    const second = useEditorStore.getState().request;
    if (second?.mode !== "new") throw new Error("new ではありません");
    expect(second.source.isbn13).toBe(ISBN_B);
  });

  it("AT-08: 同じ ISBN を続けて 2 回読むと、2 回目は無視されて注意音", async () => {
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(ISBN_A);
    await waitFor(() => expect(playSound).toHaveBeenCalledWith("attention"));

    expect(playSound.mock.calls.map(([kind]) => kind)).toEqual(["success", "attention"]);
    expect(calls("fetch_metadata")).toHaveLength(1);
    expect(useEditorStore.getState().request).toBeNull();
    expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10));
  });

  it("AT-09: 日本語入力がオンのまま（isComposing、key が Process）でも、同じように読める。数字キーは止める", async () => {
    renderScreen();
    const prevented = scan(ISBN_A, { ime: true });
    expect(prevented.every(Boolean)).toBe(true);
    await waitFor(() => expect(calls("classify_code")).toHaveLength(1));
    expect(calls("classify_code")[0]![1]).toEqual({ raw: ISBN_A });
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
  });

  it("雑誌など受け付けないバーコードは、エラー音とメッセージ。保留中の本は壊さない", async () => {
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan("4910000000001");
    expect(await screen.findByText("雑誌は非対応です")).toBeInTheDocument();
    expect(playSound).toHaveBeenLastCalledWith("error");
    expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10));
  });

  it("判定（classify_code）が失敗したら、エラー音と理由を通知する", async () => {
    route({
      classify_code: () => rejectWith({ kind: "internal", message: "判定できません" }),
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("判定できません"));
    expect(playSound).toHaveBeenCalledWith("error");
    expect(state()).toHaveTextContent(ja.scan.state.idle);
  });

  it("フォームを開いている間は、読み取りを止める", async () => {
    renderScreen();
    act(() => useEditorStore.getState().openNew({}));
    scan(ISBN_A);
    await flush();
    expect(calls("classify_code")).toHaveLength(0);
    expect(screen.getByText(ja.scan.screen.paused)).toBeInTheDocument();

    act(() => useEditorStore.getState().close());
    scan(ISBN_A);
    await waitFor(() => expect(calls("classify_code")).toHaveLength(1));
  });

  it("手入力欄: 全角の数字を半角にして、リーダーと同じ流れに渡す", async () => {
    renderScreen();
    const input = screen.getByLabelText(ja.scan.manualInput.label);
    fireEvent.change(input, { target: { value: "９７８４００３１０１１１７" } });
    fireEvent.submit(input.closest("form")!);
    await waitFor(() => expect(calls("classify_code")[0]?.[1]).toEqual({ raw: ISBN_A }));
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
  });

  it("メイン画面から引き渡されたバーコードを、読んだものとして処理する（FR-SCAN-10）", async () => {
    useScreenStore.getState().showScan(ISBN_A);
    renderScreen();
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    expect(calls("classify_code")[0]![1]).toEqual({ raw: ISBN_A });
    expect(useScreenStore.getState().handoff).toBeNull();
  });
});

describe("重複と欲しい本", () => {
  const owned = {
    id: "b1",
    title: "こころ",
    ownership: "owned",
    locationId: "loc1",
    locationPath: "自宅 > 書斎の本棚 > 3段目",
    deleted: false,
  };

  it("AT-12: 所有済みなら注意音と「所有済み: 書名 ／ 保管場所」。1 冊ずつではフォームを開かない", async () => {
    route({ find_by_isbn: [owned] });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");

    expect(
      await screen.findByText("所有済み: こころ ／ 自宅 > 書斎の本棚 > 3段目"),
    ).toBeInTheDocument();
    expect(playSound).toHaveBeenCalledWith("attention");
    expect(useEditorStore.getState().request).toBeNull();
  });

  it("「2 冊目として登録」で、フォームを開く（FR-BOOK-10）", async () => {
    route({ find_by_isbn: [owned] });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(SECOND_RAW);
    fireEvent.click(await screen.findByRole("button", { name: ja.scan.result.registerSecond }));

    await waitFor(() => expect(useEditorStore.getState().request).not.toBeNull());
    const request = useEditorStore.getState().request;
    if (request?.mode !== "new") throw new Error("new ではありません");
    expect(request.source.isbn13).toBe(ISBN_A);
    expect(request.source.secondTier).toEqual({ cCode: "0193", listPrice: 700 });
  });

  const wanted = { ...owned, id: "w1", title: "欲しかった本", ownership: "wanted" };

  it("AT-13: 欲しい本なら「所有に移しますか？」と確認し、移すと update_book で owned にする。新しい本は作らない", async () => {
    route({
      find_by_isbn: [wanted],
      update_book: { id: "w1", written: ["ownership", "purchased_on"], skippedManual: [] },
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(ja.scan.wantedDialog.title)).toBeInTheDocument();
    expect(within(dialog).getByText(/欲しかった本/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: ja.scan.wantedDialog.confirm }));

    await waitFor(() => expect(calls("update_book")).toHaveLength(1));
    expect(calls("update_book")[0]![1]).toEqual({
      id: "w1",
      changes: [{ field: "ownership", value: "owned", origin: { kind: "manual" } }],
      options: undefined,
    });
    expect(await screen.findByText("所有に移しました: 欲しかった本")).toBeInTheDocument();
    expect(useEditorStore.getState().request).toBeNull();
    expect(calls("create_books_from_drafts")).toHaveLength(0);
    expect(calls("create_book")).toHaveLength(0);
  });

  it("「移さない」なら、何も変えない", async () => {
    route({ find_by_isbn: [wanted] });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");

    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: ja.scan.wantedDialog.cancel }));

    expect(await screen.findByText("欲しい本のままにしました: 欲しかった本")).toBeInTheDocument();
    expect(calls("update_book")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("確認のダイアログを開いている間は、読み取りを止める", async () => {
    route({ find_by_isbn: [wanted] });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");
    await screen.findByRole("dialog");

    const before = calls("classify_code").length;
    scan(ISBN_B);
    await flush();
    expect(calls("classify_code")).toHaveLength(before);
  });
});

describe("連続登録（FR-SCAN-14／15）", () => {
  beforeEach(() => {
    useScanSessionStore.setState({ mode: "batch" });
  });

  const rows = () => screen.queryAllByTestId("batch-row");

  async function addTwoBooks() {
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(SECOND_RAW);
    await waitFor(() => expect(rows()).toHaveLength(1));
    scan(ISBN_B);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");
    await waitFor(() => expect(rows()).toHaveLength(2));
  }

  it("読んだ本が一覧に溜まり、取得が終わると「完了」になる。フォームは開かない", async () => {
    renderScreen();
    await addTwoBooks();

    await waitFor(() =>
      expect(rows().map((r) => r.getAttribute("data-status"))).toEqual(["done", "done"]),
    );
    expect(within(rows()[0]!).getByText(ja.scan.batch.status.done)).toBeInTheDocument();
    expect(useEditorStore.getState().request).toBeNull();
    // 1 冊目には 2段目が付いている
    expect(within(rows()[0]!).getByText(/700円/)).toBeInTheDocument();
  });

  it("取得中の行は「取得中」で、取得が終わるまで保存できない", async () => {
    let resolveFetch!: (value: unknown) => void;
    route({
      fetch_metadata: () => new Promise((resolve) => (resolveFetch = resolve)),
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");
    await waitFor(() => expect(rows()).toHaveLength(1));

    expect(rows()[0]).toHaveAttribute("data-status", "loading");
    const save = screen.getByRole("button", { name: /まとめて保存/ });
    expect(save).toBeDisabled();
    expect(screen.getByText(ja.scan.batch.waiting(1))).toBeInTheDocument();

    await act(async () => {
      resolveFetch(makeResult(ISBN_A));
      await Promise.resolve();
    });
    await waitFor(() => expect(save).toBeEnabled());
    expect(rows()[0]).toHaveAttribute("data-status", "done");
  });

  it("AT-11: オフラインでも一覧に入り、「取得待ち」のまま保存される", async () => {
    route({
      fetch_metadata: (args: { isbn13: string } | undefined) =>
        makeResult(args?.isbn13 ?? "", { offline: true }),
      create_books_from_drafts: ["new-1"],
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(SECOND_RAW);
    await waitFor(() => expect(rows()).toHaveLength(1));
    await waitFor(() => expect(rows()[0]).toHaveAttribute("data-status", "pending"));
    expect(within(rows()[0]!).getByText(ja.scan.batch.status.pending)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /まとめて保存/ }));
    await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));

    const drafts = (
      calls("create_books_from_drafts")[0]![1] as {
        drafts: { changes: { field: string; value: unknown }[] }[];
      }
    ).drafts;
    const byField = Object.fromEntries(drafts[0]!.changes.map((c) => [c.field, c.value]));
    expect(byField.meta_status).toBe("pending");
    expect(byField.isbn13).toBe(ISBN_A);
    expect(byField.c_code).toBe("0193");
    expect(byField.list_price).toBe(700);
  });

  it("取得そのものが失敗した行も、ISBN と 2段目だけで保存できる（取得待ち）", async () => {
    route({
      fetch_metadata: () => rejectWith({ kind: "internal", message: "失敗" }),
      create_books_from_drafts: ["new-1"],
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");
    await waitFor(() => expect(rows()[0]).toHaveAttribute("data-status", "pending"));

    fireEvent.click(screen.getByRole("button", { name: /まとめて保存/ }));
    await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));
    const drafts = (
      calls("create_books_from_drafts")[0]![1] as {
        drafts: { changes: { field: string; value: unknown }[] }[];
      }
    ).drafts;
    expect(drafts[0]!.changes.find((c) => c.field === "meta_status")?.value).toBe("pending");
  });

  it("共通値を、保存するとき各行に重ねて 1 回で保存し、一覧を空にする", async () => {
    route({ create_books_from_drafts: ["n1", "n2"] });
    renderScreen();
    await addTwoBooks();
    await waitFor(() =>
      expect(rows().map((r) => r.getAttribute("data-status"))).toEqual(["done", "done"]),
    );

    // 共通値: 保管場所・タグ・所有状態・購入日・購入店
    fireEvent.click(screen.getByRole("combobox", { name: ja.scan.batch.presets.location }));
    fireEvent.click(await screen.findByRole("option", { name: "自宅" }));
    fireEvent.change(screen.getByLabelText(ja.scan.batch.presets.tags), {
      target: { value: "研究/SiC, 小説" },
    });
    fireEvent.change(screen.getByLabelText(ja.scan.batch.presets.purchasedOn), {
      target: { value: "2026-10-14" },
    });
    fireEvent.change(screen.getByLabelText(ja.scan.batch.presets.purchaseStore), {
      target: { value: "紀伊國屋" },
    });

    fireEvent.click(screen.getByRole("button", { name: ja.scan.batch.saveAll(2) }));
    await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));

    const sent = (
      calls("create_books_from_drafts")[0]![1] as {
        drafts: { changes: { field: string; value: unknown; origin: unknown }[] }[];
      }
    ).drafts;
    expect(sent).toHaveLength(2);
    for (const draft of sent) {
      const byField = Object.fromEntries(draft.changes.map((c) => [c.field, c]));
      expect(byField.location_id).toEqual({
        field: "location_id",
        value: "loc1",
        origin: { kind: "manual" },
      });
      expect(byField.tags?.value).toEqual(["研究/SiC", "小説"]);
      expect(byField.purchased_on?.value).toBe("2026-10-14");
      expect(byField.purchase_store?.value).toBe("紀伊國屋");
      expect(byField.title?.origin).toEqual({ kind: "provider", name: "openbd" });
    }
    // 1 冊目には 2段目（由来 barcode）が入っている
    const first = Object.fromEntries(sent[0]!.changes.map((c) => [c.field, c]));
    expect(first.c_code).toEqual({ field: "c_code", value: "0193", origin: { kind: "barcode" } });

    await waitFor(() => expect(rows()).toHaveLength(0));
    expect(toast.success).toHaveBeenCalledWith(ja.scan.batch.saved(2), expect.anything());
    expect(useScanSessionStore.getState().rows).toEqual([]);
  });

  it("所有済みの行は「重複」で、既定では保存の対象から外れる。行ごとに対象に戻せる", async () => {
    route({
      find_by_isbn: (args: { isbn: string } | undefined) =>
        args?.isbn === ISBN_A
          ? [
              {
                id: "b1",
                title: "こころ",
                ownership: "owned",
                locationId: null,
                locationPath: "自宅 > 書斎",
                deleted: false,
              },
            ]
          : [],
      create_books_from_drafts: ["n1"],
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(ISBN_B);
    await waitFor(() => expect(rows()).toHaveLength(1));
    press("Space");
    await waitFor(() => expect(rows()).toHaveLength(2));

    expect(rows()[0]).toHaveAttribute("data-status", "duplicate");
    expect(rows()[0]).toHaveAttribute("data-included", "false");
    expect(rows()[1]).toHaveAttribute("data-included", "true");
    expect(within(rows()[0]!).getByText("所有済み: こころ ／ 自宅 > 書斎")).toBeInTheDocument();
    expect(playSound).toHaveBeenCalledWith("attention");
    expect(screen.getByRole("button", { name: ja.scan.batch.saveAll(1) })).toBeInTheDocument();

    // 対象に戻す
    fireEvent.click(within(rows()[0]!).getByRole("checkbox"));
    await waitFor(() => expect(rows()[0]).toHaveAttribute("data-included", "true"));
    expect(screen.getByRole("button", { name: ja.scan.batch.saveAll(2) })).toBeInTheDocument();
  });

  it("欲しい本は一覧に入れず、確認して所有に移す（連続でも同じ）", async () => {
    route({
      find_by_isbn: [
        {
          id: "w1",
          title: "欲しかった本",
          ownership: "wanted",
          locationId: null,
          locationPath: null,
          deleted: false,
        },
      ],
      update_book: { id: "w1", written: ["ownership"], skippedManual: [] },
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: ja.scan.wantedDialog.confirm }));
    await waitFor(() => expect(calls("update_book")).toHaveLength(1));
    expect(rows()).toHaveLength(0);
  });

  it("同じ ISBN を、別の本をはさんで読むと、2 回目は重複の行になる", async () => {
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(ISBN_B);
    await waitFor(() => expect(rows()).toHaveLength(1));
    scan(ISBN_A);
    await waitFor(() => expect(rows()).toHaveLength(2));
    press("Space");
    await waitFor(() => expect(rows()).toHaveLength(3));

    expect(rows()[2]).toHaveAttribute("data-status", "duplicate");
    expect(rows()[2]).toHaveAttribute("data-included", "false");
    expect(within(rows()[2]!).getByText(ja.scan.batch.duplicate.inBatch)).toBeInTheDocument();
    expect(rows()[0]).toHaveAttribute("data-included", "true");
  });

  it("行の編集: フォームを共通値つきで開き、OK で返った下書きで行を置き換える。保存にはその内容を使う", async () => {
    route({ create_books_from_drafts: ["n1"] });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    scan(SECOND_RAW);
    await waitFor(() => expect(rows()[0]).toHaveAttribute("data-status", "done"));

    fireEvent.change(screen.getByLabelText(ja.scan.batch.presets.purchaseStore), {
      target: { value: "紀伊國屋" },
    });
    fireEvent.click(within(rows()[0]!).getByRole("button", { name: /を編集/ }));

    const request = useEditorStore.getState().request;
    if (request?.mode !== "draft") throw new Error("draft ではありません");
    expect(request.source.isbn13).toBe(ISBN_A);
    expect(request.source.secondTier).toEqual({ cCode: "0193", listPrice: 700 });
    expect(request.source.fetched?.draft.fields.c_code?.value).toBe("0193");
    expect(request.source.presets).toEqual([
      { field: "purchase_store", value: "紀伊國屋", origin: { kind: "manual" } },
    ]);
    expect(request.source.draft).toBeNull();

    // フォームの「OK」
    act(() => {
      request.onDone({
        changes: [
          { field: "isbn13", value: ISBN_A, origin: { kind: "barcode" } },
          { field: "title", value: "直した書名", origin: { kind: "manual" } },
          { field: "purchase_store", value: "別の店", origin: { kind: "manual" } },
          { field: "meta_status", value: "ok", origin: { kind: "manual" } },
        ],
      });
      useEditorStore.getState().close();
    });
    await waitFor(() => expect(screen.getByText("直した書名")).toBeInTheDocument());
    expect(within(rows()[0]!).getByText(ja.scan.batch.editedMark)).toBeInTheDocument();

    // もう一度開くと、直した下書きを渡す
    fireEvent.click(within(rows()[0]!).getByRole("button", { name: /を編集/ }));
    const reopened = useEditorStore.getState().request;
    if (reopened?.mode !== "draft") throw new Error("draft ではありません");
    expect(reopened.source.draft?.changes.find((c) => c.field === "title")?.value).toBe(
      "直した書名",
    );
    act(() => useEditorStore.getState().close());

    fireEvent.click(screen.getByRole("button", { name: ja.scan.batch.saveAll(1) }));
    await waitFor(() => expect(calls("create_books_from_drafts")).toHaveLength(1));
    const sent = (
      calls("create_books_from_drafts")[0]![1] as {
        drafts: { changes: { field: string; value: unknown }[] }[];
      }
    ).drafts;
    const byField = Object.fromEntries(sent[0]!.changes.map((c) => [c.field, c.value]));
    expect(byField.title).toBe("直した書名");
    // 行で直した値は、共通値で消されない
    expect(byField.purchase_store).toBe("別の店");
  });

  it("一覧を空にするときは、確認してから取り除く", async () => {
    renderScreen();
    await addTwoBooks();

    fireEvent.click(screen.getByRole("button", { name: ja.scan.batch.clear }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.scan.batch.clearConfirm.cancel }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(rows()).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: ja.scan.batch.clear }));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: ja.scan.batch.clearConfirm.confirm,
      }),
    );
    await waitFor(() => expect(rows()).toHaveLength(0));
    expect(calls("create_books_from_drafts")).toHaveLength(0);
  });

  it("行を削除できる", async () => {
    renderScreen();
    await addTwoBooks();
    fireEvent.click(within(rows()[0]!).getByRole("button", { name: /を一覧から削除/ }));
    await waitFor(() => expect(rows()).toHaveLength(1));
  });

  it("保存に失敗したら、一覧は残して理由を通知する", async () => {
    route({
      create_books_from_drafts: () => rejectWith({ kind: "validation", message: "入力が不正です" }),
    });
    renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));
    press("Space");
    await waitFor(() => expect(rows()[0]).toHaveAttribute("data-status", "done"));
    fireEvent.click(screen.getByRole("button", { name: /まとめて保存/ }));

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("入力が不正です"));
    expect(rows()).toHaveLength(1);
  });
});

describe("連続登録: 画面まわりの細かい挙動", () => {
  beforeEach(() => {
    useScanSessionStore.setState({ mode: "batch" });
  });

  it("画面を離れるとき、保留中の本があれば確定して一覧に残す（読んだ本を失わない）", async () => {
    const view = renderScreen();
    scan(ISBN_A);
    await waitFor(() => expect(state()).toHaveTextContent(ja.scan.state.isbnHeld(10)));

    view.unmount();
    await waitFor(() => expect(useScanSessionStore.getState().rows).toHaveLength(1));
    expect(useScanSessionStore.getState().rows[0]?.isbn13).toBe(ISBN_A);

    // 画面に戻ると、一覧に残っている
    renderScreen();
    expect(screen.getAllByTestId("batch-row")).toHaveLength(1);
  });

  it("入力欄（手入力欄以外）にフォーカスがあるあいだは、読み取れない旨を出す", async () => {
    renderScreen();
    expect(screen.queryByText(ja.scan.screen.editing)).not.toBeInTheDocument();

    act(() => screen.getByLabelText(ja.scan.batch.presets.tags).focus());
    expect(await screen.findByText(ja.scan.screen.editing)).toBeInTheDocument();

    act(() => screen.getByLabelText(ja.scan.batch.presets.tags).blur());
    await waitFor(() => expect(screen.queryByText(ja.scan.screen.editing)).not.toBeInTheDocument());

    // 手入力欄は、数字を入れて Enter で確定できるので、出さない
    act(() => screen.getByLabelText(ja.scan.manualInput.label).focus());
    await flush();
    expect(screen.queryByText(ja.scan.screen.editing)).not.toBeInTheDocument();
  });

  it("共通値の入力欄で Enter を押すと、フォーカスが外れる（続けてリーダーで読める）", () => {
    renderScreen();
    const tags = screen.getByLabelText(ja.scan.batch.presets.tags);
    act(() => tags.focus());
    expect(tags).toHaveFocus();
    fireEvent.keyDown(tags, { key: "Enter" });
    expect(tags).not.toHaveFocus();
  });

  it("保管場所を選んだあと、フォーカスが選択欄に残らない（リーダーの数字が文字検索に入らない）", async () => {
    renderScreen();
    const trigger = screen.getByRole("combobox", { name: ja.scan.batch.presets.location });
    fireEvent.click(trigger);
    fireEvent.click(await screen.findByRole("option", { name: "自宅" }));
    await waitFor(() => expect(screen.queryByRole("option")).not.toBeInTheDocument());
    expect(trigger).not.toHaveFocus();
  });
});

describe("ISBN 一覧の取り込み（FR-IO-03）", () => {
  beforeEach(() => {
    useScanSessionStore.setState({ mode: "batch" });
  });

  it("ファイルを選んで、確定した本を一覧に足す。2段目が混ざっていればペアにし、取得は順に行う", async () => {
    const codes: ScanCode[] = [
      { kind: "isbn13", raw: ISBN_A, isbn13: ISBN_A, isbn10: null },
      { kind: "secondTier", raw: SECOND_RAW, cCode: "0193", listPrice: 700 },
      { kind: "isbn13", raw: ISBN_B, isbn13: ISBN_B, isbn10: null },
      { kind: "invalid", raw: "123", reason: "format" },
    ];
    chooseOpenFile.mockResolvedValue("/tmp/isbns.txt");
    route({ parse_isbn_list: codes });
    renderScreen();

    fireEvent.click(screen.getByRole("button", { name: ja.scan.import.button }));
    await waitFor(() => expect(screen.queryAllByTestId("batch-row")).toHaveLength(2));

    expect(calls("parse_isbn_list")[0]![1]).toEqual({ path: "/tmp/isbns.txt" });
    const rows = screen.getAllByTestId("batch-row");
    expect(within(rows[0]!).getByText(/700円/)).toBeInTheDocument();
    await waitFor(() =>
      expect(rows.map((r) => r.getAttribute("data-status"))).toEqual(["done", "done"]),
    );
    expect(calls("fetch_metadata").map(([, args]) => (args as { isbn13: string }).isbn13)).toEqual([
      ISBN_A,
      ISBN_B,
    ]);
    expect(toast.success).toHaveBeenCalledWith(
      ja.scan.import.done({ added: 2, duplicates: 0, skipped: 1 }),
    );
  });

  it("所有済みの本は、重複の行として入る", async () => {
    chooseOpenFile.mockResolvedValue("/tmp/isbns.txt");
    route({
      parse_isbn_list: [{ kind: "isbn13", raw: ISBN_A, isbn13: ISBN_A, isbn10: null }],
      find_by_isbn: [
        {
          id: "b1",
          title: "こころ",
          ownership: "owned",
          locationId: null,
          locationPath: null,
          deleted: false,
        },
      ],
    });
    renderScreen();
    fireEvent.click(screen.getByRole("button", { name: ja.scan.import.button }));
    await waitFor(() => expect(screen.queryAllByTestId("batch-row")).toHaveLength(1));
    expect(screen.getByTestId("batch-row")).toHaveAttribute("data-status", "duplicate");
    expect(screen.getByTestId("batch-row")).toHaveAttribute("data-included", "false");
  });

  it("ファイルを選ばなければ、何もしない", async () => {
    chooseOpenFile.mockResolvedValue(null);
    renderScreen();
    fireEvent.click(screen.getByRole("button", { name: ja.scan.import.button }));
    await flush();
    expect(calls("parse_isbn_list")).toHaveLength(0);
  });
});

describe("「1 冊ずつ」と「連続」の切り替え", () => {
  it("切り替えると、連続のときだけ共通値と一覧を出す。選んだ方法を覚える", () => {
    useScanSessionStore.setState({ mode: "single" });
    renderScreen();
    expect(screen.queryByTestId("batch-panel")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: new RegExp(ja.scan.screen.mode.batch) }));
    expect(screen.getByTestId("batch-panel")).toBeInTheDocument();
    expect(useScanSessionStore.getState().mode).toBe("batch");
    expect(JSON.parse(localStorage.getItem("bibcontainer.scanMode") ?? "{}")).toMatchObject({
      state: { mode: "batch" },
    });

    fireEvent.click(screen.getByRole("radio", { name: new RegExp(ja.scan.screen.mode.single) }));
    expect(screen.queryByTestId("batch-panel")).not.toBeInTheDocument();
  });
});
