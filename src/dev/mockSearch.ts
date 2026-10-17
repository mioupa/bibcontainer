// キーワード検索（書名・著者）のダミーの結果（Tauri の外で `pnpm dev` をブラウザで開いたとき用）。
// 本物は Rust の `search_metadata`。取得元ごとに、少し待ってから決まった結果を返す（検索語には関係しない）。
// 書名か著者に「エラー」を含めると、Google は HTTP 503、Open Library は通信エラーを返す（表示の確認用）。
import type { Candidate } from "@/bindings/Candidate";
import type { ProviderId } from "@/bindings/ProviderId";
import type { ProviderSearchResult } from "@/bindings/ProviderSearchResult";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { SearchHit } from "@/bindings/SearchHit";
import { IpcError } from "@/lib/ipc";

/** 取得元ごとの待ち時間（ミリ秒）。結果が取得元ごとに別々に出ることを確かめられるよう、そろえない。 */
const DELAY_MS: Record<ProviderId, number> = {
  ndl: 350,
  openlibrary: 700,
  google: 1100,
  openbd: 0,
};

function candidate(provider: ProviderId, fields: Partial<Candidate>): Candidate {
  return {
    provider,
    isbn13: "",
    title: null,
    subtitle: null,
    titleKana: null,
    authors: [],
    publisher: null,
    publishedOn: null,
    listPrice: null,
    refPriceIncl: null,
    cCode: null,
    ndc: null,
    pages: null,
    sizeText: null,
    language: null,
    description: null,
    series: null,
    volume: null,
    covers: [],
    ...fields,
  };
}

function hit(
  provider: ProviderId,
  isbn13: string | null,
  link: string | null,
  fields: Partial<Candidate>,
): SearchHit {
  return {
    provider,
    isbn13,
    candidate: candidate(provider, { isbn13: isbn13 ?? "", ...fields }),
    link,
  };
}

const SOSEKI = (kana: string | null) => ({ name: "夏目漱石", kana, role: "著" });

const NDL_HITS: SearchHit[] = [
  hit("ndl", "9784003101117", "https://ndlsearch.ndl.go.jp/books/R100000002-I000000000001", {
    title: "こころ",
    titleKana: "ココロ",
    authors: [SOSEKI("ナツメ ソウセキ")],
    publisher: "岩波書店",
    publishedOn: "2019-05",
    language: "ja",
  }),
  hit("ndl", "9784101010137", "https://ndlsearch.ndl.go.jp/books/R100000002-I000000000002", {
    title: "こころ",
    titleKana: "ココロ",
    authors: [SOSEKI("ナツメ ソウセキ")],
    publisher: "新潮社",
    publishedOn: "2004-02",
    language: "ja",
  }),
  // ISBN のない本（古い本など）
  hit("ndl", null, "https://ndlsearch.ndl.go.jp/books/R100000002-I000000000003", {
    title: "こころ",
    subtitle: "先生の遺書",
    authors: [SOSEKI(null), { name: "安部能成", kana: null, role: "序" }],
    publisher: "岩波書店",
    publishedOn: "1914",
    volume: "上",
    language: "ja",
  }),
];

const GOOGLE_HITS: SearchHit[] = [
  hit("google", "9780132350884", "https://books.google.com/books?id=mockgoogle0001", {
    title: "Clean Code",
    subtitle: "A Handbook of Agile Software Craftsmanship",
    authors: [{ name: "Robert C. Martin", kana: null, role: null }],
    publisher: "Prentice Hall",
    publishedOn: "2008-08-01",
    language: "en",
  }),
  hit("google", "9780134685991", "https://books.google.com/books?id=mockgoogle0002", {
    title: "Effective Java",
    authors: [{ name: "Joshua Bloch", kana: null, role: null }],
    publisher: "Addison-Wesley",
    publishedOn: "2018-01-06",
    language: "en",
  }),
];

const OPEN_LIBRARY_HITS: SearchHit[] = [
  hit("openlibrary", "9780192834102", "https://openlibrary.org/books/OL00000001M", {
    title: "Kokoro",
    authors: [
      { name: "Natsume Sōseki", kana: null, role: null },
      { name: "Edwin McClellan", kana: null, role: "英訳" },
    ],
    publisher: "Oxford University Press",
    publishedOn: "1996",
    language: "en",
  }),
  hit("openlibrary", "9781503971233", "https://openlibrary.org/books/OL00000002M", {
    title: "Kokoro",
    authors: [{ name: "Natsume Sōseki", kana: null, role: null }],
    publisher: "CreateSpace",
    publishedOn: "2015-03",
    language: "en",
  }),
];

const RESULTS: Record<"ndl" | "google" | "openlibrary", { total: number; hits: SearchHit[] }> = {
  ndl: { total: 42, hits: NDL_HITS },
  google: { total: 120, hits: GOOGLE_HITS },
  openlibrary: { total: 2, hits: OPEN_LIBRARY_HITS },
};

function provider(id: ProviderId, displayName: string, supportsSearch: boolean): ProviderStatus {
  return {
    enabled: true,
    hasKey: false,
    active: true,
    id,
    displayName,
    auth: { kind: "none" },
    fields: [],
    rateLimit: { group: id, minIntervalMs: 0, maxConcurrent: 1 },
    credit: "",
    supportsBatch: false,
    supportsSearch,
    needsNetwork: true,
  };
}

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export const mockSearch = {
  /** 取得元の一覧。モックでは、すべて有効で使える（openBD だけ検索に対応しない）。 */
  listProviders: (): Promise<ProviderStatus[]> =>
    Promise.resolve([
      provider("openbd", "openBD", false),
      provider("ndl", "NDLサーチ", true),
      provider("google", "Google Books", true),
      provider("openlibrary", "Open Library", true),
    ]),

  searchMetadata: async (
    id: ProviderId,
    title: string | null,
    author: string | null,
  ): Promise<ProviderSearchResult> => {
    if (id === "openbd") {
      throw new IpcError({
        kind: "validation",
        message: "openBD はキーワード検索に対応していません",
      });
    }
    await wait(DELAY_MS[id]);
    const failing = `${title ?? ""}${author ?? ""}`.includes("エラー");
    if (failing && id === "google") {
      return {
        provider: id,
        outcome: { kind: "httpError", status: 503, message: "Service Unavailable" },
        total: null,
        hits: [],
        elapsedMs: DELAY_MS[id],
      };
    }
    if (failing && id === "openlibrary") {
      return {
        provider: id,
        outcome: { kind: "networkError", message: "接続できませんでした" },
        total: null,
        hits: [],
        elapsedMs: DELAY_MS[id],
      };
    }
    const found = RESULTS[id];
    return {
      provider: id,
      outcome: { kind: "found" },
      total: found.total,
      hits: found.hits,
      elapsedMs: DELAY_MS[id],
    };
  },
};
