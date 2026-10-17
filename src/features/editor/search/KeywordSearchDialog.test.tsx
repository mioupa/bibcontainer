import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import type { ProviderId } from "@/bindings/ProviderId";
import type { ProviderSearchResult } from "@/bindings/ProviderSearchResult";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { SearchHit } from "@/bindings/SearchHit";
import { KeywordSearchDialog } from "@/features/editor/search/KeywordSearchDialog";
import { ja } from "@/i18n/ja";
import { useModalStore } from "@/lib/useModalGuard";
import { makeHit, makeProviderStatus, makeSearchResult } from "@/test/search";

const invoke = vi.hoisted(() => vi.fn());
const isTauri = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
);
vi.mock("sonner", () => ({ toast }));

const text = ja.edit.search;

/** Rust の AppError（`{ kind, message }`）で失敗する。 */
function rejectWith(payload: { kind: string; message: string }): Promise<never> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
  return Promise.reject(payload);
}

/** `search_metadata` の 1 回の呼び出し。答えは、テストが好きなときに返す。 */
interface Pending {
  provider: ProviderId;
  title: unknown;
  author: unknown;
  resolve: (result: ProviderSearchResult) => void;
  reject: (error: { kind: string; message: string }) => void;
}

let providers: ProviderStatus[];
let pending: Pending[];
let openLinkResult: () => Promise<null>;

const ALL_PROVIDERS = (): ProviderStatus[] => [
  makeProviderStatus("openbd", { displayName: "openBD", supportsSearch: false }),
  makeProviderStatus("ndl", { displayName: "NDLサーチ" }),
  makeProviderStatus("google", {
    displayName: "Google Books",
    auth: { kind: "required", label: "API キー" },
    hasKey: true,
  }),
  makeProviderStatus("openlibrary", { displayName: "Open Library" }),
];

beforeEach(() => {
  invoke.mockReset();
  toast.error.mockReset();
  isTauri.mockReturnValue(true);
  useModalStore.setState({ count: 0 });
  providers = ALL_PROVIDERS();
  pending = [];
  openLinkResult = () => Promise.resolve(null);
  invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "list_providers":
        return Promise.resolve(providers);
      case "search_metadata":
        return new Promise<ProviderSearchResult>((resolve, reject) => {
          pending.push({
            provider: args?.provider as ProviderId,
            title: args?.title,
            author: args?.author,
            resolve,
            reject,
          });
        });
      case "open_link":
        return openLinkResult();
      default:
        return rejectWith({ kind: "internal", message: `未設定のコマンド: ${command}` });
    }
  });
});

const calls = (command: string) =>
  invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args as unknown);

/** 取得元に出した検索のうち、`nth` 番目（既定は最後）に答える。 */
async function answer(provider: ProviderId, result: ProviderSearchResult, nth = -1) {
  const call = pending.filter((p) => p.provider === provider).at(nth);
  if (call === undefined) throw new Error(`${provider} への検索がありません`);
  await act(async () => {
    call.resolve(result);
    await Promise.resolve();
  });
}

async function fail(provider: ProviderId, error: { kind: string; message: string }) {
  const call = pending.filter((p) => p.provider === provider).at(-1);
  if (call === undefined) throw new Error(`${provider} への検索がありません`);
  await act(async () => {
    call.reject(error);
    await Promise.resolve();
  });
}

interface Handlers {
  onChoose: Mock<(hit: SearchHit) => void>;
  onClose: Mock<() => void>;
}

function renderDialog(initialTitle = "", initialAuthor = "") {
  const handlers: Handlers = {
    onChoose: vi.fn<(hit: SearchHit) => void>(),
    onClose: vi.fn<() => void>(),
  };
  const { unmount } = render(
    <KeywordSearchDialog initialTitle={initialTitle} initialAuthor={initialAuthor} {...handlers} />,
  );
  return { ...handlers, unmount };
}

