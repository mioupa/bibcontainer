import { describe, expect, it } from "vitest";
import { describeSync, lastSyncAt, skewTitle } from "@/features/sync/display";
import {
  formatAhead,
  formatDateTime,
  formatSyncTime,
  latestOf,
  osLabel,
} from "@/features/sync/format";
import { planSetup } from "@/features/sync/setupPlan";
import type { FolderInspection, SyncStatus } from "@/features/sync/types";
import { ja } from "@/i18n/ja";

// ローカル時間で組み立てる（テストを動かす場所のタイムゾーンに左右されないように）
const at = (month: number, day: number, hour: number, minute: number) =>
  new Date(2026, month - 1, day, hour, minute);
const NOW = at(10, 15, 13, 0);

function status(overrides: Partial<SyncStatus> = {}): SyncStatus {
  return {
    state: "idle",
    folder: "/Drive/bibcontainer-library",
    libraryId: "lib-1",
    deviceId: "d-self",
    deviceName: "MacBook Pro",
    lastExportAt: at(10, 15, 12, 4).toISOString(),
    lastImportAt: at(10, 15, 11, 50).toISOString(),
    pendingOps: 0,
    missingCovers: 0,
    error: null,
    clockSkew: null,
    devices: [],
    ...overrides,
  };
}

describe("describeSync: 上部バーの表示（状態ごと）", () => {
  it("取得前は「…」", () => {
    const display = describeSync(undefined, NOW);
    expect(display).toMatchObject({ kind: "loading", text: ja.sync.indicator.loading });
  });

  it("未設定", () => {
    const display = describeSync(status({ state: "disabled", folder: null }), NOW);
    expect(display).toMatchObject({ kind: "disabled", tone: "muted", text: "未設定" });
    // 未設定のときは、詳細も状態の説明だけ
    expect(display.details).toEqual([ja.sync.states.disabled]);
  });

  it("同期中", () => {
    expect(describeSync(status({ state: "syncing" }), NOW)).toMatchObject({
      kind: "syncing",
      tone: "info",
      text: "同期中…",
    });
  });

  it("完了: 最終の取り込み・書き出しの新しい方の時刻（今日は時刻だけ）", () => {
    expect(describeSync(status(), NOW)).toMatchObject({
      kind: "synced",
      tone: "success",
      text: "12:04",
    });
    // 取り込みの方が新しければ、そちら
    const importedLater = status({
      lastExportAt: at(10, 15, 9, 0).toISOString(),
      lastImportAt: at(10, 15, 12, 30).toISOString(),
    });
    expect(describeSync(importedLater, NOW).text).toBe("12:30");
  });

  it("完了: 前の日の時刻は、月/日を添える", () => {
    const old = status({
      lastExportAt: at(10, 14, 23, 5).toISOString(),
      lastImportAt: null,
    });
    expect(describeSync(old, NOW).text).toBe("10/14 23:05");
  });

  it("完了: まだ一度も書き出し・取り込みをしていなければ「待機中」", () => {
    const fresh = status({ lastExportAt: null, lastImportAt: null });
    expect(describeSync(fresh, NOW)).toMatchObject({ kind: "synced", text: "待機中" });
  });

  it("未書き出しあり: 件数を出し、完了の時刻より優先する", () => {
    expect(describeSync(status({ pendingOps: 1234 }), NOW)).toMatchObject({
      kind: "pending",
      tone: "warning",
      text: "未書き出し 1,234 件",
    });
  });

  it("エラー・不一致・更新が必要は、状態そのもの（未書き出しがあっても）", () => {
    expect(
      describeSync(status({ state: "error", error: "失敗", pendingOps: 5 }), NOW),
    ).toMatchObject({
      kind: "error",
      tone: "danger",
      text: "エラー",
    });
    expect(describeSync(status({ state: "mismatch" }), NOW)).toMatchObject({
      kind: "mismatch",
      tone: "danger",
      text: "ライブラリが違います",
    });
    expect(describeSync(status({ state: "updateRequired" }), NOW)).toMatchObject({
      kind: "updateRequired",
      tone: "danger",
      text: "更新が必要",
    });
  });

  it("時計のずれは、設定済みなら添える。未設定なら添えない", () => {
    const skew = { deviceId: "d-win", deviceName: "Windows デスクトップ", aheadMs: 12 * 60_000 };
    expect(describeSync(status({ clockSkew: skew }), NOW).skew).toBe(true);
    expect(describeSync(status({ clockSkew: skew, state: "error" }), NOW).skew).toBe(true);
    expect(describeSync(status({ clockSkew: skew, state: "disabled" }), NOW).skew).toBe(false);
    expect(describeSync(status(), NOW).skew).toBe(false);
  });

  it("詳細: フォルダ、最終書き出し・取り込み、未書き出し、届いていない書影、エラー、時計のずれ", () => {
    const display = describeSync(
      status({
        state: "error",
        error: "書き込めませんでした",
        pendingOps: 12,
        missingCovers: 3,
        clockSkew: { deviceId: "d-win", deviceName: null, aheadMs: 7 * 60_000 },
      }),
      NOW,
    );
    expect(display.details).toEqual([
      ja.sync.states.error,
      "フォルダ: /Drive/bibcontainer-library",
      "最終書き出し: 2026-10-15 12:04",
      "最終取り込み: 2026-10-15 11:50",
      "未書き出し: 12 件",
      "届いていない書影: 3 件",
      "書き込めませんでした",
      "端末 d-win の時計が 7 分進んでいます",
    ]);
  });

  it("詳細: 未書き出し・届いていない書影が 0 なら行を出さない。時刻がなければ「まだありません」", () => {
    const display = describeSync(status({ lastExportAt: null, lastImportAt: null }), NOW);
    expect(display.details).toEqual([
      ja.sync.states.idle,
      "フォルダ: /Drive/bibcontainer-library",
      "最終書き出し: まだありません",
      "最終取り込み: まだありません",
    ]);
  });
});

