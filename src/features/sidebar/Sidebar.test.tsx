import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { MOCK_LOCATIONS } from "@/dev/mockRows";
import { computeFacets, type FilterContext } from "@/features/library/filter";
import { buildLocationIndex, collectTagPaths } from "@/features/library/trees";
import { Sidebar } from "@/features/sidebar/Sidebar";
import { ja } from "@/i18n/ja";
import { useViewStore } from "@/stores/viewStore";
import { makeRow } from "@/test/factories";

const rows = [
  makeRow({
    id: "1",
    ownership: "owned",
    readStatus: "read",
    locationId: "loc-study-a1",
    tags: ["研究/SiC"],
    cCode: "0193",
    publisher: "岩波書店",
    publishedOn: "2020-05-01",
  }),
  makeRow({
    id: "2",
    ownership: "owned",
    readStatus: "unread",
    locationId: "loc-lab-shelf1",
    tags: ["小説"],
    cCode: "3055",
    publisher: "オーム社",
    publishedOn: "2021",
  }),
  makeRow({
    id: "3",
    ownership: "wanted",
    metaStatus: "pending",
    cCode: "0077",
    publisher: "岩波書店",
  }),
  makeRow({ id: "4", ownership: "owned", deleted: true }),
];
const ctx: FilterContext = { searchIds: null, locationIndex: buildLocationIndex(MOCK_LOCATIONS) };
const tagPaths = collectTagPaths(rows);

/** ストアの絞り込みに合わせた件数で、サイドバーを描く。 */
function Harness() {
  const filter = useViewStore((s) => s.filter);
  return (
    <Sidebar
      facets={computeFacets(rows, filter, ctx)}
      locations={MOCK_LOCATIONS}
      allTagPaths={tagPaths}
      cCodes={PROVISIONAL_C_CODE_TABLE}
    />
  );
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** 項目（ボタン）。名前は「ラベル＋件数」になる。グループの見出しや、名前が似た項目と区別するため、ラベルの直後は数字に限る。 */
function item(label: string) {
  return screen.getByRole("button", { name: new RegExp(`^${escapeRegExp(label)}\\s*\\d+$`) });
}

describe("Sidebar", () => {
  beforeEach(() => {
    useViewStore.getState().clearFilters();
  });

  it("ライブラリ（すべて／所有／欲しい本／手放した／ゴミ箱）と件数を出す（FR-LIST-04）", () => {
    render(<Harness />);
    expect(within(item(ja.sidebar.library.all)).getByText("3")).toBeInTheDocument();
    expect(within(item(ja.sidebar.library.owned)).getByText("2")).toBeInTheDocument();
    expect(within(item(ja.sidebar.library.wanted)).getByText("1")).toBeInTheDocument();
    expect(within(item(ja.sidebar.library.released)).getByText("0")).toBeInTheDocument();
    expect(within(item(ja.sidebar.library.trash)).getByText("1")).toBeInTheDocument();
  });

  it("ライブラリを選ぶと、絞り込みが変わり、選択中として示される", () => {
    render(<Harness />);
    fireEvent.click(item(ja.sidebar.library.wanted));
    expect(useViewStore.getState().filter.library).toBe("wanted");
    expect(item(ja.sidebar.library.wanted)).toHaveAttribute("aria-pressed", "true");
    // ライブラリを変えると、読書状況の件数は欲しい本だけになる
    expect(within(item(ja.sidebar.readStatus.unread)).getByText("1")).toBeInTheDocument();
  });

  it("読書状況: 同じ項目をもう一度押すと解除される", () => {
    render(<Harness />);
    const read = item(ja.sidebar.readStatus.read);
    fireEvent.click(read);
    expect(useViewStore.getState().filter.readStatus).toBe("read");
    fireEvent.click(read);
    expect(useViewStore.getState().filter.readStatus).toBeNull();
  });

  it("保管場所はツリー。親を開くと子が出て、親の件数は子孫の合計", () => {
    render(<Harness />);
    expect(screen.queryByRole("button", { name: /^書斎/ })).not.toBeInTheDocument();
    const home = item("自宅");
    expect(within(home).getByText("1")).toBeInTheDocument();

    // 親の開閉ボタン（自宅の行の左）
    fireEvent.click(within(home.parentElement!).getByRole("button", { name: ja.sidebar.expand }));
    const study = item("書斎");
    fireEvent.click(study);
    expect(useViewStore.getState().filter.locationId).toBe("loc-study");
  });

  it("タグは `/` で階層表示になる", () => {
    render(<Harness />);
    expect(screen.queryByRole("button", { name: /^SiC/ })).not.toBeInTheDocument();
    const research = item("研究");
    fireEvent.click(
      within(research.parentElement!).getByRole("button", { name: ja.sidebar.expand }),
    );
    const sic = item("SiC");
    fireEvent.click(sic);
    expect(useViewStore.getState().filter.tag).toBe("研究/SiC");
    // 絞り込み中も、ほかのタグの項目は残る
    expect(item("小説")).toBeInTheDocument();
  });

  it("自動分類: 内容（大分類 → 中分類）、出版社、取得待ち・情報不足", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: ja.sidebar.auto.heading }));
    fireEvent.click(screen.getByRole("button", { name: ja.sidebar.auto.content }));
    const literature = item("9 文学");
    expect(within(literature).getByText("1")).toBeInTheDocument();
    fireEvent.click(
      within(literature.parentElement!).getByRole("button", { name: ja.sidebar.expand }),
    );
    fireEvent.click(item("93 日本文学、小説・物語"));
    expect(useViewStore.getState().filter.auto).toEqual({ kind: "cMinor", value: "93" });

    fireEvent.click(screen.getByRole("button", { name: ja.sidebar.auto.publisher }));
    fireEvent.click(item("岩波書店"));
    expect(useViewStore.getState().filter.auto).toEqual({ kind: "publisher", value: "岩波書店" });

    expect(within(item(ja.sidebar.auto.metaPending)).getByText("1")).toBeInTheDocument();
  });

  it("絞り込みがあるときだけ「絞り込みを解除」が出て、押すと戻る", () => {
    render(<Harness />);
    expect(screen.queryByRole("button", { name: ja.sidebar.clearFilters })).not.toBeInTheDocument();
    fireEvent.click(item(ja.sidebar.library.owned));
    fireEvent.click(screen.getByRole("button", { name: ja.sidebar.clearFilters }));
    expect(useViewStore.getState().filter.library).toBe("all");
  });
});
