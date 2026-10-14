// 画面の骨組みの状態（サイドバーと詳細ペインの開閉）。端末ごとに保存する。
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { STORAGE_KEYS, settingsStorage } from "@/lib/persist";

interface LayoutState {
  sidebarOpen: boolean;
  detailOpen: boolean;
  toggleSidebar: () => void;
  toggleDetail: () => void;
  setSidebarOpen: (open: boolean) => void;
  setDetailOpen: (open: boolean) => void;
}

export const useLayoutStore = create<LayoutState>()(
  persist(
    (set) => ({
      sidebarOpen: true,
      detailOpen: true,
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      toggleDetail: () => set((s) => ({ detailOpen: !s.detailOpen })),
      setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
      setDetailOpen: (detailOpen) => set({ detailOpen }),
    }),
    {
      name: STORAGE_KEYS.layout,
      version: 1,
      storage: settingsStorage,
      partialize: (s) => ({ sidebarOpen: s.sidebarOpen, detailOpen: s.detailOpen }),
    },
  ),
);
