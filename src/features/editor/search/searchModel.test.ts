import { describe, expect, it } from "vitest";
import type { ProviderOutcome } from "@/bindings/ProviderOutcome";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import {
  authorsText,
  hasJapaneseScript,
  hitDomId,
  hitKey,
  isFailure,
  movedIndex,
  publicationText,
  searchTargets,
  sectionOrder,
  statusText,
  type SectionState,
} from "@/features/editor/search/searchModel";
import { ja } from "@/i18n/ja";
import { makeHit, makeProviderStatus, makeSearchResult } from "@/test/search";

const text = ja.edit.search;

const status = (overrides: Partial<ProviderStatus> & Pick<ProviderStatus, "id">) =>
  makeProviderStatus(overrides.id, overrides);

describe("検索語の文字（かな・漢字があるか）", () => {
  it("ひらがな・カタカナ・漢字があれば、和書の検索語", () => {
    expect(hasJapaneseScript("こころ")).toBe(true);
    expect(hasJapaneseScript("ココロ")).toBe(true);
    expect(hasJapaneseScript("夏目漱石")).toBe(true);
    expect(hasJapaneseScript("Clean 吾輩")).toBe(true);
    // 半角のカタカナも
    expect(hasJapaneseScript("ｶﾀｶﾅ")).toBe(true);
  });

  it("英数字・記号・空だけなら、和書の検索語ではない", () => {
    expect(hasJapaneseScript("Clean Code")).toBe(false);
    expect(hasJapaneseScript("Война и мир")).toBe(false);
    expect(hasJapaneseScript("1984 !?")).toBe(false);
    expect(hasJapaneseScript("")).toBe(false);
  });
});

describe("取得元の並び", () => {
  it("かな・漢字があれば NDL、Google、Open Library の順", () => {
    expect(sectionOrder("こころ", "")).toEqual(["ndl", "google", "openlibrary"]);
    expect(sectionOrder("コーヒー", "")).toEqual(["ndl", "google", "openlibrary"]);
    expect(sectionOrder("", "夏目漱石")).toEqual(["ndl", "google", "openlibrary"]);
    // 書名が英語でも、著者に漢字があれば和書の並び（混在）
    expect(sectionOrder("Kokoro", "夏目漱石")).toEqual(["ndl", "google", "openlibrary"]);
  });

  it("かな・漢字がなければ Google、Open Library、NDL の順（空も）", () => {
    expect(sectionOrder("Clean Code", "Martin")).toEqual(["google", "openlibrary", "ndl"]);
    expect(sectionOrder("", "")).toEqual(["google", "openlibrary", "ndl"]);
  });

  it("検索の対象は、検索に対応していて有効な取得元だけ。使えない（キーがない）取得元は残す", () => {
    const providers = [
      status({ id: "openbd", supportsSearch: false }),
      status({ id: "openlibrary" }),
      status({ id: "google", active: false }),
      status({ id: "ndl", enabled: false, active: false }),
    ];
    // 無効な NDL と、検索に対応しない openBD は出さない
    expect(searchTargets(providers, "Clean Code", "").map((p) => p.id)).toEqual([
      "google",
      "openlibrary",
    ]);
    // 全部有効なら、検索語で並べ替える
    const all = [
      status({ id: "openlibrary" }),
      status({ id: "google" }),
      status({ id: "ndl" }),
      status({ id: "openbd", supportsSearch: false }),
    ];
    expect(searchTargets(all, "こころ", "").map((p) => p.id)).toEqual([
      "ndl",
      "google",
      "openlibrary",
    ]);
    expect(searchTargets(all, "Kokoro", "").map((p) => p.id)).toEqual([
      "google",
      "openlibrary",
      "ndl",
    ]);
    // 元の配列は並べ替えない
    expect(all.map((p) => p.id)).toEqual(["openlibrary", "google", "ndl", "openbd"]);
  });
});