const dialog = () => screen.getByRole("dialog", { name: text.title });
const titleInput = () => within(dialog()).getByLabelText<HTMLInputElement>(text.titleLabel);
const authorInput = () => within(dialog()).getByLabelText<HTMLInputElement>(text.authorLabel);
const searchButton = () => within(dialog()).getByRole("button", { name: text.submit });
const chooseButton = () => within(dialog()).getByRole("button", { name: text.choose });
const options = () => within(dialog()).queryAllByRole("option");
const sectionTitles = () =>
  within(dialog())
    .queryAllByRole("group")
    .map((group) => group.querySelector("[id^='kw-section-']")?.textContent ?? "");
const selectedTitle = () =>
  options()
    .find((o) => o.getAttribute("aria-selected") === "true")
    ?.querySelector("span")?.textContent ?? null;

const NDL_HITS = [
  makeHit("ndl", {
    isbn13: "9784003101117",
    title: "こころ",
    authors: [{ name: "夏目漱石", kana: null, role: "著" }],
    publisher: "岩波書店",
    publishedOn: "2019-05",
    volume: "上",
  }),
  makeHit("ndl", {
    isbn13: null,
    title: "こころ（古い本）",
    subtitle: "先生の遺書",
    link: "https://ndlsearch.ndl.go.jp/books/R1-I2",
  }),
];
const GOOGLE_HITS = [
  makeHit("google", {
    isbn13: "9780132350884",
    title: "Zebra Code",
    link: "https://books.google.com/books?id=g1",
  }),
  makeHit("google", {
    isbn13: "9780134685991",
    title: "Alpha Java",
    link: "https://books.google.com/books?id=g2",
  }),
];
const OL_HITS = [
  makeHit("openlibrary", {
    isbn13: "9780192834102",
    title: "Kokoro",
    link: "https://openlibrary.org/books/OL1M",
  }),
];

/** 3 つの取得元に、それぞれ答える。 */
async function answerAll() {
  await answer("ndl", makeSearchResult("ndl", { hits: NDL_HITS, total: 42 }));
  await answer("google", makeSearchResult("google", { hits: GOOGLE_HITS, total: 2 }));
  await answer("openlibrary", makeSearchResult("openlibrary", { hits: OL_HITS, total: 1 }));
}

