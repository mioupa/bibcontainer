// 一覧のセルの描画。列 id ごとに、行（ListRow）から表示を作る。
import type { ReactNode } from "react";
import { BookCover } from "@/components/BookCover";
import type { ColumnId } from "@/features/list/columnMeta";
import { DEFAULT_PRICE_SETTINGS, formatListPrice } from "@/features/library/price";
import { ja } from "@/i18n/ja";
import { contentLabel, describeCCode } from "@/lib/cCode";
import { formatDateOnly, formatInteger, formatYen } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ListRow } from "@/types/contracts";
import { authorsText, type CellContext } from "@/features/list/cellText";

function text(value: string | null | undefined): ReactNode {
  return value ? <span className="truncate">{value}</span> : null;
}

const READ_STATUS_DOT: Record<ListRow["readStatus"], string> = {
  unread: "bg-muted-foreground/40",
  reading: "bg-info",
  read: "bg-success",
  paused: "bg-warning",
};

export function RatingStars({ rating, className }: { rating: number | null; className?: string }) {
  if (rating === null) return null;
  const value = Math.min(5, Math.max(0, Math.round(rating)));
  return (
    <span className={cn("tracking-tight text-warning", className)} aria-label={`★${value}`}>
      {"★".repeat(value)}
      <span className="text-muted-foreground/40">{"★".repeat(5 - value)}</span>
    </span>
  );
}

/** 列 id に応じたセルの中身。値がなければ空のセル。 */
export function Cell({ id, row, ctx }: { id: ColumnId; row: ListRow; ctx: CellContext }) {
  return <>{renderCell(id, row, ctx)}</>;
}

function renderCell(id: ColumnId, row: ListRow, ctx: CellContext): ReactNode {
  switch (id) {
    case "cover":
      return (
        <BookCover
          id={row.id}
          title={row.title}
          authors={authorsText(row)}
          coverHash={row.coverHash}
          variant="sm"
        />
      );
    case "title": {
      const second = row.subtitle ?? row.seriesName;
      return (
        <span className="flex min-w-0 flex-col leading-tight">
          <span className="truncate font-medium">{row.title}</span>
          {second && <span className="truncate text-xs text-muted-foreground">{second}</span>}
        </span>
      );
    }
    case "volume":
      return text(row.volume);
    case "authors":
      return text(authorsText(row));
    case "publisher":
      return text(row.publisher);
    case "publishedOn":
      return text(row.publishedOn);
    case "cCode": {
      if (row.cCode === null) return null;
      const name = contentLabel(ctx.cCodes, row.cCode);
      return (
        <span className="flex min-w-0 items-baseline gap-1.5">
          <span className="font-mono text-xs text-muted-foreground">{row.cCode}</span>
          {name && <span className="truncate">{name}</span>}
        </span>
      );
    }
    case "location":
      return text(row.locationId === null ? null : (ctx.locationPaths.get(row.locationId) ?? null));
    case "readStatus":
      return (
        <span className="flex items-center gap-1.5">
          <span className={cn("size-2 shrink-0 rounded-full", READ_STATUS_DOT[row.readStatus])} />
          <span className="truncate">{ja.labels.readStatus[row.readStatus]}</span>
        </span>
      );
    case "rating":
      return <RatingStars rating={row.rating} />;
    case "createdAt":
      return text(formatDateOnly(row.createdAt));
    case "isbn13":
      return row.isbn13 ? <span className="truncate font-mono text-xs">{row.isbn13}</span> : null;
    case "series":
      return text(row.seriesName);
    case "listPrice":
      return text(formatListPrice(row.listPrice, ctx.price ?? DEFAULT_PRICE_SETTINGS));
    case "purchasedOn":
      return text(row.purchasedOn);
    case "purchasePrice":
      return text(formatYen(row.purchasePrice));
    case "purchaseStore":
      return text(row.purchaseStore);
    case "ndc":
      return text(row.ndc);
    case "format":
      return text(describeCCode(ctx.cCodes, row.cCode)?.format);
    case "audience":
      return text(describeCCode(ctx.cCodes, row.cCode)?.audience);
    case "pages":
      return row.pages === null ? null : (
        <span className="truncate">{formatInteger(row.pages)}</span>
      );
    case "tags":
      return text(row.tags.join(ja.format.listSeparator));
    case "ownership":
      return text(ja.labels.ownership[row.ownership]);
    case "readFinishedOn":
      return text(row.readFinishedOn);
  }
}
