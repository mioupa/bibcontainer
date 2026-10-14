// どの画面を出しているか（メイン／スキャン登録／設定）。保存しない（起動のたびにメイン画面から始める）。
//
// 画面の切り替えはこのストアだけで行う。描き分けは App.tsx が担当する。
// メイン画面のボタン・ショートカット（⇧⌘N、⌘,）や、メイン画面でのスキャン検出（FR-SCAN-10）は show* を呼ぶ。
import { create } from "zustand";

export type Screen = "main" | "scan" | "settings";

/** 設定画面の節。about は「このアプリについて」（FR-CONF-07）。 */
export type SettingsSection =
  "sync" | "providers" | "scan" | "display" | "price" | "backup" | "about";

interface ScreenState {
  screen: Screen;
  settingsSection: SettingsSection;
  /**
   * メイン画面で読んだバーコード（数字列）を、スキャン登録画面に引き渡すためのもの（FR-SCAN-10）。
   * スキャン登録画面は表示時にこれを取り出して（takeHandoff）、自分で読んだものと同じ処理に通す。
   */
  handoff: string | null;
  showMain: () => void;
  showScan: (handoff?: string) => void;
  showSettings: (section?: SettingsSection) => void;
  /** 引き渡されたバーコードを取り出して消す。無ければ null。 */
  takeHandoff: () => string | null;
}

export const useScreenStore = create<ScreenState>()((set, get) => ({
  screen: "main",
  settingsSection: "providers",
  handoff: null,
  showMain: () => set({ screen: "main" }),
  showScan: (handoff) => set({ screen: "scan", handoff: handoff ?? null }),
  showSettings: (section) =>
    set((s) => ({ screen: "settings", settingsSection: section ?? s.settingsSection })),
  takeHandoff: () => {
    const { handoff } = get();
    if (handoff !== null) set({ handoff: null });
    return handoff;
  },
}));
