/** react-hook-form のエラー（項目、配列、入れ子）から、文言を取り出す。 */
export function errorText(error: unknown): string | undefined {
  if (typeof error !== "object" || error === null) return undefined;
  if ("message" in error && typeof error.message === "string" && error.message !== "") {
    return error.message;
  }
  // 配列の項目（著者）は、行ごとのエラーを持つ。最初のものを出す
  for (const child of Object.values(error as Record<string, unknown>)) {
    const text = errorText(child);
    if (text !== undefined) return text;
  }
  return undefined;
}
