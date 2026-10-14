// フォームの保存（IPC を呼ぶ部分）。何を送るかを決める部分は changes.ts の純粋関数。
import type { ChangeReport } from "@/bindings/ChangeReport";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import type { EditPlan } from "@/features/editor/changes";
import { libraryApi } from "@/features/library/api";
import type { FieldChange } from "@/types/contracts";

/** edit の保存に渡す `update_book` の引数。 */
export interface EditRequest {
  changes: FieldChange[];
  options: { overrideManual: boolean } | undefined;
}

/**
 * `EditPlan` を、`update_book` の引数にする。シリーズが変わったときは、名前を `find_or_create_series` で ID にして
 * `series_id` を足す（名前が空ならシリーズを外す）。変更がなければ null。
 */
export async function planToEditRequest(
  plan: EditPlan,
  resolveSeries: (name: string) => Promise<string> = libraryApi.findOrCreateSeries,
): Promise<EditRequest | null> {
  if (plan.changes.length === 0 && plan.series === null) return null;
  const changes = [...plan.changes];
  if (plan.series !== null) {
    const id = plan.series.name === null ? null : await resolveSeries(plan.series.name);
    changes.push({ field: "series_id", value: id, origin: plan.series.origin });
  }
  return { changes, options: plan.overrideManual ? { overrideManual: true } : undefined };
}

/** edit: 変わった項目だけを `update_book` で保存する。変更がなければ null（何も送らない）。 */
export async function saveEdit(bookId: string, plan: EditPlan): Promise<ChangeReport | null> {
  const request = await planToEditRequest(plan);
  if (request === null) return null;
  return libraryApi.updateBook(bookId, request.changes, request.options);
}

/** new: 下書きから本を作る。新しい本の ID を返す。 */
export async function saveNew(draft: NewBookDraft): Promise<string> {
  const [id] = await libraryApi.createBooksFromDrafts([draft]);
  if (id === undefined) throw new Error("本を作れませんでした");
  return id;
}
