// 設定の保存先（zustand の persist に渡す StateStorage）。
//
// - Tauri の中: Rust の `local_settings`（端末ごと。同期しない）に、型付きのセクションとして保存する。
//   ストアの保存内容とセクションの変換は src/lib/settingsBridge.ts。
// - Tauri の外（ブラウザで `pnpm dev`）: WebView の localStorage に保存する。
//
// IPC は非同期なので、ストアは最初の描画のあとに保存済みの値で更新される。最初の描画の前に反映したいときは
// `whenSettingsHydrated`（src/main.tsx がテーマのために使う）。
import { createJSONStorage, type StateStorage } from "zustand/middleware";
import type { DisplaySettings } from "@/bindings/DisplaySettings";
import type { ListSettings } from "@/bindings/ListSettings";
import type { ScanSettings } from "@/bindings/ScanSettings";
import { ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";
import {
  layoutFromList,
  listWithLayout,
  listWithTable,
  sectionsFromSettings,
  settingsFromSections,
  stateFromEnvelope,
  tableFromList,
  toEnvelope,
} from "@/lib/settingsBridge";

/** 各ストアの保存キー（zustand の persist の `name`）。 */
export const STORAGE_KEYS = {
  settings: "bibcontainer.settings",
  table: "bibcontainer.table",
  layout: "bibcontainer.layout",
} as const;

/** ストアの persist の `version`。形を変えたら上げて、migrate を書く。 */
const STORE_VERSION = 1;

/** localStorage が使えない環境（プライベートウィンドウ、テストなど）でも落ちないようにする。 */
const safeLocalStorage: StateStorage = {
  getItem: (name) => {
    try {
      return globalThis.localStorage.getItem(name);
    } catch {
      return null;
    }
  },
  setItem: (name, value) => {
    try {
      globalThis.localStorage.setItem(name, value);
    } catch {
      // 保存できなくても、画面は動かし続ける
    }
  },
  removeItem: (name) => {
    try {
      globalThis.localStorage.removeItem(name);
    } catch {
      // 同上
    }
  },
};

/**
 * 「最新の値だけを順に書く」キュー。書いている最中に新しい値が来たら、途中の値は飛ばして最新だけを次に書く
 * （スライダーなど、短い間隔で何度も変わる設定で、IPC と DB への書き込みを溜めないため）。
 */
export function createLatestWriter<T>(write: (value: T) => Promise<unknown>): (value: T) => void {
  let pending: { value: T } | null = null;
  let running = false;
  const drain = async () => {
    running = true;
    while (pending !== null) {
      const { value } = pending;
      pending = null;
      try {
        await write(value);
      } catch (error) {
        console.error("設定を保存できませんでした", error);
      }
    }
    running = false;
  };
  return (value) => {
    pending = { value };
    if (!running) void drain();
  };
}

// ---- IPC 版 ----

/** list セクションの手元の最新。table と layout のストアが分け合うので、書くときに重ねて 1 つにする。 */
let listCache: Promise<ListSettings> | null = null;
let displayBase: DisplaySettings | null = null;
let scanBase: ScanSettings | null = null;

function loadList(): Promise<ListSettings> {
  listCache ??= ipc.getListSettings().catch((error: unknown) => {
    listCache = null; // 次の呼び出しでやり直す
    throw error;
  });
  return listCache;
}

const writeList = createLatestWriter((list: ListSettings) => ipc.setListSettings(list));
const writeSettings = createLatestWriter(
  (sections: { display: DisplaySettings; scan: ScanSettings }) =>
    Promise.all([ipc.setDisplaySettings(sections.display), ipc.setScanSettings(sections.scan)]),
);

const ipcStorage: StateStorage = {
  getItem: async (name) => {
    switch (name) {
      case STORAGE_KEYS.settings: {
        const [display, scan] = await Promise.all([
          ipc.getDisplaySettings(),
          ipc.getScanSettings(),
        ]);
        displayBase = display;
        scanBase = scan;
        return toEnvelope(settingsFromSections(display, scan), STORE_VERSION);
      }
      case STORAGE_KEYS.table:
        return toEnvelope(tableFromList(await loadList()), STORE_VERSION);
      case STORAGE_KEYS.layout:
        return toEnvelope(layoutFromList(await loadList()), STORE_VERSION);
      default:
        return null;
    }
  },
  setItem: async (name, value) => {
    const state = stateFromEnvelope(value);
    if (state === null) return;
    switch (name) {
      case STORAGE_KEYS.settings: {
        // 読み込めていないとき（DB を開けなかったなど）は、書く先がないので何もしない
        if (displayBase === null || scanBase === null) return;
        const sections = sectionsFromSettings(state, { display: displayBase, scan: scanBase });
        displayBase = sections.display;
        scanBase = sections.scan;
        writeSettings(sections);
        return;
      }
      case STORAGE_KEYS.table:
      case STORAGE_KEYS.layout: {
        const apply = name === STORAGE_KEYS.table ? listWithTable : listWithLayout;
        try {
          // 重ねるのは同期的に行い（順序を守る）、書き込みは最新の値だけにまとめる
          const next = loadList().then((list) => apply(list, state));
          listCache = next;
          writeList(await next);
        } catch (error) {
          // 読み込めていない（DB を開けなかったなど）。次の保存でやり直す
          listCache = null;
          console.error("設定を保存できませんでした", error);
        }
        return;
      }
    }
  },
  removeItem: () => {
    // 設定を消す操作は、今のところない
  },
};

/** 呼ぶたびに、Tauri の中かどうかで保存先を選ぶ（テストが切り替えられるように）。 */
const switchingStorage: StateStorage = {
  getItem: (name) => (inTauri() ? ipcStorage.getItem(name) : safeLocalStorage.getItem(name)),
  setItem: (name, value) =>
    inTauri() ? ipcStorage.setItem(name, value) : safeLocalStorage.setItem(name, value),
  removeItem: (name) =>
    inTauri() ? ipcStorage.removeItem(name) : safeLocalStorage.removeItem(name),
};

export const settingsStorage = createJSONStorage(() => switchingStorage);

/** 起動のときのテスト用に、手元の最新の記憶を捨てる。 */
export function resetSettingsCacheForTest(): void {
  listCache = null;
  displayBase = null;
  scanBase = null;
}
