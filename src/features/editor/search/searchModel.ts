// キーワード検索（書名・著者）の、画面に依存しない部分: 取得元の並び、各取得元の状態の文言、結果の行の文字。
import type { CandidateAuthor } from "@/bindings/CandidateAuthor";
import type { ProviderId } from "@/bindings/ProviderId";
import type { ProviderSearchResult } from "@/bindings/ProviderSearchResult";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { SearchHit } from "@/bindings/SearchHit";
import { ja } from "@/i18n/ja";

/** 検索語に、かな・漢字（ひらがな・カタカナ・CJK の漢字）が含まれるか。 */
export function hasJapaneseScript(text: string): boolean {
  return /[\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}]/u.test(text);
}

/** 和書の検索語のときの、取得元の並び。 */
const JAPANESE_ORDER: readonly ProviderId[] = ["ndl", "google", "openlibrary"];
/** 洋書（かな・漢字を含まない検索語）のときの、取得元の並び。 */
const FOREIGN_ORDER: readonly ProviderId[] = ["google", "openlibrary", "ndl"];

/** 検索語（書名と著者）から決めた、取得元の並び。かな・漢字があれば NDL を先にし、なければ NDL を最後にする。 */
export function sectionOrder(title: string, author: string): readonly ProviderId[] {
  return hasJapaneseScript(`${title}${author}`) ? JAPANESE_ORDER : FOREIGN_ORDER;
}

/**
 * 検索の対象にする取得元: キーワード検索に対応していて、有効なもの（無効な取得元は出さない）を、
 * 検索語に合わせた順に並べる。API キーがなくて使えない取得元（`active` が false）も、理由を出すために含める。
 */
export function searchTargets(
  providers: readonly ProviderStatus[],
  title: string,
  author: string,
): ProviderStatus[] {
  const order = sectionOrder(title, author);
  const rank = (id: ProviderId) => {
    const index = order.indexOf(id);
    return index === -1 ? order.length : index;
  };
  return providers
    .filter((provider) => provider.supportsSearch && provider.enabled)
    .sort((a, b) => rank(a.id) - rank(b.id));
}

/** 取得元 1 つの検索の状態。 */
export type SectionState =
  /** 検索中。 */
  | { kind: "loading" }
  /** API キーがなく、検索していない。 */
  | { kind: "noKey" }
  /** コマンドが失敗した（検索語が不正・取得元が使えない、など）。 */
  | { kind: "error"; message: string }
  /** 答えが届いた（見つからない・通信できないも、ここに入る）。 */
  | { kind: "result"; result: ProviderSearchResult };

/** 見出しの横に出す、状態の文言。 */
export function statusText(state: SectionState): string {
  const text = ja.edit.search;
  switch (state.kind) {
    case "loading":
      return text.searching;
    case "noKey":
      return text.noKey;
    case "error":
      return state.message;
    case "result": {
      const { outcome, total, hits } = state.result;
      switch (outcome.kind) {
        case "found":
          return total !== null && total > hits.length
            ? text.countOfTotal(total, hits.length)
            : text.count(hits.length);
        case "notFound":
          return text.notFound;
        case "networkError":
          return text.networkError;
        case "httpError":
          return text.httpError(outcome.status);
        case "parseError":
          return text.parseError;
      }
    }
  }
}

/** 状態が、失敗（通信・応答・検索条件のエラー）か。画面が目立つ色で出す。 */
export function isFailure(state: SectionState): boolean {
  if (state.kind === "error") return true;
  if (state.kind !== "result") return false;
  const kind = state.result.outcome.kind;
  return kind === "networkError" || kind === "httpError" || kind === "parseError";
}

/** 結果の選択を動かす操作（↑↓・Home・End）。 */
export type Move = "next" | "prev" | "first" | "last";

/**
 * 選択を動かした先の位置（`current` は今の位置。選んでいなければ -1）。端で止まる。
 * 選んでいなければ、next は最初の行、prev は最後の行。行がなければ null。
 */
export function movedIndex(current: number, length: number, move: Move): number | null {
  if (length === 0) return null;
  switch (move) {
    case "first":
      return 0;
    case "last":
      return length - 1;
    case "next":
      return current === -1 ? 0 : Math.min(current + 1, length - 1);
    case "prev":
      return current === -1 ? length - 1 : Math.max(current - 1, 0);
  }
}

/** 結果の行を選ぶための、取得元と並びの位置から作る目印（取得元ごとの結果が増えても、選んだ行を取り違えない）。 */
export const hitKey = (provider: ProviderId, index: number) => `${provider}:${index}`;

/** 結果の行の DOM の id（`aria-activedescendant` が指す）。 */
export const hitDomId = (key: string) => `kw-hit-${key.replace(":", "-")}`;

/** 著者を、役割があれば（）をつけて「、」でつなぐ。 */
export function authorsText(authors: readonly CandidateAuthor[]): string {
  return authors
    .filter((author) => author.name !== "")
    .map((author) => (author.role ? `${author.name}（${author.role}）` : author.name))
    .join("、");
}

/** 出版社と発行日を「 · 」でつなぐ（どちらもなければ空）。 */
export function publicationText(hit: SearchHit): string {
  return [hit.candidate.publisher, hit.candidate.publishedOn]
    .filter((part): part is string => part !== null && part !== "")
    .join(" · ");
}

/** 取得元のサイトの、その本のページへのリンクの文言（`link` があるときだけ出す）。 */
export function viewOnLabel(provider: ProviderId): string {
  return ja.edit.search.viewOn[provider];
}
