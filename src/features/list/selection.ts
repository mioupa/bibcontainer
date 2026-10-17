// 一覧の行の選択（クリック、⌘／Ctrl+クリック、Shift で範囲、矢印キー）。React に依存しない純粋関数。
//
// 選択は「選ばれた行の ID の集合」と、2 つの目印で表す。
//   anchor: Shift で範囲を選ぶときの起点（最後に普通にクリック・移動した行）
//   focus : キーボードで動かす現在の行（詳細ペインにはこの行を出す）
// 範囲は「いま表示している並び」（`ids`）の上で数える。絞り込みや並べ替えで並びが変わったら、pruneSelection で整える。

export interface SelectionState {
  selected: ReadonlySet<string>;
  anchor: string | null;
  focus: string | null;
}

export const EMPTY_SELECTION: SelectionState = {
  selected: new Set(),
  anchor: null,
  focus: null,
};

export interface ClickModifiers {
  /** ⌘（macOS）／Ctrl（Windows）。行を 1 つずつ足したり外したりする。 */
  toggle: boolean;
  /** Shift。起点から押した行までを範囲で選ぶ。 */
  range: boolean;
}

export type MoveTarget = "up" | "down" | "first" | "last" | "pageUp" | "pageDown";

function rangeIds(ids: readonly string[], from: number, to: number): string[] {
  const start = Math.min(from, to);
  const end = Math.max(from, to);
  return ids.slice(start, end + 1);
}

/** 行をクリックしたときの新しい選択。`id` が `ids` にないときは何も変えない。 */
export function clickRow(
  state: SelectionState,
  ids: readonly string[],
  id: string,
  modifiers: ClickModifiers,
): SelectionState {
  const index = ids.indexOf(id);
  if (index < 0) return state;

  if (modifiers.range) {
    const anchor = state.anchor !== null && ids.includes(state.anchor) ? state.anchor : id;
    const anchorIndex = ids.indexOf(anchor);
    const range = rangeIds(ids, anchorIndex, index);
    // ⌘／Ctrl+Shift は、いまの選択に範囲を足す。Shift だけなら範囲に置き換える
    const selected = modifiers.toggle ? new Set([...state.selected, ...range]) : new Set(range);
    return { selected, anchor, focus: id };
  }

  if (modifiers.toggle) {
    const selected = new Set(state.selected);
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    return { selected, anchor: id, focus: id };
  }

  return { selected: new Set([id]), anchor: id, focus: id };
}

/** 矢印キーなどで focus を動かしたときの新しい選択。`extend` が true（Shift）なら起点から範囲で選ぶ。 */
export function moveFocus(
  state: SelectionState,
  ids: readonly string[],
  target: MoveTarget,
  options: { extend: boolean; pageSize: number },
): SelectionState {
  if (ids.length === 0) return state;
  const current = state.focus === null ? -1 : ids.indexOf(state.focus);
  const last = ids.length - 1;
  const page = Math.max(1, options.pageSize);

  let next: number;
  switch (target) {
    case "up":
      next = current < 0 ? last : Math.max(0, current - 1);
      break;
    case "down":
      next = current < 0 ? 0 : Math.min(last, current + 1);
      break;
    case "first":
      next = 0;
      break;
    case "last":
      next = last;
      break;
    case "pageUp":
      next = current < 0 ? 0 : Math.max(0, current - page);
      break;
    case "pageDown":
      next = current < 0 ? Math.min(last, page - 1) : Math.min(last, current + page);
      break;
  }

  const nextId = ids[next];
  if (nextId === undefined) return state;
  return moveFocusTo(state, ids, nextId, options);
}

/**
 * focus を `id` の行へ動かしたときの新しい選択。動かす先が決まっているとき（グリッドの上下左右など）に使う。
 * `moveFocus` と同じ規則で、`extend` が true（Shift）なら起点から `id` までを範囲で選ぶ（起点がなければ、いまの focus から。
 * focus もなければ先頭の行から）。`clickRow` の Shift+クリックとは、起点がないときの起点だけが違う
 * （クリックは押した行から、キーボードは動かす前の行から）。`id` が `ids` にないときは何も変えない。
 */
