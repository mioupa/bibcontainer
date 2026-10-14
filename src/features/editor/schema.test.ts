import { describe, expect, it } from "vitest";
import { isValidPartialDate, todayLocal } from "@/features/editor/dates";
import { emptyFormValues, type BookFormValues } from "@/features/editor/formValues";
import { parseServerValidation, validateBookForm } from "@/features/editor/schema";
import { ja } from "@/i18n/ja";

const form = (patch: Partial<BookFormValues>): BookFormValues => ({
  ...emptyFormValues(),
  title: "本",
  ...patch,
});

describe("zod の検証（コアの規則に合わせる）", () => {
  it("正しい値はエラーなし", () => {
    expect(
      validateBookForm(
        form({
          isbn13: "978-4-00-310111-7",
          cCode: "0193",
          publishedOn: "2026-10",
          listPrice: "1,300",
          pages: "128",
          purchasedOn: "2026-10-14",
          readStartedOn: "2026",
          rating: 5,
          authors: [{ name: "夏目漱石", kana: "", role: "著" }],
        }),
      ),
    ).toEqual({});
  });

  it("ISBN: チェックデジット不一致・桁違いは弾く。ISBN-10 は ISBN-13 に直して通す。空は可", () => {
    expect(validateBookForm(form({ isbn13: "9784003101118" })).isbn13).toBe(ja.edit.errors.isbn);
    expect(validateBookForm(form({ isbn13: "12345" })).isbn13).toBe(ja.edit.errors.isbn);
    expect(validateBookForm(form({ isbn13: "4003101111" }))).toEqual({});
    expect(validateBookForm(form({ isbn13: "" }))).toEqual({});
  });

  it("Cコード: 4 桁の数字だけ。全角は半角に直す", () => {
    expect(validateBookForm(form({ cCode: "193" })).cCode).toBe(ja.edit.errors.cCode);
    expect(validateBookForm(form({ cCode: "01a3" })).cCode).toBe(ja.edit.errors.cCode);
    expect(validateBookForm(form({ cCode: "０１９３" }))).toEqual({});
  });

  it("日付: YYYY・YYYY-MM・YYYY-MM-DD の実在する日付。4 つの日付欄すべて", () => {
    for (const key of ["publishedOn", "readStartedOn", "readFinishedOn", "purchasedOn"] as const) {
      expect(validateBookForm(form({ [key]: "2026-02-30" }))[key]).toBe(ja.edit.errors.date);
      expect(validateBookForm(form({ [key]: "2026/10/14" }))[key]).toBe(ja.edit.errors.date);
      expect(validateBookForm(form({ [key]: "2024-02-29" }))).toEqual({});
    }
  });

  it("金額・ページ数: 0 以上の整数。マイナス・小数・文字・大きすぎる値は弾く", () => {
    for (const key of ["listPrice", "pages", "purchasePrice"] as const) {
      expect(validateBookForm(form({ [key]: "-1" }))[key]).toBe(ja.edit.errors.integer);
      expect(validateBookForm(form({ [key]: "1.5" }))[key]).toBe(ja.edit.errors.integer);
      expect(validateBookForm(form({ [key]: "abc" }))[key]).toBe(ja.edit.errors.integer);
      expect(validateBookForm(form({ [key]: "99999999999" }))[key]).toBe(ja.edit.errors.integer);
      expect(validateBookForm(form({ [key]: "0" }))).toEqual({});
    }
  });

  it("評価は 1〜5。0 や 6 は弾く", () => {
    expect(validateBookForm(form({ rating: 0 })).rating).toBeDefined();
    expect(validateBookForm(form({ rating: 6 })).rating).toBeDefined();
    expect(validateBookForm(form({ rating: 3 }))).toEqual({});
  });

  it("書名: ISBN がなければ必須。ISBN があれば空でもよい（取得待ちで保存できる）", () => {
    expect(validateBookForm(form({ title: "  " })).title).toBe(ja.edit.errors.titleRequired);
    expect(validateBookForm(form({ title: "", isbn13: "9784003101117" }))).toEqual({});
  });

  it("著者: ヨミや役割だけで名前が空の行は弾く。まるごと空の行は無視する", () => {
    const errors = validateBookForm(
      form({
        authors: [
          { name: "", kana: "", role: "" },
          { name: "", kana: "なつめ", role: "" },
        ],
      }),
    );
    expect(errors["authors.1.name"]).toBe(ja.edit.errors.authorName);
    expect(errors["authors.0.name"]).toBeUndefined();
  });
});

describe("日付の判定", () => {
  it("うるう年と月の範囲", () => {
    expect(isValidPartialDate("2024-02-29")).toBe(true);
    expect(isValidPartialDate("2025-02-29")).toBe(false);
    expect(isValidPartialDate("2026-13")).toBe(false);
    expect(isValidPartialDate("2026-00")).toBe(false);
    expect(isValidPartialDate("0001")).toBe(true);
    expect(isValidPartialDate("26")).toBe(false);
    expect(isValidPartialDate("2026-1")).toBe(false);
  });
  it("今日の日付は、ローカルの YYYY-MM-DD", () => {
    expect(todayLocal(new Date(2026, 9, 14, 23, 59))).toBe("2026-10-14");
    expect(todayLocal(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});

describe("サーバーの検証エラー（AppError の kind: validation）を項目に結びつける", () => {
  it("項目名（Op の名前）からフォームの項目を引く", () => {
    expect(
      parseServerValidation("入力が不正です（rating）: 1 から 5 の範囲で指定してください（値: 6）"),
    ).toEqual({
      key: "rating",
      field: "rating",
      message: "1 から 5 の範囲で指定してください（値: 6）",
    });
    expect(parseServerValidation("入力が不正です（published_on）: 日付が不正です")?.key).toBe(
      "publishedOn",
    );
    expect(parseServerValidation("入力が不正です（series_id）: x")?.key).toBe("seriesName");
  });

  it("フォームにない項目・形の違うメッセージ", () => {
    expect(parseServerValidation("入力が不正です（path）: x")?.key).toBeNull();
    expect(parseServerValidation("何かのエラー")).toBeNull();
  });
});