describe("時刻・時計のずれの整形", () => {
  it("lastSyncAt / latestOf: 新しい方。片方が null ならもう一方。読めなければ無視する", () => {
    const a = at(10, 15, 12, 0).toISOString();
    const b = at(10, 15, 12, 5).toISOString();
    expect(latestOf(a, b)).toBe(b);
    expect(latestOf(b, a)).toBe(b);
    expect(latestOf(null, a)).toBe(a);
    expect(latestOf(a, null)).toBe(a);
    expect(latestOf(null, null)).toBeNull();
    expect(latestOf("壊れた値", a)).toBe(a);
    expect(latestOf("壊れた値", "これも")).toBeNull();
    expect(lastSyncAt({ lastExportAt: a, lastImportAt: b })).toBe(b);
  });

  it("formatSyncTime / formatDateTime: 読めない値は null", () => {
    expect(formatSyncTime("壊れた値", NOW)).toBeNull();
    expect(formatSyncTime(null, NOW)).toBeNull();
    expect(formatDateTime("壊れた値")).toBeNull();
    expect(formatDateTime(at(1, 2, 3, 4).toISOString())).toBe("2026-01-02 03:04");
    // 年が違う日は、月/日 + 時刻（年は出さない）
    expect(formatSyncTime(at(1, 2, 3, 4).toISOString(), NOW)).toBe("1/2 03:04");
  });

  it("formatAhead: 分・時間・日で出す", () => {
    expect(formatAhead(5 * 60_000)).toEqual({ minutes: 5, text: "5 分" });
    expect(formatAhead(5 * 60_000 + 59_000).text).toBe("5 分");
    expect(formatAhead(59 * 60_000).text).toBe("59 分");
    expect(formatAhead(60 * 60_000).text).toBe("1 時間");
    expect(formatAhead(150 * 60_000).text).toBe("2 時間 30 分");
    expect(formatAhead(24 * 60 * 60_000).text).toBe("1 日");
    expect(formatAhead((24 * 60 + 3 * 60) * 60_000).text).toBe("1 日 3 時間");
    expect(formatAhead(-1).text).toBe("0 分");
  });

  it("skewTitle: 「端末 ○○ の時計が N 分進んでいます」。名前がなければ端末 ID", () => {
    expect(skewTitle({ clockSkew: null })).toBeNull();
    expect(
      skewTitle({ clockSkew: { deviceId: "d-1", deviceName: "iMac", aheadMs: 12 * 60_000 } }),
    ).toBe("端末 iMac の時計が 12 分進んでいます");
    expect(
      skewTitle({ clockSkew: { deviceId: "d-1", deviceName: null, aheadMs: 5 * 60_000 } }),
    ).toBe("端末 d-1 の時計が 5 分進んでいます");
  });

  it("osLabel", () => {
    expect(osLabel("macos")).toBe("macOS");
    expect(osLabel("windows")).toBe("Windows");
    expect(osLabel("linux")).toBe("Linux");
    expect(osLabel("haiku")).toBe("haiku");
  });
});

