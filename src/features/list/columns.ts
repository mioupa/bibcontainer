// 一覧の列の定義（TanStack Table）。列の状態（表示・非表示、並び順、幅、並べ替え）は Table が持ち、
// 行の描画は cells.tsx が行う（行モデルを作らず、ListRow をそのまま仮想スクロールに渡す。BookTable.tsx 参照）。
import {
  columnOrderingFeature,
  columnResizingFeature,
  columnSizingFeature,
  columnVisibilityFeature,
  createColumnHelper,
  rowSortingFeature,
  tableFeatures,
} from "@tanstack/react-table";
import {
  COLUMN_IDS,
  DEFAULT_COLUMN_SIZE,
  FIXED_COLUMN_IDS,
  MAX_COLUMN_SIZE,
  MIN_COLUMN_SIZE,
} from "@/features/list/columnMeta";
import { isSortable } from "@/features/library/sort";
import type { ListRow } from "@/types/contracts";

// 並べ替えは自前の純粋関数（features/library/sort.ts）で行うので、sortedRowModel は登録しない。
// 前提の機能を先に並べる（resizing は sizing が前提）。
export const tableFeaturesDef = tableFeatures({
  rowSortingFeature,
  columnVisibilityFeature,
  columnOrderingFeature,
  columnSizingFeature,
  columnResizingFeature,
});

const helper = createColumnHelper<typeof tableFeaturesDef, ListRow>();

// 列の定義は、モジュールの中で 1 度だけ作る（useTable は columns の参照が変わると、列のモデルを作り直すため）。
export const bookColumns = helper.columns(
  COLUMN_IDS.map((id) =>
    // 並べ替えは Table の外で行うので、accessorFn の値は使わない。Table は accessorFn のある列だけを並べ替え可能とみなす。
    helper.accessor(() => null, {
      id,
      size: DEFAULT_COLUMN_SIZE[id],
      minSize: MIN_COLUMN_SIZE,
      maxSize: MAX_COLUMN_SIZE,
      enableSorting: isSortable(id),
      enableHiding: !FIXED_COLUMN_IDS.includes(id),
      enableResizing: id !== "cover",
      // 最初のクリックは常に昇順（降順始まりにすると、Table が先頭行の値の型を調べにいって行モデルを作ってしまう）
      sortDescFirst: false,
    }),
  ),
);
