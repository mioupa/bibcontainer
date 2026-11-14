import { describe, expect, it } from "vitest";
import type { ImportTarget } from "@/bindings/ImportTarget";
import type { RowIssue } from "@/bindings/RowIssue";
import { ja } from "@/i18n/ja";
import {
  TARGET_GROUPS,
  baseName,
  canProceed,
  defaultFilter,
  fileName,
  filterIssues,
  reasonText,
  setTarget,
  targetLabel,
} from "./model";

// ImportTarget の全値。bindings の型に項目が増えると、ここが型エラーになって気づける
const ALL_TARGETS = Object.keys({
  id: 0,
  isbn13: 0,
  title: 0,
  title_kana: 0,
  subtitle: 0,
  series: 0,
  volume: 0,
  volume_sort: 0,
  work_id: 0,
  authors: 0,
  publisher: 0,
  published_on: 0,
  c_code: 0,
  ndc: 0,
  list_price: 0,
  ref_price_incl: 0,
  pages: 0,
  size_text: 0,
  language: 0,
  description: 0,
  ownership: 0,
  wish_priority: 0,
  location: 0,
  tags: 0,
  read_status: 0,
  read_started_on: 0,
  read_finished_on: 0,
  rating: 0,
  review: 0,
  purchased_on: 0,
  purchase_store: 0,
  purchase_price: 0,
  purchase_cond: 0,
  purchase_memo: 0,
  memo: 0,
  trashed: 0,
  created_at: 0,
} satisfies Record<ImportTarget, 0>) as ImportTarget[];

describe("項目の一覧", () => {
  it("すべての項目に名前があり、グループにちょうど 1 回ずつ入っている", () => {
    const grouped = TARGET_GROUPS.flatMap((g) => g.targets);
    expect([...grouped].sort()).toEqual([...ALL_TARGETS].sort());
    for (const target of ALL_TARGETS) {
      expect(targetLabel(target)).not.toBe("");
    }
    expect(Object.keys(ja.csvImport.targets).sort()).toEqual([...ALL_TARGETS].sort());
  });

  it("書き出しの見出しと同じ名前", () => {
    expect(targetLabel("isbn13")).toBe("ISBN");
    expect(targetLabel("title")).toBe("書名");
    expect(targetLabel("id")).toBe("ID（登録済みかの確認だけ）");
  });
});

describe("setTarget", () => {
  it("同じ項目を別の列に割り当てると、前の列は外れる", () => {
    let m = setTarget([], 0, "title");
    m = setTarget(m, 1, "isbn13");
    m = setTarget(m, 2, "title");
    expect(m).toEqual([
      { column: 1, target: "isbn13" },
      { column: 2, target: "title" },
    ]);
  });

  it("列の項目を変えると、その列の前の対応は置き換わる。null で外す", () => {
    let m = setTarget([], 0, "title");
    m = setTarget(m, 0, "authors");
    expect(m).toEqual([{ column: 0, target: "authors" }]);
    expect(setTarget(m, 0, null)).toEqual([]);
  });
});

describe("canProceed", () => {
  it("ISBN か書名があれば進める", () => {
    expect(canProceed([])).toBe(false);
    expect(canProceed([{ column: 0, target: "authors" }])).toBe(false);
    expect(canProceed([{ column: 0, target: "isbn13" }])).toBe(true);
    expect(canProceed([{ column: 3, target: "title" }])).toBe(true);
  });
});

const issue = (status: RowIssue["status"], line: number): RowIssue => ({
  line,
  status,
  isbn13: null,
  title: null,
  reasons: [],
});

describe("問題の行の絞り込み", () => {
  const issues = [issue("skip", 2), issue("import", 3), issue("skip", 4)];

  it("warn は警告つきで取り込む行", () => {
    expect(filterIssues(issues, "skip").map((i) => i.line)).toEqual([2, 4]);
    expect(filterIssues(issues, "warn").map((i) => i.line)).toEqual([3]);
    expect(filterIssues(issues, "error")).toEqual([]);
  });

  it("最初の絞り込みは、取り込めない → 飛ばす → 値を読めない の順で、件数のあるもの", () => {
    expect(defaultFilter(issues)).toBe("skip");
    expect(defaultFilter([issue("import", 1)])).toBe("warn");
    expect(defaultFilter([issue("error", 1), issue("skip", 2)])).toBe("error");
    expect(defaultFilter([])).toBe("error");
  });
});

describe("理由の文", () => {
  it("種類ごとに文にする", () => {
    expect(reasonText({ kind: "registeredIsbn", existingTitle: "既存の本" })).toContain("既存の本");
    expect(reasonText({ kind: "sameId", trashed: false })).toBe("同じ ID の本がある");
    expect(reasonText({ kind: "duplicateInFile", firstLine: 7 })).toBe("7 行目と同じ");
    expect(reasonText({ kind: "trashedRow" })).toBe("ゴミ箱の行");
    expect(reasonText({ kind: "noTitleOrIsbn" })).toBe("書名も ISBN もありません");
    expect(reasonText({ kind: "unreadableValue", target: "published_on", value: "昨日" })).toBe(
      "発行日 を読めません（昨日）",
    );
  });

  it("Excel の桁落ちには、対処の説明を添える", () => {
    const plain = reasonText({ kind: "invalidIsbn", value: "9.78E+12", excelNumber: false });
    const excel = reasonText({ kind: "invalidIsbn", value: "9.78E+12", excelNumber: true });
    expect(plain).not.toContain("Excel");
    expect(excel).toContain("Excel");
    expect(excel).toContain("9.78E+12");
  });
});

describe("ファイル名", () => {
  it("/ と \\ の両方で区切り、拡張子を除く", () => {
    expect(fileName("/a/b/books.csv")).toBe("books.csv");
    expect(fileName("C:\\a\\books.csv")).toBe("books.csv");
    expect(baseName("/a/books.2026.csv")).toBe("books.2026");
    expect(baseName("/a/noext")).toBe("noext");
  });
});
