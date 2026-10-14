// フォームの内容から、保存するデータを作る（純粋関数）。
//
// - edit: 初期値から変わった項目だけを `FieldChange` にする（`buildEditPlan`）。変えていない項目は送らない
//   （コアは同じ値の手入力を書かないが、そもそも送らないほうが Op も通知も増えない）。
// - new / draft: 元のデータ（`DraftBase`）にフォームで直した項目を重ねて `NewBookDraft` にする
//   （`buildNewBookDraft`）。直していない項目は、元の由来のまま。
//
// 由来（origin）の規則: 利用者が値を直した項目は手入力（manual）。取得元の候補から選び、そのまま
// 直していない項目は、その取得元（provider／barcode）。読書記録・購入情報などは、由来が何でも値だけが書かれる（0006 §1）。
import type { DraftSeries } from "@/bindings/DraftSeries";
import type { FieldChange } from "@/bindings/FieldChange";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import type { Origin } from "@/bindings/Origin";
import type { DraftBase, FetchState } from "@/features/editor/draftBase";
import { FIELDS, isFieldChanged, opValue, type BookFormValues } from "@/features/editor/formValues";
import { originOfChange, type Picks } from "@/features/editor/sources";

/** 取得状態の項目に付ける由来。コアは記録を付けない項目なので、意味はない（0010 §6 と同じ名前）。 */
const META_STATUS_ORIGIN: Origin = { kind: "provider", name: "bibcontainer" };

export interface SeriesEdit {
  /** 新しいシリーズ名。null はシリーズを外す。 */
  name: string | null;
  origin: Origin;
}

export interface EditPlan {
  /** 変わった項目（`series_id` を除く）。 */
  changes: FieldChange[];
  /** シリーズが変わったとき。保存のとき `find_or_create_series` で ID にして `series_id` を足す。 */
  series: SeriesEdit | null;
  /** 取得元の候補を選んだ項目があるとき true（手修正済みの項目でも、選んだ値で置き換えるため）。 */
  overrideManual: boolean;
}

/** edit: 初期値から変わった項目だけの変更を作る。 */
export function buildEditPlan(
  initial: BookFormValues,
  current: BookFormValues,
  picks: Picks = {},
): EditPlan {
  const changes: FieldChange[] = [];
  let series: SeriesEdit | null = null;
  for (const def of FIELDS) {
    if (!isFieldChanged(def.key, initial, current)) continue;
    const value = opValue(def.key, current);
    const origin = originOfChange(picks[def.key], value);
    if (def.key === "seriesName") {
      series = { name: typeof value === "string" ? value : null, origin };
    } else {
      changes.push({ field: def.op, value, origin });
    }
  }
  const overrideManual =
    changes.some((change) => change.origin.kind !== "manual") ||
    (series !== null && series.origin.kind !== "manual");
  return { changes, series, overrideManual };
}

/**
 * new / draft: 元のデータにフォームの内容を重ねて、`NewBookDraft` を作る。
 *
 * - フォームで直した項目は、直した値と由来（手入力か、選んだ候補の取得元）に置き換える。直していない項目は元のまま。
 * - 巻次を直したときは、元の `volume_sort` を捨てる（コアが新しい巻次から作り直す。古い数値を残さない）。
 * - 取得状態: ISBN がなければ付けない（取得するものがない）。ISBN があり、取得できていない（未取得・オフライン、
 *   または ISBN を直した）ときは `pending`（あとで自動で再取得される。FR-SCAN-16）。取得済みなら下書きのまま。
 */
export function buildNewBookDraft(args: {
  base: DraftBase;
  initial: BookFormValues;
  current: BookFormValues;
  picks?: Picks;
  fetchState: FetchState;
}): NewBookDraft {
  const { base, initial, current, fetchState } = args;
  const picks = args.picks ?? {};
  const fields = new Map(base.fields);
  let seriesName: DraftSeries | null = base.seriesName;

  for (const def of FIELDS) {
    if (!isFieldChanged(def.key, initial, current)) continue;
    const value = opValue(def.key, current);
    const origin = originOfChange(picks[def.key], value);
    if (def.key === "seriesName") {
      seriesName = typeof value === "string" ? { name: value, origin } : null;
      continue;
    }
    fields.set(def.op, { value, origin });
    if (def.key === "volume") fields.delete("volume_sort");
  }

  const isbn = opValue("isbn13", current);
  const isbnChanged = isFieldChanged("isbn13", initial, current);
  if (isbn === null) {
    fields.delete("meta_status");
  } else if (fetchState !== "fetched" || isbnChanged) {
    fields.set("meta_status", { value: "pending", origin: META_STATUS_ORIGIN });
  }

  const changes: FieldChange[] = [...fields].map(([field, entry]) => ({
    field,
    value: entry.value,
    origin: entry.origin,
  }));
  return { changes, seriesName };
}
