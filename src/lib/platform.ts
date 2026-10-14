// OS の判定。ショートカットの表記（⌘ と Ctrl）と、書影 URL の形式（macOS と Windows）の切り替えに使う。

export type Platform = "mac" | "windows";

/** WebView の User-Agent から OS を判定する。Windows 以外（macOS・Linux）は mac として扱う。 */
export function detectPlatform(
  userAgent: string = globalThis.navigator?.userAgent ?? "",
): Platform {
  return /windows/i.test(userAgent) ? "windows" : "mac";
}
