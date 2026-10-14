// 価格（FR-CONF-05）。本体価格（税抜）を基本に、税込価格を併記するか、税率はいくつかを選ぶ。
import { Checkbox } from "@/components/ui/checkbox";
import { clampInteger } from "@/features/settings/clamp";
import { Group, NumberRow, Row } from "@/features/settings/fields";
import { useEditablePriceSettings } from "@/features/settings/hooks";
import { ja } from "@/i18n/ja";
import { inTauri } from "@/lib/runtime";

const clampTaxRate = clampInteger(0, 100, 10);

export function PriceSection() {
  const { value, isLoading, isError, save, saving } = useEditablePriceSettings();
  const p = ja.settings.price;

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{p.description}</p>
      {!inTauri() ? (
        <p className="text-sm">{ja.settings.browserNotice}</p>
      ) : isLoading ? (
        <p className="text-sm text-muted-foreground">{ja.settings.loading}</p>
      ) : isError || value === undefined ? (
        <p className="text-sm text-destructive">{ja.settings.loadFailed}</p>
      ) : (
        <Group title={ja.settings.sections.price}>
          <Row label={p.showTaxIncluded}>
            <Checkbox
              checked={value.showTaxIncluded}
              disabled={saving}
              onCheckedChange={(checked) => save({ ...value, showTaxIncluded: checked === true })}
              aria-label={p.showTaxIncluded}
            />
          </Row>
          <NumberRow
            label={p.taxRatePercent}
            value={value.taxRatePercent}
            clamp={clampTaxRate}
            onCommit={(taxRatePercent) => save({ ...value, taxRatePercent })}
          />
        </Group>
      )}
    </div>
  );
}
