// 一覧（表・グリッド）に行がないときの表示。読み込み中か、「該当なし」。
import { ja } from "@/i18n/ja";

export function ListEmptyState({ isLoading }: { isLoading: boolean }) {
  return (
    <div className="pointer-events-none absolute inset-x-0 top-24 flex flex-col items-center gap-1 text-sm text-muted-foreground">
      {isLoading ? (
        <p>{ja.list.loading}</p>
      ) : (
        <>
          <p>{ja.list.empty}</p>
          <p className="text-xs">{ja.list.emptyHint}</p>
        </>
      )}
    </div>
  );
}
