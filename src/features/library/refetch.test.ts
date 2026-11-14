import { describe, expect, it } from "vitest";
import {
  describeRefetchJob,
  describeRemaining,
  describeStopReason,
} from "@/features/library/refetch";
import { ja } from "@/i18n/ja";
import { makeJob } from "@/test/refetchJob";

describe("再取得の通知の文言（0027 §6）", () => {
  it("動いている間は通知しない", () => {
    expect(describeRefetchJob(makeJob())).toBeNull();
  });

  it("終わったら、要約を出す。失敗があれば error", () => {
    const job = makeJob({
      state: "finished",
      updated: 3,
      unchanged: 1,
      keptManualFields: 2,
      done: 4,
    });
    const summary = ja.toast.refetch.result({
      updated: 3,
      unchanged: 1,
      offline: 0,
      skipped: 0,
      failed: 0,
      keptManualFields: 2,
    });
    expect(describeRefetchJob(job)).toEqual({
      level: "success",
      message: summary,
      persistent: false,
    });
    expect(describeRefetchJob({ ...job, failed: 1 })?.level).toBe("error");
  });

  it("やめたら、済んだ冊数・残りと要約を出す", () => {
    const notice = describeRefetchJob(
      makeJob({
        state: "cancelled",
        stopReason: { kind: "cancelled" },
        done: 20,
        remaining: 80,
        updated: 20,
      }),
    );
    expect(notice?.persistent).toBe(false);
    expect(notice?.message).toContain("再取得をやめました。20 冊済み・残り 80 冊");
    expect(notice?.message).toContain("20 冊を更新しました");
  });

  it("止まったら、理由と続きの案内を出し、閉じるまで残す", () => {
    const notice = describeRefetchJob(
      makeJob({
        state: "stopped",
        stopReason: { kind: "offline" },
        done: 30,
        remaining: 70,
      }),
    );
    expect(notice).toEqual({
      level: "error",
      message:
        "通信できなくなりました。30 冊済み・残り 70 冊。もう一度実行すると、残りから進みます",
      persistent: true,
    });
  });

  it("使える取得元がなければ、その案内を出す", () => {
    const notice = describeRefetchJob(
      makeJob({ state: "stopped", stopReason: { kind: "noActiveProviders" } }),
    );
    expect(notice?.message).toBe(ja.toast.refetch.noProviders);
    expect(notice?.level).toBe("error");
  });

  describe("止まった理由", () => {
    const limited = (provider: string, status: number, message: string) =>
      describeStopReason({ kind: "limited", provider, status, message });

    it("取得元の名前と HTTP の番号を出す。知らない取得元は ID のまま", () => {
      expect(limited("openlibrary", 429, "Too Many Requests")).toBe(
        "Open Library が制限を返しました（HTTP 429）",
      );
      expect(limited("ndl", 403, "forbidden")).toBe("NDL が制限を返しました（HTTP 403）");
      expect(limited("openbd", 401, "x")).toBe("openBD が制限を返しました（HTTP 401）");
      expect(limited("foo", 429, "x")).toBe("foo が制限を返しました（HTTP 429）");
    });

    it("429・403 で「per day」「daily limit」を含めば、1 日の上限（大文字小文字は問わない）", () => {
      expect(limited("google", 429, "Quota exceeded: Queries PER DAY")).toBe(
        "Google Books の 1 日の上限に達しました",
      );
      expect(limited("google", 403, "Daily Limit Exceeded")).toBe(
        "Google Books の 1 日の上限に達しました",
      );
      // 401 は、メッセージに含まれていても 1 日の上限とはしない
      expect(limited("google", 401, "per day")).toBe("Google Books が制限を返しました（HTTP 401）");
    });

    it("エラーは、その message", () => {
      expect(describeStopReason({ kind: "error", message: "書き込めません" })).toBe(
        "書き込めません",
      );
    });
  });

  describe("残りの時間", () => {
    it("済んだ冊数が 0 の間は計算中", () => {
      expect(describeRemaining(makeJob({ done: 0 }))).toBe("残りを計算中");
    });
    it("1 分未満", () => {
      // 1 冊 1 秒 × 残り 20 冊 = 20 秒
      expect(
        describeRemaining(makeJob({ done: 10, total: 30, remaining: 20, elapsedMs: 10_000 })),
      ).toBe("残り 1 分未満");
    });
    it("1 分以上は切り上げる", () => {
      // 1 冊 2 秒 × 残り 361 冊 = 722 秒 = 12.03 分 → 13 分
      expect(
        describeRemaining(makeJob({ done: 50, total: 411, remaining: 361, elapsedMs: 100_000 })),
      ).toBe("残り約 13 分");
      expect(
        describeRemaining(makeJob({ done: 37, total: 412, remaining: 375, elapsedMs: 74_000 })),
      ).toBe("残り約 13 分");
      expect(
        describeRemaining(makeJob({ done: 30, total: 330, remaining: 300, elapsedMs: 60_000 })),
      ).toBe("残り約 10 分");
    });
  });
});
