import { describe, expect, it } from "vitest";
import type { ProviderTestResult } from "@/bindings/ProviderTestResult";
import { describeTestResult } from "@/features/settings/testResult";

function result(overrides: Partial<ProviderTestResult> = {}): ProviderTestResult {
  return {
    provider: "google",
    isbn13: "9784003101117",
    tried: ["9784003101117"],
    outcome: { kind: "found" },
    verdict: "found",
    title: "こころ",
    publisher: "岩波書店",
    cCode: null,
    hasCover: true,
    elapsedMs: 393,
    ...overrides,
  };
}

describe("describeTestResult（接続テストの文言と色）", () => {
  it("書誌が取れた: 書名と時間を添えて成功", () => {
    expect(describeTestResult(result(), true)).toEqual({
      text: "接続できました（こころ を取得、393 ミリ秒）",
      tone: "success",
    });
  });

  it("書誌が取れたが書名がない: 「書誌を取得」", () => {
    expect(describeTestResult(result({ title: null }), true).text).toBe(
      "接続できました（書誌を取得、393 ミリ秒）",
    );
  });

  it("どの本も見つからなかった: 成功。キーを使う取得元だけ「API キーは有効」", () => {
    const notFound = result({ outcome: { kind: "notFound" }, verdict: "notFound", title: null });
    expect(describeTestResult(notFound, true)).toEqual({
      text: "接続できました。API キーは有効です（テスト用の本はこの取得元に見つかりませんでした）",
      tone: "success",
    });
    expect(describeTestResult(notFound, false)).toEqual({
      text: "接続できました（テスト用の本はこの取得元に見つかりませんでした）",
      tone: "success",
    });
  });

  it("キーが無効・API が無効: 失敗の色", () => {
    expect(
      describeTestResult(
        result({
          outcome: { kind: "httpError", status: 403, message: "disabled" },
          verdict: "invalidKey",
        }),
        true,
      ),
    ).toEqual({ text: "API キーが無効か、Books API が有効になっていません", tone: "error" });
  });

  it("429: 1 日の上限は注意の色、割り当て 0 は失敗の色で、文言も別", () => {
    const http429 = { kind: "httpError", status: 429, message: "Queries per day" } as const;
    const daily = describeTestResult(result({ outcome: http429, verdict: "quotaExceeded" }), true);
    const zero = describeTestResult(result({ outcome: http429, verdict: "quotaZero" }), true);
    expect(daily.tone).toBe("warning");
    expect(daily.text).toContain("1 日の利用上限");
    expect(zero.tone).toBe("error");
    expect(zero.text).toContain("割り当てが 0");
    expect(daily.text).not.toBe(zero.text);
  });

  it("通信エラー: ネットワークの確認を促す（技術的なメッセージは出さない）", () => {
    expect(
      describeTestResult(
        result({
          outcome: { kind: "networkError", message: "dns error: failed to lookup" },
          verdict: "networkError",
        }),
        true,
      ),
    ).toEqual({
      text: "接続できませんでした（ネットワークを確認してください）",
      tone: "error",
    });
  });

  it("そのほかの HTTP エラー・解析エラー: ステータスや理由をそのまま出す", () => {
    expect(
      describeTestResult(
        result({
          outcome: { kind: "httpError", status: 503, message: "busy" },
          verdict: "httpError",
        }),
        false,
      ),
    ).toEqual({ text: "HTTP エラー（503）: busy", tone: "error" });
    expect(
      describeTestResult(
        result({ outcome: { kind: "parseError", message: "壊れた JSON" }, verdict: "parseError" }),
        false,
      ),
    ).toEqual({ text: "応答を読み取れませんでした: 壊れた JSON", tone: "error" });
  });
});
