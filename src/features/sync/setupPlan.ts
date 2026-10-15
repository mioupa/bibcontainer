// 初めての設定（FR-SYNC-09、0013 §7）の手順の分岐。フォルダの判定（FolderInspection）から、画面が出す手順を決める。
// 画面にも IPC にも触れない純粋な関数。
import type { FolderInspection } from "@/features/sync/types";

export type SetupPlan =
  /** フォルダが空: 新しいライブラリを作る（手元のデータを書き出す）。 */
  | { kind: "create" }
  /** ライブラリがあり、手元が空: 参加する（取り込むだけ）。 */
  | { kind: "join" }
  /**
   * 手元が既に、このライブラリに参加している（フォルダの場所が変わった、設定が途中で止まった、など）:
   * 統合も置き換えもせず、場所だけを付け替える（`join`。手元のデータは変わらず、スナップショットも書かない）。
   */
  | { kind: "reconnect" }
  /** ライブラリがあり、手元にデータがある: 統合か置き換えを選ばせる。 */
  | { kind: "choose" }
  /** 手元が既に別のライブラリに属している（AT-S8）: 同期しない。解除してから選び直す。 */
  | { kind: "otherLibrary" }
  /** 手元がどれかのライブラリに属しているのに、空のフォルダを選んだ: 新しく作らせない。先に解除する。 */
  | { kind: "leaveFirst" }
  | { kind: "notWritable" }
  | { kind: "updateRequired" };

export function planSetup(inspection: FolderInspection): SetupPlan {
  switch (inspection.kind) {
    case "empty":
      // 手元がライブラリに属しているまま新しく作ると、同じ蔵書のライブラリが 2 つできる（エンジンも断る）
      return inspection.localLibraryId !== null ? { kind: "leaveFirst" } : { kind: "create" };
    case "library":
      if (inspection.localLibraryId !== null) {
        // 同じライブラリなら場所の付け替え。違うライブラリなら、エンジンの判定は otherLibrary のはずだが、念のため同じ扱い
        return inspection.localLibraryId === inspection.libraryId
          ? { kind: "reconnect" }
          : { kind: "otherLibrary" };
      }
      return inspection.localHasData ? { kind: "choose" } : { kind: "join" };
    case "otherLibrary":
      return { kind: "otherLibrary" };
    case "notWritable":
      return { kind: "notWritable" };
    case "updateRequired":
      return { kind: "updateRequired" };
  }
}
