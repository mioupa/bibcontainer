// 設定画面（FR-CONF）。Tauri の invoke とダイアログだけを差し替え、各節のフォームが保存まで届くことを確かめる。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppInfo } from "@/bindings/AppInfo";
import type { BackupInfo } from "@/bindings/BackupInfo";
import type { MetaPrioritySets } from "@/bindings/MetaPrioritySets";
import type { ProviderSettings } from "@/bindings/ProviderSettings";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { ProviderTestResult } from "@/bindings/ProviderTestResult";
import { DEFAULT_SCAN_SETTINGS, DEFAULT_SOUND_SETTINGS } from "@/features/scan/config";
import { mockSync } from "@/dev/mockSync";
import { SettingsScreen } from "@/features/settings/SettingsScreen";
import { ja } from "@/i18n/ja";
import { useModalStore } from "@/lib/useModalGuard";
import { useScreenStore } from "@/stores/screenStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";
import { listenToMockSync, syncInvokeHandlers } from "@/test/syncIpc";

const invoke = vi.hoisted(() => vi.fn());
const isTauri = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));
// sync-status はモックの同期につなぐ。ほかのイベントは、何も受けない購読にする
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const playSound = vi.hoisted(() => vi.fn());
vi.mock("@/features/scan/playSound", () => ({ playSound }));

const chooseDirectory = vi.hoisted(() => vi.fn());
const chooseSavePath = vi.hoisted(() => vi.fn());
vi.mock("@/lib/dialog", () => ({ chooseDirectory, chooseSavePath, chooseOpenFile: vi.fn() }));

const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), info: vi.fn() }));
vi.mock("sonner", () => ({ toast, Toaster: () => null }));

type Handler = (args: Record<string, unknown> | undefined) => unknown;

const info: AppInfo = {
  name: "bibcontainer",
  version: "9.9.9",
  deviceId: "d-0123456789ab",
  schemaVersion: 2,
  dbPath: "/data/library.sqlite3",
  hlcLast: null,
  pendingOps: 0,
};

// 区分ごとの優先順位。和書は従来の既定。洋書などは、書名・著者・出版社・言語・発行日・内容紹介・書影で Google と Open Library が先
const PRIORITIES: MetaPrioritySets = {
  japanese: {
    groups: {
      title: ["openbd", "ndl", "google", "openlibrary"],
      title_kana: ["ndl"],
      authors: ["openbd", "ndl", "google", "openlibrary"],
      publisher: ["openbd", "ndl", "google", "openlibrary"],
      published_on: ["openbd", "ndl", "google", "openlibrary"],
      list_price: ["barcode", "openbd", "ndl"],
      ref_price_incl: ["openbd", "ndl"],
      c_code: ["barcode", "openbd"],
      ndc: ["ndl"],
      pages_and_size: ["ndl", "openbd", "google", "openlibrary"],
      series: ["openbd", "ndl"],
      language: ["openbd", "ndl", "google", "openlibrary"],
      description: ["openbd", "google", "openlibrary"],
      cover: ["openbd", "google", "openlibrary"],
    },
  },
  foreign: {
    groups: {
      title: ["google", "openlibrary", "ndl", "openbd"],
      title_kana: ["ndl"],
      authors: ["google", "openlibrary", "ndl", "openbd"],
      publisher: ["google", "openlibrary", "ndl", "openbd"],
      published_on: ["google", "openlibrary", "ndl", "openbd"],
      list_price: ["barcode", "openbd", "ndl"],
      ref_price_incl: ["openbd", "ndl"],
      c_code: ["barcode", "openbd"],
      ndc: ["ndl"],
      pages_and_size: ["ndl", "openbd", "google", "openlibrary"],
      series: ["openbd", "ndl"],
      language: ["google", "openlibrary", "ndl", "openbd"],
      description: ["google", "openlibrary", "openbd"],
      cover: ["google", "openlibrary", "openbd"],
    },
  },
};

function provider(overrides: Partial<ProviderStatus> & Pick<ProviderStatus, "id">): ProviderStatus {
  return {
    enabled: true,
    hasKey: false,
    active: true,
    displayName: overrides.id,
    auth: { kind: "none" },
    fields: [],
    rateLimit: { group: overrides.id, minIntervalMs: 0, maxConcurrent: 1 },
    credit: `クレジット ${overrides.id}`,
    supportsBatch: false,
    supportsSearch: false,
    needsNetwork: true,
    ...overrides,
  };
}

function testResult(overrides: Partial<ProviderTestResult> = {}): ProviderTestResult {
  return {
    provider: "openbd",
    isbn13: "9784780802047",
    tried: ["9784780802047"],
    outcome: { kind: "found" },
    verdict: "found",
    title: "おにぎりレシピ101",
    publisher: "ポット出版",
    cCode: "0077",
    hasCover: true,
    elapsedMs: 321,
    ...overrides,
  };
}

let providers: ProviderStatus[];
let providerSettings: ProviderSettings;
let backups: BackupInfo[];
let handlers: Record<string, unknown>;

