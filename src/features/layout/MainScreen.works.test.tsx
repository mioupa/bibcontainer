// 作品の手動のまとめ・外し・自動に戻す（0026。受け入れ基準 AT-36・AT-37）を、メイン画面から、モックのバックエンド
// （Tauri の外。src/dev/mockBackend.ts）で通して確かめる。Rust の merge_works と同じ規則のモックなので、まとめ方・選択・通知まで
// 画面の側の流れを確かめられる。実機の操作は docs/acceptance/phase3.md の手順。
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { FieldChange } from "@/bindings/FieldChange";
import { TooltipProvider } from "@/components/ui/tooltip";
import { mockBackend } from "@/dev/mockBackend";
import { MainScreen } from "@/features/layout/MainScreen";
import { applyLibraryChange } from "@/features/library/sync";
import { DEFAULT_SCAN_SETTINGS } from "@/features/scan/config";
import { ja } from "@/i18n/ja";
import { useEditorStore } from "@/stores/editorStore";
import { useGroupStore } from "@/stores/groupStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useScreenStore } from "@/stores/screenStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";
import { useViewStore } from "@/stores/viewStore";
import { stubElementSize } from "@/test/layout";

// Tauri の外（モックで動く）にする。ダミーの蔵書は空にして（?mockRows=0）、このテストが作った本だけにする
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
);
vi.mock("sonner", () => ({ toast }));

stubElementSize({ width: 1400, height: 700 });

beforeAll(() => {
  window.history.replaceState(null, "", "/?mockRows=0");
});

const manual = { kind: "manual" } as const;
/** モックの「本を作る」の下書き（書名・巻・著者）。 */
function draft(title: string, volume: number | null, authorName: string) {
  const changes: FieldChange[] = [
    { field: "title", value: title, origin: manual },
    { field: "volume", value: volume === null ? null : String(volume), origin: manual },
    { field: "volume_sort", value: volume, origin: manual },
    { field: "authors", value: [{ name: authorName, kana: null, role: null }], origin: manual },
  ];
  return { changes, seriesName: null };
}
const OWNER = "うちの 太郎";

/** 蔵書をすべて消して、本を作る。作った本の ID を渡した順に返す。 */
async function seed(drafts: ReturnType<typeof draft>[]): Promise<string[]> {
  const all = await mockBackend.listBooks();
  await mockBackend.purgeBooks(all.map((row) => row.id));
  return mockBackend.createBooksFromDrafts(drafts);
}

function renderMain() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <MainScreen />
      </TooltipProvider>
    </QueryClientProvider>,
  );
  return { ...view, client };
}

// 行の key に制御文字（\u001f）があるので、属性セレクターでなく、属性の値で探す
const allRowEls = () => [...document.querySelectorAll<HTMLElement>("[data-row-id]")];
const byKey = (key: string) => allRowEls().find((el) => el.dataset.rowId === key) ?? null;
const rowKeys = () => allRowEls().map((el) => el.dataset.rowId);
const groupRowEl = () => allRowEls().find((el) => el.dataset.rowId?.startsWith("work:")) ?? null;
const groupKey = () => groupRowEl()?.dataset.rowId ?? "";
const selectedKeys = () => [...useSelectionStore.getState().selected].sort();
const runMenu = async (rowKey: string, name: string) => {
  fireEvent.contextMenu(byKey(rowKey)!);
  const menu = await screen.findByRole("menu");
  fireEvent.click(within(menu).getByRole("menuitem", { name }));
};

