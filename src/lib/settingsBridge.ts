// 画面のストア（zustand）の保存内容と、Rust の設定のセクション（IPC）の相互の変換。純粋な関数だけを置く。
//
// ストアは zustand の persist で、キー名ごとに `{ state, version }` の JSON を保存する。Tauri の中では、その保存先を
// Rust の `local_settings`（型付きのセクション）にする（src/lib/persist.ts）。ストアとセクションの対応は次のとおり。
//
//   bibcontainer.settings … display（テーマ・フォント・文字サイズ）と scan（スキャン入力・通知音）
//   bibcontainer.table    … list（並べ替え・列の表示・並び順・幅）
//   bibcontainer.layout   … list（サイドバーと詳細ペインの開閉、一覧の表示の形（表・グリッド）、巻のまとめ表示）
//
// `list` は table と layout の 2 つのストアが分け合うので、書くときは、もう一方の項目を残して重ねる。
import type { DisplaySettings } from "@/bindings/DisplaySettings";
import type { ListSettings } from "@/bindings/ListSettings";
import type { ListViewMode } from "@/bindings/ListViewMode";
import type { ScanSettings as ScanSettingsDto } from "@/bindings/ScanSettings";
import {
  DEFAULT_SCAN_SETTINGS,
  DEFAULT_SOUND_SETTINGS,
  sanitizeScanSettings,
  sanitizeSoundSettings,
  type ScanSettings,
  type SoundSettings,
} from "@/features/scan/config";

/** zustand の persist が保存する形。 */
interface Envelope {
  state: unknown;
  version: number;
}

/** 保存する文字列にする。 */
export function toEnvelope(state: unknown, version: number): string {
  return JSON.stringify({ state, version } satisfies Envelope);
}

/** 保存された文字列から state を取り出す。壊れていれば null。 */
export function stateFromEnvelope(raw: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed === "object" && parsed !== null && "state" in parsed) {
      const state = (parsed as Envelope).state;
      if (typeof state === "object" && state !== null) return state as Record<string, unknown>;
    }
  } catch {
    // 壊れた保存内容は、保存がなかったものとして扱う
  }
  return null;
}

function record(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

// ---- settings ストア ↔ display + scan ----

/** settings ストアの保存内容。 */
export interface SettingsPersisted {
  theme: DisplaySettings["theme"];
  fontFamily: string | null;
  fontSize: number;
  scan: ScanSettings;
  sound: SoundSettings;
}

export function settingsFromSections(
  display: DisplaySettings,
  scan: ScanSettingsDto,
): SettingsPersisted {
  return {
    theme: display.theme,
    fontFamily: display.fontFamily,
    fontSize: display.fontSize,
    scan: {
      terminator: scan.terminator,
      timeoutSec: scan.timeoutSec,
      keyIntervalMs: scan.keyIntervalMs,
      idleCommitMs: scan.idleCommitMs,
      stripLeading: scan.stripLeading,
      stripTrailing: scan.stripTrailing,
    },
    sound: {
      muted: scan.muted,
      kinds: { ...scan.sounds },
    },
  };
}

/**
 * settings ストアの保存内容から、display と scan のセクションを作る。
 * ストアにない項目は `base`（今の保存内容）を残す。形が壊れていても、範囲に収めた値にする。
 */
export function sectionsFromSettings(
  state: unknown,
  base: { display: DisplaySettings; scan: ScanSettingsDto },
): { display: DisplaySettings; scan: ScanSettingsDto } {
  const saved = record(state);
  const scan = sanitizeScanSettings(DEFAULT_SCAN_SETTINGS, record(saved.scan));
  const sound = sanitizeSoundSettings(DEFAULT_SOUND_SETTINGS, record(saved.sound));
  const theme = saved.theme;
  return {
    display: {
      ...base.display,
      theme: theme === "light" || theme === "dark" || theme === "system" ? theme : "system",
      fontFamily:
        typeof saved.fontFamily === "string" && saved.fontFamily.trim() !== ""
          ? saved.fontFamily
          : null,
      fontSize: typeof saved.fontSize === "number" ? saved.fontSize : base.display.fontSize,
    },
    scan: {
      ...base.scan,
      ...scan,
      muted: sound.muted,
      sounds: {
        success: sound.kinds.success,
        error: sound.kinds.error,
        attention: sound.kinds.attention,
      },
    },
  };
}

// ---- table / layout ストア ↔ list ----

export interface TablePersisted {
  sorting: ListSettings["sorting"];
  columnVisibility: ListSettings["columnVisibility"];
  columnOrder: ListSettings["columnOrder"];
  columnSizing: ListSettings["columnSizing"];
}

export interface LayoutPersisted {
  sidebarOpen: boolean;
  detailOpen: boolean;
  viewMode: ListViewMode;
  groupVolumes: boolean;
}

/** 一覧の表示の形として正しい値か。 */
export function isListViewMode(value: unknown): value is ListViewMode {
  return value === "table" || value === "grid";
}

export function tableFromList(list: ListSettings): TablePersisted {
  return {
    sorting: list.sorting,
    columnVisibility: list.columnVisibility,
    columnOrder: list.columnOrder,
    columnSizing: list.columnSizing,
  };
}

export function layoutFromList(list: ListSettings): LayoutPersisted {
  return {
    sidebarOpen: list.sidebarOpen,
    detailOpen: list.detailOpen,
    viewMode: list.viewMode,
    groupVolumes: list.groupVolumes,
  };
}

/** table ストアの保存内容を、list セクションに重ねる（layout の項目は残す）。 */
export function listWithTable(list: ListSettings, state: unknown): ListSettings {
  const saved = record(state);
  return {
    ...list,
    sorting: Array.isArray(saved.sorting)
      ? (saved.sorting as ListSettings["sorting"])
      : list.sorting,
    columnVisibility: record(saved.columnVisibility) as ListSettings["columnVisibility"],
    columnOrder: Array.isArray(saved.columnOrder)
      ? saved.columnOrder.filter((id): id is string => typeof id === "string")
      : list.columnOrder,
    columnSizing: record(saved.columnSizing) as ListSettings["columnSizing"],
  };
}

/** layout ストアの保存内容を、list セクションに重ねる（table の項目は残す）。 */
export function listWithLayout(list: ListSettings, state: unknown): ListSettings {
  const saved = record(state);
  return {
    ...list,
    sidebarOpen: typeof saved.sidebarOpen === "boolean" ? saved.sidebarOpen : list.sidebarOpen,
    detailOpen: typeof saved.detailOpen === "boolean" ? saved.detailOpen : list.detailOpen,
    viewMode: isListViewMode(saved.viewMode) ? saved.viewMode : list.viewMode,
    groupVolumes: typeof saved.groupVolumes === "boolean" ? saved.groupVolumes : list.groupVolumes,
  };
}