function route(extra: Record<string, unknown> = {}) {
  providerSettings = {
    enabled: { openbd: true, ndl: true, google: true, openlibrary: true },
    priorities: structuredClone(PRIORITIES),
    contactUrl: "https://github.com/mioupa/bibcontainer",
  };
  providers = [
    provider({
      id: "openbd",
      displayName: "openBD",
      credit: "書誌情報・書影: openBD (https://openbd.jp/)",
    }),
    provider({
      id: "ndl",
      displayName: "NDLサーチ",
      credit: "書誌情報: 国立国会図書館サーチ API (https://ndlsearch.ndl.go.jp/)",
    }),
    provider({
      id: "google",
      displayName: "Google Books",
      auth: { kind: "required", label: "API キー" },
      active: false,
      credit: "Powered by Google Books API (https://books.google.com/)",
    }),
    provider({
      id: "openlibrary",
      displayName: "Open Library",
      credit: "書誌情報・書影: Open Library (https://openlibrary.org/)",
    }),
  ];
  backups = [
    {
      path: "/b/daily-20261014.sqlite3",
      fileName: "daily-20261014.sqlite3",
      kind: "daily",
      createdAt: "2026-10-14T03:00:00Z",
      sizeBytes: 2048,
    },
    {
      path: "/b/manual-20261013.sqlite3",
      fileName: "manual-20261013.sqlite3",
      kind: "manual",
      createdAt: null,
      sizeBytes: 5 * 1024 * 1024,
    },
  ];
  handlers = {
    app_info: info,
    get_provider_settings: () => providerSettings,
    set_provider_settings: (args: { settings: ProviderSettings } | undefined) => {
      providerSettings = args!.settings;
      return providerSettings;
    },
    list_providers: () => providers,
    set_api_key: (args: { provider: string } | undefined) => {
      providers = providers.map((p) =>
        p.id === args?.provider ? { ...p, hasKey: true, active: true } : p,
      );
      return null;
    },
    delete_api_key: (args: { provider: string } | undefined) => {
      providers = providers.map((p) =>
        p.id === args?.provider ? { ...p, hasKey: false, active: false } : p,
      );
      return null;
    },
    test_provider: testResult(),
    get_price_settings: { showTaxIncluded: false, taxRatePercent: 10 },
    set_price_settings: (args: { settings: unknown } | undefined) => args!.settings,
    get_backup_settings: { directory: "/data/backups", dailyKeep: 7, weeklyKeep: 4 },
    set_backup_settings: (args: { settings: unknown } | undefined) => args!.settings,
    list_backups: () => backups,
    manual_backup: {
      path: "/b/manual-20261014.sqlite3",
      fileName: "manual-20261014.sqlite3",
      kind: "manual",
      createdAt: "2026-10-14T04:00:00Z",
      sizeBytes: 100,
    },
    restore_backup: {
      schemaVersion: 2,
      oldDeviceId: "d-old",
      newDeviceId: "d-new",
      movedUnflushedOps: 0,
      preRestorePath: "/data/backups/pre-restore-1.sqlite3",
    },
    open_backup_dir: null,
    open_logs_dir: null,
    // 同期のコマンドは、モックの同期（状態を切り替えられる）につなぐ
    ...syncInvokeHandlers,
    // 端末ごとの設定（ストアの保存先）
    get_list_settings: {
      columnVisibility: {},
      columnOrder: [],
      columnSizing: {},
      sorting: [],
      sidebarOpen: true,
      detailOpen: true,
    },
    set_list_settings: (args: { settings: unknown } | undefined) => args!.settings,
    get_scan_settings: {
      terminator: "enter",
      timeoutSec: 10,
      keyIntervalMs: 30,
      idleCommitMs: 100,
      stripLeading: 0,
      stripTrailing: 0,
      sounds: {
        success: { enabled: true, volume: 0.6 },
        error: { enabled: true, volume: 0.6 },
        attention: { enabled: true, volume: 0.6 },
      },
      muted: false,
    },
    set_scan_settings: (args: { settings: unknown } | undefined) => args!.settings,
    get_display_settings: { theme: "system", fontFamily: null, fontSize: 16 },
    set_display_settings: (args: { settings: unknown } | undefined) => args!.settings,
    export_csv: 123,
    export_json: null,
    ...extra,
  };
}

function installInvoke() {
  invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (!(command in handlers)) {
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      return Promise.reject({ kind: "internal", message: `未設定のコマンド: ${command}` });
    }
    const handler = handlers[command];
    return Promise.resolve(typeof handler === "function" ? (handler as Handler)(args) : handler);
  });
}

const calls = (command: string) => invoke.mock.calls.filter(([name]) => name === command);

