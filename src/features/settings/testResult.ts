// 取得元の接続テストの結果を、画面に出す文言と色にする（FR-CONF-02）。
// 利用者が知りたいのは「通信できたか、API キーは有効か」。そのため、応答は返ってきたが
// テスト用の本が取得元になかっただけのときは、失敗ではなく成功として出す。
import type { ProviderTestResult } from "@/bindings/ProviderTestResult";
import { ja } from "@/i18n/ja";

/** 結果の色。success: 通信できた、warning: 通信はできたが使えない（上限）、error: 使えない。 */
export type TestTone = "success" | "warning" | "error";

export interface TestDescription {
  text: string;
  tone: TestTone;
}

/**
 * @param usesKey API キーを使う取得元か（「API キーは有効です」と言えるのは、キーを送って 200 が返ったとき）
 */
export function describeTestResult(result: ProviderTestResult, usesKey: boolean): TestDescription {
  const t = ja.settings.providers.test;
  switch (result.verdict) {
    case "found":
      return { text: t.found(result.title, result.elapsedMs), tone: "success" };
    case "notFound":
      return { text: usesKey ? t.notFoundKeyValid : t.notFound, tone: "success" };
    case "invalidKey":
      return { text: t.invalidKey, tone: "error" };
    case "quotaExceeded":
      return { text: t.quotaExceeded, tone: "warning" };
    case "quotaZero":
      return { text: t.quotaZero, tone: "error" };
    case "networkError":
      return { text: t.networkError, tone: "error" };
    case "httpError":
      return {
        text:
          result.outcome.kind === "httpError"
            ? t.httpError(result.outcome.status, result.outcome.message)
            : t.httpError(0, ""),
        tone: "error",
      };
    case "parseError":
      return {
        text: t.parseError(result.outcome.kind === "parseError" ? result.outcome.message : ""),
        tone: "error",
      };
  }
}
