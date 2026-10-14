// 他の取得元の値（FR-META-05）。`fetch_metadata` の統合結果（`MetaMerged`）から、フォームの項目ごとの候補を作る。
//
// `MetaFieldChoice` は、採用した値（`chosen`）と、他の取得元の値（`others`。採用値と同じものは除いてある）を持つ。
// 候補の一覧は「採用した値、他の取得元の値…」の順。書影（cover）の候補は URL の一覧で、別に扱う。
import type { CandidateAuthor } from "@/bindings/CandidateAuthor";
import type { MetaMerged } from "@/bindings/MetaMerged";
import type { MetaPick } from "@/bindings/MetaPick";
import type { MetaSource } from "@/bindings/MetaSource";
import type { MetaValue } from "@/bindings/MetaValue";
import {
  emptyFormValues,
  fieldDef,
  opValue,
  sameOpValue,
  type AuthorRow,
  type FormKey,
} from "@/features/editor/formValues";
import { ja } from "@/i18n/ja";

export interface FieldCandidate {
  source: MetaSource;
  /** フォームの項目に入れる値。 */
  formValue: string | AuthorRow[];
  /** 選んだ時点の Op の値（いまの値と同じか比べるのに使う）。 */
  opValue: unknown;
  /** メニューに出す表示。長い文は切る。 */
  label: string;
}

export interface CoverCandidate {
  source: MetaSource;
  url: string;
}

const LABEL_LIMIT = 48;

function ellipsis(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  const chars = Array.from(flat);
  return chars.length > LABEL_LIMIT ? `${chars.slice(0, LABEL_LIMIT).join("")}…` : flat;
}

function authorLabel(author: CandidateAuthor): string {
  return author.role ? `${author.name}（${author.role}）` : author.name;
}

function toCandidate(key: FormKey, pick: MetaPick): FieldCandidate | null {
  const value: MetaValue = pick.value;
  let formValue: string | AuthorRow[];
  let label: string;
  switch (value.type) {
    case "text":
      formValue = value.value;
      label = ellipsis(value.value);
      break;
    case "number":
      formValue = String(value.value);
      label = ellipsis(formValue);
      break;
    case "authors":
      formValue = value.value.map((a) => ({
        name: a.name,
        kana: a.kana ?? "",
        role: a.role ?? "",
      }));
      label = ellipsis(value.value.map(authorLabel).join(ja.format.listSeparator));
      break;
    case "covers":
      return null;
  }
  const values = emptyFormValues();
  (values as unknown as Record<string, unknown>)[key] = formValue;
  return { source: pick.source, formValue, opValue: opValue(key, values), label };
}

/** フォームの項目の候補（採用した値、他の取得元の値…）。候補がなければ空。同じ値は 1 つにまとめる。 */
export function candidatesFor(merged: MetaMerged | null, key: FormKey): FieldCandidate[] {
  const meta = fieldDef(key).meta;
  if (!merged || meta === undefined || meta === "cover") return [];
  const choice = merged.fields.find((field) => field.field === meta);
  if (!choice) return [];
  const picks = [...(choice.chosen ? [choice.chosen] : []), ...choice.others];
  const result: FieldCandidate[] = [];
  for (const pick of picks) {
    const candidate = toCandidate(key, pick);
    if (candidate === null || candidate.opValue === null) continue;
    if (result.some((other) => sameOpValue(other.opValue, candidate.opValue))) continue;
    result.push(candidate);
  }
  return result;
}

/** 書影の候補（取得元ごとの URL）。採用した書影は、すでに保存してあるので含めない。 */
export function coverCandidates(merged: MetaMerged | null): CoverCandidate[] {
  const choice = merged?.fields.find((field) => field.field === "cover");
  if (!choice) return [];
  return choice.others.flatMap((pick) =>
    pick.value.type === "covers"
      ? pick.value.value.map((cover) => ({ source: pick.source, url: cover.url }))
      : [],
  );
}

/** メニューに候補を出すか。候補が 2 つ以上あるか、候補が 1 つでも、いまの値と違うとき（戻せるように）。 */
export function showCandidates(candidates: readonly FieldCandidate[], currentOpValue: unknown) {
  if (candidates.length === 0) return false;
  if (candidates.length >= 2) return true;
  return !sameOpValue(candidates[0]?.opValue, currentOpValue);
}
