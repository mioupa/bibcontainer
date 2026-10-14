import { describe, expect, it } from "vitest";
import { mockBackend } from "@/dev/mockBackend";

// ブラウザで `pnpm dev` を開いたときのモックが、フォーム・詳細・一括変更の流れで動くこと
describe("mockBackend（登録・編集・一括変更）", () => {
  it("下書きから本を作り、詳細で読め、更新・タグの変更もできる", async () => {
    const [id] = await mockBackend.createBooksFromDrafts([
      {
        changes: [
          { field: "title", value: "モックの本", origin: { kind: "manual" } },
          { field: "isbn13", value: "9784003101117", origin: { kind: "barcode" } },
          { field: "memo", value: "最初のメモ", origin: { kind: "manual" } },
        ],
        seriesName: { name: "モックのシリーズ", origin: { kind: "manual" } },
      },
    ]);
    expect(id).toBeDefined();
    const created = (await mockBackend.getRows([id ?? ""]))[0];
    expect(created).toMatchObject({
      title: "モックの本",
      isbn13: "9784003101117",
      seriesName: "モックのシリーズ",
    });

    // 行にない項目（メモ）も、詳細に残る
    expect((await mockBackend.getBookDetail(id ?? "")).memo).toBe("最初のメモ");

    const report = await mockBackend.updateBook(id ?? "", [
      { field: "title", value: "直した本", origin: { kind: "manual" } },
      { field: "memo", value: "直したメモ", origin: { kind: "manual" } },
    ]);
    expect(report.written).toEqual(["title", "memo"]);
    const detail = await mockBackend.getBookDetail(id ?? "");
    expect(detail).toMatchObject({ title: "直した本", memo: "直したメモ" });

    expect(await mockBackend.modifyTags([id ?? ""], ["小説", "新刊"], [])).toEqual([id]);
    expect(await mockBackend.modifyTags([id ?? ""], ["小説"], ["新刊"])).toEqual([id]);
    expect((await mockBackend.getRows([id ?? ""]))[0]?.tags).toEqual(["小説"]);
    // 変わらないとき、変えた本はない
    expect(await mockBackend.modifyTags([id ?? ""], ["小説"], [])).toEqual([]);

    expect((await mockBackend.findByIsbn("9784003101117")).map((m) => m.id)).toContain(id);
  });
});
