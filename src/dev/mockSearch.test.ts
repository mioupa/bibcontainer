import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockSearch } from "@/dev/mockSearch";

// ブラウザで `pnpm dev` を開いたときの、キーワード検索のダミーの結果
describe("mockSearch", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const search = async (id: Parameters<typeof mockSearch.searchMetadata>[0], title: string) => {
    const promise = mockSearch.searchMetadata(id, title, null);
    await vi.runAllTimersAsync();
    return promise;
  };

  it("検索に対応する取得元は NDL・Google・Open Library（openBD は対応しない）", async () => {
    const providers = await mockSearch.listProviders();
    expect(providers.filter((p) => p.supportsSearch).map((p) => p.id)).toEqual([
      "ndl",
      "google",
      "openlibrary",
    ]);
    expect(providers.every((p) => p.enabled && p.active)).toBe(true);
  });

  it("NDL は ISBN のない本を含む 3 件、Google は link つきの 2 件、Open Library は 2 件", async () => {
    const ndl = await search("ndl", "こころ");
    expect(ndl.hits).toHaveLength(3);
    expect(ndl.hits.some((hit) => hit.isbn13 === null)).toBe(true);
    const google = await search("google", "Clean Code");
    expect(google.hits).toHaveLength(2);
    expect(google.hits.every((hit) => hit.link?.startsWith("https://books.google.com/"))).toBe(
      true,
    );
    const openLibrary = await search("openlibrary", "Kokoro");
    expect(openLibrary.hits).toHaveLength(2);
    expect(openLibrary.outcome).toEqual({ kind: "found" });
  });

  it("書名に「エラー」を入れると、Google は HTTP 503、Open Library は通信エラー", async () => {
    expect((await search("google", "エラー")).outcome).toEqual({
      kind: "httpError",
      status: 503,
      message: "Service Unavailable",
    });
    expect((await search("openlibrary", "エラー")).outcome.kind).toBe("networkError");
    expect((await search("ndl", "エラー")).outcome).toEqual({ kind: "found" });
  });

  it("検索に対応しない取得元は、validation で断る", async () => {
    const promise = mockSearch.searchMetadata("openbd", "本", null);
    await expect(promise).rejects.toMatchObject({ kind: "validation" });
  });
});