describe("メイン画面: 作品の手動のまとめ・外し（AT-36・AT-37）", () => {
  let volumeIds: string[];
  let anthologyId: string;

  beforeEach(async () => {
    invoke.mockReset();
    for (const fn of [toast.success, toast.info, toast.error]) fn.mockReset();
    useViewStore.getState().clearFilters();
    useViewStore.getState().setSearchText("");
    useSelectionStore.getState().clear();
    useTableStore.getState().resetColumns();
    useLayoutStore.setState({
      sidebarOpen: true,
      detailOpen: false,
      viewMode: "table",
      groupVolumes: true,
    });
    useSettingsStore.setState({ theme: "system", scan: { ...DEFAULT_SCAN_SETTINGS } });
    useScreenStore.setState({ screen: "main", handoff: null });
    useEditorStore.getState().close();
    useGroupStore.getState().reset();

    // スローループ 1〜7 巻（同じ著者）と、書名に作品名を含むが著者が違い、巻のない「公式アンソロジー」
    const ids = await seed([
      ...[1, 2, 3, 4, 5, 6, 7].map((n) => draft("スローループ", n, OWNER)),
      draft("スローループ 公式アンソロジー", null, "アンソロジー 編集部"),
    ]);
    volumeIds = ids.slice(0, 7);
    anthologyId = ids[7] as string;
  });

  it("AT-36: 作品の行と別の本を選んで「1 つの作品にまとめる」と、1 つの作品（8 冊）になり、その行が選ばれる。あとから登録した巻（自動の 1 冊）は、その作品に入る", async () => {
    const { client } = renderMain();
    await screen.findByRole("treegrid");
    await waitFor(() => expect(groupRowEl()).not.toBeNull());
    // 巻の 7 冊が 1 行にまとまり、アンソロジーは別の行（著者が違うので、自動ではまとまらない）
    expect(within(groupRowEl()!).getByText("7 冊")).toBeInTheDocument();
    expect(byKey(anthologyId)).not.toBeNull();
    const autoKey = groupKey();
    expect(autoKey.startsWith("work:すろーるーぷ\u001f")).toBe(true);

    // 作品の行と、アンソロジーの行を選んで、メニューから「1 つの作品にまとめる」
    fireEvent.click(groupRowEl()!);
    fireEvent.click(byKey(anthologyId)!, { metaKey: true });
    expect(selectedKeys()).toEqual([autoKey, anthologyId].sort());
    await runMenu(anthologyId, ja.list.contextMenu.mergeWorks);

    // 8 冊が 1 つの作品（手動）。その行が、畳んだまま、選ばれて focus される
    await waitFor(() => expect(within(groupRowEl()!).getByText("8 冊")).toBeInTheDocument());
    expect(toast.success).toHaveBeenCalledWith(ja.toast.worksMerged(8));
    const mergedKey = groupKey();
    expect(mergedKey.startsWith("work:\u001f")).toBe(true);
    expect(mergedKey).not.toBe(autoKey);
    expect(byKey(anthologyId)).toBeNull();
    await waitFor(() => expect(selectedKeys()).toEqual([mergedKey]));
    expect(useSelectionStore.getState().focus).toBe(mergedKey);
    expect(useSelectionStore.getState().pendingWork).toBeNull();
    expect(groupRowEl()).toHaveAttribute("aria-selected", "true");
    expect(groupRowEl()).toHaveAttribute("aria-expanded", "false");
    expect(rowKeys()).toEqual([mergedKey]);

    // あとから登録した巻（自動の 1 冊。書名の末尾だけに巻がある）は、書名・著者が同じなので、その作品に入る（AT-37 の前半）
    const [newId] = await mockBackend.createBooksFromDrafts([draft("スローループ 8", null, OWNER)]);
    await act(async () => {
      await applyLibraryChange(client, { books: [newId as string], others: false });
    });
    await waitFor(() => expect(within(groupRowEl()!).getByText("9 冊")).toBeInTheDocument());
    expect(groupKey()).toBe(mergedKey);
    expect(rowKeys()).toEqual([mergedKey]);
  });

  it("AT-37: 作品の中の 1 冊を「作品から外す」と、ふつうの行になり、選ばれる。「自動に戻す」で作品に戻る", async () => {
    // 先にまとめておく（モックのバックエンドで）
    await mockBackend.mergeWorks([...volumeIds, anthologyId]);
    renderMain();
    await screen.findByRole("treegrid");
    await waitFor(() => expect(groupRowEl()).not.toBeNull());
    const key = groupKey();
    expect(within(groupRowEl()!).getByText("8 冊")).toBeInTheDocument();

    // 作品を開いて、3 巻を選び、「作品から外す」
    fireEvent.click(within(groupRowEl()!).getByRole("button", { name: ja.list.group.expand }));
    const third = volumeIds[2] as string;
    fireEvent.click(byKey(third)!);
    await runMenu(third, ja.list.contextMenu.excludeFromWorks);

    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.toast.worksExcluded(1)));
    // 作品は 7 冊に減り、3 巻は、作品の外のふつうの行（字下げなし）になる
    await waitFor(() => expect(within(groupRowEl()!).getByText("7 冊")).toBeInTheDocument());
    expect(groupKey()).toBe(key);
    expect(byKey(third)).toHaveAttribute("aria-level", "1");
    expect(byKey(third)).not.toHaveAttribute("aria-expanded");
    await waitFor(() => expect(selectedKeys()).toEqual([third]));
    expect(useSelectionStore.getState().focus).toBe(third);
    expect((await mockBackend.getRows([third]))[0]?.workId).toBe("none");
    expect(useSelectionStore.getState().pendingWork).toBeNull();

    // もう一度「作品から外す」を選ぶことはできない（押せない）。「自動に戻す」で、元の作品に戻る
    fireEvent.contextMenu(byKey(third)!);
    const menu = await screen.findByRole("menu");
    expect(
      within(menu).getByRole("menuitem", { name: ja.list.contextMenu.excludeFromWorks }),
    ).toHaveAttribute("data-disabled");
    fireEvent.click(within(menu).getByRole("menuitem", { name: ja.list.contextMenu.resetWorks }));

    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(ja.toast.worksReset(1)));
    // 3 巻の上書きがなくなり、書名・著者が同じなので、自動で、同じ手動の作品に入る（作品は開いたまま。3 巻は字下げされる）
    await waitFor(() => expect(within(groupRowEl()!).getByText("8 冊")).toBeInTheDocument());
    expect((await mockBackend.getRows([third]))[0]?.workId).toBeNull();
    expect(groupKey()).toBe(key);
    await waitFor(() => expect(byKey(third)).toHaveAttribute("aria-level", "2"));
    expect(selectedKeys()).toEqual([third]);
  });

  it("まとめる対象が 1 冊だけなら、押せない。「巻をまとめる」をオフにすると、項目を出さない", async () => {
    renderMain();
    await screen.findByRole("treegrid");
    await waitFor(() => expect(byKey(anthologyId)).not.toBeNull());
    fireEvent.contextMenu(byKey(anthologyId)!);
    let menu = await screen.findByRole("menu");
    expect(
      within(menu).getByRole("menuitem", { name: ja.list.contextMenu.mergeWorks }),
    ).toHaveAttribute("data-disabled");
    fireEvent.keyDown(menu, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());

    act(() => {
      useLayoutStore.setState({ groupVolumes: false });
    });
    await waitFor(() => expect(screen.queryByRole("treegrid")).not.toBeInTheDocument());
    fireEvent.contextMenu(byKey(anthologyId)!);
    menu = await screen.findByRole("menu");
    expect(
      within(menu).queryByRole("menuitem", { name: ja.list.contextMenu.mergeWorks }),
    ).not.toBeInTheDocument();
  });
});
