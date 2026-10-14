import { describe, expect, it } from "vitest";
import {
  emptyFormValues,
  fieldDef,
  fieldDefByOp,
  isFieldChanged,
  opValue,
  parseIntegerInput,
  parseIsbnInput,
  valuesFromDetail,
  type BookFormValues,
} from "@/features/editor/formValues";
import { makeDetail } from "@/test/detail";

const values = (patch: Partial<BookFormValues>): BookFormValues => ({
  ...emptyFormValues(),
  ...patch,
});

describe("opValue（コアに渡す値）", () => {
  it("文字列は前後の空白を除き、空なら null（書名は空文字）", () => {
    expect(opValue("title", values({ title: "  坊っちゃん " }))).toBe("坊っちゃん");
    expect(opValue("title", values({ title: "  " }))).toBe("");
    expect(opValue("publisher", values({ publisher: "  " }))).toBeNull();
    expect(opValue("publisher", values({ publisher: " 岩波書店 " }))).toBe("岩波書店");
  });

  it("複数行の項目は、末尾の空白・改行だけを除く。空白だけなら null", () => {
    expect(opValue("memo", values({ memo: "a\nb\n\n" }))).toBe("a\nb");
    expect(opValue("memo", values({ memo: " \n " }))).toBeNull();
  });

  it("日付・Cコードは全角を半角にする", () => {
    expect(opValue("publishedOn", values({ publishedOn: "２０２６－１０" }))).toBe("2026-10");
    expect(opValue("cCode", values({ cCode: " ０１９３ " }))).toBe("0193");
    expect(opValue("cCode", values({ cCode: "" }))).toBeNull();
  });

  it("ISBN はハイフン・全角を直し、ISBN-10 は ISBN-13 にする", () => {
    expect(parseIsbnInput("978-4-00-310111-7")).toBe("9784003101117");
    expect(parseIsbnInput("４－００－３１０１１１－１")).toBe("9784003101117");
    expect(parseIsbnInput("")).toBeNull();
    // 正しくない値はそのまま返す（検証が弾く）
    expect(parseIsbnInput("1234")).toBe("1234");
  });

  it("金額・ページ数は整数にする（桁区切りのカンマ、「円」、全角を許す）。数字以外が残れば文字列のまま", () => {
    expect(parseIntegerInput("1,300円")).toBe(1300);
    expect(parseIntegerInput("１３００")).toBe(1300);
    expect(parseIntegerInput(" ")).toBeNull();
    expect(parseIntegerInput("12a")).toBe("12a");
    expect(opValue("listPrice", values({ listPrice: "700" }))).toBe(700);
  });

  it("著者は空の行を除き、ヨミ・役割の空は null にする", () => {
    const authors = [
      { name: " 夏目漱石 ", kana: "", role: "著" },
      { name: "", kana: "", role: "" },
      { name: "x", kana: "え", role: " " },
    ];
    expect(opValue("authors", values({ authors }))).toEqual([
      { name: "夏目漱石", kana: null, role: "著" },
      { name: "x", kana: "え", role: null },
    ]);
  });

  it("タグは前後の空白・空・重複を除く（順序は保つ）", () => {
    expect(opValue("tags", values({ tags: [" 研究/SiC ", "", "小説", "小説"] }))).toEqual([
      "研究/SiC",
      "小説",
    ]);
  });
});

describe("isFieldChanged", () => {
  it("書式の違いだけ（空白・全角・ハイフン・空の著者行）は、変えたことにならない", () => {
    const initial = values({ isbn13: "9784003101117", cCode: "0193", authors: [] });
    const current = values({
      isbn13: "978-4-00-310111-7",
      cCode: "０１９３",
      authors: [{ name: "", kana: "", role: "" }],
    });
    expect(isFieldChanged("isbn13", initial, current)).toBe(false);
    expect(isFieldChanged("cCode", initial, current)).toBe(false);
    expect(isFieldChanged("authors", initial, current)).toBe(false);
  });

  it("値を変えたら変えたことになる", () => {
    expect(isFieldChanged("title", values({ title: "a" }), values({ title: "b" }))).toBe(true);
    expect(isFieldChanged("rating", values({ rating: null }), values({ rating: 3 }))).toBe(true);
  });
});

describe("valuesFromDetail", () => {
  it("get_book_detail の値を、入力欄の値にする", () => {
    const detail = makeDetail({
      title: "おにぎりレシピ101",
      listPrice: 1300,
      pages: 128,
      authors: [{ name: "佐藤", kana: null, role: "著" }],
      tags: ["料理"],
      rating: 4,
      readStatus: "read",
      locationId: "loc-1",
    });
    const form = valuesFromDetail(detail, "料理シリーズ");
    expect(form).toMatchObject({
      title: "おにぎりレシピ101",
      listPrice: "1300",
      pages: "128",
      authors: [{ name: "佐藤", kana: "", role: "著" }],
      tags: ["料理"],
      rating: 4,
      readStatus: "read",
      locationId: "loc-1",
      seriesName: "料理シリーズ",
    });
    // 取り込んだ値を変えずにそのまま保存しても、変更にならない
    for (const key of Object.keys(form) as (keyof BookFormValues)[]) {
      expect(isFieldChanged(key, form, valuesFromDetail(detail, "料理シリーズ"))).toBe(false);
    }
  });
});

describe("フォームの項目の一覧", () => {
  it("Op の項目名から引ける（サーバーの検証エラーを項目に結びつける）", () => {
    expect(fieldDefByOp("published_on")?.key).toBe("publishedOn");
    expect(fieldDefByOp("series_id")?.key).toBe("seriesName");
    expect(fieldDefByOp("no_such_field")).toBeUndefined();
    expect(fieldDef("coverHash").meta).toBe("cover");
  });
});
