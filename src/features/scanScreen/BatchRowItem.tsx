// 連続登録の一覧の 1 行（FR-SCAN-14）。状態（取得中／完了／一部取得／取得失敗／取得待ち／重複）、
// 保存の対象にするか、編集、削除。
import {
  CircleCheckIcon,
  CircleXIcon,
  CopyIcon,
  LoaderCircleIcon,
  PencilIcon,
  TriangleAlertIcon,
  Trash2Icon,
  WifiOffIcon,
} from "lucide-react";
import type { ReactElement } from "react";
import { BookCover } from "@/components/BookCover";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  rowLoading,
  rowStatus,
  rowSummary,
  type BatchRow,
  type LookupOutcome,
  type RowStatus,
} from "@/features/scanScreen/batch";
import { ja } from "@/i18n/ja";
import { cCodeFullLabel } from "@/lib/cCode";
import { formatYen } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { CCodeTable } from "@/types/contracts";

const STATUS_ICON: Record<RowStatus, ReactElement> = {
  loading: <LoaderCircleIcon className="animate-spin text-info" aria-hidden />,
  done: <CircleCheckIcon className="text-success" aria-hidden />,
  partial: <TriangleAlertIcon className="text-warning" aria-hidden />,
  failed: <CircleXIcon className="text-destructive" aria-hidden />,
  pending: <WifiOffIcon className="text-warning" aria-hidden />,
  duplicate: <CopyIcon className="text-warning" aria-hidden />,
};

interface BatchRowItemProps {
  row: BatchRow;
  lookup: LookupOutcome | undefined;
  cCodes: CCodeTable;
  onInclude: (included: boolean) => void;
  onEdit: () => void;
  onRemove: () => void;
}

export function BatchRowItem({
  row,
  lookup,
  cCodes,
  onInclude,
  onEdit,
  onRemove,
}: BatchRowItemProps) {
  const status = rowStatus(row, lookup);
  const loading = rowLoading(row, lookup);
  const summary = rowSummary(row, lookup);
  const title = summary.title ?? ja.scan.batch.untitled;
  const b = ja.scan.batch;
  const cCodeLabel = cCodeFullLabel(cCodes, summary.cCode) ?? summary.cCode;
  const duplicateText =
    row.duplicate === null
      ? null
      : row.duplicate.kind === "owned"
        ? b.duplicate.owned(row.duplicate.title, row.duplicate.locationPath)
        : b.duplicate.inBatch;

  return (
    <tr
      data-testid="batch-row"
      data-status={status}
      data-included={row.included}
      className={cn("border-b align-top last:border-b-0", !row.included && "opacity-60")}
    >
      <td className="w-10 px-3 py-3">
        <Checkbox
          checked={row.included}
          onCheckedChange={(checked) => onInclude(checked === true)}
          aria-label={b.includeRow(title)}
        />
      </td>
      <td className="w-14 py-3">
        <BookCover
          id={row.key}
          title={title}
          authors={summary.authors}
          coverHash={summary.coverHash}
          variant="sm"
        />
      </td>
      <td className="min-w-0 py-3 pr-3">
        <p className="font-medium break-words">{title}</p>
        <p className="text-xs text-muted-foreground">
          {[summary.authors, summary.publisher]
            .filter((text) => text !== "" && text !== null)
            .join(" ／ ")}
        </p>
        <p className="text-xs text-muted-foreground tabular-nums">
          {[
            row.isbn13,
            cCodeLabel,
            summary.listPrice === null ? null : formatYen(summary.listPrice),
          ]
            .filter((text): text is string => text !== null && text !== "")
            .join(" ／ ")}
        </p>
        {duplicateText !== null && <p className="mt-1 text-xs text-warning">{duplicateText}</p>}
      </td>
      <td className="w-44 py-3 pr-3 text-sm">
        <span className="inline-flex items-center gap-1.5 [&_svg]:size-4">
          {STATUS_ICON[status]}
          <span>{b.status[status]}</span>
        </span>
        {row.edited !== null && (
          <span className="ml-2 text-xs text-muted-foreground">{b.editedMark}</span>
        )}
        {loading && status === "duplicate" && (
          <span className="ml-1 inline-flex items-center [&_svg]:size-3.5">
            <LoaderCircleIcon className="animate-spin text-info" aria-hidden />
            <span className="sr-only">{b.status.loading}</span>
          </span>
        )}
      </td>
      <td className="w-24 py-3 pr-3">
        <div className="flex items-center justify-end gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onEdit}
            disabled={loading}
            aria-label={b.editRow(title)}
            title={b.edit}
          >
            <PencilIcon />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onRemove}
            aria-label={b.removeRow(title)}
            title={b.remove}
          >
            <Trash2Icon />
          </Button>
        </div>
      </td>
    </tr>
  );
}
