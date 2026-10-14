import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_FILTER } from "@/features/library/filter";
import { COLUMN_IDS } from "@/features/list/columnMeta";
import { STORAGE_KEYS } from "@/lib/persist";
import { DEFAULT_SCAN_SETTINGS, DEFAULT_SOUND_SETTINGS } from "@/features/scan/config";
import { useLayoutStore } from "@/stores/layoutStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";
import { useViewStore } from "@/stores/viewStore";

describe("settingsStore", () => {
  beforeEach(() => {
    useSettingsStore.setState({
      theme: "system",
      fontFamily: null,
      fontSize: 16,
      scan: { ...DEFAULT_SCAN_SETTINGS },
      sound: structuredClone(DEFAULT_SOUND_SETTINGS),
    });
  });

  it("テーマを切り替えられる（OS に連動／ライト／ダーク）", () => {
    expect(useSettingsStore.getState().theme).toBe("system");
    useSettingsStore.getState().setTheme("dark");
    expect(useSettingsStore.getState().theme).toBe("dark");
    useSettingsStore.getState().setTheme("light");
    expect(useSettingsStore.getState().theme).toBe("light");
  });

  it("スキャン設定は範囲に収まる（タイムアウト 3〜30 秒）", () => {
    const { setScan } = useSettingsStore.getState();
    setScan({ timeoutSec: 1 });
    expect(useSettingsStore.getState().scan.timeoutSec).toBe(3);
    setScan({ timeoutSec: 99 });
    expect(useSettingsStore.getState().scan.timeoutSec).toBe(30);
    setScan({ timeoutSec: 15 });
    expect(useSettingsStore.getState().scan.timeoutSec).toBe(15);
    setScan({ timeoutSec: Number.NaN });
    expect(useSettingsStore.getState().scan.timeoutSec).toBe(10);
    setScan({ stripLeading: 9, keyIntervalMs: 0 });
    expect(useSettingsStore.getState().scan).toMatchObject({ stripLeading: 4, keyIntervalMs: 5 });
  });

  it("末尾キーは Enter・Tab・なし。知らない値は Enter に戻す", () => {
    const { setScan } = useSettingsStore.getState();
    setScan({ terminator: "tab" });
    expect(useSettingsStore.getState().scan.terminator).toBe("tab");
    setScan({ terminator: "none" });
    expect(useSettingsStore.getState().scan.terminator).toBe("none");
    setScan({ terminator: "bogus" as never });
    expect(useSettingsStore.getState().scan.terminator).toBe("enter");
  });

  it("音量は種類ごとに 0〜1 に収まり、ミュートと種類ごとの有効を切り替えられる", () => {
    const { setSound } = useSettingsStore.getState();
    setSound({ kinds: { success: { volume: 5 }, error: { volume: -1 } } });
    const { kinds } = useSettingsStore.getState().sound;
    expect(kinds.success.volume).toBe(1);
    expect(kinds.error.volume).toBe(0);
    expect(kinds.attention).toEqual(DEFAULT_SOUND_SETTINGS.kinds.attention);
    setSound({ muted: true });
    expect(useSettingsStore.getState().sound.muted).toBe(true);
    setSound({ kinds: { attention: { enabled: false } } });
    expect(useSettingsStore.getState().sound.kinds.attention).toMatchObject({
      enabled: false,
      volume: DEFAULT_SOUND_SETTINGS.kinds.attention.volume,
    });
  });

  it("文字サイズは 12〜24px に収まる。フォントは空なら OS の既定（null）", () => {
    const { setFontSize, setFontFamily } = useSettingsStore.getState();
    setFontSize(99);
    expect(useSettingsStore.getState().fontSize).toBe(24);
    setFontSize(1);
    expect(useSettingsStore.getState().fontSize).toBe(12);
    setFontSize(Number.NaN);
    expect(useSettingsStore.getState().fontSize).toBe(16);
    setFontFamily("Hiragino Sans");
    expect(useSettingsStore.getState().fontFamily).toBe("Hiragino Sans");
    setFontFamily(null);
    expect(useSettingsStore.getState().fontFamily).toBeNull();
  });
});

