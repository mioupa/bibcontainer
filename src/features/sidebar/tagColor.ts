// タグの色。Catppuccin のアクセント色の名前（`rosewater`〜`lavender`）を保存し、表示は `var(--ctp-<名前>)`
// （ライト・ダークで自動的に切り替わる）。知らない名前（将来の別の形式など）は、色なしとして扱う。
import { COVER_ACCENTS } from "@/lib/coverColor";

/** 保存した色の名前から、CSS の色。知らない名前・色なしは undefined。 */
export function tagColorVar(color: string | null): string | undefined {
  return color !== null && (COVER_ACCENTS as readonly string[]).includes(color)
    ? `var(--ctp-${color})`
    : undefined;
}