describe("キーワード検索ダイアログ", () => {
  describe("探し始め", () => {
    it("書名か著者が入った状態で開くと、すぐ探す（取得元ごとに別々の呼び出し。入力は整えて渡す）", async () => {
      renderDialog("  こころ ", "");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      expect(calls("search_metadata")).toEqual([
        { provider: "ndl", title: "こころ", author: null },
        { provider: "google", title: "こころ", author: null },
        { provider: "openlibrary", title: "こころ", author: null },
      ]);
      expect(titleInput().value).toBe("  こころ ");
    });

    it("著者だけでも探す", async () => {
      renderDialog("", "夏目漱石");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      expect(calls("search_metadata")[0]).toEqual({
        provider: "ndl",
        title: null,
        author: "夏目漱石",
      });
    });

    it("どちらも空なら、探さない。ボタンは押せず、Enter でも送らず、書名の欄にフォーカスがある", async () => {
      renderDialog();
      await waitFor(() => expect(calls("list_providers")).toHaveLength(1));
      expect(await screen.findByText(text.idle)).toBeInTheDocument();
      expect(searchButton()).toBeDisabled();
      expect(titleInput()).toHaveFocus();
      fireEvent.keyDown(titleInput(), { key: "Enter" });
      fireEvent.change(authorInput(), { target: { value: "   " } });
      expect(searchButton()).toBeDisabled();
      fireEvent.keyDown(authorInput(), { key: "Enter" });
      expect(calls("search_metadata")).toHaveLength(0);
    });

    it("入力しただけでは探さない。Enter か「探す」ボタンで探す（変換中の Enter では探さない）", async () => {
      renderDialog();
      await waitFor(() => expect(calls("list_providers")).toHaveLength(1));
      fireEvent.change(titleInput(), { target: { value: "Clean" } });
      fireEvent.change(titleInput(), { target: { value: "Clean Code" } });
      await waitFor(() => expect(searchButton()).toBeEnabled());
      expect(calls("search_metadata")).toHaveLength(0);

      // 日本語入力の変換を確定する Enter は、探さない
      fireEvent.keyDown(titleInput(), { key: "Enter", isComposing: true });
      fireEvent.keyDown(titleInput(), { key: "Enter", keyCode: 229 });
      expect(calls("search_metadata")).toHaveLength(0);

      fireEvent.keyDown(titleInput(), { key: "Enter" });
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      fireEvent.change(authorInput(), { target: { value: "Martin" } });
      fireEvent.click(searchButton());
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(6));
      expect(calls("search_metadata")[5]).toEqual({
        provider: "ndl",
        title: "Clean Code",
        author: "Martin",
      });
    });

    it("ダイアログを開いている間は、画面全体のショートカットを止める（useModalGuard）", () => {
      renderDialog("こころ");
      expect(useModalStore.getState().count).toBe(1);
    });
  });

  describe("取得元ごとの見出し", () => {
    it("かな・漢字の検索語は NDL、Google、Open Library の順。英語だけなら Google、Open Library、NDL の順", async () => {
      const { unmount } = renderDialog("こころ");
      await waitFor(() =>
        expect(sectionTitles()).toEqual(["NDLサーチ", text.googleHeading, "Open Library"]),
      );
      unmount();

      pending = [];
      renderDialog("Clean Code");
      await waitFor(() =>
        expect(sectionTitles()).toEqual([text.googleHeading, "Open Library", "NDLサーチ"]),
      );
    });

    it("無効な取得元と、検索に対応しない取得元（openBD）は出さない", async () => {
      providers = [
        makeProviderStatus("ndl", { displayName: "NDLサーチ", enabled: false, active: false }),
        makeProviderStatus("google", { displayName: "Google Books" }),
        makeProviderStatus("openbd", { displayName: "openBD", supportsSearch: false }),
      ];
      renderDialog("Clean Code");
      await waitFor(() => expect(sectionTitles()).toEqual([text.googleHeading]));
      expect(calls("search_metadata")).toEqual([
        { provider: "google", title: "Clean Code", author: null },
      ]);
    });

    it("取得元ごとに別々に埋まる（先に届いた取得元から、状態と件数が出る）", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      expect(within(dialog()).getAllByText(text.searching)).toHaveLength(3);

      await answer("openlibrary", makeSearchResult("openlibrary", { hits: OL_HITS, total: 1 }));
      expect(within(dialog()).getAllByText(text.searching)).toHaveLength(2);
      expect(options().map((o) => o.textContent)).toEqual([expect.stringContaining("Kokoro")]);

      await answer("ndl", makeSearchResult("ndl", { hits: NDL_HITS, total: 42 }));
      expect(within(dialog()).getByText("全 42 件のうち 2 件")).toBeInTheDocument();
      expect(within(dialog()).getAllByText(text.searching)).toHaveLength(1);
      // 表示の順（NDL → Google → Open Library）。Google が来ていなくても、NDL の結果が先
      expect(options().map((o) => o.querySelector("span")?.textContent)).toEqual([
        "こころ",
        "こころ（古い本）",
        "Kokoro",
      ]);
    });

    it("API キーがなくて使えない取得元は、理由を出して、探さない", async () => {
      providers = ALL_PROVIDERS().map((p) =>
        p.id === "google" ? { ...p, hasKey: false, active: false } : p,
      );
      renderDialog("Clean Code");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(2));
      expect(calls("search_metadata").map((c) => (c as { provider: string }).provider)).toEqual([
        "openlibrary",
        "ndl",
      ]);
      expect(sectionTitles()).toContain(text.googleHeading);
      expect(within(dialog()).getByText(text.noKey)).toBeInTheDocument();
      expect(text.noKey).toContain("設定の「取得元」");
      // キーがないので、Google の画像は、検索欄のそばにも見出しにも出さない
      expect(screen.queryByRole("img", { name: text.poweredByGoogle })).not.toBeInTheDocument();
    });

    it("コマンドの失敗（validation）は、その取得元の見出しだけに出す。ほかの取得元は、そのまま出る", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await fail("google", {
        kind: "validation",
        message: "Google Books は使えません（API キー）",
      });
      await answer("ndl", makeSearchResult("ndl", { hits: NDL_HITS }));
      await answer("openlibrary", makeSearchResult("openlibrary", { hits: OL_HITS }));
      const google = within(dialog()).getByRole("group", { name: text.googleHeading });
      expect(within(google).getByText("Google Books は使えません（API キー）")).toBeInTheDocument();
      expect(within(google).queryAllByRole("option")).toHaveLength(0);
      expect(options()).toHaveLength(3);
    });

    it("見つからない・通信できない・HTTP エラー・応答を読めない、を取得元ごとに出す", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answer("ndl", makeSearchResult("ndl", { outcome: { kind: "notFound" } }));
      await answer(
        "google",
        makeSearchResult("google", { outcome: { kind: "httpError", status: 503, message: "x" } }),
      );
      await answer(
        "openlibrary",
        makeSearchResult("openlibrary", { outcome: { kind: "networkError", message: "x" } }),
      );
      expect(within(dialog()).getByText(text.notFound)).toBeInTheDocument();
      expect(within(dialog()).getByText(text.httpError(503))).toBeInTheDocument();
      expect(within(dialog()).getByText(text.networkError)).toBeInTheDocument();
      // 失敗は目立つ色（見つからないは、そうでない）
      expect(within(dialog()).getByText(text.httpError(503))).toHaveClass("text-destructive");
      expect(within(dialog()).getByText(text.notFound)).not.toHaveClass("text-destructive");
      expect(options()).toHaveLength(0);
    });

    it("探し直すと、前の結果を消し、前の検索の遅い答えは捨てる", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answer("ndl", makeSearchResult("ndl", { hits: NDL_HITS }));
      expect(options()).toHaveLength(2);

      fireEvent.change(titleInput(), { target: { value: "吾輩は猫である" } });
      fireEvent.keyDown(titleInput(), { key: "Enter" });
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(6));
      // 前の結果は消えて、検索中に戻る
      expect(options()).toHaveLength(0);
      expect(within(dialog()).getAllByText(text.searching)).toHaveLength(3);

      // 前の検索の Google・Open Library の答えが、あとから届いても、出さない
      await answer("google", makeSearchResult("google", { hits: GOOGLE_HITS }), 0);
      await answer("openlibrary", makeSearchResult("openlibrary", { hits: OL_HITS }), 0);
      expect(options()).toHaveLength(0);
      expect(within(dialog()).getAllByText(text.searching)).toHaveLength(3);

      // 今の検索の答えは出る
      const fresh = makeHit("ndl", { isbn13: null, title: "吾輩は猫である" });
      await answer("ndl", makeSearchResult("ndl", { hits: [fresh] }), 1);
      expect(options().map((o) => o.querySelector("span")?.textContent)).toEqual([
        "吾輩は猫である",
      ]);
    });
  });

  describe("Google Books の表示", () => {
    const searchBox = () => within(screen.getByRole("search", { name: text.searchLabel }));

    it("Google Books を探すときは、検索欄のそばにも「Powered by Google」を出す（結果が届く前から。白の地に、そのままの大きさ）", async () => {
      renderDialog();
      await waitFor(() => expect(calls("list_providers")).toHaveLength(1));
      const image = await searchBox().findByRole("img", { name: text.poweredByGoogle });
      expect(image).toHaveAttribute("src", expect.stringContaining("png"));
      expect(image).toHaveAttribute("width", "62");
      expect(image).toHaveAttribute("height", "30");
      expect(image.parentElement).toHaveClass("bg-white");
      // 入力欄と「探す」ボタンと同じ領域にある。結果の見出しの画像は、まだない
      expect(searchBox().getByLabelText(text.titleLabel)).toBeInTheDocument();
      expect(searchBox().getByRole("button", { name: text.submit })).toBeInTheDocument();
      expect(within(dialog()).getAllByRole("img")).toHaveLength(1);
    });

    it("Google Books を探さないとき（キーがない・無効）は、検索欄のそばに出さない", async () => {
      providers = ALL_PROVIDERS().map((p) =>
        p.id === "google" ? { ...p, hasKey: false, active: false } : p,
      );
      const { unmount } = renderDialog();
      await waitFor(() => expect(calls("list_providers")).toHaveLength(1));
      expect(await screen.findByText(text.idle)).toBeInTheDocument();
      expect(searchBox().queryByRole("img")).not.toBeInTheDocument();
      unmount();

      providers = ALL_PROVIDERS().map((p) =>
        p.id === "google" ? { ...p, enabled: false, active: false } : p,
      );
      renderDialog();
      expect(await screen.findByText(text.idle)).toBeInTheDocument();
      expect(searchBox().queryByRole("img")).not.toBeInTheDocument();
    });

    it("「Powered by Google」の画像を、結果の見出しに添える。各行に「Google Books で見る」を出し、結果の順を変えない", async () => {
      renderDialog("Clean Code");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answerAll();

      const google = within(dialog()).getByRole("group", { name: text.googleHeading });
      const image = within(google).getByRole("img", { name: text.poweredByGoogle });
      expect(image).toHaveAttribute("src", expect.stringContaining("png"));
      expect(image).toHaveAttribute("width", "62");
      expect(image).toHaveAttribute("height", "30");
      // 暗いテーマでも読めるよう、白の地に置く
      expect(image.parentElement).toHaveClass("bg-white");

      const rows = within(google).getAllByRole("option");
      // 取得元が返した順のまま（書名順にしない）
      expect(rows.map((row) => row.querySelector("span")?.textContent)).toEqual([
        "Zebra Code",
        "Alpha Java",
      ]);
      for (const row of rows) {
        expect(within(row).getByRole("button", { name: text.viewOn.google })).toBeInTheDocument();
      }
      // 画像は、検索欄のそばと Google の結果の見出しの 2 か所だけ（結果の行には出さない）
      expect(within(dialog()).getAllByRole("img", { name: text.poweredByGoogle })).toHaveLength(2);
      expect(within(dialog()).getAllByRole("img")).toHaveLength(2);
    });

    it("NDL・Open Library の行には、「NDL サーチで見る」「Open Library で見る」。リンクがない行には出さない", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answerAll();
      const [first, second] = options();
      // NDL の 1 件目は link なし、2 件目は link あり
      expect(within(first as HTMLElement).queryByRole("button")).not.toBeInTheDocument();
      expect(
        within(second as HTMLElement).getByRole("button", { name: text.viewOn.ndl }),
      ).toBeInTheDocument();
      expect(
        within(dialog()).getAllByRole("button", { name: text.viewOn.openlibrary }),
      ).toHaveLength(1);
    });

    it("リンクを押すと、その URL で open_link を呼ぶ。行は選ばず、入れもしない", async () => {
      const { onChoose } = renderDialog("Clean Code");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answerAll();
      const google = within(dialog()).getByRole("group", { name: text.googleHeading });
      const links = within(google).getAllByRole("button", { name: text.viewOn.google });
      fireEvent.click(links[1] as HTMLElement);
      fireEvent.doubleClick(links[1] as HTMLElement);
      await waitFor(() => expect(calls("open_link")).toHaveLength(1));
      expect(calls("open_link")[0]).toEqual({ url: "https://books.google.com/books?id=g2" });
      expect(options().every((o) => o.getAttribute("aria-selected") === "false")).toBe(true);
      expect(chooseButton()).toBeDisabled();
      expect(onChoose).not.toHaveBeenCalled();
    });

    it("リンクを開けなかったときは、通知で理由を出す", async () => {
      openLinkResult = () => rejectWith({ kind: "validation", message: "このリンクは開けません" });
      renderDialog("Clean Code");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answerAll();
      fireEvent.click(
        within(dialog()).getAllByRole("button", { name: text.viewOn.google })[0] as HTMLElement,
      );
      await waitFor(() => expect(toast.error).toHaveBeenCalledWith("このリンクは開けません"));
    });
  });

  describe("結果の行", () => {
    it("書名・副書名・巻次、著者（役割つき）、出版社 · 発行日、ISBN（なければ「ISBN なし」）を文字だけで出す", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answer("ndl", makeSearchResult("ndl", { hits: NDL_HITS }));
      const [first, second] = options() as [HTMLElement, HTMLElement];
      expect(first).toHaveTextContent("こころ");
      expect(first).toHaveTextContent(text.volume("上"));
      expect(first).toHaveTextContent("夏目漱石（著）");
      expect(first).toHaveTextContent("岩波書店 · 2019-05");
      expect(first).toHaveTextContent("9784003101117");
      expect(second).toHaveTextContent("こころ（古い本）");
      expect(second).toHaveTextContent("先生の遺書");
      expect(second).toHaveTextContent(text.noIsbn);
      // 書影は出さない
      for (const option of options()) expect(within(option).queryByRole("img")).toBeNull();
    });
  });

  describe("選ぶ", () => {
    async function openWithResults() {
      const handlers = renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      await answerAll();
      return handlers;
    }

    it("行のクリックで選ぶ（取得元をまたいで 1 つだけ）。選ぶまで「入れる」は押せない", async () => {
      await openWithResults();
      expect(chooseButton()).toBeDisabled();
      fireEvent.click(options()[1] as HTMLElement);
      expect(selectedTitle()).toBe("こころ（古い本）");
      fireEvent.click(options()[4] as HTMLElement);
      expect(selectedTitle()).toBe("Kokoro");
      expect(options().filter((o) => o.getAttribute("aria-selected") === "true")).toHaveLength(1);
      expect(chooseButton()).toBeEnabled();
    });

    it("「入れる」で、選んだ行の結果を渡す", async () => {
      const { onChoose } = await openWithResults();
      fireEvent.click(options()[2] as HTMLElement);
      fireEvent.click(chooseButton());
      expect(onChoose).toHaveBeenCalledExactlyOnceWith(GOOGLE_HITS[0]);
    });

    it("行のダブルクリックで、その行を渡す", async () => {
      const { onChoose } = await openWithResults();
      fireEvent.doubleClick(options()[4] as HTMLElement);
      expect(onChoose).toHaveBeenCalledExactlyOnceWith(OL_HITS[0]);
    });

    it("入力欄の ↑↓ で選択を動かす。リストに移り、aria-activedescendant が選んだ行を指す", async () => {
      await openWithResults();
      const list = within(dialog()).getByRole("listbox", { name: text.resultsLabel });
      // 選んでいなければ、↓ は最初の行
      fireEvent.keyDown(titleInput(), { key: "ArrowDown" });
      expect(selectedTitle()).toBe("こころ");
      expect(list).toHaveFocus();
      expect(list).toHaveAttribute("aria-activedescendant", (options()[0] as HTMLElement).id);
      // 取得元をまたいで、表示の順に進む
      fireEvent.keyDown(list, { key: "ArrowDown" });
      fireEvent.keyDown(list, { key: "ArrowDown" });
      expect(selectedTitle()).toBe("Zebra Code");
      fireEvent.keyDown(list, { key: "ArrowUp" });
      expect(selectedTitle()).toBe("こころ（古い本）");
      // 端で止まる
      fireEvent.keyDown(list, { key: "ArrowUp" });
      fireEvent.keyDown(list, { key: "ArrowUp" });
      expect(selectedTitle()).toBe("こころ");
      fireEvent.keyDown(list, { key: "End" });
      expect(selectedTitle()).toBe("Kokoro");
      fireEvent.keyDown(list, { key: "ArrowDown" });
      expect(selectedTitle()).toBe("Kokoro");
      fireEvent.keyDown(list, { key: "Home" });
      expect(selectedTitle()).toBe("こころ");
    });

    it("選んだ行をスクロールして見えるようにする", async () => {
      await openWithResults();
      const scrollIntoView = vi.spyOn(Element.prototype, "scrollIntoView");
      fireEvent.keyDown(titleInput(), { key: "ArrowDown" });
      fireEvent.keyDown(titleInput(), { key: "ArrowUp" });
      expect(scrollIntoView).toHaveBeenCalledWith({ block: "nearest" });
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(options()[0]);
      scrollIntoView.mockRestore();
    });

    it("入力欄の ↑ は、選んでいなければ最後の行。結果がなければ何も起きない", async () => {
      renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      // まだ結果がない
      fireEvent.keyDown(titleInput(), { key: "ArrowDown" });
      expect(titleInput()).toHaveFocus();
      await answerAll();
      fireEvent.keyDown(titleInput(), { key: "ArrowUp" });
      expect(selectedTitle()).toBe("Kokoro");
    });

    it("リストにフォーカスがあるときの Enter で選ぶ。入力欄の Enter は、探し直して選択を外す", async () => {
      const { onChoose } = await openWithResults();
      fireEvent.keyDown(titleInput(), { key: "ArrowDown" });
      fireEvent.keyDown(titleInput(), { key: "Enter" });
      expect(onChoose).not.toHaveBeenCalled();
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(6));
      expect(chooseButton()).toBeDisabled();

      await answer("ndl", makeSearchResult("ndl", { hits: NDL_HITS }));
      const list = within(dialog()).getByRole("listbox");
      fireEvent.click(options()[1] as HTMLElement);
      fireEvent.keyDown(list, { key: "Enter" });
      expect(onChoose).toHaveBeenCalledExactlyOnceWith(NDL_HITS[1]);
    });

    it("リンクのボタンにフォーカスがあるときの Enter では、選ばない", async () => {
      const { onChoose } = await openWithResults();
      fireEvent.click(options()[1] as HTMLElement);
      const link = within(options()[1] as HTMLElement).getByRole("button");
      fireEvent.keyDown(link, { key: "Enter" });
      expect(onChoose).not.toHaveBeenCalled();
    });

    it("リンクは、選んでいる行のものだけ Tab で届く", async () => {
      await openWithResults();
      const links = within(dialog()).getAllByRole("button", { name: /で見る/ });
      expect(links.every((link) => link.tabIndex === -1)).toBe(true);
      fireEvent.click(options()[1] as HTMLElement);
      expect(within(options()[1] as HTMLElement).getByRole("button").tabIndex).toBe(0);
    });
  });

  describe("閉じる", () => {
    it("「閉じる」と Esc は onClose（外側のクリックでは閉じない）", async () => {
      const { onClose } = renderDialog("こころ");
      await waitFor(() => expect(calls("search_metadata")).toHaveLength(3));
      fireEvent.pointerDown(document.body);
      expect(onClose).not.toHaveBeenCalled();
      fireEvent.keyDown(titleInput(), { key: "Escape" });
      await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
      fireEvent.click(within(dialog()).getByRole("button", { name: text.close }));
      expect(onClose).toHaveBeenCalledTimes(2);
    });
  });

  it("取得元の一覧を読めなければ、その理由を出す", async () => {
    invoke.mockImplementation((command: string) =>
      command === "list_providers"
        ? rejectWith({ kind: "internal", message: "一覧を読めません" })
        : Promise.resolve(null),
    );
    renderDialog("こころ");
    expect(await screen.findByRole("alert")).toHaveTextContent("一覧を読めません");
    expect(calls("search_metadata")).toHaveLength(0);
  });

  it("検索できる取得元がなければ、その旨を出す", async () => {
    providers = [makeProviderStatus("openbd", { supportsSearch: false })];
    renderDialog("こころ");
    expect(await within(dialog()).findByText(text.noProviders)).toBeInTheDocument();
    expect(within(dialog()).queryAllByRole("group")).toHaveLength(0);
  });
});
