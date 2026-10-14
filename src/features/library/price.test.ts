import { describe, expect, it } from "vitest";
import {
  DEFAULT_PRICE_SETTINGS,
  formatListPrice,
  taxIncludedPrice,
} from "@/features/library/price";

describe("価格の表示（FR-CONF-05）", () => {
  it("税込価格は、円未満を切り捨てる（浮動小数点の誤差で 1 円ずれない）", () => {
    expect(taxIncludedPrice(700, 10)).toBe(770);
    expect(taxIncludedPrice(1300, 10)).toBe(1430);
    expect(taxIncludedPrice(1234, 10)).toBe(1357);
    expect(taxIncludedPrice(1000, 8)).toBe(1080);
    expect(taxIncludedPrice(1000, 0)).toBe(1000);
    // 700 × 1.1 は 770.0000000000001 になる
    expect(taxIncludedPrice(700, 10)).not.toBe(771);
  });

  it("既定は本体価格（税抜）だけ。設定で、税込を併記する", () => {
    expect(formatListPrice(1300, DEFAULT_PRICE_SETTINGS)).toBe("1,300円");
    expect(formatListPrice(1300, { showTaxIncluded: true, taxRatePercent: 10 })).toBe(
      "1,300円（税込 1,430円）",
    );
    expect(formatListPrice(1300, { showTaxIncluded: true, taxRatePercent: 8 })).toBe(
      "1,300円（税込 1,404円）",
    );
  });

  it("価格がなければ空文字", () => {
    expect(formatListPrice(null, { showTaxIncluded: true, taxRatePercent: 10 })).toBe("");
  });
});
