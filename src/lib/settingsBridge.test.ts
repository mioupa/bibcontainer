import { describe, expect, it } from "vitest";
import type { DisplaySettings } from "@/bindings/DisplaySettings";
import type { ListSettings } from "@/bindings/ListSettings";
import type { ScanSettings } from "@/bindings/ScanSettings";
import { DEFAULT_SCAN_SETTINGS, DEFAULT_SOUND_SETTINGS } from "@/features/scan/config";
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

const display: DisplaySettings = { theme: "dark", fontFamily: null, fontSize: 16 };
const scan: ScanSettings = {
  terminator: "tab",
  timeoutSec: 12,
  keyIntervalMs: 40,
  idleCommitMs: 150,
  stripLeading: 1,
  stripTrailing: 2,
  sounds: {
    success: { enabled: true, volume: 0.9 },
    error: { enabled: false, volume: 0.3 },
    attention: { enabled: true, volume: 0.5 },
  },
  muted: true,
};
const list: ListSettings = {
  columnVisibility: { isbn13: true },
  columnOrder: ["title", "authors"],
  columnSizing: { title: 400 },
  sorting: [{ id: "authors", desc: true }],
  sidebarOpen: false,
  detailOpen: true,
};

describe("保存形式（zustand persist の { state, version }）", () => {
  it("書いた state を読み戻せる。壊れた内容は null", () => {
    const raw = toEnvelope({ a: 1 }, 1);
    expect(JSON.parse(raw)).toEqual({ state: { a: 1 }, version: 1 });
    expect(stateFromEnvelope(raw)).toEqual({ a: 1 });
    expect(stateFromEnvelope("{壊れた")).toBeNull();
    expect(stateFromEnvelope('{"state":3}')).toBeNull();
    expect(stateFromEnvelope('"x"')).toBeNull();
  });
});

describe("settings ストア ↔ display + scan", () => {
  it("セクションからストアの内容へ。通知音の種類ごとの設定とミュートも運ぶ", () => {
    const state = settingsFromSections(display, scan);
    expect(state.theme).toBe("dark");
    expect(state.scan).toEqual({
      terminator: "tab",
      timeoutSec: 12,
      keyIntervalMs: 40,
      idleCommitMs: 150,
      stripLeading: 1,
      stripTrailing: 2,
    });
    expect(state.sound.muted).toBe(true);
    expect(state.sound.kinds.error).toEqual({ enabled: false, volume: 0.3 });
  });

  it("往復しても変わらない", () => {
    const state = settingsFromSections(display, scan);
    expect(sectionsFromSettings(state, { display, scan })).toEqual({ display, scan });
  });

  it("壊れた・範囲外の値は、範囲に収める（既定値に戻す）", () => {
    const sections = sectionsFromSettings(
      {
        theme: "neon",
        fontSize: 18,
        fontFamily: "  ",
        scan: { timeoutSec: 999, terminator: "gamepad" },
        sound: { kinds: { success: { volume: 9 } } },
      },
      { display, scan },
    );
    expect(sections.display).toEqual({ theme: "system", fontFamily: null, fontSize: 18 });
    expect(sections.scan.timeoutSec).toBe(30);
    expect(sections.scan.terminator).toBe("enter");
    expect(sections.scan.sounds.success.volume).toBe(1);
    expect(sections.scan.sounds.error).toEqual(DEFAULT_SOUND_SETTINGS.kinds.error);
    expect(sections.scan.keyIntervalMs).toBe(DEFAULT_SCAN_SETTINGS.keyIntervalMs);
  });

  it("形そのものが違っても落ちない", () => {
    for (const bad of [null, undefined, 3, "x", []]) {
      const sections = sectionsFromSettings(bad, { display, scan });
      expect(sections.display.theme).toBe("system");
      expect(sections.scan.terminator).toBe("enter");
    }
  });
});

describe("table / layout ストア ↔ list", () => {
  it("list を 2 つのストアの内容に分ける", () => {
    expect(tableFromList(list)).toEqual({
      sorting: list.sorting,
      columnVisibility: list.columnVisibility,
      columnOrder: ["title", "authors"],
      columnSizing: list.columnSizing,
    });
    expect(layoutFromList(list)).toEqual({ sidebarOpen: false, detailOpen: true });
  });

  it("table の保存は、layout の項目を残して重ねる。列の並び順も list.columnOrder に書く（FR-LIST-01）", () => {
    const next = listWithTable(list, {
      sorting: [{ id: "title", desc: false }],
      columnVisibility: { tags: true },
      columnOrder: ["authors", "title", "publisher"],
      columnSizing: { title: 500 },
    });
    expect(next.sorting).toEqual([{ id: "title", desc: false }]);
    expect(next.columnVisibility).toEqual({ tags: true });
    expect(next.columnOrder).toEqual(["authors", "title", "publisher"]);
    expect(next.columnSizing).toEqual({ title: 500 });
    expect(next.sidebarOpen).toBe(false);
    expect(next.detailOpen).toBe(true);
  });

  it("table の保存に並び順がない（古い保存内容）・形が違うときは、今の並び順を残す。文字列でない要素は捨てる", () => {
    const old = { sorting: [], columnVisibility: {}, columnSizing: {} };
    expect(listWithTable(list, old).columnOrder).toEqual(["title", "authors"]);
    expect(listWithTable(list, { ...old, columnOrder: "title" }).columnOrder).toEqual([
      "title",
      "authors",
    ]);
    expect(listWithTable(list, { ...old, columnOrder: ["title", 3, null] }).columnOrder).toEqual([
      "title",
    ]);
    // 空の並び順は「既定の順」の意味で、そのまま書く
    expect(listWithTable(list, { ...old, columnOrder: [] }).columnOrder).toEqual([]);
  });

  it("layout の保存は、table の項目を残して重ねる。形が違う項目は今の値のまま", () => {
    const next = listWithLayout(list, { sidebarOpen: true, detailOpen: "いいえ" });
    expect(next.sidebarOpen).toBe(true);
    expect(next.detailOpen).toBe(true);
    expect(next.sorting).toEqual(list.sorting);
    expect(next.columnSizing).toEqual(list.columnSizing);
  });
});
