import { describe, expect, it } from "vitest";
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import { decideConfirmed } from "@/features/scanScreen/decision";

function match(overrides: Partial<IsbnMatch> = {}): IsbnMatch {
  return {
    id: "b1",
    title: "こころ",
    ownership: "owned",
    locationId: null,
    locationPath: null,
    deleted: false,
    ...overrides,
  };
}

describe("decideConfirmed", () => {
  it("登録がなければ新しい本", () => {
    expect(decideConfirmed([])).toEqual({ kind: "new" });
  });

  it("所有で登録済みなら重複（FR-SCAN-17）", () => {
    const owned = match({ locationPath: "自宅 > 書斎" });
    expect(decideConfirmed([owned])).toEqual({ kind: "owned", match: owned });
  });

  it("欲しい本に登録済みなら、所有に移すか確認する（FR-SCAN-18）", () => {
    const wanted = match({ ownership: "wanted" });
    expect(decideConfirmed([wanted])).toEqual({ kind: "wanted", match: wanted });
  });

  it("所有と欲しい本の両方があれば、所有（重複）を優先する", () => {
    const wanted = match({ id: "w", ownership: "wanted" });
    const owned = match({ id: "o" });
    expect(decideConfirmed([wanted, owned])).toEqual({ kind: "owned", match: owned });
  });

  it("ゴミ箱の本は無いものとして扱う", () => {
    expect(decideConfirmed([match({ deleted: true })])).toEqual({ kind: "new" });
    const wanted = match({ id: "w", ownership: "wanted" });
    expect(decideConfirmed([match({ deleted: true }), wanted])).toEqual({
      kind: "wanted",
      match: wanted,
    });
  });

  it("手放した本だけのときは、新しい本として登録できる", () => {
    expect(decideConfirmed([match({ ownership: "released" })])).toEqual({ kind: "new" });
  });
});
