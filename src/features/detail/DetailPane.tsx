// 右の詳細ペイン（FR-LIST-06）。選んだ本の大きな書影と、全項目を出す。
//
// 一覧用の行（ListRow）の項目は、選んですぐに出す。感想・メモ・内容紹介・取得元など ListRow にない項目は、
// `get_book_detail` の結果（detail）が届いたら加える（MainScreen が取得して渡す）。
import type { ReactNode } from "react";
import type { PriceSettings } from "@/bindings/PriceSettings";
import { BookCover } from "@/components/BookCover";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { RatingStars } from "@/features/list/cells";
import { authorsText } from "@/features/list/cellText";
import { DEFAULT_PRICE_SETTINGS, formatListPrice } from "@/features/library/price";
import { ja } from "@/i18n/ja";
import { cCodeFullLabel } from "@/lib/cCode";
import { formatDateOnly, formatInteger, formatYen } from "@/lib/format";
import type { BookDetail, CCodeTable, ListRow } from "@/types/contracts";

interface DetailPaneProps {
  /** 詳細を出す本（focus している行）。選択がなければ null。 */
  row: ListRow | null;
  /** `get_book_detail` の結果。まだ届いていない・読めなかったときは null。 */
  detail?: BookDetail | null;
  /** 選択している冊数。2 冊以上のときは、詳細ではなく件数と案内を出す。 */
  selectedCount: number;
  cCodes: CCodeTable;
  locationPaths: ReadonlyMap<string, string>;
  /** 価格の表示設定（税込を併記するか）。 */
  price?: PriceSettings;
  onEdit: (id: string) => void;
  onRefetch: (id: string) => void;
  /** 欲しい本を購入済みにする（欲しい本のときだけボタンを出す）。 */
  onMarkPurchased?: (id: string) => void;
}

