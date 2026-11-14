// CSV 取り込みの純粋な部品（0028）。項目のグループと順、対応づけの更新、問題の行の絞り込み。
import type { ColumnMapping } from "@/bindings/ColumnMapping";
import type { ImportTarget } from "@/bindings/ImportTarget";
import type { IssueReason } from "@/bindings/IssueReason";
import type { RowIssue } from "@/bindings/RowIssue";
import { ja } from "@/i18n/ja";

export type TargetGroupKey = keyof typeof ja.csvImport.groups;

/** 項目の選択肢のグループと並び順。 */
export const TARGET_GROUPS: readonly { key: TargetGroupKey; targets: readonly ImportTarget[] }[] = [
  {
    key: "bibliographic",
    targets: [
      "isbn13",
      "title",
      "title_kana",
      "subtitle",
      "series",
      "volume",
      "volume_sort",
      "authors",
      "publisher",
      "published_on",
      "c_code",
      "ndc",
      "list_price",
      "ref_price_incl",
      "pages",
      "size_text",
      "language",
      "description",
    ],
  },
  {
    key: "classification",
    targets: ["work_id", "ownership", "wish_priority", "location", "tags"],
  },
  {
    key: "reading",
    targets: ["read_status", "read_started_on", "read_finished_on", "rating", "review"],
  },
  {
    key: "purchase",
    targets: ["purchased_on", "purchase_store", "purchase_price", "purchase_cond", "purchase_memo"],
  },
  { key: "other", targets: ["memo", "id", "trashed", "created_at"] },
];

export function targetLabel(target: ImportTarget): string {
  return ja.csvImport.targets[target];
}

export function targetOfColumn(mapping: readonly ColumnMapping[], column: number) {
  return mapping.find((m) => m.column === column)?.target ?? null;
}

/**
 * 列の対応先を変える（null は「取り込まない」）。1 つの項目は 1 つの列からだけ取り込むので、
 * 同じ項目に対応づけてあった別の列は外す。
 */
export function setTarget(
  mapping: readonly ColumnMapping[],
  column: number,
  target: ImportTarget | null,
): ColumnMapping[] {
  const rest = mapping.filter((m) => m.column !== column && m.target !== target);
  return target === null ? rest : [...rest, { column, target }];
}

/** ISBN か書名のどちらかがあれば、先へ進める。 */
export function canProceed(mapping: readonly ColumnMapping[]): boolean {
  return mapping.some((m) => m.target === "isbn13" || m.target === "title");
}

/** 問題の行の絞り込み。warn は「警告つきで取り込む行」。 */
export type IssueFilter = "error" | "skip" | "warn";
export const ISSUE_FILTERS: readonly IssueFilter[] = ["error", "skip", "warn"];

/** 1 つの絞り込みで描く行の上限。 */
export const MAX_ISSUE_ROWS = 500;
/** 新しく作る保管場所・シリーズの一覧で出す件数。 */
export const MAX_LISTED_NAMES = 20;

export function filterIssues(issues: readonly RowIssue[], filter: IssueFilter): RowIssue[] {
  const status = filter === "warn" ? "import" : filter;
  return issues.filter((issue) => issue.status === status);
}

/** 最初に開く絞り込み。件数のあるもののうち、取り込めない・飛ばす・値を読めないの順で最初のもの。 */
export function defaultFilter(issues: readonly RowIssue[]): IssueFilter {
  return ISSUE_FILTERS.find((f) => filterIssues(issues, f).length > 0) ?? "error";
}

/** パスからファイル名を取る（/ と \ の両方）。 */
export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** ファイル名から拡張子を除く。 */
export function baseName(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** 理由の文。ISBN を読めない理由で Excel の桁落ちが疑われるときは、対処の説明を添える。 */
export function reasonText(reason: IssueReason): string {
  const t = ja.csvImport.reason;
  switch (reason.kind) {
    case "registeredIsbn":
      return t.registeredIsbn(reason.existingTitle);
    case "sameId":
      return t.sameId(reason.trashed);
    case "duplicateInFile":
      return t.duplicateInFile(reason.firstLine);
    case "trashedRow":
      return t.trashedRow;
    case "invalidIsbn":
      return reason.excelNumber
        ? `${t.invalidIsbn(reason.value)} ${t.excelHint}`
        : t.invalidIsbn(reason.value);
    case "noTitleOrIsbn":
      return t.noTitleOrIsbn;
    case "unreadableValue":
      return t.unreadableValue(targetLabel(reason.target), reason.value);
  }
}
