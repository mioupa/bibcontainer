// テスト用の検索結果（SearchHit・ProviderSearchResult）を作る。
import type { Candidate } from "@/bindings/Candidate";
import type { ProviderId } from "@/bindings/ProviderId";
import type { ProviderSearchResult } from "@/bindings/ProviderSearchResult";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { SearchHit } from "@/bindings/SearchHit";

/** 検索結果の 1 件。`isbn13`・`link` を除く指定は、候補（`candidate`）の項目。ISBN とリンクの既定は、どちらもなし。 */
export function makeHit(
  provider: ProviderId,
  fields: Omit<Partial<Candidate>, "isbn13"> & {
    isbn13?: string | null;
    link?: string | null;
  } = {},
): SearchHit {
  const { isbn13 = null, link = null, ...candidate } = fields;
  return {
    provider,
    isbn13,
    link,
    candidate: {
      provider,
      isbn13: isbn13 ?? "",
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
      ...candidate,
    },
  };
}

/** `search_metadata` の結果（取得元 1 つ分）。既定は、見つかった・総件数なし・結果なし。 */
export function makeSearchResult(
  provider: ProviderId,
  overrides: Partial<ProviderSearchResult> = {},
): ProviderSearchResult {
  return {
    provider,
    outcome: { kind: "found" },
    total: null,
    hits: [],
    elapsedMs: 100,
    ...overrides,
  };
}

/** `list_providers` の 1 件。既定は、有効で使える・検索に対応・キー不要。 */
export function makeProviderStatus(
  id: ProviderId,
  overrides: Partial<ProviderStatus> = {},
): ProviderStatus {
  return {
    enabled: true,
    hasKey: false,
    active: true,
    id,
    displayName: id,
    auth: { kind: "none" },
    fields: [],
    rateLimit: { group: id, minIntervalMs: 0, maxConcurrent: 1 },
    credit: `クレジット ${id}`,
    supportsBatch: false,
    supportsSearch: true,
    needsNetwork: true,
    ...overrides,
  };
}
