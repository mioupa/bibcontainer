import { describe, expect, it } from "vitest";
import { splitUrls } from "@/lib/linkify";

describe("splitUrls", () => {
  it("かっこの中の URL を取り出し、前後の文字はそのまま残す", () => {
    expect(splitUrls("書誌情報: 国立国会図書館サーチ API (https://ndlsearch.ndl.go.jp/)")).toEqual([
      { kind: "text", text: "書誌情報: 国立国会図書館サーチ API (" },
      { kind: "url", url: "https://ndlsearch.ndl.go.jp/" },
      { kind: "text", text: ")" },
    ]);
  });

  it("URL がなければ、全体が 1 つの文字。空の文字列は何も返さない", () => {
    expect(splitUrls("Powered by Google")).toEqual([{ kind: "text", text: "Powered by Google" }]);
    expect(splitUrls("")).toEqual([]);
  });

  it("文の終わりの「.」「、」「。」は URL に含めない。http は URL として扱わず、複数の URL も扱う", () => {
    expect(
      splitUrls("見る: https://openbd.jp/path?a=1。または https://books.google.com/."),
    ).toEqual([
      { kind: "text", text: "見る: " },
      { kind: "url", url: "https://openbd.jp/path?a=1" },
      { kind: "text", text: "。または " },
      { kind: "url", url: "https://books.google.com/" },
      { kind: "text", text: "." },
    ]);
    expect(splitUrls("http://example.com")).toEqual([{ kind: "text", text: "http://example.com" }]);
  });

  it("分けた部分をつなぐと、元の文字列に戻る", () => {
    const source =
      "書誌情報・書影: Open Library (https://openlibrary.org/) と https://github.com/a/b.";
    const joined = splitUrls(source)
      .map((part) => (part.kind === "text" ? part.text : part.url))
      .join("");
    expect(joined).toBe(source);
  });
});
