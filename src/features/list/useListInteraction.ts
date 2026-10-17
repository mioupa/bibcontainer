// 一覧（表・グリッド）が共通で持つ、選択まわりの動き。表示の形（行か、カードか）に関わらないものだけを置く。
//
// - usePruneSelection … 絞り込みや削除で消えた本を、選択から外す。
// - useScrollToFocus … focus した本までスクロールする規則（0011 §16）。
// - useListInteraction … 共通のキー（編集・ゴミ箱・⌘A・Esc）、クリックの修飾キー、右クリックの対象の決め方、
//   巻のグループの開閉（FR-LIST-08）。矢印・Home/End・PageUp/Down の動かし方は、表とグリッドで違うので、それぞれが持つ。
//
// 選択は「一覧の行（entry）の key」の上で行う。key は、本の ID か、巻のグループの key（グループの key を選ぶと、その全巻を選んだことになる。
// 操作の対象にするときは expandToBookIds で本の ID にする）。
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useRef,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { expandToBookIds, type GroupedList } from "@/features/library/grouping";
import { collapseGroup, expandGroup, toggleGroup } from "@/features/list/groupActions";
import type { ListAction } from "@/features/list/listActions";
import { matchShortcut } from "@/features/shortcuts/shortcuts";
import { detectPlatform } from "@/lib/platform";
import { useSelectionStore } from "@/stores/selectionStore";

// OS は起動中に変わらないので、1 度だけ判定する
const PLATFORM = detectPlatform();

/** 絞り込みや削除で消えた行を、選択から外す。`ids` は、いま表示している行の key（本の ID と、グループの key）。 */
export function usePruneSelection(ids: readonly string[]): void {
  const prune = useSelectionStore((s) => s.prune);
  useEffect(() => {
    prune(new Set(ids));
  }, [ids, prune]);
}

/**
 * focus した本（検索欄の ISBN など、ほかの場所から選んだ本を含む）が見えるよう、その位置までスクロールする
 * （見えているなら動かない）。`positions` は、本の ID → スクロール先の番号（表は行の番号、グリッドは何行目か）。
 *
 * 動かすのは、focus が変わったとき（と、`reveal` で選び直したとき）だけ。行が差し替わっただけ（`library-changed` で
 * 行が更新されたときなど）では動かさない。動かすと、ユーザーがスクロールした位置から、選択行へ引き戻してしまう。
 * 絞り込みの更新より先に focus が変わることがあるので、その本がまだ並びにない間は「スクロール待ち」にして、
 * 並びに現れたときに 1 度だけ動く。
 */
export function useScrollToFocus(
  positions: ReadonlyMap<string, number>,
  scrollTo: (index: number) => void,
): void {
  const focus = useSelectionStore((s) => s.focus);
  const revealTick = useSelectionStore((s) => s.revealTick);
  const pending = useRef<string | null>(null);
  // 呼び出し側の関数（毎回新しい）を、effect の依存に入れずに、いつも最新のものとして呼ぶ
  const scroll = useEffectEvent(scrollTo);
  useEffect(() => {
    pending.current = focus;
  }, [focus, revealTick]);
  useEffect(() => {
    const target = pending.current;
    if (target === null) return;
    const index = positions.get(target);
    if (index === undefined) return;
    pending.current = null;
    scroll(index);
  }, [focus, revealTick, positions]);
}

export interface ListInteractionOptions {
  /** いま表示している行の key（表示の順。本の ID と、グループの key）。 */
  ids: readonly string[];
  /** 表示している並び（グループの key から全巻を引く）。 */
  grouped: GroupedList;
  /** ゴミ箱を開いているか。 */
  inTrash: boolean;
  onOpenEdit: (id: string) => void;
  onAction: (action: Exclude<ListAction, "edit">, ids: string[]) => void;
}

