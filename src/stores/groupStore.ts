// 巻をまとめた表示（FR-LIST-08）で、開いている（展開している）グループの key。保存しない（アプリを開いている間だけ）。
// グループの key は features/library/grouping.ts。グループがなくなっても key は残ってよい（あとで同じグループが現れたら、また開く）。
import { create } from "zustand";

interface GroupState {
  expanded: ReadonlySet<string>;
  toggle: (key: string) => void;
  expand: (key: string) => void;
  collapse: (key: string) => void;
  reset: () => void;
}

export const useGroupStore = create<GroupState>()((set) => ({
  expanded: new Set(),
  toggle: (key) =>
    set((s) => {
      const expanded = new Set(s.expanded);
      if (!expanded.delete(key)) expanded.add(key);
      return { expanded };
    }),
  // すでに開いている（閉じている）なら、状態の参照を変えない（購読している画面を無駄に再描画しない）
  expand: (key) =>
    set((s) => (s.expanded.has(key) ? s : { expanded: new Set(s.expanded).add(key) })),
  collapse: (key) =>
    set((s) => {
      if (!s.expanded.has(key)) return s;
      const expanded = new Set(s.expanded);
      expanded.delete(key);
      return { expanded };
    }),
  reset: () => set({ expanded: new Set() }),
}));
