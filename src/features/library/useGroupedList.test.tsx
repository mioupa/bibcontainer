import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useGroupedList } from "@/features/library/useGroupedList";
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
});
