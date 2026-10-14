import { describe, expect, it } from "vitest";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import {
  EMPTY_PRESETS,
  buildNewBookDrafts,
  canSave,
  createRow,
  fallbackDraft,
  loadingCount,
  parseTagInput,
  presetChanges,
  rowDraft,
  rowLoading,
  rowNewBookDraft,
  rowStatus,
  rowSummary,
  type BatchRow,
  type LookupOutcome,
} from "@/features/scanScreen/batch";
import { ISBN_A, ISBN_B, done, makeResult } from "@/features/scanScreen/testData";

const SECOND = { cCode: "0193", listPrice: 700, raw: "1920193007008" };

function row(isbn13 = ISBN_A, overrides: Partial<BatchRow> = {}): BatchRow {
  return {
    ...createRow(`row-${isbn13}`, { isbn13, isbn10: null, secondTier: null }, null),
    ...overrides,
  };
}

describe("rowStatus（行の状態の決め方）", () => {
  it("取得結果がまだなければ『取得中』", () => {
    expect(rowStatus(row(), undefined)).toBe("loading");
    expect(rowLoading(row(), undefined)).toBe(true);
  });

  it("取得状態 ok → 完了、partial → 一部取得、failed → 取得失敗、pending → 取得待ち", () => {
    expect(rowStatus(row(), done(ISBN_A, { status: "ok" }))).toBe("done");
    expect(rowStatus(row(), done(ISBN_A, { status: "partial" }))).toBe("partial");
    expect(rowStatus(row(), done(ISBN_A, { status: "failed" }))).toBe("failed");
    expect(rowStatus(row(), done(ISBN_A, { status: "pending" }))).toBe("pending");
  });

  it("オフラインの取得結果（取得待ちの下書き）は『取得待ち』（AT-11）", () => {
    expect(rowStatus(row(), done(ISBN_A, { offline: true }))).toBe("pending");
  });

  it("fetch_metadata 自体が失敗したときも、取得待ちとして保存できる（FR-SCAN-16）", () => {
    const failed: LookupOutcome = { status: "failed", message: "x" };
    expect(rowStatus(row(), failed)).toBe("pending");
    expect(rowDraft(row(), failed)?.fields.meta_status.value).toBe("pending");
  });

  it("重複の行は、取得の状態より先に『重複』", () => {
    const owned = row(ISBN_A, {
      duplicate: { kind: "owned", title: "こころ", locationPath: null },
    });
    expect(rowStatus(owned, done(ISBN_A))).toBe("duplicate");
    expect(rowStatus(owned, undefined)).toBe("duplicate");
    expect(rowStatus(row(ISBN_A, { duplicate: { kind: "inBatch" } }), done(ISBN_A))).toBe(
      "duplicate",
    );
  });

  it("行を直したあとは、取得結果を待たず、直した内容の取得状態で決める", () => {
    const edited: NewBookDraft = {
      changes: [
        { field: "title", value: "直した書名", origin: { kind: "manual" } },
        { field: "meta_status", value: "partial", origin: { kind: "manual" } },
      ],
    };
    const r = row(ISBN_A, { edited });
    expect(rowLoading(r, undefined)).toBe(false);
    expect(rowStatus(r, undefined)).toBe("partial");
  });
});

describe("rowDraft / rowSummary", () => {
  it("取得結果の下書きに 2段目を重ねる", () => {
    const r = row(ISBN_A, { secondTier: SECOND });
    const draft = rowDraft(r, done(ISBN_A, { cCode: "0093", price: 600 }));
    expect(draft?.fields.c_code).toEqual({ value: "0193", origin: { kind: "barcode" } });
    expect(draft?.fields.list_price).toEqual({ value: 700, origin: { kind: "barcode" } });
  });

  it("取得中は null", () => {
    expect(rowDraft(row(), undefined)).toBeNull();
  });

  it("一覧に出す書名・著者・出版社・Cコード・価格を取り出す", () => {
    const r = row(ISBN_A, { secondTier: SECOND });
    expect(rowSummary(r, done(ISBN_A, { title: "こころ" }))).toEqual({
      title: "こころ",
      authors: "夏目漱石",
      publisher: "岩波書店",
      cCode: "0193",
      listPrice: 700,
      coverHash: null,
    });
  });

  it("直した行は、直した内容を出す", () => {
    const edited: NewBookDraft = {
      changes: [
        { field: "title", value: "直した書名", origin: { kind: "manual" } },
        { field: "cover_hash", value: "ab".repeat(32), origin: { kind: "manual" } },
      ],
    };
    const summary = rowSummary(row(ISBN_A, { edited }), done(ISBN_A, { title: "元の書名" }));
    expect(summary.title).toBe("直した書名");
    expect(summary.coverHash).toBe("ab".repeat(32));
  });

  it("取得中の行は、空の内容", () => {
    expect(rowSummary(row(), undefined)).toMatchObject({ title: null, authors: "" });
  });
});

describe("fallbackDraft", () => {
  it("ISBN と 2段目だけの、取得待ちの下書き", () => {
    const draft = fallbackDraft(ISBN_A, SECOND);
    expect(draft.fields.isbn13).toEqual({ value: ISBN_A, origin: { kind: "barcode" } });
    expect(draft.fields.meta_status.value).toBe("pending");
    expect(draft.fields.c_code?.value).toBe("0193");
    expect(draft.fields.list_price?.value).toBe(700);
    expect(draft.fields.title).toBeUndefined();
  });
});

