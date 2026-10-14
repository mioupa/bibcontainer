// 書誌の再取得（FR-META-13）の結果を、利用者に見せる文言にする。
import type { RefetchSummary } from "@/bindings/RefetchSummary";
import { ja } from "@/i18n/ja";

export type RefetchNotice = { level: "success" | "info" | "error"; message: string };

/**
 * 再取得の結果の通知。使える取得元がない・通信できなかったときは、その理由を出す。
 * 手で直した項目を残した数（AT-16）も添える。
 */
export function describeRefetch(summary: RefetchSummary): RefetchNotice {
  if (summary.noActiveProviders) return { level: "error", message: ja.toast.refetch.noProviders };
  const processed = summary.updated + summary.unchanged;
  if (summary.offline > 0 && processed === 0 && summary.failed === 0) {
    return { level: "info", message: ja.toast.refetch.offline };
  }
  const keptManualFields = summary.items.reduce((sum, item) => sum + item.skippedManual.length, 0);
  const message = ja.toast.refetch.result({
    updated: summary.updated,
    unchanged: summary.unchanged,
    offline: summary.offline,
    skipped: summary.skipped,
    failed: summary.failed,
    keptManualFields,
  });
  return { level: summary.failed > 0 ? "error" : "success", message };
}
