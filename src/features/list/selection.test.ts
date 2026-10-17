import { describe, expect, it } from "vitest";
import {
  EMPTY_SELECTION,
  clickRow,
  moveFocus,
  moveFocusTo,
  pruneSelection,
  selectAll,
  type SelectionState,
} from "@/features/list/selection";

const ids = ["a", "b", "c", "d", "e", "f"];
const none = { toggle: false, range: false };
const toggle = { toggle: true, range: false };
const range = { toggle: false, range: true };
const both = { toggle: true, range: true };
const sel = (state: SelectionState) => [...state.selected].sort();

describe("clickRow", () => {
  it("普通のクリックは、その行だけを選ぶ", () => {
    const s = clickRow(clickRow(EMPTY_SELECTION, ids, "b", none), ids, "d", none);
    expect(sel(s)).toEqual(["d"]);
    expect(s.anchor).toBe("d");
    expect(s.focus).toBe("d");
  });

  it("⌘／Ctrl+クリックは、1 行ずつ足したり外したりする", () => {
    let s = clickRow(EMPTY_SELECTION, ids, "a", none);
    s = clickRow(s, ids, "c", toggle);
    expect(sel(s)).toEqual(["a", "c"]);
    s = clickRow(s, ids, "a", toggle);
    expect(sel(s)).toEqual(["c"]);
  });

  it("Shift+クリックは、起点から範囲で選ぶ（上下どちらの向きでも）", () => {
    let s = clickRow(EMPTY_SELECTION, ids, "b", none);
    s = clickRow(s, ids, "e", range);
    expect(sel(s)).toEqual(["b", "c", "d", "e"]);
    expect(s.anchor).toBe("b");
    s = clickRow(s, ids, "a", range);
    expect(sel(s)).toEqual(["a", "b"]);
  });

  it("⌘／Ctrl+Shift+クリックは、いまの選択に範囲を足す", () => {
    let s = clickRow(EMPTY_SELECTION, ids, "a", none);
    s = clickRow(s, ids, "e", toggle);
    s = clickRow(s, ids, "f", both);
    expect(sel(s)).toEqual(["a", "e", "f"]);
  });

  it("起点がないときの Shift+クリックは、その行だけを選ぶ", () => {
    expect(sel(clickRow(EMPTY_SELECTION, ids, "c", range))).toEqual(["c"]);
  });

  it("一覧にない ID は無視する", () => {
    const s = clickRow(EMPTY_SELECTION, ids, "zzz", none);
    expect(s).toBe(EMPTY_SELECTION);
  });
});

describe("moveFocus", () => {
  const opts = { extend: false, pageSize: 3 };

  it("矢印で動かすと、その行だけが選ばれる。端で止まる", () => {
    let s = moveFocus(EMPTY_SELECTION, ids, "down", opts);
    expect(s.focus).toBe("a");
    s = moveFocus(s, ids, "down", opts);
    expect(s.focus).toBe("b");
    expect(sel(s)).toEqual(["b"]);
    s = moveFocus(s, ids, "up", opts);
    s = moveFocus(s, ids, "up", opts);
    expect(s.focus).toBe("a");
    const last = moveFocus(moveFocus(EMPTY_SELECTION, ids, "last", opts), ids, "down", opts);
    expect(last.focus).toBe("f");
  });

  it("何も選んでいないとき、上矢印は最後の行へ", () => {
    expect(moveFocus(EMPTY_SELECTION, ids, "up", opts).focus).toBe("f");
  });

  it("Shift+矢印は、起点を動かさずに範囲を広げ縮める", () => {
    let s = clickRow(EMPTY_SELECTION, ids, "c", none);
    s = moveFocus(s, ids, "down", { extend: true, pageSize: 3 });
    s = moveFocus(s, ids, "down", { extend: true, pageSize: 3 });
    expect(sel(s)).toEqual(["c", "d", "e"]);
    expect(s.anchor).toBe("c");
    s = moveFocus(s, ids, "up", { extend: true, pageSize: 3 });
    expect(sel(s)).toEqual(["c", "d"]);
    // 起点より上へ越えると、向きが逆の範囲になる
    for (let i = 0; i < 3; i++) s = moveFocus(s, ids, "up", { extend: true, pageSize: 3 });
    expect(sel(s)).toEqual(["a", "b", "c"]);
    expect(s.focus).toBe("a");
  });

  it("Home / End / PageUp / PageDown", () => {
    expect(moveFocus(EMPTY_SELECTION, ids, "first", opts).focus).toBe("a");
    expect(moveFocus(EMPTY_SELECTION, ids, "last", opts).focus).toBe("f");
    const start = clickRow(EMPTY_SELECTION, ids, "a", none);
    expect(moveFocus(start, ids, "pageDown", opts).focus).toBe("d");
    const end = clickRow(EMPTY_SELECTION, ids, "f", none);
    expect(moveFocus(end, ids, "pageUp", opts).focus).toBe("c");
    expect(
      moveFocus(moveFocus(start, ids, "last", { extend: true, pageSize: 3 }), ids, "first", {
        extend: true,
        pageSize: 3,
      }).selected.size,
    ).toBe(1);
  });

  it("行がないときは何も変えない", () => {
    expect(moveFocus(EMPTY_SELECTION, [], "down", opts)).toBe(EMPTY_SELECTION);
  });
});

