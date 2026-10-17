import { describe, expect, it } from "vitest";
import { MOCK_LOCATIONS, generateMockRows, normalizeSortKey } from "@/dev/mockRows";
import { mockBackend, mockRowCountFromLocation } from "@/dev/mockBackend";
import { groupRows } from "@/features/library/grouping";
import { stripTrailingVolume } from "@/features/library/works";

function isValidIsbn13(isbn: string): boolean {
  if (!/^\d{13}$/.test(isbn)) return false;
  let sum = 0;
  for (let i = 0; i < 12; i++) sum += Number(isbn[i]) * (i % 2 === 0 ? 1 : 3);
  return (10 - (sum % 10)) % 10 === Number(isbn[12]);
}

describe("ダミーデータ", () => {
  it("同じ種なら同じ結果になる（再現できる）", () => {
    expect(generateMockRows(50, 1)).toEqual(generateMockRows(50, 1));
    expect(generateMockRows(50, 1)).not.toEqual(generateMockRows(50, 2));
  });

  it("5,000〜20,000 冊を作れ、ID が重ならない", () => {
    const rows = generateMockRows(20_000);
    expect(rows).toHaveLength(20_000);
    expect(new Set(rows.map((r) => r.id)).size).toBe(20_000);
  });

  it("ISBN は有効なチェックデジットを持つ。0005 の ListRow の制約を満たす", () => {
    const rows = generateMockRows(2000);
    const locationIds = new Set(MOCK_LOCATIONS.map((l) => l.id));
    for (const row of rows) {
      if (row.isbn13 !== null) expect(isValidIsbn13(row.isbn13)).toBe(true);
      if (row.cCode !== null) expect(row.cCode).toMatch(/^\d{4}$/);
      if (row.locationId !== null) expect(locationIds.has(row.locationId)).toBe(true);
      expect(row.sortTitle).toBe(normalizeSortKey(row.titleKana ?? row.title));
      expect(row.volume === null).toBe(row.volumeSort === null);
      if (row.rating !== null) expect(row.rating).toBeGreaterThanOrEqual(1);
    }
    // 各種の値が出ている（絞り込みの試験に使える）
    expect(new Set(rows.map((r) => r.ownership))).toEqual(new Set(["owned", "wanted", "released"]));
    expect(new Set(rows.map((r) => r.metaStatus)).size).toBe(4);
    expect(rows.some((r) => r.deleted)).toBe(true);
    expect(rows.some((r) => r.tags.some((t) => t.includes("/")))).toBe(true);
  });

  it("約 25% の行は、同じ作品の巻（FR-LIST-08）: 同じ書名・著者・出版社で、巻が 1〜n（n は 2〜40）", () => {
    const rows = generateMockRows(20_000);
    const grouped = groupRows(rows, true, new Set());
    const share = grouped.groupOfBook.size / rows.length;
    expect(share).toBeGreaterThan(0.15);
    expect(share).toBeLessThan(0.4);

    // 大きなグループは作品の巻。全巻が同じ出版社・著者（偶然に同じ書名・著者になった別の本が混ざったものは除く）
    const big = [...grouped.groups.values()].filter((g) => g.members.length >= 10);
    expect(big.length).toBeGreaterThan(20);
    const pure = big.filter(
      (g) =>
        new Set(g.members.map((m) => m.publisher)).size === 1 &&
        g.members.every((m) => m.volume !== null || /\d+$/.test(m.title)),
    );
    expect(pure.length / big.length).toBeGreaterThan(0.9);
    for (const group of pure) {
      expect(group.members.length).toBeLessThanOrEqual(40);
      expect(group.firstVolume).toBe("1");
      expect(Number(group.lastVolume)).toBe(group.members.length);
    }
    expect(Math.max(...pure.map((g) => g.members.length))).toBeGreaterThanOrEqual(30);

    // 巻の大半は巻次の欄をもち（volumeSort と一致）、約 10% は巻次がなく、書名が「書名 n」
    let withVolume = 0;
    let inTitle = 0;
    for (const group of pure) {
      for (const [index, member] of group.members.entries()) {
        if (member.volume !== null) {
          withVolume += 1;
          expect(member.volumeSort).toBe(Number(member.volume));
          expect(member.title).toBe(group.baseTitle);
        } else {
          inTitle += 1;
          expect(stripTrailingVolume(member.title, null)).toEqual({
            baseTitle: group.baseTitle,
            impliedVolume: index + 1,
          });
        }
      }
    }
    expect(inTitle / (withVolume + inTitle)).toBeGreaterThan(0.04);
    expect(inTitle / (withVolume + inTitle)).toBeLessThan(0.2);
  });

  it("並べ替えキーは NFKC・小文字・ひらがなに正規化する", () => {
    expect(normalizeSortKey("ＡＢＣ カタカナ")).toBe("abc かたかな");
  });

  it("冊数は URL の ?mockRows= で変えられる（範囲に収まる）", () => {
    expect(mockRowCountFromLocation("")).toBe(5000);
    expect(mockRowCountFromLocation("?mockRows=20000")).toBe(20_000);
    expect(mockRowCountFromLocation("?mockRows=abc")).toBe(5000);
    expect(mockRowCountFromLocation("?mockRows=-5")).toBe(5000);
    expect(mockRowCountFromLocation("?mockRows=9999999")).toBe(100_000);
  });

  it("モックの検索: 空白で区切った語をすべて含む本の ID を返す（かな・全角・大文字小文字は区別しない）", async () => {
    const rows = await mockBackend.listBooks();
    const target = rows.find((r) => r.authors.length > 0 && r.publisher !== null)!;
    const byTitle = await mockBackend.searchBooks(target.title);
    expect(byTitle).toContain(target.id);
    const both = await mockBackend.searchBooks(`${target.title} ${target.publisher}`);
    expect(both).toContain(target.id);
    const none = await mockBackend.searchBooks("存在しない語zzzzqqqq");
    expect(none).toEqual([]);
    // ヨミのカタカナでも、ひらがなの ヨミ に当たる
    const withKana = rows.find((r) => r.titleKana !== null)!;
    const katakana = withKana.titleKana!.replace(/[ぁ-ゖ]/g, (c) =>
      String.fromCharCode(c.charCodeAt(0) + 0x60),
    );
    expect(await mockBackend.searchBooks(katakana)).toContain(withKana.id);
  });
});
