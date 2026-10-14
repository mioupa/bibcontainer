// 書誌取得の下書き（`fetch_metadata` の戻り値の `draft`）と、本の作成・更新に渡す変更（`FieldChange`）の変換。
//
// `BookDraft.fields` は、書籍の Op の項目名（snake_case）→ `{ value, origin }`。取得状態（`meta_status`）も
// 項目の 1 つで、`FieldChange` にするだけで、取得待ちの本が取得待ちのまま保存される（AT-11）。
// 値がない項目はキーごとない。シリーズは名前で持ち（`seriesName`）、保存するときに Rust が `series_id` にする。
import type { BookDraft } from "@/bindings/BookDraft";
import type { FieldChange } from "@/bindings/FieldChange";
import type { NewBookDraft } from "@/bindings/NewBookDraft";

/** 下書きの項目を、`FieldChange` の配列にする（項目の順は下書きのまま。値のない項目は含まれない）。 */
export function draftToChanges(draft: BookDraft): FieldChange[] {
  return Object.entries(draft.fields).flatMap(([field, entry]) =>
    entry === undefined ? [] : [{ field, value: entry.value, origin: entry.origin }],
  );
}

/**
 * 下書きから、`create_books_from_drafts` に渡す 1 冊分を作る。
 *
 * `edits` は、フォームで利用者が直した項目（`origin: { kind: "manual" }` の `FieldChange`）。同じ項目の
 * 下書きの値を置き換える（下書きにない項目は足す）。`seriesName` は下書きのものを渡す。フォームでシリーズを
 * 直したときは、`edits` に `series_id`（既存のシリーズを選んだとき）を入れるか、`seriesName` を `override` で渡す。
 */
export function toNewBookDraft(
  draft: BookDraft,
  edits: readonly FieldChange[] = [],
  override: { seriesName?: NewBookDraft["seriesName"] } = {},
): NewBookDraft {
  const edited = new Set(edits.map((change) => change.field));
  const changes = [
    ...draftToChanges(draft).filter((change) => !edited.has(change.field)),
    ...edits,
  ];
  return {
    changes,
    seriesName: "seriesName" in override ? override.seriesName : draft.seriesName,
  };
}
