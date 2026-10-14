import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { DetailPane } from "@/features/detail/DetailPane";
import { ja } from "@/i18n/ja";
import { makeDetail } from "@/test/detail";
import { makeRow } from "@/test/factories";

const row = makeRow({
  id: "detail-1",
  title: "おにぎりレシピ101",
  subtitle: "新装版",
  authors: [
    { name: "佐藤 太郎", kana: null, role: "著" },
    { name: "鈴木 花子", kana: null, role: null },
  ],
  publisher: "ポット出版",
  publishedOn: "2020-05",
  isbn13: "9784780802047",
  cCode: "0077",
  listPrice: 1300,
  ownership: "owned",
  locationId: "loc",
  tags: ["料理", "仕事/参考書"],
  readStatus: "read",
  rating: 4,
  purchasePrice: 1430,
  metaStatus: "ok",
});

function renderPane(overrides: Partial<Parameters<typeof DetailPane>[0]> = {}) {
  const props = {
    row,
    selectedCount: 1,
    cCodes: PROVISIONAL_C_CODE_TABLE,
    locationPaths: new Map([["loc", "自宅 > 書斎 > 本棚A"]]),
    onEdit: vi.fn(),
    onRefetch: vi.fn(),
    ...overrides,
  };
  render(<DetailPane {...props} />);
  return props;
}

describe("DetailPane（FR-LIST-06）", () => {
  it("選んだ本の項目をすべて出す", () => {
    renderPane();
    expect(
      screen.getByRole("heading", { level: 2, name: "おにぎりレシピ101" }),
    ).toBeInTheDocument();
    expect(screen.getAllByText("佐藤 太郎、鈴木 花子").length).toBeGreaterThan(0);
    const dd = (label: string) => screen.getByText(label).nextElementSibling as HTMLElement;
    expect(dd(ja.detail.fields.publisher)).toHaveTextContent("ポット出版");
    expect(dd(ja.detail.fields.isbn13)).toHaveTextContent("9784780802047");
    expect(dd(ja.detail.fields.cCode)).toHaveTextContent(/^0077\s一般・単行本・家事$/);
    expect(dd(ja.detail.fields.listPrice)).toHaveTextContent("1,300円");
    expect(dd(ja.detail.fields.location)).toHaveTextContent("自宅 > 書斎 > 本棚A");
    expect(dd(ja.detail.fields.tags)).toHaveTextContent("料理、仕事/参考書");
    expect(dd(ja.detail.fields.readStatus)).toHaveTextContent(ja.labels.readStatus.read);
    expect(dd(ja.detail.fields.purchasePrice)).toHaveTextContent("1,430円");
    expect(dd(ja.detail.fields.metaStatus)).toHaveTextContent(ja.labels.metaStatus.ok);
  });

  it("書影がないとき、書名を描いた仮の表紙を出す", () => {
    renderPane();
    const cover = screen.getByRole("img", { name: "おにぎりレシピ101" });
    expect(within(cover).getByText("おにぎりレシピ101")).toBeInTheDocument();
  });

  it("値がない項目は「—」を出す", () => {
    renderPane({ row: makeRow({ title: "空の本" }) });
    expect(screen.getByText(ja.detail.fields.publisher).nextElementSibling).toHaveTextContent(
      ja.detail.noValue,
    );
  });

  it("編集と再取得のボタンが、本の ID を渡して呼ばれる", () => {
    const props = renderPane();
    fireEvent.click(screen.getByRole("button", { name: ja.detail.edit }));
    fireEvent.click(screen.getByRole("button", { name: ja.detail.refetch }));
    expect(props.onEdit).toHaveBeenCalledWith("detail-1");
    expect(props.onRefetch).toHaveBeenCalledWith("detail-1");
  });

  it("選択がないときは案内を、2 冊以上のときは冊数を出す", () => {
    renderPane({ row: null, selectedCount: 0 });
    expect(screen.getByText(ja.detail.empty)).toBeInTheDocument();
  });

  it("複数選択: 冊数と案内（詳細は出さない）", () => {
    renderPane({ selectedCount: 3 });
    expect(screen.getByText(ja.detail.multiple(3))).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2 })).not.toBeInTheDocument();
  });

  describe("get_book_detail の項目（詳細を取得できたとき）", () => {
    const detail = makeDetail({
      id: "detail-1",
      description: "料理の本です。\n二行目。",
      review: "おいしそう",
      memo: "貸出中",
      sizeText: "15cm",
      readStartedOn: "2026-01-02",
      purchaseCond: "used",
      purchaseMemo: "古本市で購入",
      manualFields: ["title", "published_on"],
    });

    it("ListRow にない項目（内容紹介・感想・メモ・大きさ・購入メモなど）を出す", () => {
      renderPane({ detail });
      const dd = (label: string) => screen.getByText(label).nextElementSibling as HTMLElement;
      expect(dd(ja.detail.fields.description)).toHaveTextContent("料理の本です。");
      expect(dd(ja.detail.fields.review)).toHaveTextContent("おいしそう");
      expect(dd(ja.detail.fields.sizeText)).toHaveTextContent("15cm");
      expect(dd(ja.detail.fields.readStartedOn)).toHaveTextContent("2026-01-02");
      expect(dd(ja.detail.fields.purchaseCond)).toHaveTextContent(ja.detail.purchaseCond.used);
      expect(dd(ja.detail.fields.purchaseMemo)).toHaveTextContent("古本市で購入");
      expect(screen.getAllByText("貸出中").length).toBeGreaterThan(0);
      // 手で直した項目は、項目名で出す
      expect(dd(ja.detail.fields.manualFields)).toHaveTextContent("書名、発行日");
    });

    it("別の本の詳細は使わない（選び直した直後に、前の本の項目を出さない）", () => {
      renderPane({ detail: { ...detail, id: "other-book" } });
      expect(screen.queryByText(ja.detail.fields.review)).not.toBeInTheDocument();
    });

    it("詳細がまだなければ、ListRow の項目だけを出す", () => {
      renderPane({ detail: null });
      expect(screen.queryByText(ja.detail.fields.review)).not.toBeInTheDocument();
      expect(screen.getByText(ja.detail.fields.publisher)).toBeInTheDocument();
    });
  });

  it("設定が税込を併記なら、本体価格に税込を併記する（FR-CONF-05）", () => {
    renderPane({ price: { showTaxIncluded: true, taxRatePercent: 10 } });
    expect(screen.getByText(ja.detail.fields.listPrice).nextElementSibling).toHaveTextContent(
      "1,300円（税込 1,430円）",
    );
  });

  it("欲しい本のときだけ「購入済みにする」を出し、本の ID を渡す（FR-WISH-01）", () => {
    const onMarkPurchased = vi.fn();
    const { unmount } = render(
      <DetailPane
        row={{ ...row, ownership: "wanted" }}
        selectedCount={1}
        cCodes={PROVISIONAL_C_CODE_TABLE}
        locationPaths={new Map()}
        onEdit={vi.fn()}
        onRefetch={vi.fn()}
        onMarkPurchased={onMarkPurchased}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: ja.detail.markPurchased }));
    expect(onMarkPurchased).toHaveBeenCalledWith("detail-1");
    unmount();

    renderPane({ onMarkPurchased });
    expect(screen.queryByRole("button", { name: ja.detail.markPurchased })).not.toBeInTheDocument();
  });
});