describe("tableStore", () => {
  beforeEach(() => {
    useTableStore.getState().resetColumns();
  });

  it("既定の列は FR-LIST-01 のとおり。選べる列は隠れている", () => {
    const { columnVisibility } = useTableStore.getState();
    for (const id of [
      "isbn13",
      "series",
      "listPrice",
      "purchasedOn",
      "purchasePrice",
      "purchaseStore",
      "ndc",
      "format",
      "audience",
      "pages",
      "tags",
      "ownership",
      "readFinishedOn",
    ]) {
      expect(columnVisibility[id]).toBe(false);
    }
    for (const id of [
      "cover",
      "title",
      "volume",
      "authors",
      "publisher",
      "publishedOn",
      "cCode",
      "location",
      "readStatus",
      "rating",
      "createdAt",
    ]) {
      expect(columnVisibility[id]).not.toBe(false);
    }
  });

  it("並べ替え・表示・幅を、値でも更新関数でも変えられる", () => {
    const { setSorting, setColumnVisibility, setColumnSizing } = useTableStore.getState();
    setSorting([
      { id: "publisher", desc: false },
      { id: "rating", desc: true },
    ]);
    expect(useTableStore.getState().sorting).toHaveLength(2);
    setSorting((old) => old.slice(0, 1));
    expect(useTableStore.getState().sorting).toEqual([{ id: "publisher", desc: false }]);

    setColumnVisibility((old) => ({ ...old, isbn13: true }));
    expect(useTableStore.getState().columnVisibility.isbn13).toBe(true);

    setColumnSizing({ title: 400 });
    setColumnSizing((old) => ({ ...old, authors: 123.4 }));
    expect(useTableStore.getState().columnSizing).toEqual({ title: 400, authors: 123 });
  });

  it("知らない列・並べ替えできない列・隠せない列（書名）は取り除く。幅は範囲に収める", () => {
    const { setSorting, setColumnVisibility, setColumnSizing } = useTableStore.getState();
    setSorting([
      { id: "cover", desc: false },
      { id: "nonexistent", desc: false },
      { id: "title", desc: true },
    ]);
    expect(useTableStore.getState().sorting).toEqual([{ id: "title", desc: true }]);
    setColumnVisibility({ title: false, bogus: false, pages: true });
    expect(useTableStore.getState().columnVisibility).toEqual({ pages: true });
    setColumnSizing({ title: 5, authors: 99_999, bogus: 100 });
    expect(useTableStore.getState().columnSizing).toEqual({ title: 40, authors: 800 });
  });

  it("既定に戻せる", () => {
    const { setSorting, setColumnSizing, moveColumn, resetColumns } = useTableStore.getState();
    setSorting([{ id: "rating", desc: true }]);
    setColumnSizing({ title: 500 });
    moveColumn("publisher", "up");
    resetColumns();
    expect(useTableStore.getState().sorting).toEqual([{ id: "title", desc: false }]);
    expect(useTableStore.getState().columnSizing).toEqual({});
    expect(useTableStore.getState().columnOrder).toEqual([]);
  });

  it("列の並び順は、既定では空（既定の順）。1 つ上・下へ動かすと、全列の並びを持つ（FR-LIST-01）", () => {
    expect(useTableStore.getState().columnOrder).toEqual([]);
    useTableStore.getState().moveColumn("authors", "up");
    const order = useTableStore.getState().columnOrder;
    expect(order).toHaveLength(COLUMN_IDS.length);
    expect(order.slice(0, 4)).toEqual(["cover", "title", "authors", "volume"]);
    useTableStore.getState().moveColumn("authors", "down");
    expect(useTableStore.getState().columnOrder.slice(0, 4)).toEqual([
      "cover",
      "title",
      "volume",
      "authors",
    ]);
    // 端の列を端の外へは動かせない
    useTableStore.getState().moveColumn("cover", "up");
    expect(useTableStore.getState().columnOrder[0]).toBe("cover");
  });

  it("並び順は更新関数でも変えられる。知らない列と重複は取り除く", () => {
    useTableStore.getState().setColumnOrder(["rating", "bogus", "title", "rating"]);
    expect(useTableStore.getState().columnOrder).toEqual(["rating", "title"]);
    useTableStore.getState().setColumnOrder((old) => [...old, "cover"]);
    expect(useTableStore.getState().columnOrder).toEqual(["rating", "title", "cover"]);
  });

  it("並び順を保存し、読み込み直しても保たれる（端末ごと）。壊れた並び順は捨てる", async () => {
    useTableStore.getState().moveColumn("rating", "up");
    const raw = localStorage.getItem(STORAGE_KEYS.table) ?? "null";
    const saved = JSON.parse(raw) as { state: { columnOrder: string[] } };
    expect(saved.state.columnOrder.indexOf("rating")).toBe(COLUMN_IDS.indexOf("rating") - 1);

    // 画面の状態を捨てて（これも保存し直されるので）、保存済みの内容を戻してから読み込み直す
    useTableStore.setState({ columnOrder: [] });
    localStorage.setItem(STORAGE_KEYS.table, raw);
    await useTableStore.persist.rehydrate();
    expect(useTableStore.getState().columnOrder.indexOf("rating")).toBe(
      COLUMN_IDS.indexOf("rating") - 1,
    );

    localStorage.setItem(
      STORAGE_KEYS.table,
      JSON.stringify({ state: { columnOrder: ["title", 7, "bogus", "title"] }, version: 1 }),
    );
    await useTableStore.persist.rehydrate();
    expect(useTableStore.getState().columnOrder).toEqual(["title"]);

    localStorage.setItem(
      STORAGE_KEYS.table,
      JSON.stringify({ state: { columnOrder: "title" }, version: 1 }),
    );
    await useTableStore.persist.rehydrate();
    expect(useTableStore.getState().columnOrder).toEqual([]);
  });
});

