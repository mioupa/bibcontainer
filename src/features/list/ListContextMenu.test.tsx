// 右クリックメニューの、作品の手動のまとめ・外し・自動に戻す（0026）。巻をまとめる設定がオンのときだけ出て、
// 押せるかは、対象（選択を本に広げたもの）の本の `workId` で決まる。MainScreen と同じく、useGroupedList の結果を grouped として渡す。
import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { groupRows } from "@/features/library/grouping";
import { useGroupedList } from "@/features/library/useGroupedList";
import { BookTable, type BookTableProps } from "@/features/list/BookTable";
import { ja } from "@/i18n/ja";
import { useGroupStore } from "@/stores/groupStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { useTableStore } from "@/stores/tableStore";
import { makeRow } from "@/test/factories";
import { stubElementSize } from "@/test/layout";
import type { ListRow } from "@/types/contracts";

stubElementSize({ width: 1400, height: 600 });

const author = (name: string) => [{ name, kana: null, role: null }];
const volume = (id: string, title: string, n: number, extra: Partial<ListRow> = {}) =>
  makeRow({
    id,
    title,
    sortTitle: `${title}${String(n).padStart(2, "0")}`,
    volume: String(n),
    volumeSort: n,
    authors: author("山田 太郎"),
    ...extra,
  });

/**
 * あ（ふつうの本）／風 1〜3（自動のグループ）／う（作品から外した本）／雲 1・2（手動の作品 w-1。書名が違う）／ん（ふつうの本）
 */
function makeRows(): ListRow[] {
  return [
    makeRow({ id: "plain-1", title: "あ", sortTitle: "あ" }),
    volume("wind-1", "風", 1),
    volume("wind-2", "風", 2),
    volume("wind-3", "風", 3),
    makeRow({ id: "excluded", title: "う", sortTitle: "う", workId: "none" }),
    volume("cloud-1", "雲", 1, { authors: author("鈴木 花子"), workId: "w-1" }),
    volume("cloud-2", "雲 外伝", 2, { authors: author("佐藤 一郎"), workId: "w-1" }),
    makeRow({ id: "plain-2", title: "ん", sortTitle: "ん" }),
  ];
}
const WIND_IDS = ["wind-1", "wind-2", "wind-3"];
const CLOUD_IDS = ["cloud-1", "cloud-2"];
const WIND_KEY = (() => {
  const grouped = groupRows(makeRows(), true, new Set());
  return grouped.groupOfBook.get("wind-1") ?? "";
})();
const CLOUD_KEY = "work:\u001fw-1";

function GroupedTable(props: BookTableProps) {
  const grouped = useGroupedList(props.rows);
  return <BookTable {...props} grouped={grouped} />;
}
function renderTable(rows: ListRow[] = makeRows()) {
  const props: BookTableProps = {
    rows,
    cCodes: PROVISIONAL_C_CODE_TABLE,
    locationPaths: new Map(),
    isLoading: false,
    onOpenEdit: vi.fn(),
    onAction: vi.fn(),
    onBulkChange: vi.fn(),
  };
  return { ...render(<GroupedTable {...props} />), props };
}

// 行の key に制御文字（\u001f）があるので、属性セレクターでなく、属性の値で探す
const byKey = (key: string) =>
  [...document.querySelectorAll<HTMLElement>("[data-row-id]")].find(
    (el) => el.dataset.rowId === key,
  ) ?? null;
const openMenu = (key: string) => {
  fireEvent.contextMenu(byKey(key)!);
  return screen.findByRole("menu");
};
const item = (menu: HTMLElement, name: string) => within(menu).getByRole("menuitem", { name });
const isDisabled = (element: HTMLElement) => element.hasAttribute("data-disabled");
const menuNames = (menu: HTMLElement) =>
  within(menu)
    .getAllByRole("menuitem")
    .map((el) => el.textContent);
const lastCall = (fn: unknown) => (fn as ReturnType<typeof vi.fn>).mock.calls.at(-1) as unknown[];