describe("planSetup: フォルダの判定 × 手元のデータ → 設定の手順（FR-SYNC-09、0013 §7）", () => {
  const inspection = (overrides: Partial<FolderInspection>): FolderInspection => ({
    kind: "empty",
    libraryFolder: "/Drive/bibcontainer-library",
    libraryId: null,
    devices: [],
    localHasData: false,
    localLibraryId: null,
    ...overrides,
  });

  it("空のフォルダ: 新しいライブラリを作る（手元のデータの有無によらない）", () => {
    expect(planSetup(inspection({ kind: "empty", localHasData: false }))).toEqual({
      kind: "create",
    });
    expect(planSetup(inspection({ kind: "empty", localHasData: true }))).toEqual({
      kind: "create",
    });
  });

  it("ライブラリあり・手元が空: 参加する", () => {
    expect(planSetup(inspection({ kind: "library", localHasData: false }))).toEqual({
      kind: "join",
    });
  });

  it("ライブラリあり・手元にデータ: 統合か置き換えを選ばせる", () => {
    expect(planSetup(inspection({ kind: "library", localHasData: true }))).toEqual({
      kind: "choose",
    });
  });

  it("ライブラリあり・手元が同じライブラリに参加済み: 場所の付け替え（再接続）。手元のデータの有無によらない", () => {
    for (const localHasData of [false, true]) {
      expect(
        planSetup(
          inspection({
            kind: "library",
            localHasData,
            libraryId: "lib-1",
            localLibraryId: "lib-1",
          }),
        ),
      ).toEqual({ kind: "reconnect" });
    }
  });

  it("ライブラリあり・手元が別のライブラリに参加済み（判定は otherLibrary のはずだが、念のため）: 同期しない", () => {
    expect(
      planSetup(
        inspection({
          kind: "library",
          localHasData: true,
          libraryId: "lib-1",
          localLibraryId: "lib-other",
        }),
      ),
    ).toEqual({ kind: "otherLibrary" });
  });

  it("空のフォルダでも、手元がライブラリに属していれば、新しく作らせない（先に解除）", () => {
    for (const localHasData of [false, true]) {
      expect(
        planSetup(inspection({ kind: "empty", localHasData, localLibraryId: "lib-local" })),
      ).toEqual({ kind: "leaveFirst" });
    }
  });

  it("別のライブラリ・書き込めない・更新が必要: 同期しない（手元のデータの有無によらない）", () => {
    for (const localHasData of [false, true]) {
      expect(planSetup(inspection({ kind: "otherLibrary", localHasData }))).toEqual({
        kind: "otherLibrary",
      });
      expect(planSetup(inspection({ kind: "notWritable", localHasData }))).toEqual({
        kind: "notWritable",
      });
      expect(planSetup(inspection({ kind: "updateRequired", localHasData }))).toEqual({
        kind: "updateRequired",
      });
    }
  });
});
