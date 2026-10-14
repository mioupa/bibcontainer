// UI の文字列はすべてここに置く。コンポーネントに日本語を直接書かない。
// 将来、多言語化するときは同じ形のオブジェクトを増やして切り替える。
import type { ErrorKind } from "@/bindings/ErrorKind";

export const ja = {
  app: {
    title: "bibcontainer",
    tagline: "蔵書管理",
  },
  appInfo: {
    heading: "アプリ情報",
    name: "アプリ名",
    version: "バージョン",
    deviceId: "端末 ID",
    schemaVersion: "DB のスキーマ版",
    dbPath: "DB のパス",
    hlcLast: "HLC の前回値",
    pendingOps: "未書き出しの変更",
    none: "なし",
    loading: "読み込み中…",
  },
  errorScreen: {
    title: "アプリを開始できませんでした",
    reason: "原因",
    // エラーの種類ごとの案内。ErrorKind を増やすと、ここも書かないと型エラーになる。
    hints: {
      schema_too_new:
        "このデータベースは、より新しい版の bibcontainer で作られています。アプリを最新版に更新してください。データベースは変更していません。",
      db_unavailable:
        "データベースを開けませんでした。ログフォルダのログファイルに詳しい内容があります。",
      database: "データベースの操作に失敗しました。ログフォルダのログファイルを確認してください。",
      internal: "アプリ内部の不具合です。ログフォルダのログファイルを確認してください。",
    } satisfies Record<ErrorKind, string>,
  },
} as const;

export type Messages = typeof ja;
