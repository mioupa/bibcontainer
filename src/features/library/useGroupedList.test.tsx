import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useGroupedList } from "@/features/library/useGroupedList";
import type { PendingWorkSelection } from "@/features/list/selection";
import { useGroupStore } from "@/stores/groupStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { makeRow } from "@/test/factories";
import type { ListRow } from "@/types/contracts";

const author = [{ name: "山田 太郎", kana: null, role: null }];
const volume = (title: string, n: number) =>
  makeRow({
    id: `${title}-${n}`,
    title,
    sortTitle: title,
    volume: String(n),
    volumeSort: n,
    authors: author,
  });
const rows: ListRow[] = [
  volume("風", 1),
  volume("風", 2),
  makeRow({ id: "solo", title: "雲", sortTitle: "雲" }),
  volume("海", 1),
  volume("海", 2),
];

describe("useGroupedList", () => {
  beforeEach(() => {
    useLayoutStore.setState({ groupVolumes: true });
    useGroupStore.getState().reset();
    useSelectionStore.getState().clear();
  });
  afterEach(() => {
    useLayoutStore.setState({ groupVolumes: false });
  });

  it("設定がオンなら、グループにまとめる。オフなら、1 冊ずつの行", () => {
    const { result } = renderHook(() => useGroupedList(rows));
    expect(result.current.enabled).toBe(true);
    expect(result.current.entries.map((e) => e.kind)).toEqual(["group", "book", "group"]);
    act(() => {
      useLayoutStore.setState({ groupVolumes: false });
    });
    expect(result.current.enabled).toBe(false);
    expect(result.current.entries.map((e) => e.key)).toEqual(rows.map((r) => r.id));
  });

  it("開いたグループ（groupStore）は、メンバーの行が続く。展開の状態だけが変わるときは、グループの計算を使い回す", () => {
    const { result } = renderHook(() => useGroupedList(rows));
    const before = result.current;
    const key = before.entries[0]?.key ?? "";
    act(() => useGroupStore.getState().expand(key));
    expect(result.current.entries.map((e) => e.key)).toEqual([
      key,
      "風-1",
      "風-2",
      "solo",
      before.entries.at(-1)?.key,
    ]);
    expect(result.current.groups).toBe(before.groups);
    expect(result.current.bookIds).toBe(before.bookIds);
  });

  it("focus している本が、畳まれたグループの巻なら、そのグループだけが開き、開いた状態が保存される", () => {
    const { result } = renderHook(() => useGroupedList(rows));
    expect(useGroupStore.getState().expanded.size).toBe(0);
    act(() => useSelectionStore.getState().reveal("海-2"));
    const open = result.current.entries.filter((e) => e.kind === "group" && e.expanded);
    expect(open).toHaveLength(1);
    expect(result.current.entries.map((e) => e.key)).toContain("海-1");
    expect(result.current.entries.map((e) => e.key)).not.toContain("風-1");
    // effect のあと、groupStore にも入っている
    expect([...useGroupStore.getState().expanded]).toEqual([open[0]?.key]);
  });

  it("まとめる設定をオフにすると、選択のグループがメンバーに、focus のグループが最初のメンバーに置き換わる", () => {
    const { result } = renderHook(() => useGroupedList(rows));
    const key = result.current.entries[0]?.key ?? "";
    act(() => {
      useSelectionStore.setState({ selected: new Set([key, "solo"]), anchor: key, focus: key });
    });
    act(() => {
      useLayoutStore.setState({ groupVolumes: false });
    });
    const state = useSelectionStore.getState();
    expect([...state.selected].sort()).toEqual(["solo", "風-1", "風-2"]);
    expect(state.focus).toBe("風-1");
    expect(state.anchor).toBe("風-1");
  });

  it("オフのとき、グループの選択がなければ、選択には触らない", () => {
    useLayoutStore.setState({ groupVolumes: false });
    act(() => {
      useSelectionStore.setState({ selected: new Set(["solo"]), anchor: "solo", focus: "solo" });
    });
    const before = useSelectionStore.getState().selected;
    renderHook(() => useGroupedList(rows));
    expect(useSelectionStore.getState().selected).toBe(before);
  });

  // 作品の操作（まとめる・外す・自動に戻す。0026）のあとの選択。操作は Rust で済んでも、行の更新はあとから届く
  describe("作品の操作のあとの選択（pendingWork）", () => {
    const KEY = "work:\u001fw-1";
    const MERGED = ["solo", "海-1", "海-2"];
    const withWork = (target: readonly string[], workId: string | null) =>
      rows.map((row) => (target.includes(row.id) ? { ...row, workId } : row));
    const pending = (over: Partial<PendingWorkSelection> = {}): PendingWorkSelection => ({
      workId: "w-1",
      bookIds: MERGED,
      groupKey: KEY,
      expiresAt: Date.now() + 10_000,
      ...over,
    });
    const render = (initial: readonly ListRow[] = rows) =>
      renderHook(({ list }) => useGroupedList(list), { initialProps: { list: initial } });
    const selection = () => {
      const { selected, anchor, focus } = useSelectionStore.getState();
      return { selected: [...selected].sort(), anchor, focus };
    };

    it("行が更新されるまでは選択に触らず、更新が届いたら、できた作品の行を選ぶ（畳んだまま）", () => {
      const view = render();
      const seaKey = view.result.current.entries[2]?.key ?? "";
      act(() => {
        useSelectionStore.setState({
          selected: new Set([seaKey, "solo"]),
          anchor: "solo",
          focus: "solo",
        });
        useSelectionStore.getState().setPendingWork(pending());
      });
      // まだ古い行（workId がない）なので、待つ
      expect(selection().selected).toEqual([seaKey, "solo"].sort());
      expect(useSelectionStore.getState().pendingWork).not.toBeNull();
      const tick = useSelectionStore.getState().revealTick;

      view.rerender({ list: withWork(MERGED, "w-1") });
      expect(selection()).toEqual({ selected: [KEY], anchor: KEY, focus: KEY });
      expect(useSelectionStore.getState().pendingWork).toBeNull();
      // 一覧がその行までスクロールするよう、revealTick が進む
      expect(useSelectionStore.getState().revealTick).toBe(tick + 1);
      // 作品の行は畳んだまま（グループの中の本に focus が来ていないので、開かない）
      const group = view.result.current.entries.find((e) => e.key === KEY);
      expect(group).toMatchObject({ kind: "group", expanded: false });
      expect(useGroupStore.getState().expanded.has(KEY)).toBe(false);
    });

    it("作品の行が並びになければ（絞り込みで 1 冊だけ）、表示中の対象の本を選ぶ", () => {
      // 海-1・海-2 は絞り込みで隠れている
      const shown = withWork(MERGED, "w-1").filter((row) => !row.id.startsWith("海"));
      const view = render(rows.filter((row) => !row.id.startsWith("海")));
      act(() => useSelectionStore.getState().setPendingWork(pending()));
      view.rerender({ list: shown });
      expect(selection()).toEqual({ selected: ["solo"], anchor: "solo", focus: "solo" });
      expect(useSelectionStore.getState().pendingWork).toBeNull();
    });

    it("作品から外した・自動に戻したあとは、更新が届いたら、対象の本を選ぶ（グループの行ではなく）", () => {
      const view = render();
      const windKey = view.result.current.entries[0]?.key ?? "";
      act(() => {
        useSelectionStore.setState({
          selected: new Set([windKey]),
          anchor: windKey,
          focus: windKey,
        });
        useSelectionStore
          .getState()
          .setPendingWork(pending({ workId: "none", bookIds: ["風-1", "風-2"], groupKey: null }));
      });
      expect(selection().selected).toEqual([windKey]);

      view.rerender({ list: withWork(["風-1", "風-2"], "none") });
      expect(selection()).toEqual({
        selected: ["風-1", "風-2"],
        anchor: "風-1",
        focus: "風-1",
      });
      expect(useSelectionStore.getState().pendingWork).toBeNull();
      // グループがなくなったので、風 1・風 2 はふつうの行（グループを開いた状態は残さない）
      expect(view.result.current.entries.map((e) => e.key)).toContain("風-1");
      expect(view.result.current.groups.size).toBe(1);
      expect(useGroupStore.getState().expanded.size).toBe(0);
    });

    it("期限を過ぎても更新が届かなければ、選ばずに捨てる（あとの選択を奪わない）", () => {
      const view = render();
      act(() => {
        useSelectionStore.setState({ selected: new Set(["solo"]), anchor: "solo", focus: "solo" });
        useSelectionStore.getState().setPendingWork(pending({ expiresAt: Date.now() - 1 }));
      });
      view.rerender({ list: [...rows] });
      expect(useSelectionStore.getState().pendingWork).toBeNull();
      expect(selection()).toEqual({ selected: ["solo"], anchor: "solo", focus: "solo" });
    });

    it("更新が届くまでに、利用者が選び直したら（クリック・矢印・⌘A・Esc・検索欄の ISBN）、更新待ちの選択は取り消す", () => {
      const ids = ["solo", "海-1"];
      const store = () => useSelectionStore.getState();
      const changes: [string, () => void][] = [
        ["click", () => store().click(ids, "solo", { toggle: false, range: false })],
        ["move", () => store().move(ids, "down", { extend: false, pageSize: 1 })],
        ["moveTo", () => store().moveTo(ids, "solo", { extend: false })],
        ["selectAll", () => store().selectAll(ids)],
        ["reveal", () => store().reveal("solo")],
        ["clear", () => store().clear()],
      ];
      render();
      for (const [name, change] of changes) {
        act(() => store().setPendingWork(pending()));
        expect(store().pendingWork, name).not.toBeNull();
        act(change);
        expect(store().pendingWork, name).toBeNull();
      }
    });
  });
});