export interface ListInteraction {
  /**
   * 表・グリッドで共通のキーを処理する。処理した（または無視すべき）ときは true を返すので、呼び出し側はそこで終わる。
   * false のときだけ、呼び出し側が自分の移動のキー（矢印など）を調べる。
   * 共通のキー: 日本語入力の変換中（無視）、Enter（編集。グループの行では開閉）、ゴミ箱へ移動、⌘/Ctrl+A（すべて選ぶ）、
   * ほかの修飾キーつき（無視）、Esc（選択を外す）。
   */
  handleCommonKey: (event: KeyboardEvent) => boolean;
  /** 行・カードのクリック。⌘（macOS）／Ctrl（Windows）で足し引き、Shift で範囲。 */
  handleItemClick: (id: string, event: MouseEvent) => void;
  /**
   * 右クリック（`ContextMenuTrigger` の `onContextMenu` に渡す）。右クリックした本が選択の外なら、その本だけを選ぶ。
   * 本（`data-row-id` のある要素）の外ならメニューを出さない。
   */
  handleContextMenu: (event: MouseEvent<HTMLElement>) => void;
  /** グループを開く。 */
  expandGroup: (key: string) => void;
  /** グループを畳む（見えなくなるメンバーの選択は、グループの選択に置き換える）。 */
  collapseGroup: (key: string) => void;
  /** 開いていれば畳み、閉じていれば開く（開閉のボタン・Enter・ダブルクリック）。 */
  toggleGroup: (key: string) => void;
}

export function useListInteraction({
  ids,
  grouped,
  inTrash,
  onOpenEdit,
  onAction,
}: ListInteractionOptions): ListInteraction {
  const click = useSelectionStore((s) => s.click);
  const selectAll = useSelectionStore((s) => s.selectAll);
  const clear = useSelectionStore((s) => s.clear);

  const handleCommonKey = (event: KeyboardEvent): boolean => {
    if (event.nativeEvent.isComposing) return true;
    const { selected, focus } = useSelectionStore.getState();
    const shortcut = matchShortcut(event.nativeEvent, PLATFORM);
    if (shortcut === "edit") {
      if (focus !== null) {
        event.preventDefault();
        // グループの行は、編集の代わりに開閉する（編集の対象にできるのは、1 冊の本）
        if (grouped.groups.has(focus)) toggleGroup(focus, grouped);
        else onOpenEdit(focus);
      }
      return true;
    }
    if (shortcut === "trash") {
      // ゴミ箱の中では、さらにゴミ箱へ移すことはない（完全に削除は、確認のあるメニューから）
      const targets = inTrash ? [] : expandToBookIds(selected, grouped);
      if (targets.length > 0) {
        event.preventDefault();
        onAction("trash", targets);
      }
      return true;
    }

    const mod = PLATFORM === "mac" ? event.metaKey : event.ctrlKey;
    if (mod && event.code === "KeyA") {
      event.preventDefault();
      selectAll(ids);
      return true;
    }
    if (event.altKey || event.ctrlKey || event.metaKey) return true;
    if (event.code === "Escape") {
      clear();
      return true;
    }
    return false;
  };

  const handleItemClick = useCallback(
    (id: string, event: MouseEvent) => {
      click(ids, id, {
        toggle: PLATFORM === "mac" ? event.metaKey : event.ctrlKey,
        range: event.shiftKey,
      });
    },
    [click, ids],
  );

  const handleContextMenu = (event: MouseEvent<HTMLElement>) => {
    const element = (event.target as HTMLElement).closest<HTMLElement>("[data-row-id]");
    const id = element?.dataset.rowId;
    if (!id) {
      event.preventDefault();
      return;
    }
    if (!useSelectionStore.getState().selected.has(id)) {
      click(ids, id, { toggle: false, range: false });
    }
  };

  const toggle = useCallback((key: string) => toggleGroup(key, grouped), [grouped]);
  const collapse = useCallback((key: string) => collapseGroup(key, grouped), [grouped]);

  return {
    handleCommonKey,
    handleItemClick,
    handleContextMenu,
    expandGroup,
    collapseGroup: collapse,
    toggleGroup: toggle,
  };
}
