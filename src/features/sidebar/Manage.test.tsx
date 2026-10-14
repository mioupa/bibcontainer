import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TagInfo } from "@/bindings/TagInfo";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { MOCK_LOCATIONS } from "@/dev/mockRows";
import { libraryApi } from "@/features/library/api";
import { computeFacets, type FilterContext } from "@/features/library/filter";
import { buildLocationIndex, collectTagPaths } from "@/features/library/trees";
import { Sidebar } from "@/features/sidebar/Sidebar";
import { ja } from "@/i18n/ja";
import { IpcError } from "@/lib/ipc";
import { useViewStore } from "@/stores/viewStore";
import { makeRow } from "@/test/factories";

vi.mock("sonner", () => ({
  toast: Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
}));
vi.mock("@/features/library/api", () => ({
  libraryApi: {
    createLocation: vi.fn(),
    renameLocation: vi.fn(),
    moveLocation: vi.fn(),
    deleteLocation: vi.fn(),
    renameTag: vi.fn(),
    setTagColor: vi.fn(),
  },
  usingMockData: () => false,
}));
const api = vi.mocked(libraryApi);

const rows = [
  makeRow({ id: "1", locationId: "loc-study-a1", tags: ["研究/SiC", "研究/GaN"] }),
  makeRow({ id: "2", tags: ["小説"] }),
];
const ctx: FilterContext = { searchIds: null, locationIndex: buildLocationIndex(MOCK_LOCATIONS) };
const tagPaths = collectTagPaths(rows);
const tags: TagInfo[] = [
  { name: "研究/SiC", count: 1, color: "red", id: "t1" },
  { name: "小説", count: 1, color: null, id: null },
];

function Harness() {
  const filter = useViewStore((s) => s.filter);
  return (
    <Sidebar
      facets={computeFacets(rows, filter, ctx)}
      locations={MOCK_LOCATIONS}
      allTagPaths={tagPaths}
      cCodes={PROVISIONAL_C_CODE_TABLE}
      tags={tags}
    />
  );
}

/** 項目の行（ラベル＋件数のボタンの親）。「研究」と「研究室」を区別するため、ラベルの直後は数字（件数）に限る。 */
const rowOf = (label: string) =>
  screen.getByRole("button", { name: new RegExp(`^${label}\\d`) }).parentElement as HTMLElement;
/** 項目の子を開く。 */
const expand = (label: string) =>
  fireEvent.click(within(rowOf(label)).getByRole("button", { name: ja.sidebar.expand }));

const menuButton = (name: string) =>
  screen.getByRole("button", { name: ja.sidebar.manage.menuLabel(name) });
const openMenu = (name: string) => {
  fireEvent.pointerDown(menuButton(name), { button: 0, ctrlKey: false });
};
const dialog = () => screen.findByRole("dialog");
const text = ja.sidebar.manage;

