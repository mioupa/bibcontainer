// 列の対応（CSV の列ごとに、取り込む先の項目を選ぶ）。
import type { ColumnMapping } from "@/bindings/ColumnMapping";
import type { CsvEncoding } from "@/bindings/CsvEncoding";
import type { CsvPreview } from "@/bindings/CsvPreview";
import type { ImportTarget } from "@/bindings/ImportTarget";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { ja } from "@/i18n/ja";
import { TARGET_GROUPS, canProceed, targetLabel, targetOfColumn } from "./model";

const selectClass =
  "h-8 w-full rounded-md border border-input bg-transparent px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30";

interface MappingStepProps {
  fileName: string;
  preview: CsvPreview | null;
  /** 自動判定した文字コード（強制しているあいだも、選択肢の表示に使う）。 */
  autoEncoding: CsvEncoding | null;
  encoding: CsvEncoding | null;
  hasHeader: boolean;
  mapping: ColumnMapping[];
  loading: boolean;
  onEncoding: (encoding: CsvEncoding | null) => void;
  onHasHeader: (hasHeader: boolean) => void;
  onTarget: (column: number, target: ImportTarget | null) => void;
}

export function MappingStep(props: MappingStepProps) {
  const t = ja.csvImport.mapping;
  const { preview, mapping } = props;
  const encodingValue = props.encoding ?? "auto";
  return (
    <div className="flex min-h-0 flex-col gap-3">
      <p className="text-sm text-muted-foreground">{t.description}</p>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
        <span className="font-medium break-all" data-testid="csv-file-name">
          {t.file(props.fileName)}
        </span>
        <label className="flex items-center gap-2">
          {t.encoding}
          <select
            className={`${selectClass} w-auto`}
            aria-label={t.encoding}
            value={encodingValue}
            disabled={props.loading}
            onChange={(event) => {
              const v = event.target.value;
              props.onEncoding(v === "auto" ? null : (v as CsvEncoding));
            }}
          >
            <option value="auto">
              {props.autoEncoding === null
                ? t.encodingAutoPending
                : t.encodingAuto(props.autoEncoding)}
            </option>
            <option value="utf8">{t.encodingOptions.utf8}</option>
            <option value="shiftJis">{t.encodingOptions.shiftJis}</option>
          </select>
        </label>
        <div className="flex items-center gap-2">
          <Checkbox
            id="csv-has-header"
            checked={props.hasHeader}
            disabled={props.loading}
            onCheckedChange={(value) => props.onHasHeader(value === true)}
          />
          <Label htmlFor="csv-has-header">{t.hasHeader}</Label>
        </div>
        {preview !== null && (
          <span className="text-muted-foreground" data-testid="csv-row-count">
            {t.rowCount(preview.rowCount)}
          </span>
        )}
      </div>

      {preview === null ? (
        props.loading && <p className="text-sm text-muted-foreground">{t.loading}</p>
      ) : (
        <div className="min-h-0 overflow-auto rounded-lg border">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-muted text-left text-xs">
              <tr>
                <th className="px-3 py-2 font-medium">{t.column}</th>
                <th className="px-3 py-2 font-medium">{t.sample}</th>
                <th className="px-3 py-2 font-medium">{t.target}</th>
              </tr>
            </thead>
            <tbody className="divide-y">
              {preview.columns.map((column) => {
                const name = column.header ?? t.columnName(column.index);
                const current = targetOfColumn(mapping, column.index);
                return (
                  <tr key={column.index}>
                    <th scope="row" className="px-3 py-2 text-left font-medium">
                      {name}
                    </th>
                    <td className="max-w-56 px-3 py-2 text-xs text-muted-foreground">
                      {column.samples.length === 0 ? (
                        t.noSample
                      ) : (
                        <ul>
                          {column.samples.map((sample, i) => (
                            <li key={i} className="truncate">
                              {sample}
                            </li>
                          ))}
                        </ul>
                      )}
                    </td>
                    <td className="px-3 py-2">
                      <select
                        className={selectClass}
                        aria-label={t.targetOf(name)}
                        value={current ?? ""}
                        onChange={(event) =>
                          props.onTarget(
                            column.index,
                            event.target.value === "" ? null : (event.target.value as ImportTarget),
                          )
                        }
                      >
                        <option value="">{t.skip}</option>
                        {TARGET_GROUPS.map((group) => (
                          <optgroup key={group.key} label={ja.csvImport.groups[group.key]}>
                            {group.targets.map((target) => (
                              <option key={target} value={target}>
                                {targetLabel(target)}
                              </option>
                            ))}
                          </optgroup>
                        ))}
                      </select>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {preview !== null && !canProceed(mapping) && (
        <p className="text-sm text-muted-foreground" data-testid="csv-need-target">
          {t.needTarget}
        </p>
      )}
    </div>
  );
}