export function DetailPane({
  row,
  detail = null,
  selectedCount,
  cCodes,
  locationPaths,
  price = DEFAULT_PRICE_SETTINGS,
  onEdit,
  onRefetch,
  onMarkPurchased,
}: DetailPaneProps) {
  if (selectedCount > 1) {
    return (
      <div aria-label={ja.detail.ariaLabel} className="flex h-full flex-col gap-2 p-6 text-sm">
        <p className="text-base font-semibold">{ja.detail.multiple(selectedCount)}</p>
        <p className="text-muted-foreground">{ja.detail.multipleHint}</p>
      </div>
    );
  }
  if (!row) {
    return (
      <div aria-label={ja.detail.ariaLabel} className="flex h-full items-center justify-center p-6">
        <p className="text-center text-sm text-muted-foreground">{ja.detail.empty}</p>
      </div>
    );
  }

  // 詳細が別の本のものなら使わない（選び直した直後に、前の本の項目を出さない）
  const extra = detail !== null && detail.id === row.id ? detail : null;
  const authors = authorsText(row);
  const location = row.locationId === null ? null : (locationPaths.get(row.locationId) ?? null);
  const manualFields = extra?.manualFields.map((op) => ja.detail.fieldNames[op] ?? op) ?? [];

  return (
    <div aria-label={ja.detail.ariaLabel} className="flex h-full min-h-0 flex-col">
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-4 p-4">
          <div className="flex justify-center">
            <BookCover
              id={row.id}
              title={row.title}
              authors={authors}
              coverHash={row.coverHash}
              variant="lg"
            />
          </div>

          <div className="flex flex-col gap-1">
            <h2 className="text-base leading-snug font-semibold break-words">{row.title}</h2>
            {row.subtitle && <p className="text-sm text-muted-foreground">{row.subtitle}</p>}
            {authors && <p className="text-sm break-words">{authors}</p>}
          </div>

          <Section title={ja.detail.sections.basic}>
            <Field label={ja.detail.fields.titleKana} value={row.titleKana} />
            <Field label={ja.detail.fields.series} value={row.seriesName} />
            <Field label={ja.detail.fields.volume} value={row.volume} />
            <Field label={ja.detail.fields.publisher} value={row.publisher} />
            <Field label={ja.detail.fields.publishedOn} value={row.publishedOn} />
            <Field label={ja.detail.fields.isbn13} value={row.isbn13} mono />
            <Field
              label={ja.detail.fields.cCode}
              value={cCodeFullLabel(cCodes, row.cCode) ?? row.cCode}
            />
            <Field label={ja.detail.fields.ndc} value={row.ndc} />
            <Field
              label={ja.detail.fields.listPrice}
              value={formatListPrice(row.listPrice, price) || null}
            />
            {/* 参考税込価格（楽天など）は一覧の行にないので、詳細が届いたら出す */}
            {extra && (
              <Field
                label={ja.detail.fields.refPriceIncl}
                value={formatYen(extra.refPriceIncl) || null}
              />
            )}
            <Field
              label={ja.detail.fields.pages}
              value={row.pages === null ? null : formatInteger(row.pages)}
            />
            {extra && (
              <>
                <Field label={ja.detail.fields.sizeText} value={extra.sizeText} />
                <Field label={ja.detail.fields.language} value={extra.language} />
                <Field label={ja.detail.fields.description} value={extra.description} multiline />
              </>
            )}
          </Section>

          <Section title={ja.detail.sections.classification}>
            <Field label={ja.detail.fields.ownership} value={ja.labels.ownership[row.ownership]} />
            {row.ownership === "wanted" && (
              <Field
                label={ja.detail.fields.wishPriority}
                value={row.wishPriority === null ? null : ja.labels.wishPriority[row.wishPriority]}
              />
            )}
            <Field label={ja.detail.fields.location} value={location} />
            <Field
              label={ja.detail.fields.tags}
              value={row.tags.length > 0 ? row.tags.join(ja.format.listSeparator) : null}
            />
          </Section>

          <Section title={ja.detail.sections.reading}>
            <Field
              label={ja.detail.fields.readStatus}
              value={ja.labels.readStatus[row.readStatus]}
            />
            {extra && <Field label={ja.detail.fields.readStartedOn} value={extra.readStartedOn} />}
            <Field label={ja.detail.fields.readFinishedOn} value={row.readFinishedOn} />
            <Field
              label={ja.detail.fields.rating}
              value={row.rating === null ? null : <RatingStars rating={row.rating} />}
            />
            {extra && <Field label={ja.detail.fields.review} value={extra.review} multiline />}
          </Section>

          <Section title={ja.detail.sections.purchase}>
            <Field label={ja.detail.fields.purchasedOn} value={row.purchasedOn} />
            <Field
              label={ja.detail.fields.purchasePrice}
              value={formatYen(row.purchasePrice) || null}
            />
            <Field label={ja.detail.fields.purchaseStore} value={row.purchaseStore} />
            {extra && (
              <>
                <Field
                  label={ja.detail.fields.purchaseCond}
                  value={
                    extra.purchaseCond === null ? null : ja.detail.purchaseCond[extra.purchaseCond]
                  }
                />
                <Field label={ja.detail.fields.purchaseMemo} value={extra.purchaseMemo} multiline />
              </>
            )}
          </Section>

          {extra && (
            <Section title={ja.detail.fields.memo}>
              <Field label={ja.detail.fields.memo} value={extra.memo} multiline />
            </Section>
          )}

          <Section title={ja.detail.sections.meta}>
            <Field
              label={ja.detail.fields.metaStatus}
              value={ja.labels.metaStatus[row.metaStatus]}
            />
            {extra && (
              <Field
                label={ja.detail.fields.manualFields}
                value={manualFields.length > 0 ? manualFields.join(ja.format.listSeparator) : null}
              />
            )}
            <Field label={ja.detail.fields.createdAt} value={formatDateOnly(row.createdAt)} />
          </Section>
        </div>
      </ScrollArea>

      <Separator />
      <div className="flex flex-wrap gap-2 p-3">
        <Button size="sm" onClick={() => onEdit(row.id)}>
          {ja.detail.edit}
        </Button>
        <Button size="sm" variant="outline" onClick={() => onRefetch(row.id)}>
          {ja.detail.refetch}
        </Button>
        {row.ownership === "wanted" && onMarkPurchased && (
          <Button size="sm" variant="outline" onClick={() => onMarkPurchased(row.id)}>
            {ja.detail.markPurchased}
          </Button>
        )}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="text-xs font-semibold text-muted-foreground">{title}</h3>
      <dl className="grid grid-cols-[6.5rem_1fr] gap-x-2 gap-y-1 text-sm">{children}</dl>
    </section>
  );
}

function Field({
  label,
  value,
  mono = false,
  multiline = false,
}: {
  label: string;
  value: ReactNode;
  mono?: boolean;
  multiline?: boolean;
}) {
  const empty = value === null || value === undefined || value === "";
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd
        className={
          mono
            ? "font-mono text-xs break-all"
            : multiline
              ? "break-words whitespace-pre-wrap"
              : "break-words"
        }
      >
        {empty ? <span className="text-muted-foreground">{ja.detail.noValue}</span> : value}
      </dd>
    </>
  );
}
