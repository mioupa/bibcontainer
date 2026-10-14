// 設定の数値を範囲に収める関数。

/** 整数に収める関数を作る。数でないときは既定値。 */
export function clampInteger(min: number, max: number, fallback: number) {
  return (value: number): number => {
    if (!Number.isFinite(value)) return fallback;
    return Math.min(max, Math.max(min, Math.round(value)));
  };
}
