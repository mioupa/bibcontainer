import { describe, expect, it } from "vitest";
import type { BookDraft } from "@/bindings/BookDraft";
import {
  applySecondTier,
  toMetaSecondTier,
  withSecondTier,
} from "@/features/scanScreen/secondTier";
import type { FetchMetadataResult } from "@/types/contracts";

const OPENBD = { kind: "provider", name: "openbd" } as const;

function draft(): BookDraft {
  return {
    fields: {
      isbn13: { value: "9784003101117", origin: { kind: "barcode" } },
      title: { value: "こころ", origin: OPENBD },
      c_code: { value: "0093", origin: OPENBD },
      list_price: { value: 600, origin: OPENBD },
      meta_status: { value: "ok", origin: { kind: "provider", name: "bibcontainer" } },
    },
    seriesName: null,
  };
}

describe("applySecondTier", () => {
  it("Cコードと本体価格を、由来 barcode で上書きする（2段目が最優先）", () => {
    const result = applySecondTier(draft(), { cCode: "0193", listPrice: 700 });
    expect(result.fields.c_code).toEqual({ value: "0193", origin: { kind: "barcode" } });
    expect(result.fields.list_price).toEqual({ value: 700, origin: { kind: "barcode" } });
  });

  it("ほかの項目はそのまま", () => {
    const result = applySecondTier(draft(), { cCode: "0193", listPrice: 700 });
    expect(result.fields.title).toEqual({ value: "こころ", origin: OPENBD });
    expect(result.fields.isbn13?.value).toBe("9784003101117");
    expect(result.fields.meta_status.value).toBe("ok");
  });

  it("下書きに Cコード・価格がなくても足す", () => {
    const base: BookDraft = {
      fields: { meta_status: { value: "pending", origin: { kind: "manual" } } },
      seriesName: null,
    };
    const result = applySecondTier(base, { cCode: "0193", listPrice: 700 });
    expect(result.fields.c_code?.value).toBe("0193");
    expect(result.fields.list_price?.value).toBe(700);
  });

  it("価格が不明（null）なときは、取得元の価格を上書きしない。Cコードだけ上書きする（AT-04）", () => {
    const result = applySecondTier(draft(), { cCode: "0193", listPrice: null });
    expect(result.fields.list_price).toEqual({ value: 600, origin: OPENBD });
    expect(result.fields.c_code).toEqual({ value: "0193", origin: { kind: "barcode" } });
  });

  it("Cコードが null のときは、Cコードを上書きしない", () => {
    const result = applySecondTier(draft(), { cCode: null, listPrice: 700 });
    expect(result.fields.c_code).toEqual({ value: "0093", origin: OPENBD });
    expect(result.fields.list_price?.value).toBe(700);
  });

  it("2段目がない・中身がないときは、渡した下書きをそのまま返す", () => {
    const base = draft();
    expect(applySecondTier(base, null)).toBe(base);
    expect(applySecondTier(base, undefined)).toBe(base);
    expect(applySecondTier(base, { cCode: null, listPrice: null })).toBe(base);
  });

  it("渡した下書きは書き換えない", () => {
    const base = draft();
    const snapshot = structuredClone(base);
    applySecondTier(base, { cCode: "0193", listPrice: 700 });
    expect(base).toEqual(snapshot);
  });
});

describe("withSecondTier", () => {
  const result = (): FetchMetadataResult => ({
    merged: { isbn13: "9784003101117", fields: [] },
    draft: draft(),
    reports: [],
    offline: false,
    coverHash: null,
  });

  it("取得結果の下書きに 2段目を重ねる（ほかの項目は変えない）", () => {
    const base = result();
    const next = withSecondTier(base, { cCode: "0193", listPrice: 700 });
    expect(next.draft.fields.c_code?.value).toBe("0193");
    expect(next.offline).toBe(false);
    expect(base.draft.fields.c_code?.value).toBe("0093");
  });

  it("2段目がなければ、同じ取得結果を返す", () => {
    const base = result();
    expect(withSecondTier(base, null)).toBe(base);
  });
});

describe("toMetaSecondTier", () => {
  it("フォームと書誌取得に渡す形にする。2段目がなければ undefined", () => {
    expect(toMetaSecondTier({ cCode: "0193", listPrice: 700 })).toEqual({
      cCode: "0193",
      listPrice: 700,
    });
    expect(toMetaSecondTier(null)).toBeUndefined();
  });
});
