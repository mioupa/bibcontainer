import { describe, expect, it } from "vitest";
import { SEARCH_REPLACED_KEYS, hasValue, keysToFill } from "@/features/editor/fetchApply";
import { emptyFormValues, type BookFormValues } from "@/features/editor/formValues";

const values = (overrides: Partial<BookFormValues> = {}): BookFormValues => ({
  ...emptyFormValues(),
  ...overrides,
});
const author = (name: string) => ({ name, kana: "", role: "" });

describe("keysToFill（取得結果を入れる項目）", () => {
  const initial = values();

  it("直していない項目に、取得結果が変えたものだけを入れる。ISBN の欄は決めない", () => {
    const next = values({ title: "取得した書名", publisher: "岩波書店", isbn13: "9784003101117" });
    expect(keysToFill({ initial, current: initial, next })).toEqual(["title", "publisher"]);
  });

  it("手で直した項目は、そのまま（取得結果が違っても入れない）", () => {
    const current = values({ title: "自分で入れた書名", publisher: "自費出版" });
    const next = values({ title: "取得した書名", publisher: "岩波書店", pages: "100" });
    expect(keysToFill({ initial, current, next })).toEqual(["pages"]);
  });

  it("前の取得のままの項目は、新しい取得結果（空になる項目も）で置き換える", () => {
    const previous = values({ title: "前の書名", publisher: "前の出版社" });
    const next = values({ title: "新しい書名" });
    expect(keysToFill({ initial: previous, current: previous, next })).toEqual([
      "title",
      "publisher",
    ]);
  });

  it("検索結果を選んだとき: 書名と著者は、直してあっても、取得結果に値があれば置き換える", () => {
    const current = values({
      title: "探すために入れた書名",
      authors: [author("探すために入れた著者")],
      publisher: "自分で入れた出版社",
    });
    const next = values({
      title: "こころ",
      authors: [author("夏目漱石")],
      publisher: "岩波書店",
    });
    expect(keysToFill({ initial, current, next, replace: SEARCH_REPLACED_KEYS })).toEqual([
      "title",
      "authors",
    ]);
  });

  it("置き換える項目でも、取得結果に値がなければ、直した値を残す", () => {
    const current = values({ title: "探すために入れた書名", authors: [author("入れた著者")] });
    const next = values({ title: "", authors: [], publisher: "岩波書店" });
    expect(keysToFill({ initial, current, next, replace: SEARCH_REPLACED_KEYS })).toEqual([
      "publisher",
    ]);
  });
});

describe("hasValue", () => {
  it("文字列は、空白だけなら値なし。著者は、名前のない行だけなら値なし", () => {
    expect(hasValue("title", values({ title: "  " }))).toBe(false);
    expect(hasValue("title", values({ title: "本" }))).toBe(true);
    expect(hasValue("authors", values({ authors: [author("")] }))).toBe(false);
    expect(hasValue("authors", values({ authors: [author("夏目漱石")] }))).toBe(true);
  });
});
