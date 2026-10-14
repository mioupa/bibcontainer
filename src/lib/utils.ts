import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** クラス名を条件つきで連結し、Tailwind のクラスの衝突を後勝ちで解消する。 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
