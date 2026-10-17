// 一覧（表・グリッド）が共通で使う、操作の型と props。
import type { PriceSettings } from "@/bindings/PriceSettings";
import type { GroupedList } from "@/features/library/grouping";
import type { BulkChange } from "@/features/list/bulk";
import type { CCodeTable, ListRow } from "@/types/contracts";

/**
 * 一覧から起こせる操作（右クリックメニューとキーボード）。実際の処理は呼び出し側が行う。
 * location・addTags・removeTags は、値を選ぶダイアログを開く操作。markPurchased は、欲しい本を所有にする
 * （`ids` には欲しい本だけが入る）。
 */
export type ListAction =
  | "edit"
  | "refetch"
  | "export"
  | "trash"
  | "restore"
  | "purge"
  | "location"
  | "addTags"
  | "removeTags"
  | "markPurchased";

export type { BulkChange };

/** 表（`BookTable`）とグリッド（`BookGrid`）が受け取る props。2 つは同じ props で差し替えられる。 */
export interface ListViewProps {
  /** 表示する行（絞り込み・並べ替え済み）。 */
  rows: ListRow[];
  /**
   * 巻をまとめた並び（FR-LIST-08。`useGroupedList(rows)`）。ないときは、`rows` をそのまま 1 冊ずつ並べる。
   * あるときは、表示・選択はこちらの `entries` が基準（グループの行を含む）で、`rows` は使わない。
   */
  grouped?: GroupedList;
  cCodes: CCodeTable;
  locationPaths: ReadonlyMap<string, string>;
  /** 価格の表示設定（税込を併記するか）。 */
  price?: PriceSettings;
  isLoading: boolean;
  /** Enter・ダブルクリック・メニューの「編集」で、編集を開く。 */
  onOpenEdit: (id: string) => void;
  /** メニューやキーボードからの操作。ids は対象の本の ID（グループを選んでいれば、その全巻）。 */
  onAction: (action: Exclude<ListAction, "edit">, ids: string[]) => void;
  /** メニューの「一括変更」から選んだ変更（読書状況・所有状態・評価）。ids は対象の本の ID。 */
  onBulkChange: (change: BulkChange, ids: string[]) => void;
  /** ゴミ箱を開いているか。そうなら、メニューは「ゴミ箱へ移動」の代わりに「戻す」「完全に削除」を出す。 */
  inTrash?: boolean;
}
