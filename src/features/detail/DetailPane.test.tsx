import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import { DetailPane } from "@/features/detail/DetailPane";
import { ja } from "@/i18n/ja";
import { makeDetail } from "@/test/detail";
import { makeRow } from "@/test/factories";
import type { BookDetail } from "@/types/contracts";

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

    it("参考税込価格（ref_price_incl）を出す。値がなければ「—」（FR-LIST-06）", () => {
      const { unmount } = render(
        <DetailPane
          row={row}
          detail={makeDetail({ id: "detail-1", refPriceIncl: 1430 })}
          selectedCount={1}
          cCodes={PROVISIONAL_C_CODE_TABLE}
          locationPaths={new Map()}
          onEdit={vi.fn()}
          onRefetch={vi.fn()}
        />,
      );
      const dd = (label: string) => screen.getByText(label).nextElementSibling as HTMLElement;
      expect(dd(ja.detail.fields.refPriceIncl)).toHaveTextContent("1,430円");
      unmount();

      renderPane({ detail: makeDetail({ id: "detail-1", refPriceIncl: null }) });
      expect(dd(ja.detail.fields.refPriceIncl)).toHaveTextContent(ja.detail.noValue);
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

  // BookDetail（get_book_detail）の項目それぞれについて、詳細ペインに出すか、出さない理由を決めておく。
  // 項目を足したときに、ここが型エラーになり、出し忘れに気づける（FR-LIST-06「全項目」）。
  const DETAIL_FIELDS: Record<keyof BookDetail, { label: string } | { hidden: string }> = {
    id: { hidden: "内部の ID" },
    isbn13: { label: ja.detail.fields.isbn13 },
    title: { hidden: "見出しに出す" },
    titleKana: { label: ja.detail.fields.titleKana },
    subtitle: { hidden: "見出しの下に出す" },
    seriesId: { label: ja.detail.fields.series }, // 名前（seriesName）で出す
    volume: { label: ja.detail.fields.volume },
    volumeSort: { hidden: "並べ替え用の内部の値" },
    authors: { hidden: "見出しの下に出す" },
    publisher: { label: ja.detail.fields.publisher },
    publishedOn: { label: ja.detail.fields.publishedOn },
    cCode: { label: ja.detail.fields.cCode },
    ndc: { label: ja.detail.fields.ndc },
    listPrice: { label: ja.detail.fields.listPrice },
    refPriceIncl: { label: ja.detail.fields.refPriceIncl },
    pages: { label: ja.detail.fields.pages },
    sizeText: { label: ja.detail.fields.sizeText },
    language: { label: ja.detail.fields.language },
    description: { label: ja.detail.fields.description },
    coverHash: { hidden: "書影の画像で出す" },
    ownership: { label: ja.detail.fields.ownership },
    wishPriority: { label: ja.detail.fields.wishPriority }, // 欲しい本のときだけ
    locationId: { label: ja.detail.fields.location },
    tags: { label: ja.detail.fields.tags },
    readStatus: { label: ja.detail.fields.readStatus },
    readStartedOn: { label: ja.detail.fields.readStartedOn },
    readFinishedOn: { label: ja.detail.fields.readFinishedOn },
    rating: { label: ja.detail.fields.rating },
    review: { label: ja.detail.fields.review },
    purchasedOn: { label: ja.detail.fields.purchasedOn },
    purchaseStore: { label: ja.detail.fields.purchaseStore },
    purchasePrice: { label: ja.detail.fields.purchasePrice },
    purchaseCond: { label: ja.detail.fields.purchaseCond },
    purchaseMemo: { label: ja.detail.fields.purchaseMemo },
    memo: { label: ja.detail.fields.memo },
    metaStatus: { label: ja.detail.fields.metaStatus },
    deleted: { hidden: "ゴミ箱の本は、一覧で薄く出す" },
    purged: { hidden: "内部の状態（完全に削除した本は出ない）" },
    createdAt: { label: ja.detail.fields.createdAt },
    fieldSources: { hidden: "取得元の記録（編集フォームの項目の横に出す）" },
    manualFields: { label: ja.detail.fields.manualFields },
    extra: { hidden: "新しい版の端末から届いた、この版が知らない項目" },
  };

  it("BookDetail の項目は、出さないと決めたもの以外すべて、値つきで出す（FR-LIST-06「全項目」）", () => {
    const full = makeRow({
      id: "all-1",
      title: "全項目の本",
      titleKana: "ゼンコウモクノホン",
      isbn13: "9784780802047",
      volume: "3",
      seriesId: "s1",
      seriesName: "全項目シリーズ",
      publisher: "ポット出版",
      publishedOn: "2020-05",
      cCode: "0077",
      ndc: "596",
      listPrice: 1300,
      pages: 200,
      ownership: "wanted",
      wishPriority: "high",
      locationId: "loc",
      tags: ["料理"],
      readStatus: "reading",
      readFinishedOn: "2026-02-03",
      rating: 4,
      purchasedOn: "2026-01-05",
      purchasePrice: 1430,
      purchaseStore: "近所の本屋",
    });
    const detail = makeDetail({
      id: "all-1",
      refPriceIncl: 1430,
      sizeText: "15cm",
      language: "jpn",
      description: "内容紹介",
      readStartedOn: "2026-01-06",
      review: "感想",
      purchaseCond: "new",
      purchaseMemo: "購入メモ",
      memo: "メモ",
      manualFields: ["title"],
    });
    render(
      <DetailPane
        row={full}
        detail={detail}
        selectedCount={1}
        cCodes={PROVISIONAL_C_CODE_TABLE}
        locationPaths={new Map([["loc", "自宅 > 書斎"]])}
        onEdit={vi.fn()}
        onRefetch={vi.fn()}
      />,
    );
    for (const [key, entry] of Object.entries(DETAIL_FIELDS)) {
      if (!("label" in entry)) continue;
      // 「メモ」は見出しと項目名が同じなので、項目名（dt）だけを探す
      const terms = screen.getAllByText(entry.label).filter((el) => el.tagName === "DT");
      expect(terms, `${key}（${entry.label}）の項目がない`).toHaveLength(1);
      expect(
        terms[0]!.nextElementSibling,
        `${key}（${entry.label}）に値がない`,
      ).not.toHaveTextContent(ja.detail.noValue);
    }
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
