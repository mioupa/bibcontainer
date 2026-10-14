// 直前に読んだ本（書影・書名・Cコードと名称・本体価格）と、結果（成功・重複・エラー）を色で示すカード（9.3）。
import { CircleCheckIcon, CircleXIcon, CopyIcon, InfoIcon, TriangleAlertIcon } from "lucide-react";
import type { ReactElement } from "react";
import { BookCover } from "@/components/BookCover";
import { Button } from "@/components/ui/button";
import { useCCodeTable } from "@/features/library/hooks";
import type { MessageLevel } from "@/features/scan/pairing";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import { cCodeFullLabel } from "@/lib/cCode";
import { formatYen } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Author } from "@/types/contracts";

const LEVEL_STYLE: Record<MessageLevel, string> = {
  success: "border-success bg-success/15",
  warning: "border-warning bg-warning/15",
  error: "border-destructive bg-destructive/15",
  info: "border-info bg-info/15",
};

const LEVEL_ICON: Record<MessageLevel, ReactElement> = {
  success: <CircleCheckIcon className="text-success" aria-hidden />,
  warning: <TriangleAlertIcon className="text-warning" aria-hidden />,
  error: <CircleXIcon className="text-destructive" aria-hidden />,
  info: <InfoIcon className="text-info" aria-hidden />,
};

function authorNames(authors: readonly Author[] | undefined): string {
  return (authors ?? []).map((author) => author.name).join("、");
}

interface LastReadCardProps {
  /** 所有済みの本を、2 冊目として登録する（FR-BOOK-10）。 */
  onRegisterSecond: () => void;
}

export function LastReadCard({ onRegisterSecond }: LastReadCardProps) {
  const last = useScanSessionStore((s) => s.last);
  const result = useScanSessionStore((s) => s.result);
  const outcome = useScanSessionStore((s) => (last?.isbn13 ? s.lookups[last.isbn13] : undefined));
  const cCodes = useCCodeTable();

  const draft = outcome?.status === "done" ? outcome.result.draft.fields : null;
  const title = draft?.title?.value ?? null;
  const authors = authorNames(draft?.authors?.value);
  const coverHash = draft?.cover_hash?.value ?? null;
  // 2段目は、Cコードと本体価格で最優先（読んでいれば、取得結果の値より先に使う）
  const cCode = last?.secondTier?.cCode ?? draft?.c_code?.value ?? null;
  const price = last?.secondTier?.listPrice ?? draft?.list_price?.value ?? null;
  const loading = last?.isbn13 != null && outcome === undefined;
  const cCodeLabel = cCodeFullLabel(cCodes, cCode) ?? cCode;

  return (
    <section
      aria-label={ja.scan.last.heading}
      className="flex flex-col gap-4 rounded-xl border bg-card p-4"
    >
      <h2 className="text-sm font-semibold text-muted-foreground">{ja.scan.last.heading}</h2>

      {last === null ? (
        <p className="py-6 text-center text-sm text-muted-foreground">{ja.scan.last.empty}</p>
      ) : (
        <div className="flex gap-4">
          <div className="w-28 shrink-0">
            <BookCover
              id={last.isbn13 ?? "second-tier"}
              title={title ?? last.isbn13 ?? ""}
              authors={authors}
              coverHash={coverHash}
              variant="lg"
            />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-lg leading-snug font-semibold break-words">
              {loading ? ja.scan.last.fetching : (title ?? ja.scan.last.noTitle)}
            </p>
            {authors !== "" && <p className="text-sm text-muted-foreground">{authors}</p>}
            <dl className="mt-3 grid grid-cols-[5rem_1fr] gap-x-2 gap-y-1 text-sm">
              <dt className="text-muted-foreground">{ja.scan.last.isbn}</dt>
              <dd className="tabular-nums">{last.isbn13 ?? "—"}</dd>
              <dt className="text-muted-foreground">{ja.scan.last.cCode}</dt>
              <dd>{cCodeLabel ?? ja.scan.last.noSecondTier}</dd>
              <dt className="text-muted-foreground">{ja.scan.last.price}</dt>
              <dd className="tabular-nums">
                {price !== null
                  ? formatYen(price)
                  : last.secondTier !== null
                    ? ja.scan.last.priceUnknown
                    : "—"}
              </dd>
            </dl>
          </div>
        </div>
      )}

      {result !== null && (
        <div
          aria-live="polite"
          data-level={result.level}
          className={cn(
            "flex items-start gap-3 rounded-lg border-l-4 px-3 py-2.5 text-sm",
            LEVEL_STYLE[result.level],
          )}
        >
          <span className="mt-0.5 shrink-0 [&_svg]:size-5">{LEVEL_ICON[result.level]}</span>
          <div className="min-w-0 flex-1">
            <p className="font-medium break-words">{result.text}</p>
            {result.duplicate !== undefined && (
              <Button variant="outline" size="sm" className="mt-2" onClick={onRegisterSecond}>
                <CopyIcon />
                {ja.scan.result.registerSecond}
              </Button>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