const MERGE = ja.list.contextMenu.mergeWorks;
const EXCLUDE = ja.list.contextMenu.excludeFromWorks;
const RESET = ja.list.contextMenu.resetWorks;

describe("右クリックメニュー: 作品の手動のまとめ・外し・自動に戻す（0026）", () => {
  beforeEach(() => {
    useTableStore.getState().resetColumns();
    useSelectionStore.getState().clear();
    useGroupStore.getState().reset();
    useLayoutStore.setState({ groupVolumes: true });
  });
  afterEach(() => {
    useLayoutStore.setState({ groupVolumes: false });
    useGroupStore.getState().reset();
  });

  describe("出す条件と位置", () => {
    it("巻をまとめる設定がオンなら、「CSV に書き出し…」のあとに区切りを入れて、3 つの項目を出す", async () => {
      renderTable();
      const menu = await openMenu("plain-1");
      const names = menuNames(menu);
      const exportAt = names.indexOf(ja.list.contextMenu.export);
      expect(exportAt).toBeGreaterThan(0);
      expect(names.slice(exportAt, exportAt + 4)).toEqual([
        ja.list.contextMenu.export,
        MERGE,
        EXCLUDE,
        RESET,
      ]);
      expect(names.at(-1)).toBe(ja.list.contextMenu.trash);
      // 書き出しとの間、ゴミ箱との間に区切り（ふだんの 1 つに、1 つ増える）
      expect(within(menu).getAllByRole("separator")).toHaveLength(2);
    });

    it("巻をまとめる設定がオフなら、3 つの項目も、その区切りも出さない", async () => {
      useLayoutStore.setState({ groupVolumes: false });
      renderTable();
      const menu = await openMenu("plain-1");
      for (const name of [MERGE, EXCLUDE, RESET]) {
        expect(within(menu).queryByRole("menuitem", { name })).not.toBeInTheDocument();
      }
      expect(within(menu).getAllByRole("separator")).toHaveLength(1);
    });
  });

  describe("押せる条件", () => {
    it("ふつうの 1 冊（上書きなし）: 「まとめる」は 2 冊に満たないので押せない。「外す」は押せる。「自動に戻す」は上書きがないので押せない", async () => {
      const { props } = renderTable();
      const menu = await openMenu("plain-1");
      expect(isDisabled(item(menu, MERGE))).toBe(true);
      expect(isDisabled(item(menu, EXCLUDE))).toBe(false);
      expect(isDisabled(item(menu, RESET))).toBe(true);
      // 押せない項目を選んでも、何も起きない
      fireEvent.click(item(menu, MERGE));
      expect(props.onAction).not.toHaveBeenCalled();
    });

    it("作品から外した 1 冊: 「外す」は押せない。「自動に戻す」は押せる", async () => {
      const { props } = renderTable();
      const menu = await openMenu("excluded");
      expect(isDisabled(item(menu, MERGE))).toBe(true);
      expect(isDisabled(item(menu, EXCLUDE))).toBe(true);
      expect(isDisabled(item(menu, RESET))).toBe(false);
      fireEvent.click(item(menu, RESET));
      expect(lastCall(props.onAction)).toEqual(["resetWorks", ["excluded"]]);
    });

    it("ふつうの 2 冊を選ぶと、「まとめる」が押せて、2 冊の ID を表示の順に渡す", async () => {
      const { props } = renderTable();
      fireEvent.click(byKey("plain-2")!);
      fireEvent.click(byKey("plain-1")!, { metaKey: true });
      const menu = await openMenu("plain-1");
      expect(isDisabled(item(menu, MERGE))).toBe(false);
      fireEvent.click(item(menu, MERGE));
      expect(lastCall(props.onAction)).toEqual(["mergeWorks", ["plain-1", "plain-2"]]);
    });

    it("畳んだグループの行を選ぶと、全巻が対象（1 つの行でも、巻が 2 冊以上なので「まとめる」が押せる）", async () => {
      const { props } = renderTable();
      let menu = await openMenu(WIND_KEY);
      expect([...useSelectionStore.getState().selected]).toEqual([WIND_KEY]);
      expect(isDisabled(item(menu, MERGE))).toBe(false);
      expect(isDisabled(item(menu, EXCLUDE))).toBe(false);
      // 自動の作品の本は、上書きがないので、「自動に戻す」は押せない
      expect(isDisabled(item(menu, RESET))).toBe(true);

      fireEvent.click(item(menu, MERGE));
      expect(lastCall(props.onAction)).toEqual(["mergeWorks", WIND_IDS]);
      menu = await openMenu(WIND_KEY);
      fireEvent.click(item(menu, EXCLUDE));
      expect(lastCall(props.onAction)).toEqual(["excludeFromWorks", WIND_IDS]);
    });

    it("手動の作品の行は、3 つとも押せる。「自動に戻す」は全巻を渡す", async () => {
      const { props } = renderTable();
      const menu = await openMenu(CLOUD_KEY);
      expect(isDisabled(item(menu, MERGE))).toBe(false);
      expect(isDisabled(item(menu, EXCLUDE))).toBe(false);
      expect(isDisabled(item(menu, RESET))).toBe(false);
      fireEvent.click(item(menu, RESET));
      expect(lastCall(props.onAction)).toEqual(["resetWorks", CLOUD_IDS]);
    });

    it("作品の行とふつうの本を一緒に選ぶと、全巻とその本を、表示の順で渡す（作品の行と、その作品の巻を重ねて選んでも重ならない）", async () => {
      const { props } = renderTable();
      fireEvent.click(byKey("plain-2")!);
      fireEvent.click(byKey(WIND_KEY)!, { metaKey: true });
      fireEvent.click(byKey(CLOUD_KEY)!, { metaKey: true });
      const menu = await openMenu(WIND_KEY);
      fireEvent.click(item(menu, MERGE));
      expect(lastCall(props.onAction)).toEqual([
        "mergeWorks",
        [...WIND_IDS, ...CLOUD_IDS, "plain-2"],
      ]);

      // 開いた作品の行と、その中の巻を選んでも、冊数は重ならない
      fireEvent.click(within(byKey(WIND_KEY)!).getByRole("button", { name: ja.list.group.expand }));
      fireEvent.click(byKey(WIND_KEY)!);
      fireEvent.click(byKey("wind-2")!, { metaKey: true });
      const again = await openMenu(WIND_KEY);
      fireEvent.click(item(again, EXCLUDE));
      expect(lastCall(props.onAction)).toEqual(["excludeFromWorks", WIND_IDS]);
    });

    it("対象の一部だけに上書きがあれば、「外す」「自動に戻す」は押せる（外した本とふつうの本を選んだとき）", async () => {
      renderTable();
      fireEvent.click(byKey("excluded")!);
      fireEvent.click(byKey("plain-1")!, { metaKey: true });
      const menu = await openMenu("excluded");
      expect(isDisabled(item(menu, MERGE))).toBe(false);
      expect(isDisabled(item(menu, EXCLUDE))).toBe(false);
      expect(isDisabled(item(menu, RESET))).toBe(false);
    });

    it("作品から外した本だけを 2 冊選ぶと、「まとめる」「自動に戻す」は押せて、「外す」は押せない", async () => {
      renderTable(
        makeRows().map((row) => (row.id === "plain-1" ? { ...row, workId: "none" } : row)),
      );
      fireEvent.click(byKey("plain-1")!);
      fireEvent.click(byKey("excluded")!, { metaKey: true });
      const menu = await openMenu("excluded");
      expect(isDisabled(item(menu, MERGE))).toBe(false);
      expect(isDisabled(item(menu, EXCLUDE))).toBe(true);
      expect(isDisabled(item(menu, RESET))).toBe(false);
    });
  });
});