describe("moveFocusTo（行き先が決まった focus の移動。グリッドの上下左右）", () => {
  it("行き先の行だけを選び、起点・focus もそこへ動かす。行がなければ何も変えない", () => {
    const s = moveFocusTo(clickRow(EMPTY_SELECTION, ids, "b", none), ids, "e", { extend: false });
    expect(sel(s)).toEqual(["e"]);
    expect(s).toMatchObject({ anchor: "e", focus: "e" });
    const same = clickRow(EMPTY_SELECTION, ids, "b", none);
    expect(moveFocusTo(same, ids, "zzz", { extend: false })).toBe(same);
    expect(moveFocusTo(EMPTY_SELECTION, [], "a", { extend: true })).toBe(EMPTY_SELECTION);
  });

  it("extend（Shift）は、起点を動かさずに、起点から行き先までを範囲で選ぶ。向きが逆でもよい", () => {
    let s = clickRow(EMPTY_SELECTION, ids, "c", none);
    s = moveFocusTo(s, ids, "e", { extend: true });
    expect(sel(s)).toEqual(["c", "d", "e"]);
    expect(s).toMatchObject({ anchor: "c", focus: "e" });
    s = moveFocusTo(s, ids, "a", { extend: true });
    expect(sel(s)).toEqual(["a", "b", "c"]);
    expect(s).toMatchObject({ anchor: "c", focus: "a" });
  });

  it("起点がないときの起点は、動かす前の focus（moveFocus と同じ）。focus もなければ先頭の行", () => {
    const focusOnly: SelectionState = { selected: new Set(["b"]), anchor: null, focus: "b" };
    expect(sel(moveFocusTo(focusOnly, ids, "d", { extend: true }))).toEqual(["b", "c", "d"]);
    expect(moveFocusTo(focusOnly, ids, "d", { extend: true }).anchor).toBe("b");
    // Shift+クリックの規則とは、起点がないときだけ違う（クリックは押した行が起点）
    expect(sel(clickRow(focusOnly, ids, "d", { toggle: false, range: true }))).toEqual(["d"]);

    const s = moveFocusTo(EMPTY_SELECTION, ids, "c", { extend: true });
    expect(sel(s)).toEqual(["a", "b", "c"]);
    expect(s).toMatchObject({ anchor: "a", focus: "c" });
    // 先頭の行へ動かすなら、その行だけ
    expect(sel(moveFocusTo(EMPTY_SELECTION, ids, "a", { extend: true }))).toEqual(["a"]);
  });

  it("起点が表示から消えているときも、focus から範囲を選び直す", () => {
    const stale: SelectionState = { selected: new Set(["x"]), anchor: "x", focus: "b" };
    const s = moveFocusTo(stale, ids, "d", { extend: true });
    expect(sel(s)).toEqual(["b", "c", "d"]);
    expect(s.anchor).toBe("b");
  });

  it("moveFocus が行き先を決めたあとの結果と同じ", () => {
    for (const start of ["a", "c", "f"]) {
      for (const target of ["down", "up", "first", "last", "pageDown", "pageUp"] as const) {
        for (const extend of [false, true]) {
          const from = clickRow(EMPTY_SELECTION, ids, start, none);
          const moved = moveFocus(from, ids, target, { extend, pageSize: 2 });
          const viaTo = moveFocusTo(from, ids, moved.focus!, { extend });
          expect(sel(viaTo), `${start} ${target} ${extend}`).toEqual(sel(moved));
          expect(viaTo.anchor).toBe(moved.anchor);
        }
      }
    }
  });
});

describe("selectAll / pruneSelection", () => {
  it("すべて選ぶ", () => {
    expect(sel(selectAll(EMPTY_SELECTION, ids))).toEqual(ids);
  });

  it("表示から消えた行を外す。変わらなければ同じ参照を返す", () => {
    const s = clickRow(clickRow(EMPTY_SELECTION, ids, "a", none), ids, "c", toggle);
    const pruned = pruneSelection(s, new Set(["a", "b"]));
    expect(sel(pruned)).toEqual(["a"]);
    expect(pruned.focus).toBeNull();
    expect(pruned.anchor).toBe(null);
    expect(pruneSelection(s, new Set(ids))).toBe(s);
  });
});
