import { describe, expect, it } from "vitest";
import type { MetaPriorities } from "@/bindings/MetaPriorities";
import type { MetaPrioritySets } from "@/bindings/MetaPrioritySets";
import {
  META_GROUPS,
  PRIORITY_PROFILES,
  isFixedSource,
  moveSource,
  orderOf,
  withGroupOrder,
  withProfileGroupOrder,
} from "@/features/settings/priorities";

describe("moveSource（優先順位の並べ替え）", () => {
  it("1 つ下へ、1 つ上へ動かす", () => {
    expect(moveSource(["openbd", "ndl", "google"], 0, 1)).toEqual(["ndl", "openbd", "google"]);
    expect(moveSource(["openbd", "ndl", "google"], 2, -1)).toEqual(["openbd", "google", "ndl"]);
  });

  it("端から外へは動かさない", () => {
    expect(moveSource(["openbd", "ndl"], 0, -1)).toEqual(["openbd", "ndl"]);
    expect(moveSource(["openbd", "ndl"], 1, 1)).toEqual(["openbd", "ndl"]);
  });

  it("2段目（barcode）は動かせず、その上にも動かせない（本体価格・Cコードで最優先）", () => {
    expect(moveSource(["barcode", "openbd", "ndl"], 0, 1)).toEqual(["barcode", "openbd", "ndl"]);
    expect(moveSource(["barcode", "openbd", "ndl"], 1, -1)).toEqual(["barcode", "openbd", "ndl"]);
    expect(moveSource(["barcode", "openbd", "ndl"], 1, 1)).toEqual(["barcode", "ndl", "openbd"]);
    expect(isFixedSource("barcode")).toBe(true);
    expect(isFixedSource("openbd")).toBe(false);
  });

  it("元の配列は書き換えない", () => {
    const order = ["openbd", "ndl"] as const;
    moveSource(order, 0, 1);
    expect(order).toEqual(["openbd", "ndl"]);
  });
});

describe("withGroupOrder / orderOf", () => {
  const priorities: MetaPriorities = {
    groups: { title: ["openbd", "ndl", "google"], cover: ["openbd", "google"] },
  };

  it("1 つの行だけを差し替え、ほかの行はそのまま", () => {
    const next = withGroupOrder(priorities, "title", ["google", "openbd", "ndl"]);
    expect(next.groups.title).toEqual(["google", "openbd", "ndl"]);
    expect(next.groups.cover).toEqual(["openbd", "google"]);
    expect(priorities.groups.title).toEqual(["openbd", "ndl", "google"]);
  });

  it("設定にない行は空", () => {
    expect(orderOf(priorities, "ndc")).toEqual([]);
    expect(orderOf(priorities, "cover")).toEqual(["openbd", "google"]);
  });

  it("画面に出す行は 14 個で、重複しない", () => {
    expect(META_GROUPS).toHaveLength(14);
    expect(new Set(META_GROUPS).size).toBe(14);
  });
});

describe("withProfileGroupOrder（和書・洋書などの組）", () => {
  const sets: MetaPrioritySets = {
    japanese: { groups: { title: ["openbd", "ndl", "google"], cover: ["openbd", "google"] } },
    foreign: { groups: { title: ["google", "ndl", "openbd"], cover: ["google", "openbd"] } },
  };

  it("指定した組の 1 つの行だけを差し替え、もう一方の組と、ほかの行はそのまま", () => {
    const next = withProfileGroupOrder(sets, "foreign", "title", ["ndl", "google", "openbd"]);
    expect(next.foreign.groups.title).toEqual(["ndl", "google", "openbd"]);
    expect(next.foreign.groups.cover).toEqual(["google", "openbd"]);
    expect(next.japanese).toEqual(sets.japanese);
    // 元は書き換えない
    expect(sets.foreign.groups.title).toEqual(["google", "ndl", "openbd"]);

    const other = withProfileGroupOrder(sets, "japanese", "cover", ["google", "openbd"]);
    expect(other.japanese.groups.cover).toEqual(["google", "openbd"]);
    expect(other.foreign).toEqual(sets.foreign);
  });

  it("組は和書、洋書などの順で 2 つ", () => {
    expect(PRIORITY_PROFILES).toEqual(["japanese", "foreign"]);
  });
});