describe("サイドバーの管理（保管場所・タグ）", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useViewStore.getState().clearFilters();
    for (const fn of [
      api.createLocation,
      api.renameLocation,
      api.moveLocation,
      api.deleteLocation,
      api.setTagColor,
    ]) {
      fn.mockResolvedValue(null as never);
    }
    api.renameTag.mockResolvedValue(["1"]);
  });

  describe("保管場所", () => {
    it("見出しの「＋」で、最上位に保管場所を作る", async () => {
      render(<Harness />);
      fireEvent.click(screen.getByRole("button", { name: text.locations.add }));
      const dlg = await dialog();
      fireEvent.change(within(dlg).getByLabelText(text.locations.nameLabel), {
        target: { value: " 書庫 " },
      });
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.create }));
      await waitFor(() => expect(api.createLocation).toHaveBeenCalledWith("書庫", null));
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    });

    it("名前が空なら作らず、理由を出す", async () => {
      render(<Harness />);
      fireEvent.click(screen.getByRole("button", { name: text.locations.add }));
      const dlg = await dialog();
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.create }));
      expect(await within(dlg).findByRole("alert")).toHaveTextContent(text.locations.emptyName);
      expect(api.createLocation).not.toHaveBeenCalled();
    });

    it("項目のメニューから、子の保管場所を作る・名前を変える", async () => {
      render(<Harness />);
      expand("自宅");
      openMenu("書斎");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.locations.addChild }));
      let dlg = await dialog();
      expect(within(dlg).getByText(text.locations.createChild("書斎"))).toBeInTheDocument();
      fireEvent.change(within(dlg).getByLabelText(text.locations.nameLabel), {
        target: { value: "本棚C" },
      });
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.create }));
      await waitFor(() => expect(api.createLocation).toHaveBeenCalledWith("本棚C", "loc-study"));
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

      openMenu("書斎");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.locations.rename }));
      dlg = await dialog();
      const input = within(dlg).getByLabelText<HTMLInputElement>(text.locations.nameLabel);
      expect(input.value).toBe("書斎");
      fireEvent.change(input, { target: { value: "書庫" } });
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.renameConfirm }));
      await waitFor(() => expect(api.renameLocation).toHaveBeenCalledWith("loc-study", "書庫"));
    });

    it("右クリックでも、同じメニューが開く", async () => {
      render(<Harness />);
      expand("自宅");
      fireEvent.contextMenu(rowOf("書斎"));
      expect(
        await screen.findByRole("menuitem", { name: text.locations.move }),
      ).toBeInTheDocument();
    });

    it("移動: 自分と子孫は移動先に出ない。「最上位」へ移せる", async () => {
      render(<Harness />);
      expand("自宅");
      openMenu("書斎");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.locations.move }));
      const dlg = await dialog();
      fireEvent.click(within(dlg).getByRole("combobox", { name: text.locations.moveTarget }));
      const options = (await screen.findAllByRole("option")).map((o) => o.textContent);
      expect(options).toContain("自宅");
      expect(options).toContain("研究室");
      // 書斎の下（本棚A など）と、書斎自身は選べない
      expect(options).not.toContain("書斎");
      expect(options).not.toContain("本棚A");
      fireEvent.click(screen.getByRole("option", { name: text.locations.moveRoot }));
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.moveConfirm }));
      await waitFor(() => expect(api.moveLocation).toHaveBeenCalledWith("loc-study", null));
    });

    it("削除: 確認して削除する。絞り込み中の保管場所を消したら、絞り込みも外す", async () => {
      render(<Harness />);
      useViewStore.getState().toggleLocation("loc-lab");
      openMenu("研究室");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.locations.delete }));
      const dlg = await dialog();
      expect(within(dlg).getByText(text.locations.deleteDescription("研究室"))).toBeInTheDocument();
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.deleteConfirm }));
      await waitFor(() => expect(api.deleteLocation).toHaveBeenCalledWith("loc-lab"));
      await waitFor(() => expect(useViewStore.getState().filter.locationId).toBeNull());
    });

    it("子や本がある保管場所は、コアが conflict で断る。その旨を表示し、ダイアログは開いたまま", async () => {
      api.deleteLocation.mockRejectedValue(
        new IpcError({ kind: "conflict", message: "子の保管場所があります" }),
      );
      render(<Harness />);
      expand("自宅");
      openMenu("書斎");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.locations.delete }));
      const dlg = await dialog();
      fireEvent.click(within(dlg).getByRole("button", { name: text.locations.deleteConfirm }));
      expect(await within(dlg).findByRole("alert")).toHaveTextContent(text.locations.deleteBlocked);
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
  });

  describe("タグ", () => {
    it("名前の変更: 子（研究/…）もまとめて変わることを案内し、絞り込み中なら新しい名前に追随する", async () => {
      render(<Harness />);
      useViewStore.getState().toggleTag("研究/SiC");
      openMenu("研究");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.tags.rename }));
      const dlg = await dialog();
      expect(within(dlg).getByText(text.tags.renameDescription("研究", true))).toBeInTheDocument();
      fireEvent.change(within(dlg).getByLabelText(text.tags.nameLabel), {
        target: { value: "仕事/研究" },
      });
      fireEvent.click(within(dlg).getByRole("button", { name: text.tags.renameConfirm }));
      await waitFor(() => expect(api.renameTag).toHaveBeenCalledWith("研究", "仕事/研究"));
      await waitFor(() => expect(useViewStore.getState().filter.tag).toBe("仕事/研究/SiC"));
    });

    it("同じ名前・空・空の階層ができる名前は、送らずに理由を出す", async () => {
      render(<Harness />);
      openMenu("小説");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.tags.rename }));
      const dlg = await dialog();
      const input = within(dlg).getByLabelText(text.tags.nameLabel);
      const submit = () =>
        fireEvent.click(within(dlg).getByRole("button", { name: text.tags.renameConfirm }));
      submit();
      expect(await within(dlg).findByRole("alert")).toHaveTextContent(text.tags.sameName);
      fireEvent.change(input, { target: { value: "a//b" } });
      submit();
      await waitFor(() =>
        expect(within(dlg).getByRole("alert")).toHaveTextContent(text.tags.invalidName),
      );
      expect(api.renameTag).not.toHaveBeenCalled();
    });

    it("色の設定: 色を選ぶ。「色なし」で外す。設定した色は、項目に点で出る", async () => {
      const { container } = render(<Harness />);
      // 設定済みの色（研究/SiC は red）
      expand("研究");
      expect(container.querySelector('[style*="--ctp-red"]')).not.toBeNull();

      openMenu("小説");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.tags.color }));
      let dlg = await dialog();
      fireEvent.click(within(dlg).getByRole("button", { name: "blue" }));
      await waitFor(() => expect(api.setTagColor).toHaveBeenCalledWith("小説", "blue"));
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

      openMenu("小説");
      fireEvent.click(await screen.findByRole("menuitem", { name: text.tags.color }));
      dlg = await dialog();
      fireEvent.click(within(dlg).getByRole("button", { name: text.tags.colorNone }));
      await waitFor(() => expect(api.setTagColor).toHaveBeenLastCalledWith("小説", null));
    });
  });
});
