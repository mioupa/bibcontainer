// 初めての設定（FR-SYNC-09、0013 §7）の手順の分岐。フォルダの判定（FolderInspection）から、画面が出す手順を決める。
// 画面にも IPC にも触れない純粋な関数。
import type { FolderInspection } from "@/features/sync/types";

export type SetupPlan =
  /** フォルダが空: 新しいライブラリを作る（手元のデータを書き出す）。 */
  | { kind: "create" }
  /** ライブラリがあり、手元が空: 参加する（取り込むだけ）。 */
  | { kind: "join" }
  /** ライブラリがあり、手元にデータがある: 統合か置き換えを選ばせる。 */
  | { kind: "choose" }
  /** 手元が既に別のライブラリに属している（AT-S8）: 同期しない。解除してから選び直す。 */
  | { kind: "otherLibrary" }
  | { kind: "notWritable" }
  | { kind: "updateRequired" };

export function planSetup(inspection: FolderInspection): SetupPlan {
  switch (inspection.kind) {
    case "empty":
      return { kind: "create" };
    case "library":
      return inspection.localHasData ? { kind: "choose" } : { kind: "join" };
    case "otherLibrary":
      return { kind: "otherLibrary" };
    case "notWritable":
      return { kind: "notWritable" };
    case "updateRequired":
      return { kind: "updateRequired" };
  }
}