function renderSettings(
  section: Parameters<typeof useScreenStore.setState>[0] extends never ? never : string,
) {
  useScreenStore.setState({
    screen: "settings",
    settingsSection: section as "sync",
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <SettingsScreen />
    </QueryClientProvider>,
  );
  return { client, ...view };
}

/** 数の入力欄に値を入れて、フォーカスを外して確定する。 */
function commitNumber(label: string, value: string) {
  const input = screen.getByLabelText(label);
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
  return input;
}

beforeEach(() => {
  invoke.mockReset();
  listen.mockReset();
  listen.mockImplementation(
    (name: string, callback: (event: { payload: never }) => void) =>
      listenToMockSync(name, callback as never) ?? Promise.resolve(() => {}),
  );
  playSound.mockReset();
  chooseDirectory.mockReset();
  chooseSavePath.mockReset();
  toast.error.mockReset();
  toast.success.mockReset();
  isTauri.mockReturnValue(true);
  mockSync.control.reset();
  mockSync.control.setDelays({ setupMs: 0, syncMs: 0 });
  useSettingsStore.setState({
    theme: "system",
    fontFamily: null,
    fontSize: 16,
    scan: { ...DEFAULT_SCAN_SETTINGS },
    sound: structuredClone(DEFAULT_SOUND_SETTINGS),
  });
  useTableStore.getState().resetColumns();
  route();
  installInvoke();
});

describe("設定画面の骨組み", () => {
  it("左に 7 つの節の一覧、右に選んだ節の内容。節を選ぶと切り替わる", async () => {
    renderSettings("sync");
    const nav = screen.getByRole("navigation", { name: ja.settings.navLabel });
    for (const name of Object.values(ja.settings.sections)) {
      expect(within(nav).getByRole("button", { name })).toBeInTheDocument();
    }
    expect(within(nav).getByRole("button", { name: ja.settings.sections.sync })).toHaveAttribute(
      "aria-current",
      "page",
    );

    fireEvent.click(within(nav).getByRole("button", { name: ja.settings.sections.price }));
    expect(useScreenStore.getState().settingsSection).toBe("price");
    expect(await screen.findByLabelText(ja.settings.price.showTaxIncluded)).toBeInTheDocument();
  });

  it("メイン画面へ戻るボタンで、メイン画面に戻る", () => {
    renderSettings("sync");
    fireEvent.click(screen.getByRole("button", { name: ja.settings.back }));
    expect(useScreenStore.getState().screen).toBe("main");
  });
});

describe("同期（FR-CONF-01）", () => {
  // 手順・状態・警告の細かい確認は src/features/sync/SyncSection.test.tsx
  it("未設定なら、フォルダを選ぶボタンと端末名・端末 ID・Google Drive の注意を出す", async () => {
    renderSettings("sync");
    expect(
      await screen.findByRole("button", { name: ja.sync.setup.chooseFolder }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(ja.sync.deviceName.label)).toHaveValue("MacBook Pro");
    expect(screen.getByText("d-0123456789ab")).toBeInTheDocument();
    expect(screen.getByTestId("drive-notes")).toBeInTheDocument();
  });
});

describe("取得元（FR-CONF-02、FR-META-03）", () => {
  it("取得元の一覧と、使用中か・キーがないと使われない旨を出す", async () => {
    renderSettings("providers");
    expect(await screen.findByRole("heading", { name: "openBD" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "NDLサーチ" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Google Books" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Open Library" })).toBeInTheDocument();
    // Google はキーを入れるまで使われない（Open Library はキー不要で、使われている）
    expect(screen.getByText(ja.settings.providers.googleNote)).toBeInTheDocument();
    expect(screen.getAllByText(ja.settings.providers.active)).toHaveLength(3);
    expect(screen.getAllByText(ja.settings.providers.inactive)).toHaveLength(1);
  });

  it("有効・無効を切り替えると、設定を丸ごと保存する", async () => {
    renderSettings("providers");
    fireEvent.click(
      await screen.findByRole("checkbox", { name: ja.settings.providers.enabled("openBD") }),
    );

    await waitFor(() => expect(calls("set_provider_settings")).toHaveLength(1));
    const saved = (calls("set_provider_settings")[0]![1] as { settings: ProviderSettings })
      .settings;
    expect(saved.enabled).toEqual({ openbd: false, ndl: true, google: true, openlibrary: true });
    expect(saved.priorities).toEqual(PRIORITIES);
    expect(saved.contactUrl).toBe("https://github.com/mioupa/bibcontainer");
  });

  it("API キー: 保存すると set_api_key に渡し、入力欄を空に戻して「設定済み」だけを出す", async () => {
    renderSettings("providers");
    const input = await screen.findByLabelText(
      new RegExp(`^${ja.settings.providers.apiKey.label("Google Books")}`),
    );
    expect(input).toHaveAttribute("type", "password");
    expect(screen.getByText(ja.settings.providers.apiKey.notSet)).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "  secret-key  " } });
    fireEvent.click(screen.getByRole("button", { name: ja.settings.providers.apiKey.save }));

    await waitFor(() => expect(calls("set_api_key")).toHaveLength(1));
    expect(calls("set_api_key")[0]![1]).toEqual({ provider: "google", key: "  secret-key  " });
    await waitFor(() => expect(input).toHaveValue(""));
    expect(await screen.findByText(ja.settings.providers.apiKey.saved)).toBeInTheDocument();
    // キーを画面に残さない
    expect(document.body.textContent).not.toContain("secret-key");
    // キーを入れたら、Google の注意は消え、使用中になる
    await waitFor(() =>
      expect(screen.queryByText(ja.settings.providers.googleNote)).not.toBeInTheDocument(),
    );
    expect(screen.getAllByText(ja.settings.providers.active)).toHaveLength(4);
  });

  it("API キーを削除できる。キーがなければ削除ボタンは押せない", async () => {
    providers = providers.map((p) =>
      p.id === "google" ? { ...p, hasKey: true, active: true } : p,
    );
    renderSettings("providers");
    const del = await screen.findByRole("button", { name: ja.settings.providers.apiKey.delete });
    expect(del).toBeEnabled();
    fireEvent.click(del);

    await waitFor(() => expect(calls("delete_api_key")).toHaveLength(1));
    expect(calls("delete_api_key")[0]![1]).toEqual({ provider: "google" });
    await waitFor(() => expect(del).toBeDisabled());
  });

  it("キーが空のときは、保存ボタンを押せない", async () => {
    renderSettings("providers");
    const save = await screen.findByRole("button", { name: ja.settings.providers.apiKey.save });
    expect(save).toBeDisabled();
  });

  it("接続テスト: test_provider の結果を出す", async () => {
    renderSettings("providers");
    const buttons = await screen.findAllByRole("button", {
      name: ja.settings.providers.test.button,
    });
    fireEvent.click(buttons[0]!);

    await waitFor(() => expect(calls("test_provider")).toHaveLength(1));
    expect(calls("test_provider")[0]![1]).toEqual({ id: "openbd" });
    const result = await screen.findByTestId("test-result-openbd");
    expect(result).toHaveTextContent("接続できました（おにぎりレシピ101 を取得、321 ミリ秒）");
    expect(result).toHaveAttribute("data-tone", "success");
    expect(result).toHaveClass("text-success");
  });

  /** 取得元（0: openBD、1: NDL、2: Google）の接続テストを、`result` が返る状態で押す。 */
  async function runTest(index: number, id: string, result: ProviderTestResult) {
    route({ test_provider: result });
    installInvoke();
    renderSettings("providers");
    const buttons = await screen.findAllByRole("button", {
      name: ja.settings.providers.test.button,
    });
    fireEvent.click(buttons[index]!);
    return screen.findByTestId(`test-result-${id}`);
  }

  it("接続テスト: 200 でテスト用の本がなかっただけなら、成功として出す（キーを使う取得元は「キーは有効」）", async () => {
    const google = await runTest(
      2,
      "google",
      testResult({
        provider: "google",
        isbn13: "9780141439518",
        tried: ["9784003101117", "9784101010014", "9780141439518"],
        outcome: { kind: "notFound" },
        verdict: "notFound",
        title: null,
        publisher: null,
        cCode: null,
        hasCover: false,
        elapsedMs: 393,
      }),
    );
    expect(google).toHaveTextContent(
      "接続できました。API キーは有効です（テスト用の本はこの取得元に見つかりませんでした）",
    );
    expect(google).toHaveAttribute("data-tone", "success");
    expect(google).toHaveClass("text-success");
  });

  it("接続テスト: キーを使わない取得元では「API キーは有効」と言わない", async () => {
    const ndl = await runTest(
      1,
      "ndl",
      testResult({
        provider: "ndl",
        outcome: { kind: "notFound" },
        verdict: "notFound",
        title: null,
      }),
    );
    expect(ndl).toHaveTextContent(
      "接続できました（テスト用の本はこの取得元に見つかりませんでした）",
    );
    expect(ndl).not.toHaveTextContent("API キー");
    expect(ndl).toHaveAttribute("data-tone", "success");
  });

  it("接続テスト: キーが無効（401・403・400）なら、そう伝える", async () => {
    const google = await runTest(
      2,
      "google",
      testResult({
        provider: "google",
        outcome: { kind: "httpError", status: 400, message: "API key not valid." },
        verdict: "invalidKey",
        title: null,
      }),
    );
    expect(google).toHaveTextContent("API キーが無効か、Books API が有効になっていません");
    expect(google).toHaveAttribute("data-tone", "error");
    expect(google).toHaveClass("text-destructive");
  });

  it("接続テスト: 429 は、1 日の上限と、割り当て 0 を区別して出す", async () => {
    const daily = await runTest(
      2,
      "google",
      testResult({
        provider: "google",
        outcome: { kind: "httpError", status: 429, message: "Queries per day" },
        verdict: "quotaExceeded",
        title: null,
      }),
    );
    expect(daily).toHaveTextContent("1 日の利用上限に達しています");
    expect(daily).toHaveAttribute("data-tone", "warning");
    expect(daily).toHaveClass("text-warning");
  });

  it("接続テスト: 割り当てが 0 の 429 は、上限とは別の文言", async () => {
    const zero = await runTest(
      2,
      "google",
      testResult({
        provider: "google",
        outcome: {
          kind: "httpError",
          status: 429,
          message: "Queries per day（割り当ての上限は 0 です）",
        },
        verdict: "quotaZero",
        title: null,
      }),
    );
    expect(zero).toHaveTextContent("割り当てが 0 です");
    expect(zero).not.toHaveTextContent("1 日の利用上限に達しています");
    expect(zero).toHaveAttribute("data-tone", "error");
  });

  it("接続テストが通信エラーなら、ネットワークの確認を促す", async () => {
    const ndl = await runTest(
      1,
      "ndl",
      testResult({
        provider: "ndl",
        outcome: { kind: "networkError", message: "接続できません" },
        verdict: "networkError",
        title: null,
        elapsedMs: 10,
      }),
    );
    expect(ndl).toHaveTextContent("接続できませんでした（ネットワークを確認してください）");
    expect(ndl).toHaveAttribute("data-tone", "error");
  });

  it("接続テスト: 上のどれでもない HTTP エラーは、ステータスと理由をそのまま出す", async () => {
    const openbd = await runTest(
      0,
      "openbd",
      testResult({
        outcome: { kind: "httpError", status: 503, message: "busy" },
        verdict: "httpError",
        title: null,
      }),
    );
    expect(openbd).toHaveTextContent("HTTP エラー（503）: busy");
    expect(openbd).toHaveAttribute("data-tone", "error");
  });

  it("API キーを保存したら、前の接続テストの結果は消える", async () => {
    route({
      test_provider: testResult({
        provider: "google",
        outcome: { kind: "httpError", status: 400, message: "API key not valid." },
        verdict: "invalidKey",
        title: null,
      }),
    });
    installInvoke();
    renderSettings("providers");
    const buttons = await screen.findAllByRole("button", {
      name: ja.settings.providers.test.button,
    });
    fireEvent.click(buttons[2]!);
    expect(await screen.findByTestId("test-result-google")).toBeInTheDocument();

    fireEvent.change(
      screen.getByLabelText(new RegExp(`^${ja.settings.providers.apiKey.label("Google Books")}`)),
      { target: { value: "new-key" } },
    );
    fireEvent.click(screen.getByRole("button", { name: ja.settings.providers.apiKey.save }));
    await waitFor(() => expect(calls("set_api_key")).toHaveLength(1));
    await waitFor(() => expect(screen.queryByTestId("test-result-google")).not.toBeInTheDocument());
  });

  it("キーがなくて接続テストが失敗したら、Rust の理由を通知する", async () => {
    route({
      test_provider: () =>
        // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
        Promise.reject({ kind: "validation", message: "API キーを入れてください" }),
    });
    installInvoke();
    renderSettings("providers");
    const buttons = await screen.findAllByRole("button", {
      name: ja.settings.providers.test.button,
    });
    fireEvent.click(buttons[2]!);
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("API キーを入れてください"));
  });

  it("優先順位: 並べ替えると、その行だけを変えて保存する。2段目は動かせない", async () => {
    renderSettings("providers");
    const title = await screen.findByTestId("priority-title");
    fireEvent.click(
      within(title).getByRole("button", {
        name: ja.settings.providers.priorities.moveDown("書名・副書名", "openBD"),
      }),
    );

    await waitFor(() => expect(calls("set_provider_settings")).toHaveLength(1));
    const saved = (calls("set_provider_settings")[0]![1] as { settings: ProviderSettings })
      .settings;
    expect(saved.priorities.japanese.groups.title).toEqual([
      "ndl",
      "openbd",
      "google",
      "openlibrary",
    ]);
    expect(saved.priorities.japanese.groups.cover).toEqual(["openbd", "google", "openlibrary"]);
    // 洋書などの組は、そのまま
    expect(saved.priorities.foreign).toEqual(PRIORITIES.foreign);

    // 本体価格: 2段目は先頭に固定。その下の取得元は、2段目の上へは動かせない
    const price = screen.getByTestId("priority-list_price");
    expect(price).toHaveTextContent(ja.settings.providers.sources.barcode);
    expect(
      within(price).queryByRole("button", {
        name: ja.settings.providers.priorities.moveUp(
          "本体価格",
          ja.settings.providers.sources.barcode,
        ),
      }),
    ).not.toBeInTheDocument();
    expect(
      within(price).getByRole("button", {
        name: ja.settings.providers.priorities.moveUp("本体価格", "openBD"),
      }),
    ).toBeDisabled();
    expect(
      within(price).getByRole("button", {
        name: ja.settings.providers.priorities.moveDown("本体価格", "NDLサーチ"),
      }),
    ).toBeDisabled();
  });

  it("優先順位: 切り替えは「和書」が初期で、「洋書など」の順を見せ、そのあいだ説明を出す", async () => {
    renderSettings("providers");
    const p = ja.settings.providers.priorities;
    const japanese = await screen.findByRole("tab", { name: p.profiles.japanese });
    const foreign = screen.getByRole("tab", { name: p.profiles.foreign });
    expect(screen.getByRole("tablist", { name: p.profileLabel })).toBeInTheDocument();
    expect(japanese).toHaveAttribute("aria-selected", "true");
    expect(foreign).toHaveAttribute("aria-selected", "false");
    expect(screen.getByText(p.profileNote)).toBeInTheDocument();

    // 初期は和書の順（書名: openBD が先頭、書影: openBD が先頭）
    const names = (testId: string) =>
      within(screen.getByTestId(testId))
        .getAllByRole("listitem")
        .map((li) => li.textContent?.replace(/^\d+/, ""));
    expect(names("priority-title")).toEqual([
      "openBD",
      "NDLサーチ",
      "Google Books",
      "Open Library",
    ]);
    expect(names("priority-cover")).toEqual(["openBD", "Google Books", "Open Library"]);

    // 洋書などに切り替えると、その組の順が出る（Google・Open Library が先）
    fireEvent.mouseDown(foreign);
    await waitFor(() => expect(foreign).toHaveAttribute("aria-selected", "true"));
    expect(japanese).toHaveAttribute("aria-selected", "false");
    expect(names("priority-title")).toEqual([
      "Google Books",
      "Open Library",
      "NDLサーチ",
      "openBD",
    ]);
    expect(names("priority-cover")).toEqual(["Google Books", "Open Library", "openBD"]);
    // 切り替えだけでは保存しない。説明は出たまま。2段目の固定の注記も同じ
    expect(calls("set_provider_settings")).toHaveLength(0);
    expect(screen.getByText(p.profileNote)).toBeInTheDocument();
    expect(screen.getByText(p.fixedBarcode)).toBeInTheDocument();
    // 2段目は、洋書などの組でも本体価格の先頭に固定
    expect(screen.getByTestId("priority-list_price")).toHaveTextContent(
      ja.settings.providers.sources.barcode,
    );

    // 和書に戻すと、和書の順
    fireEvent.mouseDown(japanese);
    await waitFor(() => expect(japanese).toHaveAttribute("aria-selected", "true"));
    expect(names("priority-title")).toEqual([
      "openBD",
      "NDLサーチ",
      "Google Books",
      "Open Library",
    ]);
  });

  it("優先順位: 洋書などで並べ替えても和書の順は変わらず、保存は両方の組を送る", async () => {
    renderSettings("providers");
    const p = ja.settings.providers.priorities;
    fireEvent.mouseDown(await screen.findByRole("tab", { name: p.profiles.foreign }));
    const title = await screen.findByTestId("priority-title");
    // 洋書などの書名: Google Books を 1 つ下へ（Google Books、Open Library → Open Library、Google Books）
    fireEvent.click(
      within(title).getByRole("button", { name: p.moveDown("書名・副書名", "Google Books") }),
    );

    await waitFor(() => expect(calls("set_provider_settings")).toHaveLength(1));
    const saved = (calls("set_provider_settings")[0]![1] as { settings: ProviderSettings })
      .settings;
    expect(saved.priorities.foreign.groups.title).toEqual([
      "openlibrary",
      "google",
      "ndl",
      "openbd",
    ]);
    // 洋書などの組の、ほかの行はそのまま
    expect(saved.priorities.foreign.groups.cover).toEqual(["google", "openlibrary", "openbd"]);
    // 和書の組は、変わらないまま両方を送る
    expect(saved.priorities.japanese).toEqual(PRIORITIES.japanese);
    expect(Object.keys(saved.priorities).sort()).toEqual(["foreign", "japanese"]);

    // 保存後も洋書などの組を見たまま。並べ替えた順が出る
    await waitFor(() =>
      expect(
        within(screen.getByTestId("priority-title"))
          .getAllByRole("listitem")
          .map((li) => li.textContent?.replace(/^\d+/, "")),
      ).toEqual(["Open Library", "Google Books", "NDLサーチ", "openBD"]),
    );
    expect(screen.getByRole("tab", { name: p.profiles.foreign })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    // 和書に切り替えても、和書の順は元のまま
    fireEvent.mouseDown(screen.getByRole("tab", { name: p.profiles.japanese }));
    await waitFor(() =>
      expect(
        within(screen.getByTestId("priority-title"))
          .getAllByRole("listitem")
          .map((li) => li.textContent?.replace(/^\d+/, "")),
      ).toEqual(["openBD", "NDLサーチ", "Google Books", "Open Library"]),
    );

    // 和書で並べ替えると、洋書などの並べ替えを保ったまま保存する
    fireEvent.click(
      within(screen.getByTestId("priority-title")).getByRole("button", {
        name: p.moveDown("書名・副書名", "openBD"),
      }),
    );
    await waitFor(() => expect(calls("set_provider_settings")).toHaveLength(2));
    const second = (calls("set_provider_settings")[1]![1] as { settings: ProviderSettings })
      .settings;
    expect(second.priorities.japanese.groups.title).toEqual([
      "ndl",
      "openbd",
      "google",
      "openlibrary",
    ]);
    expect(second.priorities.foreign.groups.title).toEqual([
      "openlibrary",
      "google",
      "ndl",
      "openbd",
    ]);
  });

  it("User-Agent の連絡先を保存できる。理由つきの失敗は通知する", async () => {
    renderSettings("providers");
    const input = await screen.findByLabelText(ja.settings.providers.contactUrl.label);
    const save = within(input.closest("section")!).getByRole("button", { name: ja.settings.save });
    expect(save).toBeDisabled();
    fireEvent.change(input, { target: { value: " https://example.com/me " } });
    fireEvent.click(save);

    await waitFor(() => expect(calls("set_provider_settings")).toHaveLength(1));
    expect(
      (calls("set_provider_settings")[0]![1] as { settings: ProviderSettings }).settings.contactUrl,
    ).toBe("https://example.com/me");
  });

  it("ブラウザ（Tauri の外）では、使えない旨を出す", () => {
    isTauri.mockReturnValue(false);
    invoke.mockClear();
    renderSettings("providers");
    expect(screen.getByText(ja.settings.browserNotice)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("スキャン（FR-CONF-03）", () => {
  it("末尾キー・タイムアウト・キー間隔・除く文字数を、範囲に収めて保存する", async () => {
    renderSettings("scan");

    fireEvent.click(screen.getByRole("combobox", { name: ja.settings.scan.terminator.label }));
    fireEvent.click(await screen.findByRole("option", { name: ja.settings.scan.terminator.tab }));
    expect(useSettingsStore.getState().scan.terminator).toBe("tab");

    const timeout = commitNumber(ja.settings.scan.timeoutSec, "99");
    expect(useSettingsStore.getState().scan.timeoutSec).toBe(30);
    await waitFor(() => expect(timeout).toHaveValue(30));
    commitNumber(ja.settings.scan.timeoutSec, "1");
    expect(useSettingsStore.getState().scan.timeoutSec).toBe(3);

    commitNumber(ja.settings.scan.keyIntervalMs, "50");
    expect(useSettingsStore.getState().scan.keyIntervalMs).toBe(50);
    commitNumber(ja.settings.scan.stripLeading, "2");
    commitNumber(ja.settings.scan.stripTrailing, "9");
    expect(useSettingsStore.getState().scan.stripLeading).toBe(2);
    expect(useSettingsStore.getState().scan.stripTrailing).toBe(4);
  });

  it("Tauri の中では、変更が Rust の scan セクション（set_scan_settings）に保存される", async () => {
    await useSettingsStore.persist.rehydrate();
    renderSettings("scan");
    commitNumber(ja.settings.scan.timeoutSec, "15");
    fireEvent.click(screen.getByRole("checkbox", { name: ja.settings.sound.muted }));

    await waitFor(() => expect(calls("set_scan_settings").length).toBeGreaterThan(0));
    const last = calls("set_scan_settings").at(-1)![1] as {
      settings: { timeoutSec: number; muted: boolean };
    };
    expect(last.settings.timeoutSec).toBe(15);
    expect(last.settings.muted).toBe(true);
  });

  it("音の種類ごとの有効・音量、ミュート、試聴", () => {
    renderSettings("scan");

    fireEvent.click(
      screen.getByRole("checkbox", {
        name: ja.settings.sound.enabled(ja.settings.sound.kinds.error),
      }),
    );
    expect(useSettingsStore.getState().sound.kinds.error.enabled).toBe(false);
    expect(useSettingsStore.getState().sound.kinds.success.enabled).toBe(true);

    fireEvent.change(
      screen.getByRole("slider", {
        name: ja.settings.sound.volumeOf(ja.settings.sound.kinds.success),
      }),
      { target: { value: "0.25" } },
    );
    expect(useSettingsStore.getState().sound.kinds.success.volume).toBe(0.25);
    expect(useSettingsStore.getState().sound.kinds.attention.volume).toBe(0.6);

    fireEvent.click(screen.getByRole("checkbox", { name: ja.settings.sound.muted }));
    expect(useSettingsStore.getState().sound.muted).toBe(true);

    const attention = screen.getByTestId("sound-attention");
    fireEvent.click(within(attention).getByRole("button", { name: ja.settings.sound.play }));
    expect(playSound).toHaveBeenCalledWith("attention");
  });
});

describe("表示（FR-CONF-04）", () => {
  it("テーマを選べる", async () => {
    renderSettings("display");
    fireEvent.click(screen.getByRole("combobox", { name: ja.settings.theme.label }));
    fireEvent.click(await screen.findByRole("option", { name: ja.settings.theme.dark }));
    expect(useSettingsStore.getState().theme).toBe("dark");
  });

  it("フォント: 名前を入れると保存し、空にすると OS の既定（null）に戻る", () => {
    renderSettings("display");
    const input = screen.getByLabelText(ja.settings.display.fontFamily.label);
    fireEvent.change(input, { target: { value: " Noto Sans JP " } });
    fireEvent.blur(input);
    expect(useSettingsStore.getState().fontFamily).toBe("Noto Sans JP");

    fireEvent.click(screen.getByRole("button", { name: ja.settings.display.fontFamily.reset }));
    expect(useSettingsStore.getState().fontFamily).toBeNull();
  });

  it("文字サイズは 12〜24 に収める", () => {
    renderSettings("display");
    commitNumber(ja.settings.display.fontSize, "40");
    expect(useSettingsStore.getState().fontSize).toBe(24);
    commitNumber(ja.settings.display.fontSize, "5");
    expect(useSettingsStore.getState().fontSize).toBe(12);
  });

  it("一覧の列を切り替えられる（メイン画面の列と同じ設定）。書名は隠せない", () => {
    renderSettings("display");
    const isbn = screen.getByRole("checkbox", { name: ja.list.columns.isbn13 });
    expect(isbn).not.toBeChecked();
    fireEvent.click(isbn);
    expect(useTableStore.getState().columnVisibility.isbn13).toBe(true);

    const publisher = screen.getByRole("checkbox", { name: ja.list.columns.publisher });
    expect(publisher).toBeChecked();
    fireEvent.click(publisher);
    expect(useTableStore.getState().columnVisibility.publisher).toBe(false);

    expect(screen.getByRole("checkbox", { name: ja.list.columns.title })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: ja.settings.display.columns.reset }));
    expect(useTableStore.getState().columnVisibility.isbn13).toBe(false);
    expect(useTableStore.getState().columnVisibility.publisher).toBeUndefined();
  });
});

describe("表示: 一覧の列の並び順（FR-LIST-01）", () => {
  it("矢印で並び順を変えられる（メイン画面の列と同じ設定）。既定に戻すと並び順も戻る", () => {
    useTableStore.getState().resetColumns();
    renderSettings("display");
    fireEvent.click(
      screen.getByRole("button", { name: ja.list.columnPicker.moveDown(ja.list.columns.cover) }),
    );
    expect(useTableStore.getState().columnOrder.slice(0, 3)).toEqual(["title", "cover", "volume"]);

    fireEvent.click(screen.getByRole("button", { name: ja.settings.display.columns.reset }));
    expect(useTableStore.getState().columnOrder).toEqual([]);
  });
});

describe("価格（FR-CONF-05）", () => {
  it("税込の併記と税率を保存する", async () => {
    renderSettings("price");
    const checkbox = await screen.findByRole("checkbox", {
      name: ja.settings.price.showTaxIncluded,
    });
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);

    await waitFor(() => expect(calls("set_price_settings")).toHaveLength(1));
    expect(calls("set_price_settings")[0]![1]).toEqual({
      settings: { showTaxIncluded: true, taxRatePercent: 10 },
    });
    await waitFor(() => expect(checkbox).toBeChecked());

    commitNumber(ja.settings.price.taxRatePercent, "150");
    await waitFor(() => expect(calls("set_price_settings")).toHaveLength(2));
    expect(calls("set_price_settings")[1]![1]).toEqual({
      settings: { showTaxIncluded: true, taxRatePercent: 100 },
    });
  });
});

describe("バックアップ（FR-CONF-06、FR-IO-05）", () => {
  it("保存先・世代数・バックアップの一覧を出す", async () => {
    renderSettings("backup");
    expect(await screen.findByTestId("backup-directory")).toHaveTextContent("/data/backups");
    expect(screen.getByLabelText(ja.settings.backup.dailyKeep)).toHaveValue(7);
    expect(screen.getByLabelText(ja.settings.backup.weeklyKeep)).toHaveValue(4);

    const list = await screen.findByTestId("backup-list");
    expect(within(list).getByText("daily-20261014.sqlite3")).toBeInTheDocument();
    expect(within(list).getByText(/日次/)).toBeInTheDocument();
    expect(within(list).getByText(/5\.0 MB/)).toBeInTheDocument();
    expect(within(list).getByText(/日時不明/)).toBeInTheDocument();
  });

  it("保存先のフォルダを選んで保存する。既定の場所に戻す（空にして保存）", async () => {
    chooseDirectory.mockResolvedValue("/Volumes/Backup/bibcontainer");
    renderSettings("backup");
    fireEvent.click(
      await screen.findByRole("button", { name: ja.settings.backup.location.change }),
    );

    await waitFor(() => expect(calls("set_backup_settings")).toHaveLength(1));
    expect(calls("set_backup_settings")[0]![1]).toEqual({
      settings: { directory: "/Volumes/Backup/bibcontainer", dailyKeep: 7, weeklyKeep: 4 },
    });

    fireEvent.click(screen.getByRole("button", { name: ja.settings.backup.location.reset }));
    await waitFor(() => expect(calls("set_backup_settings")).toHaveLength(2));
    expect(
      (calls("set_backup_settings")[1]![1] as { settings: { directory: string } }).settings
        .directory,
    ).toBe("");
  });

  it("フォルダの選択をやめたら、何も保存しない", async () => {
    chooseDirectory.mockResolvedValue(null);
    renderSettings("backup");
    fireEvent.click(
      await screen.findByRole("button", { name: ja.settings.backup.location.change }),
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(calls("set_backup_settings")).toHaveLength(0);
  });

  it("世代数を範囲に収めて保存する（日次 1〜365、週次 1〜52）", async () => {
    renderSettings("backup");
    await screen.findByTestId("backup-directory");
    commitNumber(ja.settings.backup.dailyKeep, "999");
    await waitFor(() => expect(calls("set_backup_settings")).toHaveLength(1));
    expect(
      (calls("set_backup_settings")[0]![1] as { settings: { dailyKeep: number } }).settings
        .dailyKeep,
    ).toBe(365);

    commitNumber(ja.settings.backup.weeklyKeep, "0");
    await waitFor(() => expect(calls("set_backup_settings")).toHaveLength(2));
    expect(
      (calls("set_backup_settings")[1]![1] as { settings: { weeklyKeep: number } }).settings
        .weeklyKeep,
    ).toBe(1);
  });

  it("今すぐバックアップして、一覧を取り直す", async () => {
    renderSettings("backup");
    fireEvent.click(await screen.findByRole("button", { name: ja.settings.backup.now }));

    await waitFor(() => expect(calls("manual_backup")).toHaveLength(1));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        ja.settings.backup.nowDone("manual-20261014.sqlite3"),
      ),
    );
    await waitFor(() => expect(calls("list_backups").length).toBeGreaterThanOrEqual(2));
  });

  it("復元: 確認ダイアログで承諾すると restore_backup を呼び、全件を取り直す", async () => {
    const { client } = renderSettings("backup");
    const invalidate = vi.spyOn(client, "invalidateQueries");
    fireEvent.click(
      await screen.findByRole("button", {
        name: ja.settings.backup.list.restoreOf("daily-20261014.sqlite3"),
      }),
    );

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(ja.settings.backup.restore.title)).toBeInTheDocument();
    expect(calls("restore_backup")).toHaveLength(0);
    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.settings.backup.restore.confirm }),
    );

    await waitFor(() => expect(calls("restore_backup")).toHaveLength(1));
    expect(calls("restore_backup")[0]![1]).toEqual({ path: "/b/daily-20261014.sqlite3" });
    // 行が丸ごと入れ替わるので、全クエリを取り直す（reloadLibrary）
    await waitFor(() => expect(invalidate).toHaveBeenCalledWith());
    expect(toast.success).toHaveBeenCalledWith(
      ja.settings.backup.restore.done,
      expect.objectContaining({
        description: ja.settings.backup.restore.doneDescription(
          "/data/backups/pre-restore-1.sqlite3",
        ),
      }),
    );
  });

  it("復元: 同期中のときだけ、確認のダイアログに「ほかの端末の変更をもう一度取り込む」旨を足す（0013 §10）", async () => {
    const open = async () => {
      fireEvent.click(
        await screen.findByRole("button", {
          name: ja.settings.backup.list.restoreOf("daily-20261014.sqlite3"),
        }),
      );
      return screen.findByRole("dialog");
    };

    // 同期が未設定: 足さない
    const first = renderSettings("backup");
    const plain = await open();
    expect(within(plain).queryByTestId("restore-sync-note")).not.toBeInTheDocument();
    first.unmount();

    // 同期中: 足す
    act(() => mockSync.control.setScenario("idle"));
    renderSettings("backup");
    const synced = await open();
    expect(within(synced).getByTestId("restore-sync-note")).toHaveTextContent(
      ja.settings.backup.restore.syncNote,
    );
    expect(within(synced).getByText(ja.settings.backup.restore.syncNote)).toBeInTheDocument();
  });

  it("復元の確認でキャンセルしたら、何もしない。確認を出している間は、画面全体のショートカットを止める（9.5）", async () => {
    renderSettings("backup");
    expect(useModalStore.getState().count).toBe(0);
    fireEvent.click(
      await screen.findByRole("button", {
        name: ja.settings.backup.list.restoreOf("daily-20261014.sqlite3"),
      }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(useModalStore.getState().count).toBe(1);
    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.settings.backup.restore.cancel }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(useModalStore.getState().count).toBe(0);
    expect(calls("restore_backup")).toHaveLength(0);
  });

  it("復元に失敗したら、理由を通知する", async () => {
    route({
      restore_backup: () =>
        // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
        Promise.reject({ kind: "invalid_backup", message: "バックアップとして使えません" }),
    });
    installInvoke();
    renderSettings("backup");
    fireEvent.click(
      await screen.findByRole("button", {
        name: ja.settings.backup.list.restoreOf("daily-20261014.sqlite3"),
      }),
    );
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(
      within(dialog).getByRole("button", { name: ja.settings.backup.restore.confirm }),
    );
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("バックアップとして使えません"));
  });

  it("フォルダを開く", async () => {
    renderSettings("backup");
    fireEvent.click(await screen.findByRole("button", { name: ja.settings.backup.location.open }));
    await waitFor(() => expect(calls("open_backup_dir")).toHaveLength(1));
  });

  it("書き出し: CSV は全件（ids なし）、JSON は全データ。保存先はダイアログで選ぶ", async () => {
    chooseSavePath.mockResolvedValueOnce("/tmp/books.csv").mockResolvedValueOnce("/tmp/books.json");
    renderSettings("backup");
    fireEvent.click(await screen.findByRole("button", { name: ja.settings.backup.export.csv }));
    await waitFor(() => expect(calls("export_csv")).toHaveLength(1));
    expect(calls("export_csv")[0]![1]).toEqual({ path: "/tmp/books.csv", ids: undefined });
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.toast.exported(123)));

    fireEvent.click(screen.getByRole("button", { name: ja.settings.backup.export.json }));
    await waitFor(() => expect(calls("export_json")).toHaveLength(1));
    expect(calls("export_json")[0]![1]).toEqual({ path: "/tmp/books.json" });
  });

  it("書き出しの保存先を選ばなければ、何も書かない", async () => {
    chooseSavePath.mockResolvedValue(null);
    renderSettings("backup");
    fireEvent.click(await screen.findByRole("button", { name: ja.settings.backup.export.csv }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(calls("export_csv")).toHaveLength(0);
  });
});

