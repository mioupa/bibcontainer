// このフォルダのテストが使う、書誌取得の結果などの作り方。
import type { BookDraft } from "@/bindings/BookDraft";
import type { MetaStatus } from "@/bindings/MetaStatus";
import type { LookupOutcome } from "@/features/scanScreen/batch";
import type { FetchMetadataResult } from "@/types/contracts";

const OPENBD = { kind: "provider", name: "openbd" } as const;

export function makeDraft(
  isbn13: string,
  options: { title?: string; status?: MetaStatus; cCode?: string; price?: number } = {},
): BookDraft {
  const { title = `本 ${isbn13}`, status = "ok", cCode, price } = options;
  const fields: BookDraft["fields"] = {
    isbn13: { value: isbn13, origin: { kind: "barcode" } },
    meta_status: { value: status, origin: { kind: "provider", name: "bibcontainer" } },
  };
  if (status !== "pending") {
    fields.title = { value: title, origin: OPENBD };
    fields.authors = { value: [{ name: "夏目漱石", kana: null, role: "著" }], origin: OPENBD };
    fields.publisher = { value: "岩波書店", origin: OPENBD };
  }
  if (cCode !== undefined) fields.c_code = { value: cCode, origin: OPENBD };
  if (price !== undefined) fields.list_price = { value: price, origin: OPENBD };
  return { fields, seriesName: null };
}

export function makeResult(
  isbn13: string,
  options: Parameters<typeof makeDraft>[1] & { offline?: boolean } = {},
): FetchMetadataResult {
  const { offline = false, ...draftOptions } = options;
  return {
    merged: { isbn13, fields: [] },
    draft: makeDraft(isbn13, {
      ...draftOptions,
      status: draftOptions.status ?? (offline ? "pending" : "ok"),
    }),
    reports: [],
    offline,
    coverHash: null,
  };
}

export function done(
  isbn13: string,
  options: Parameters<typeof makeResult>[1] = {},
): LookupOutcome {
  return { status: "done", result: makeResult(isbn13, options) };
}

export const ISBN_A = "9784003101117";
export const ISBN_B = "9784780802047";
export const ISBN_C = "9784101010014";