describe("layoutStore", () => {
  it("サイドバーと詳細ペインを折りたためる", () => {
    useLayoutStore.setState({ sidebarOpen: true, detailOpen: true });
    useLayoutStore.getState().toggleSidebar();
    useLayoutStore.getState().toggleDetail();
    expect(useLayoutStore.getState()).toMatchObject({ sidebarOpen: false, detailOpen: false });
    useLayoutStore.getState().toggleDetail();
    expect(useLayoutStore.getState().detailOpen).toBe(true);
  });
});

describe("viewStore", () => {
  beforeEach(() => {
    useViewStore.getState().clearFilters();
  });

  it("同じ項目をもう一度選ぶと、絞り込みが外れる。ライブラリは常にどれか 1 つ", () => {
    const view = useViewStore.getState();
    view.setLibrary("owned");
    view.toggleReadStatus("unread");
    view.toggleTag("研究");
    view.toggleLocation("loc-1");
    view.toggleAuto({ kind: "publisher", value: "岩波書店" });
    expect(useViewStore.getState().filter).toEqual({
      library: "owned",
      readStatus: "unread",
      tag: "研究",
      locationId: "loc-1",
      auto: { kind: "publisher", value: "岩波書店" },
    });

    useViewStore.getState().toggleReadStatus("unread");
    useViewStore.getState().toggleTag("研究");
    useViewStore.getState().toggleLocation("loc-1");
    useViewStore.getState().toggleAuto({ kind: "publisher", value: "岩波書店" });
    expect(useViewStore.getState().filter).toEqual({ ...DEFAULT_FILTER, library: "owned" });

    useViewStore.getState().clearFilters();
    expect(useViewStore.getState().filter).toEqual(DEFAULT_FILTER);
  });

  it("自動分類は、別の分類を選ぶと置き換わる", () => {
    useViewStore.getState().toggleAuto({ kind: "year", value: "2020" });
    useViewStore.getState().toggleAuto({ kind: "author", value: "佐藤 太郎" });
    expect(useViewStore.getState().filter.auto).toEqual({ kind: "author", value: "佐藤 太郎" });
  });
});

describe("selectionStore", () => {
  const ids = ["a", "b", "c"];
  beforeEach(() => {
    useSelectionStore.getState().clear();
  });

  it("クリック・矢印・すべて選択・消去", () => {
    const store = useSelectionStore.getState();
    store.click(ids, "b", { toggle: false, range: false });
    expect([...useSelectionStore.getState().selected]).toEqual(["b"]);
    useSelectionStore.getState().move(ids, "down", { extend: true, pageSize: 5 });
    expect([...useSelectionStore.getState().selected].sort()).toEqual(["b", "c"]);
    useSelectionStore.getState().selectAll(ids);
    expect(useSelectionStore.getState().selected.size).toBe(3);
    useSelectionStore.getState().clear();
    expect(useSelectionStore.getState().selected.size).toBe(0);
    expect(useSelectionStore.getState().focus).toBeNull();
  });

  it("表示から消えた行は選択から外れる。変化がなければ状態を差し替えない", () => {
    useSelectionStore.getState().selectAll(ids);
    const before = useSelectionStore.getState();
    useSelectionStore.getState().prune(new Set(ids));
    expect(useSelectionStore.getState().selected).toBe(before.selected);
    useSelectionStore.getState().prune(new Set(["a"]));
    expect([...useSelectionStore.getState().selected]).toEqual(["a"]);
  });
});