describe("このアプリについて（FR-CONF-07、9.4、10.6）", () => {
  it("バージョン、端末 ID、各取得元のクレジット表記（NDLサーチの API を使っている旨を含む）", async () => {
    renderSettings("about");
    await waitFor(() => expect(screen.getByTestId("about-version")).toHaveTextContent("9.9.9"));
    expect(await screen.findByText("d-0123456789ab")).toBeInTheDocument();

    const credits = await screen.findByTestId("credits");
    await waitFor(() =>
      expect(credits).toHaveTextContent(
        "書誌情報: 国立国会図書館サーチ API (https://ndlsearch.ndl.go.jp/)",
      ),
    );
    expect(credits).toHaveTextContent("書誌情報・書影: openBD (https://openbd.jp/)");
    expect(credits).toHaveTextContent("Powered by Google Books API (https://books.google.com/)");
    expect(credits).toHaveTextContent("書誌情報・書影: Open Library (https://openlibrary.org/)");
    expect(screen.getByText(ja.settings.about.ndlNote)).toBeInTheDocument();
    expect(ja.settings.about.ndlNote).toContain("NDLサーチ");
  });

  it("取得元の一覧を読めないとき（ブラウザなど）も、クレジットと NDL の表記を出す", () => {
    isTauri.mockReturnValue(false);
    renderSettings("about");
    const credits = screen.getByTestId("credits");
    expect(credits).toHaveTextContent("国立国会図書館サーチ API");
    expect(credits).toHaveTextContent("openBD");
    expect(credits).toHaveTextContent("Google Books");
    expect(credits).toHaveTextContent("Open Library (https://openlibrary.org/)");
    expect(screen.getByText(ja.settings.about.ndlNote)).toBeInTheDocument();
  });

  it("ログフォルダを開く", async () => {
    renderSettings("about");
    fireEvent.click(await screen.findByRole("button", { name: ja.settings.about.openLogs }));
    await waitFor(() => expect(calls("open_logs_dir")).toHaveLength(1));
  });
});