export function moveFocusTo(
  state: SelectionState,
  ids: readonly string[],
  id: string,
  options: { extend: boolean },
): SelectionState {
  const next = ids.indexOf(id);
  if (next < 0) return state;

  if (options.extend) {
    const current = state.focus === null ? -1 : ids.indexOf(state.focus);
    const anchor =
      state.anchor !== null && ids.includes(state.anchor)
        ? state.anchor
        : (ids[Math.max(current, 0)] ?? id);
    return {
      selected: new Set(rangeIds(ids, ids.indexOf(anchor), next)),
      anchor,
      focus: id,
    };
  }
  return { selected: new Set([id]), anchor: id, focus: id };
}

export function selectAll(state: SelectionState, ids: readonly string[]): SelectionState {
  if (ids.length === 0) return state;
  return {
    selected: new Set(ids),
    anchor: state.anchor !== null && ids.includes(state.anchor) ? state.anchor : (ids[0] ?? null),
    focus: state.focus !== null && ids.includes(state.focus) ? state.focus : (ids[0] ?? null),
  };
}

/** 絞り込みや削除で表示から消えた行を、選択・起点・focus から外す。変わらなければ同じ参照を返す。 */
export function pruneSelection(
  state: SelectionState,
  visible: ReadonlySet<string>,
): SelectionState {
  const has = (id: string) => visible.has(id);
  let changed = false;
  const selected = new Set<string>();
  for (const id of state.selected) {
    if (has(id)) selected.add(id);
    else changed = true;
  }
  const anchor = state.anchor !== null && has(state.anchor) ? state.anchor : null;
  const focus = state.focus !== null && has(state.focus) ? state.focus : null;
  if (anchor !== state.anchor || focus !== state.focus) changed = true;
  return changed ? { selected, anchor, focus } : state;
}

/**
 * 巻のグループを畳んだときの選択（FR-LIST-08）。畳むと、メンバーの行は一覧から消えるので、選んでいたメンバーは
 * グループの key の選択に置き換える（グループの行を選んだことになる）。起点・focus がメンバーなら、グループの key に移す。
 * 変わらなければ同じ参照を返す。
 */
export function collapseSelection(
  state: SelectionState,
  groupKey: string,
  memberIds: readonly string[],
): SelectionState {
  const members = new Set(memberIds);
  let hit = false;
  const selected = new Set<string>();
  for (const id of state.selected) {
    if (members.has(id)) hit = true;
    else selected.add(id);
  }
  if (hit) selected.add(groupKey);
  const anchor = state.anchor !== null && members.has(state.anchor) ? groupKey : state.anchor;
  const focus = state.focus !== null && members.has(state.focus) ? groupKey : state.focus;
  if (!hit && anchor === state.anchor && focus === state.focus) return state;
  return { selected, anchor, focus };
}

/**
 * 巻をまとめる設定をオフにしたときの選択。選んでいたグループの key は、メンバーの ID に置き換える（メンバーの行が一覧に現れる）。
 * 起点・focus がグループの key なら、そのグループの最初のメンバーに移す。`membersOf` は、グループの key なら
 * メンバーの ID（巻の順）を、そうでなければ undefined を返す。変わらなければ同じ参照を返す。
 */
export function ungroupSelection(
  state: SelectionState,
  membersOf: (key: string) => readonly string[] | undefined,
): SelectionState {
  let changed = false;
  const selected = new Set<string>();
  for (const id of state.selected) {
    const members = membersOf(id);
    if (members === undefined) selected.add(id);
    else {
      changed = true;
      for (const member of members) selected.add(member);
    }
  }
  const move = (key: string | null): string | null => {
    if (key === null) return null;
    const members = membersOf(key);
    if (members === undefined) return key;
    changed = true;
    return members[0] ?? null;
  };
  const anchor = move(state.anchor);
  const focus = move(state.focus);
  return changed ? { selected, anchor, focus } : state;
}
