// 同期の状態の表示の規則（9.1、FR-SYNC-08）。状態（SyncStatus）から、上部バーの見た目と詳細の行を決める。
// 画面にも IPC にも触れない純粋な関数（時刻は引数）。
import { formatAhead, formatDateTime, formatSyncTime, latestOf } from "@/features/sync/format";
import type { SyncStatus } from "@/features/sync/types";
import { ja } from "@/i18n/ja";
import { formatInteger } from "@/lib/format";

/** 上部バーの見た目の種類。 */
export type SyncDisplayKind =
  | "loading"
  | "disabled"
  | "syncing"
  | "synced"
  | "pending"
  | "error"
  | "mismatch"
  | "updateRequired";

/** 色の調子。muted は目立たせない、info は動作中、success は完了、warning は注意、danger は止まっている。 */
export type SyncTone = "muted" | "info" | "success" | "warning" | "danger";

export interface SyncDisplay {
  kind: SyncDisplayKind;
  tone: SyncTone;
  /** 上部バーの「同期: 」のあとに出す文字。 */
  text: string;
  /** 他の端末の時計のずれの警告を添えるか。 */
  skew: boolean;
  /** ツールチップの詳細（1 行ずつ）。 */
  details: string[];
}

const TONES: Record<SyncDisplayKind, SyncTone> = {
  loading: "muted",
  disabled: "muted",
  syncing: "info",
  synced: "success",
  pending: "warning",
  error: "danger",
  mismatch: "danger",
  updateRequired: "danger",
};

/** 完了の表示に出す時刻 = 最終の取り込み・書き出しの新しい方。 */
export function lastSyncAt(
  status: Pick<SyncStatus, "lastExportAt" | "lastImportAt">,
): string | null {
  return latestOf(status.lastExportAt, status.lastImportAt);
}

/** 時計のずれの警告文（設定の節の警告と、ツールチップで使う）。ずれがなければ null。 */
export function skewTitle(status: Pick<SyncStatus, "clockSkew">): string | null {
  const skew = status.clockSkew;
  if (skew === null) return null;
  const name = skew.deviceName ?? skew.deviceId;
  return ja.sync.warnings.skew.title(name, formatAhead(skew.aheadMs).text);
}

function detailLines(status: SyncStatus): string[] {
  const t = ja.sync.tooltip;
  const lines: string[] = [ja.sync.states[status.state]];
  if (status.state === "disabled") return lines;
  if (status.folder !== null) lines.push(`${t.folder}: ${status.folder}`);
  lines.push(`${t.lastExport}: ${formatDateTime(status.lastExportAt) ?? t.never}`);
  lines.push(`${t.lastImport}: ${formatDateTime(status.lastImportAt) ?? t.never}`);
  if (status.pendingOps > 0)
    lines.push(`${t.pending}: ${t.count(formatInteger(status.pendingOps))}`);
  if (status.missingCovers > 0) {
    lines.push(`${t.missingCovers}: ${t.count(formatInteger(status.missingCovers))}`);
  }
  if (status.error !== null) lines.push(status.error);
  const skew = skewTitle(status);
  if (skew !== null) lines.push(skew);
  return lines;
}

/**
 * 状態ごとの表示。
 * - 未設定／同期中／エラー／不一致／更新が必要は、状態そのもの。
 * - 待機中（idle）は、未書き出しの変更があれば「未書き出し N 件」、なければ「完了」（最終の取り込み・書き出しの新しい方の時刻）。
 * - 時計のずれは、状態にかかわらず（設定済みなら）添える。
 */
export function describeSync(status: SyncStatus | undefined, now: Date): SyncDisplay {
  const t = ja.sync.indicator;
  if (status === undefined) {
    return { kind: "loading", tone: TONES.loading, text: t.loading, skew: false, details: [] };
  }
  const details = detailLines(status);
  const skew = status.state !== "disabled" && status.clockSkew !== null;
  const make = (kind: SyncDisplayKind, text: string): SyncDisplay => ({
    kind,
    tone: TONES[kind],
    text,
    skew,
    details,
  });

  switch (status.state) {
    case "disabled":
      return make("disabled", t.disabled);
    case "syncing":
      return make("syncing", t.syncing);
    case "error":
      return make("error", t.error);
    case "mismatch":
      return make("mismatch", t.mismatch);
    case "updateRequired":
      return make("updateRequired", t.updateRequired);
    case "idle": {
      if (status.pendingOps > 0) {
        return make("pending", t.pending(formatInteger(status.pendingOps)));
      }
      return make("synced", formatSyncTime(lastSyncAt(status), now) ?? t.waiting);
    }
  }
}
