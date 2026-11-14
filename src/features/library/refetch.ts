// 書誌の再取得（FR-META-13、0027）の状態を、利用者に見せる文言にする。
import type { RefetchJobStatus } from "@/bindings/RefetchJobStatus";
import type { RefetchStopReason } from "@/bindings/RefetchStopReason";
import { ja } from "@/i18n/ja";

export type RefetchNotice = {
  level: "success" | "info" | "error";
  message: string;
  /** true なら、閉じるまで残す（止まったとき）。 */
  persistent: boolean;
};

/** 取得元の表示名。知らない ID は、そのまま出す。 */
export function providerName(id: string): string {
  return ja.toast.refetch.providerNames[id] ?? id;
}

/** 取得元が 1 日の上限を返したか（429・403 で、メッセージに「per day」「daily limit」がある）。 */
function isDailyLimit(status: number, message: string): boolean {
  if (status !== 429 && status !== 403) return false;
  const text = message.toLowerCase();
  return text.includes("per day") || text.includes("daily limit");
}

/** 止まった理由の文言。 */
export function describeStopReason(reason: RefetchStopReason): string {
  const text = ja.toast.refetch.stopReason;
  switch (reason.kind) {
    case "limited": {
      const name = providerName(reason.provider);
      return isDailyLimit(reason.status, reason.message)
        ? text.daily(name)
        : text.limited(name, reason.status);
    }
    case "offline":
      return text.offline;
    case "error":
      return reason.message;
    case "noActiveProviders":
      return ja.toast.refetch.noProviders;
    case "cancelled":
      return "";
  }
}

function summaryOf(job: RefetchJobStatus): string {
  return ja.toast.refetch.result({
    updated: job.updated,
    unchanged: job.unchanged,
    offline: job.offline,
    skipped: job.skipped,
    failed: job.failed,
    keptManualFields: job.keptManualFields,
  });
}

/** 終わった作業の通知。まだ動いている作業には null。 */
export function describeRefetchJob(job: RefetchJobStatus): RefetchNotice | null {
  switch (job.state) {
    case "running":
      return null;
    case "finished":
      return {
        level: job.failed > 0 ? "error" : "success",
        message: summaryOf(job),
        persistent: false,
      };
    case "cancelled":
      return {
        level: "info",
        message: `${ja.toast.refetch.cancelled(job.done, job.remaining)}。${summaryOf(job)}`,
        persistent: false,
      };
    case "stopped": {
      const reason = job.stopReason;
      if (reason?.kind === "noActiveProviders") {
        return { level: "error", message: ja.toast.refetch.noProviders, persistent: true };
      }
      const head = reason === null ? "" : `${describeStopReason(reason)}。`;
      return {
        level: "error",
        message: `${head}${ja.toast.refetch.stoppedTail(job.done, job.remaining)}`,
        persistent: true,
      };
    }
  }
}

/** ステータスバーの「残り…」。それまでの速さ（`elapsedMs / done`）から出す。 */
export function describeRemaining(job: RefetchJobStatus): string {
  const text = ja.statusBar.refetch;
  if (job.done === 0) return text.calculating;
  const remainingMs = (job.elapsedMs / job.done) * job.remaining;
  if (remainingMs < 60_000) return text.underMinute;
  return text.remaining(Math.ceil(remainingMs / 60_000));
}
