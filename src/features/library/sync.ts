// `library-changed`（Rust が、データを変えたあとに出すイベント）を受けて、一覧のキャッシュを更新する（0005 §4）。
//
// イベントは `{ books: string[], others: boolean }`。変わった本の行だけ `get_rows` で取り直して、
// TanStack Query のキャッシュ（全書籍の一覧）に差し込む。`others` が true なら、保管場所・タグ・シリーズも
// 取り直す。検索結果の ID は、本が変わると古くなるので、まとめて取り直す。
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import type { LibraryChanged } from "@/bindings/LibraryChanged";
import { libraryApi } from "@/features/library/api";
import { libraryKeys } from "@/features/library/hooks";
import { onLibraryChanged } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";
import type { ListRow } from "@/types/contracts";

/**
 * 一覧（ID 順＝登録順）に、取り直した行を反映する。
 * - 既にある本は、新しい行に置き換える（位置は変えない）。
 * - 要求したのに返らなかった本（完全削除など）は、一覧から外す。
 * - 一覧にまだない本は、ID 順の位置に足す（ID は UUIDv7 なので、文字列の順が登録順）。
 * 渡された配列は書き換えず、変わらなければ同じ配列を返す。
 */
export function mergeRows(
  current: readonly ListRow[],
  requestedIds: readonly string[],
  fetched: readonly ListRow[],
): readonly ListRow[] {
  const requested = new Set(requestedIds);
  const fetchedById = new Map(fetched.map((row) => [row.id, row]));
  const placed = new Set<string>();
  let changed = false;
  const next: ListRow[] = [];
  for (const row of current) {
    if (!requested.has(row.id)) {
      next.push(row);
      continue;
    }
    const replacement = fetchedById.get(row.id);
    changed = true;
    if (replacement) {
      next.push(replacement);
      placed.add(row.id);
    }
  }
  const added = fetched.filter((row) => requested.has(row.id) && !placed.has(row.id));
  if (added.length > 0) {
    changed = true;
    next.push(...added);
    next.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  }
  return changed ? next : current;
}

/** 変更の通知を、キャッシュに反映する。 */
export async function applyLibraryChange(
  client: QueryClient,
  change: LibraryChanged,
): Promise<void> {
  const refresh: Promise<unknown>[] = [];
  if (change.books.length > 0) {
    const fetched = await libraryApi.getRows(change.books);
    client.setQueryData<readonly ListRow[]>(libraryKeys.books, (current) =>
      current === undefined ? undefined : mergeRows(current, change.books, fetched),
    );
  }
  if (change.others) {
    for (const queryKey of [libraryKeys.locations, libraryKeys.tags, libraryKeys.series]) {
      refresh.push(client.invalidateQueries({ queryKey }));
    }
  }
  if (change.books.length > 0 || change.others) {
    refresh.push(client.invalidateQueries({ queryKey: libraryKeys.searches }));
  }
  if (change.books.length > 0) {
    // 詳細ペイン・フォームの詳細と、購入店の候補（本の変更で件数が変わる）。見ていない詳細は、次に開くとき取り直す
    refresh.push(client.invalidateQueries({ queryKey: libraryKeys.bookDetails }));
    refresh.push(client.invalidateQueries({ queryKey: libraryKeys.purchaseStores }));
  }
  await Promise.all(refresh);
}

/**
 * 一覧・保管場所・タグ・シリーズ・検索結果を、すべて取り直す。
 * バックアップから復元したあとなど、行が丸ごと入れ替わったときに呼ぶ（`library-changed` は変わった行の ID を
 * 持たないため、通知だけでは一覧の全行は取り直されない）。
 */
export function reloadLibrary(client: QueryClient): Promise<void> {
  return client.invalidateQueries();
}

/**
 * `library-changed` を購読して、キャッシュを更新し続ける。画面の最上位で 1 回だけ呼ぶ。
 * Tauri の外（モックで動いているとき）は何もしない。
 * 通知は順に処理する（古い取得結果が、新しい結果を上書きしないように）。
 */
export function useLibraryChangeSync(): void {
  const client = useQueryClient();
  useEffect(() => {
    if (!inTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let queue: Promise<void> = Promise.resolve();
    onLibraryChanged((change) => {
      queue = queue
        .then(() => applyLibraryChange(client, change))
        .catch((error: unknown) => {
          // 取り直しに失敗しても、次の通知で追いつけるよう、購読は続ける
          console.error("library-changed の反映に失敗しました", error);
        });
    })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch((error: unknown) => {
        console.error("library-changed を購読できませんでした", error);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [client]);
}
