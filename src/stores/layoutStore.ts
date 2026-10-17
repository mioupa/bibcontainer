// 画面の骨組みの状態（サイドバーと詳細ペインの開閉、一覧の表示の形、巻のまとめ表示）。端末ごとに保存する。
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ListViewMode } from "@/bindings/ListViewMode";
import { STORAGE_KEYS, settingsStorage } from "@/lib/persist";
import { isListViewMode } from "@/lib/settingsBridge";

interface LayoutState {
  sidebarOpen: boolean;
  detailOpen: boolean;
  /** 一覧の表示の形（FR-LIST-07）。表か、書影のグリッド。 */
  viewMode: ListViewMode;
  /** 同じシリーズの巻を、続けて 1 つのまとまりとして出す（FR-LIST-08。グリッドのとき）。 */
  groupVolumes: boolean;
  toggleSidebar: () => void;
  toggleDetail: () => void;
  setSidebarOpen: (open: boolean) => void;
  setDetailOpen: (open: boolean) => void;
  setViewMode: (viewMode: ListViewMode) => void;
  setGroupVolumes: (groupVolumes: boolean) => void;
}

export const useLayoutStore = create<LayoutState>()(
  persist(
    (set) => ({
      sidebarOpen: true,
      detailOpen: true,
      viewMode: "table",
      groupVolumes: false,
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      toggleDetail: () => set((s) => ({ detailOpen: !s.detailOpen })),
      setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
      setDetailOpen: (detailOpen) => set({ detailOpen }),
      setViewMode: (viewMode) => set({ viewMode: isListViewMode(viewMode) ? viewMode : "table" }),
      setGroupVolumes: (groupVolumes) => set({ groupVolumes }),
    }),
    {
      name: STORAGE_KEYS.layout,
      version: 1,
      storage: settingsStorage,
      partialize: (s) => ({
        sidebarOpen: s.sidebarOpen,
        detailOpen: s.detailOpen,
        viewMode: s.viewMode,
        groupVolumes: s.groupVolumes,
      }),
      // 保存された値が壊れていても（localStorage を手で書き換えたなど）、その項目だけ既定値のままにする
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<LayoutState, "sidebarOpen" | "detailOpen" | "viewMode" | "groupVolumes">
        >;
        return {
          ...current,
          sidebarOpen:
            typeof saved.sidebarOpen === "boolean" ? saved.sidebarOpen : current.sidebarOpen,
          detailOpen: typeof saved.detailOpen === "boolean" ? saved.detailOpen : current.detailOpen,
          viewMode: isListViewMode(saved.viewMode) ? saved.viewMode : current.viewMode,
          groupVolumes:
            typeof saved.groupVolumes === "boolean" ? saved.groupVolumes : current.groupVolumes,
        };
      },
    },
  ),
);
