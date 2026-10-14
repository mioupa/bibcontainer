// 端末ごとの設定（テーマ・フォント・文字サイズ・スキャン入力・通知音）。
// 保存先は src/lib/persist.ts（Tauri の中では Rust の local_settings。外では localStorage）。
import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
  DEFAULT_SCAN_SETTINGS,
  DEFAULT_SOUND_SETTINGS,
  sanitizeScanSettings,
  sanitizeSoundSettings,
  type ScanSettings,
  type SoundSettings,
  type SoundSettingsPatch,
} from "@/features/scan/config";
import { STORAGE_KEYS, settingsStorage } from "@/lib/persist";
import type { ThemePreference } from "@/lib/theme";

/** 文字サイズ（px。画面全体の基準の大きさ）の範囲と既定値。 */
export const FONT_SIZE = { min: 12, max: 24, default: 16 } as const;

/** 文字サイズを範囲に収める。数でないときは既定値。 */
export function clampFontSize(value: number): number {
  if (!Number.isFinite(value)) return FONT_SIZE.default;
  return Math.min(FONT_SIZE.max, Math.max(FONT_SIZE.min, Math.round(value)));
}

interface SettingsState {
  theme: ThemePreference;
  /** フォントの名前。null は OS の日本語フォント（既定）。 */
  fontFamily: string | null;
  fontSize: number;
  scan: ScanSettings;
  sound: SoundSettings;
  setTheme: (theme: ThemePreference) => void;
  setFontFamily: (fontFamily: string | null) => void;
  setFontSize: (fontSize: number) => void;
  setScan: (patch: Partial<ScanSettings>) => void;
  setSound: (patch: SoundSettingsPatch) => void;
}

function isThemePreference(value: unknown): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      theme: "system",
      fontFamily: null,
      fontSize: FONT_SIZE.default,
      scan: DEFAULT_SCAN_SETTINGS,
      sound: DEFAULT_SOUND_SETTINGS,
      setTheme: (theme) => set({ theme }),
      setFontFamily: (fontFamily) => set({ fontFamily }),
      setFontSize: (fontSize) => set({ fontSize: clampFontSize(fontSize) }),
      setScan: (patch) => set((s) => ({ scan: sanitizeScanSettings(s.scan, patch) })),
      setSound: (patch) => set((s) => ({ sound: sanitizeSoundSettings(s.sound, patch) })),
    }),
    {
      name: STORAGE_KEYS.settings,
      version: 1,
      storage: settingsStorage,
      partialize: (s) => ({
        theme: s.theme,
        fontFamily: s.fontFamily,
        fontSize: s.fontSize,
        scan: s.scan,
        sound: s.sound,
      }),
      // 保存された値が壊れていても、範囲に収めて読み込む
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<SettingsState, "theme" | "fontFamily" | "fontSize" | "scan" | "sound">
        >;
        return {
          ...current,
          theme: isThemePreference(saved.theme) ? saved.theme : current.theme,
          fontFamily:
            typeof saved.fontFamily === "string" && saved.fontFamily.trim() !== ""
              ? saved.fontFamily
              : null,
          fontSize:
            typeof saved.fontSize === "number" ? clampFontSize(saved.fontSize) : current.fontSize,
          scan: sanitizeScanSettings(current.scan, saved.scan ?? {}),
          sound: sanitizeSoundSettings(current.sound, saved.sound ?? {}),
        };
      },
    },
  ),
);
