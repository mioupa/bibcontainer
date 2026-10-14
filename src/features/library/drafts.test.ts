import { describe, expect, it } from "vitest";
import type { BookDraft } from "@/bindings/BookDraft";
import type { FieldChange } from "@/bindings/FieldChange";
import { draftToChanges, toNewBookDraft } from "@/features/library/drafts";

const barcode = { kind: "barcode" } as const;
const openbd = { kind: "provider", name: "openbd" } as const;
const status = {
  value: "pending",
  origin: { kind: "provider", name: "bibcontainer" },
} as const;

const draft: BookDraft = {
  fields: {
    isbn13: { value: "9784003101117", origin: barcode },
    title: { value: "吾輩は猫である", origin: openbd },
    authors: { value: [{ name: "夏目漱石", kana: null, role: "著" }], origin: openbd },
    list_price: { value: 700, origin: barcode },
    volume: { value: "上", origin: openbd },
    volume_sort: { value: 1, origin: openbd },
    meta_status: status,
  },
  seriesName: { name: "岩波文庫", origin: openbd },
};

describe("draftToChanges", () => {
  it("下書きの項目を、値と由来つきの FieldChange にする。取得状態も項目として入る", () => {
    const changes = draftToChanges(draft);
    expect(changes.map((c) => c.field)).toEqual([
      "isbn13",
      "title",
      "authors",
      "list_price",
      "volume",
      "volume_sort",
      "meta_status",
    ]);
    expect(changes[0]).toEqual({ field: "isbn13", value: "9784003101117", origin: barcode });
    expect(changes.find((c) => c.field === "title")?.origin).toEqual(openbd);
    expect(changes.find((c) => c.field === "meta_status")?.value).toBe("pending");
    expect(changes.find((c) => c.field === "authors")?.value).toEqual([
      { name: "夏目漱石", kana: null, role: "著" },
    ]);
  });

  it("値のない項目（キーがない・undefined）は含めない", () => {
    expect(draftToChanges({ fields: { meta_status: status }, seriesName: null })).toEqual([
      { field: "meta_status", value: "pending", origin: status.origin },
    ]);
    const sparse: BookDraft = {
      fields: {
        title: undefined,
        isbn13: { value: "9784003101117", origin: barcode },
        meta_status: status,
      },
      seriesName: null,
    };
    expect(draftToChanges(sparse).map((c) => c.field)).toEqual(["isbn13", "meta_status"]);
  });
});

describe("toNewBookDraft", () => {
  it("下書きの項目とシリーズ名を、そのまま作成の入力にする", () => {
    const built = toNewBookDraft(draft);
    expect(built.changes).toEqual(draftToChanges(draft));
    expect(built.seriesName).toEqual({ name: "岩波文庫", origin: openbd });
  });

  it("フォームで直した項目は、下書きの値を置き換える（由来は手入力）。下書きにない項目は足す", () => {
    const edits: FieldChange[] = [
      { field: "title", value: "吾輩は猫である（改）", origin: { kind: "manual" } },
      { field: "memo", value: "メモ", origin: { kind: "manual" } },
    ];
    const built = toNewBookDraft(draft, edits);
    const titles = built.changes.filter((c) => c.field === "title");
    expect(titles).toEqual([edits[0]]);
    expect(built.changes.at(-1)).toEqual(edits[1]);
    // 直していない項目は、取得元・2段目の由来のまま
    expect(built.changes.find((c) => c.field === "list_price")?.origin).toEqual(barcode);
    expect(built.changes.find((c) => c.field === "isbn13")?.origin).toEqual(barcode);
    expect(built.changes).toHaveLength(draftToChanges(draft).length + 1);
  });

  it("シリーズ名は、渡せば置き換え（null で外す）、渡さなければ下書きのまま", () => {
    const named = { name: "別のシリーズ", origin: { kind: "manual" } } as const;
    expect(toNewBookDraft(draft, [], { seriesName: named }).seriesName).toEqual(named);
    expect(toNewBookDraft(draft, [], { seriesName: null }).seriesName).toBeNull();
    expect(toNewBookDraft(draft, [], {}).seriesName).toEqual(draft.seriesName);
  });

  it("元の下書きは書き換えない", () => {
    const before = JSON.stringify(draft);
    toNewBookDraft(draft, [{ field: "title", value: "x", origin: { kind: "manual" } }]);
    expect(JSON.stringify(draft)).toBe(before);
  });
});