describe("各取得元の状態の文言", () => {
  const result = (outcome: ProviderOutcome, total: number | null, hits: number): SectionState => ({
    kind: "result",
    result: makeSearchResult("ndl", { outcome, total, hits: makeHits(hits) }),
  });
  const makeHits = (n: number) =>
    Array.from({ length: n }, (_, i) => makeHit("ndl", { isbn13: null, title: `本 ${i}` }));

  it("検索中・API キーなし・コマンドの失敗", () => {
    expect(statusText({ kind: "loading" })).toBe(text.searching);
    expect(statusText({ kind: "noKey" })).toBe(text.noKey);
    expect(text.noKey).toContain("API キーがないので探していません");
    expect(statusText({ kind: "error", message: "書名か著者を入れてください" })).toBe(
      "書名か著者を入れてください",
    );
  });

  it("見つかったら件数。総件数が表示した数より多ければ「全 X 件のうち N 件」", () => {
    expect(statusText(result({ kind: "found" }, null, 3))).toBe("3 件");
    expect(statusText(result({ kind: "found" }, 3, 3))).toBe("3 件");
    expect(statusText(result({ kind: "found" }, 120, 20))).toBe("全 120 件のうち 20 件");
    // 総件数が少なく出ても、表示した数を出す
    expect(statusText(result({ kind: "found" }, 1, 2))).toBe("2 件");
  });

  it("見つからない・通信できない・HTTP エラー・応答を読めない", () => {
    expect(statusText(result({ kind: "notFound" }, 0, 0))).toBe("見つかりませんでした");
    expect(statusText(result({ kind: "networkError", message: "x" }, null, 0))).toBe(
      "通信できませんでした",
    );
    expect(statusText(result({ kind: "httpError", status: 503, message: "x" }, null, 0))).toBe(
      "取得元がエラーを返しました（HTTP 503）",
    );
    expect(statusText(result({ kind: "parseError", message: "x" }, null, 0))).toBe(
      "応答を読めませんでした",
    );
  });

  it("失敗として目立たせるのは、通信・HTTP・応答のエラーと、コマンドの失敗だけ", () => {
    expect(isFailure({ kind: "error", message: "x" })).toBe(true);
    expect(isFailure(result({ kind: "networkError", message: "x" }, null, 0))).toBe(true);
    expect(isFailure(result({ kind: "httpError", status: 500, message: "x" }, null, 0))).toBe(true);
    expect(isFailure(result({ kind: "parseError", message: "x" }, null, 0))).toBe(true);
    expect(isFailure(result({ kind: "notFound" }, 0, 0))).toBe(false);
    expect(isFailure(result({ kind: "found" }, 1, 1))).toBe(false);
    expect(isFailure({ kind: "loading" })).toBe(false);
    expect(isFailure({ kind: "noKey" })).toBe(false);
  });
});

describe("結果の行の文字", () => {
  it("著者は、役割があれば（）をつけて「、」でつなぐ。名前のない行は除く", () => {
    expect(
      authorsText([
        { name: "夏目漱石", kana: null, role: "著" },
        { name: "Edwin McClellan", kana: null, role: null },
        { name: "", kana: "ヨミ", role: "訳" },
      ]),
    ).toBe("夏目漱石（著）、Edwin McClellan");
    expect(authorsText([])).toBe("");
  });

  it("出版社と発行日は「 · 」でつなぐ。片方だけ・どちらもなし", () => {
    const hit = (publisher: string | null, publishedOn: string | null) =>
      makeHit("ndl", { publisher, publishedOn });
    expect(publicationText(hit("岩波書店", "2019-05"))).toBe("岩波書店 · 2019-05");
    expect(publicationText(hit("岩波書店", null))).toBe("岩波書店");
    expect(publicationText(hit(null, "1914"))).toBe("1914");
    expect(publicationText(hit(null, null))).toBe("");
  });

  it("行の目印は取得元と位置から作る。DOM の id にしても使える", () => {
    expect(hitKey("google", 2)).toBe("google:2");
    expect(hitDomId(hitKey("google", 2))).toBe("kw-hit-google-2");
  });
});

describe("選択の動かし方", () => {
  it("選んでいなければ、next は最初、prev は最後。first・last はどこからでも端", () => {
    expect(movedIndex(-1, 5, "next")).toBe(0);
    expect(movedIndex(-1, 5, "prev")).toBe(4);
    expect(movedIndex(2, 5, "first")).toBe(0);
    expect(movedIndex(2, 5, "last")).toBe(4);
  });

  it("1 つずつ動かし、端で止まる。行がなければ動かない", () => {
    expect(movedIndex(1, 5, "next")).toBe(2);
    expect(movedIndex(1, 5, "prev")).toBe(0);
    expect(movedIndex(4, 5, "next")).toBe(4);
    expect(movedIndex(0, 5, "prev")).toBe(0);
    expect(movedIndex(-1, 0, "next")).toBeNull();
    expect(movedIndex(-1, 0, "last")).toBeNull();
  });
});