describe("createRow", () => {
  it("重複でない行は保存の対象、重複の行は既定で対象から外す", () => {
    const book = { isbn13: ISBN_A, isbn10: null, secondTier: null };
    expect(createRow("k", book, null).included).toBe(true);
    expect(createRow("k", book, { kind: "inBatch" }).included).toBe(false);
  });
});

describe("共通値（presets）", () => {
  it("入れた項目だけを、由来 manual の FieldChange にする", () => {
    expect(presetChanges(EMPTY_PRESETS)).toEqual([]);
    expect(
      presetChanges({
        locationId: "loc-1",
        tags: ["研究/SiC", "小説"],
        ownership: "wanted",
        purchasedOn: "2026-10-14",
        purchaseStore: " 紀伊國屋 ",
      }),
    ).toEqual([
      { field: "location_id", value: "loc-1", origin: { kind: "manual" } },
      { field: "tags", value: ["研究/SiC", "小説"], origin: { kind: "manual" } },
      { field: "ownership", value: "wanted", origin: { kind: "manual" } },
      { field: "purchased_on", value: "2026-10-14", origin: { kind: "manual" } },
      { field: "purchase_store", value: "紀伊國屋", origin: { kind: "manual" } },
    ]);
  });

  it("タグの入力は、カンマ・読点・改行で区切り、空と重複を除く", () => {
    expect(parseTagInput(" 研究/SiC, 小説、小説\n 技術 ,, ")).toEqual(["研究/SiC", "小説", "技術"]);
    expect(parseTagInput("")).toEqual([]);
  });
});

describe("rowNewBookDraft / buildNewBookDrafts（共通値の重ね方）", () => {
  const presets = presetChanges({
    ...EMPTY_PRESETS,
    locationId: "loc-1",
    ownership: "wanted",
  });

  it("直していない行: 下書きの項目に、共通値（手入力）を重ねる。取得状態も保存される", () => {
    const draft = rowNewBookDraft(
      row(),
      done(ISBN_A, { status: "pending", offline: true }),
      presets,
    );
    const byField = Object.fromEntries(draft!.changes.map((c) => [c.field, c]));
    expect(byField.location_id).toEqual({
      field: "location_id",
      value: "loc-1",
      origin: { kind: "manual" },
    });
    expect(byField.ownership?.value).toBe("wanted");
    expect(byField.meta_status?.value).toBe("pending");
    expect(byField.isbn13?.origin).toEqual({ kind: "barcode" });
  });

  it("直した行: 行の内容を優先し、行にない項目だけを共通値で補う", () => {
    const edited: NewBookDraft = {
      changes: [
        { field: "title", value: "直した書名", origin: { kind: "manual" } },
        { field: "location_id", value: "loc-9", origin: { kind: "manual" } },
      ],
    };
    const draft = rowNewBookDraft(row(ISBN_A, { edited }), undefined, presets);
    const byField = Object.fromEntries(draft!.changes.map((c) => [c.field, c.value]));
    expect(byField.location_id).toBe("loc-9");
    expect(byField.ownership).toBe("wanted");
    expect(byField.title).toBe("直した書名");
  });

  it("取得中の行は null", () => {
    expect(rowNewBookDraft(row(), undefined, presets)).toBeNull();
  });

  it("シリーズ名は、下書きのものを引き継ぐ", () => {
    const result = makeResult(ISBN_A);
    const outcome: LookupOutcome = {
      status: "done",
      result: {
        ...result,
        draft: { ...result.draft, seriesName: { name: "岩波文庫", origin: { kind: "manual" } } },
      },
    };
    expect(rowNewBookDraft(row(), outcome, [])!.seriesName).toEqual({
      name: "岩波文庫",
      origin: { kind: "manual" },
    });
  });

  it("保存の対象の行だけを、一覧の順に保存する形にする", () => {
    const rows = [
      row(ISBN_A),
      row("9784101010014", { included: false }),
      row(ISBN_B, { secondTier: SECOND }),
    ];
    const lookups = {
      [ISBN_A]: done(ISBN_A),
      [ISBN_B]: done(ISBN_B),
      "9784101010014": done("9784101010014"),
    };
    const drafts = buildNewBookDrafts(rows, lookups, {
      ...EMPTY_PRESETS,
      purchaseStore: "紀伊國屋",
    });
    expect(drafts).toHaveLength(2);
    expect(drafts.map((d) => d.changes.find((c) => c.field === "isbn13")?.value)).toEqual([
      ISBN_A,
      ISBN_B,
    ]);
    for (const d of drafts) {
      expect(d.changes.find((c) => c.field === "purchase_store")?.value).toBe("紀伊國屋");
    }
    expect(drafts[1]!.changes.find((c) => c.field === "c_code")?.value).toBe("0193");
  });
});

describe("canSave / loadingCount", () => {
  const rows = [row(ISBN_A), row(ISBN_B), row("9784101010014", { included: false })];

  it("保存の対象に取得中の行があるあいだは保存できない（取得が終わるのを待つ）", () => {
    expect(canSave(rows, { [ISBN_A]: done(ISBN_A) })).toBe(false);
    expect(loadingCount(rows, { [ISBN_A]: done(ISBN_A) })).toBe(1);
  });

  it("対象の行がすべて取得済みなら保存できる。対象外の行の取得は待たない", () => {
    expect(canSave(rows, { [ISBN_A]: done(ISBN_A), [ISBN_B]: done(ISBN_B) })).toBe(true);
  });

  it("対象の行が 1 冊もなければ保存できない", () => {
    expect(canSave([], {})).toBe(false);
    expect(canSave([row(ISBN_A, { included: false })], { [ISBN_A]: done(ISBN_A) })).toBe(false);
  });
});
