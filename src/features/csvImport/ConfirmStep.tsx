// 確認（csv_import_check の結果）。件数、新しく作るもの、問題の行の一覧。
import { useState } from "react";
import type { CsvImportPlan } from "@/bindings/CsvImportPlan";
import { ja } from "@/i18n/ja";
import {
  ISSUE_FILTERS,
  MAX_ISSUE_ROWS,
  MAX_LISTED_NAMES,
  defaultFilter,
  filterIssues,
  reasonText,
  type IssueFilter,
} from "./model";

function NameList({ heading, names }: { heading: string; names: string[] }) {
  if (names.length === 0) return null;
  const shown = names.slice(0, MAX_LISTED_NAMES);
  const rest = names.length - shown.length;
  return (
    <div className="text-sm">
      <p className="font-medium">
        {heading}（{names.length}）
      </p>
      <p className="text-xs text-muted-foreground">
        {shown.join("、")}
        {rest > 0 && `、${ja.csvImport.confirm.more(rest)}`}
      </p>
    </div>
  );
}

export function ConfirmStep({ plan }: { plan: CsvImportPlan }) {
  const t = ja.csvImport.confirm;
  const [filter, setFilter] = useState<IssueFilter>(() => defaultFilter(plan.issues));
  const skipTotal =
    plan.skipped.registeredIsbn +
    plan.skipped.sameId +
    plan.skipped.duplicateInFile +
    plan.skipped.trashedRow;
  const breakdown = [
    [plan.skipped.registeredIsbn, t.skipBreakdown.registeredIsbn],
    [plan.skipped.sameId, t.skipBreakdown.sameId],
    [plan.skipped.duplicateInFile, t.skipBreakdown.duplicateInFile],
    [plan.skipped.trashedRow, t.skipBreakdown.trashedRow],
  ] as const;
  const rows = filterIssues(plan.issues, filter);
  const shown = rows.slice(0, MAX_ISSUE_ROWS);

  return (
    <div className="flex min-h-0 flex-col gap-3 overflow-auto">
      <div className="flex flex-col gap-1 text-sm" data-testid="csv-summary">
        <p className="font-medium">{t.willImport(plan.toImport)}</p>
        {plan.toFetch > 0 && <p className="text-muted-foreground">{t.willFetch(plan.toFetch)}</p>}
        {skipTotal > 0 && (
          <div>
            <p>{t.skipped(skipTotal)}</p>
            <ul className="ml-4 list-disc text-xs text-muted-foreground">
              {breakdown
                .filter(([count]) => count > 0)
                .map(([count, format]) => (
                  <li key={format(0)}>{format(count)}</li>
                ))}
            </ul>
          </div>
        )}
        {plan.errors > 0 && <p className="text-destructive">{t.errors(plan.errors)}</p>}
        {plan.warnedRows > 0 && <p>{t.warned(plan.warnedRows)}</p>}
      </div>
      <NameList heading={t.newLocations} names={plan.newLocations} />
      <NameList heading={t.newSeries} names={plan.newSeries} />

      {plan.issues.length > 0 && (
        <div className="flex min-h-0 flex-col gap-2">
          <div role="group" aria-label={t.issuesHeading} className="flex flex-wrap gap-1">
            {ISSUE_FILTERS.map((f) => {
              const count = filterIssues(plan.issues, f).length;
              return (
                <button
                  key={f}
                  type="button"
                  aria-pressed={filter === f}
                  className="rounded-md border px-2 py-1 text-xs aria-pressed:bg-accent aria-pressed:font-medium"
                  onClick={() => setFilter(f)}
                >
                  {t.filterCount(t.filter[f], count)}
                </button>
              );
            })}
          </div>
          {rows.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t.noIssues}</p>
          ) : (
            <div className="max-h-64 overflow-auto rounded-lg border">
              <table className="w-full text-xs" data-testid="csv-issues">
                <thead className="sticky top-0 bg-muted text-left">
                  <tr>
                    <th className="px-2 py-1 font-medium">{t.line}</th>
                    <th className="px-2 py-1 font-medium">{t.content}</th>
                    <th className="px-2 py-1 font-medium">{t.reason}</th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {shown.map((issue) => (
                    <tr key={`${issue.line}-${issue.status}`} className="align-top">
                      <td className="px-2 py-1 whitespace-nowrap">{t.lineNumber(issue.line)}</td>
                      <td className="px-2 py-1">
                        {issue.title !== null && issue.title !== "" && <p>{issue.title}</p>}
                        {issue.isbn13 !== null && issue.isbn13 !== "" && (
                          <p className="font-mono text-muted-foreground">{issue.isbn13}</p>
                        )}
                      </td>
                      <td className="px-2 py-1">
                        {issue.reasons.map((reason, i) => (
                          <p key={i}>{reasonText(reason)}</p>
                        ))}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {rows.length > shown.length && (
                <p className="px-2 py-1 text-xs text-muted-foreground">
                  {t.moreRows(rows.length - shown.length)}
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
