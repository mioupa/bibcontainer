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

/** 設定済みの端末が、同期フォルダを選び直したときの手順（`planChangeFolder`）。 */
export type ChangeFolderPlan =
  /** 同じライブラリのフォルダ: 解除せずに `join` で場所だけを付け替える（取り込み済みの範囲は引き継ぎ、スナップショットは書かない）。 */
  | { kind: "reconnect" }
  /** ライブラリがない（`library.json` がない）フォルダ: 付け替えない。Drive の同期が終わっていないだけのこともある。 */
  | { kind: "empty" }
  /** 別のライブラリのフォルダ: 付け替えない。参加し直したいときは、解除してから選び直す。 */
  | { kind: "otherLibrary" }
  | { kind: "notWritable" }
  | { kind: "updateRequired" }
  /** 手元がどのライブラリにも属していない（設定済みの画面を開いたまま、別の操作で解除されたときなど）。 */
  | { kind: "notJoined" };

/**
 * 設定済みの端末が選び直したフォルダの判定（`FolderInspection`）から、付け替えられるかを決める。
 * 付け替えられるのは、フォルダのライブラリ ID が、手元のライブラリ ID と同じときだけ（0016）。
 */
export function planChangeFolder(inspection: FolderInspection): ChangeFolderPlan {
  switch (inspection.kind) {
    case "empty":
      return { kind: "empty" };
    case "library":
      if (inspection.localLibraryId === null) return { kind: "notJoined" };
      return inspection.localLibraryId === inspection.libraryId
        ? { kind: "reconnect" }
        : { kind: "otherLibrary" };
    case "otherLibrary":
      return { kind: "otherLibrary" };
    case "notWritable":
      return { kind: "notWritable" };
    case "updateRequired":
      return { kind: "updateRequired" };
  }
}
