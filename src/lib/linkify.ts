// 文章の中の https の URL を見つけて、文字と URL に分ける（クレジット表記の URL をリンクにするため）。

export type TextPart = { kind: "text"; text: string } | { kind: "url"; url: string };

/** URL の終わり: 空白、かっこ、日本語の句読点・かぎかっこ。 */
const URL_PATTERN = /https:\/\/[^\s()（）「」、。，．]+/gu;

/** 文字列を、URL でない部分（text）と https の URL（url）に分ける。つなぐと、元の文字列に戻る。 */
export function splitUrls(source: string): TextPart[] {
  const parts: TextPart[] = [];
  let last = 0;
  for (const match of source.matchAll(URL_PATTERN)) {
    // URL の末尾の「.」「,」などは、文の区切りとして URL に含めない
    const url = match[0].replace(/[.,;:!?]+$/, "");
    const start = match.index;
    if (start > last) parts.push({ kind: "text", text: source.slice(last, start) });
    parts.push({ kind: "url", url });
    last = start + url.length;
  }
  if (last < source.length) parts.push({ kind: "text", text: source.slice(last) });
  return parts;
}
