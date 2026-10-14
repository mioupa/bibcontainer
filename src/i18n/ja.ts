// UI の文字列はすべて i18n に置く。コンポーネントに日本語を直接書かない。
//
// 文言は機能ごとのファイル（src/i18n/ja/*.ts）に分けてあり、ここで 1 つの `ja` にまとめる。
// 呼び出し側は今までどおり `ja.list.empty` のように引く。文言を足すときは、該当する機能のファイルに足す。
//   common.ts   アプリ名、列挙値の表示名、値の整形、ショートカット名、通知
//   errors.ts   エラー画面
//   list.ts     メイン画面（上部バー、サイドバー、一覧、詳細ペイン、ステータスバー）
//   scan.ts     スキャン入力・ペアリング
//   edit.ts     編集・登録フォーム
//   settings.ts 設定
// 将来、多言語化するときは同じ形のオブジェクトを増やして切り替える。
import { app, appInfo, cCode, dialog, format, labels, shortcuts, toast } from "./ja/common";
import { edit } from "./ja/edit";
import { errorScreen } from "./ja/errors";
import { detail, list, sidebar, statusBar, topBar } from "./ja/list";
import { scan } from "./ja/scan";
import { settings } from "./ja/settings";

export const ja = {
  app,
  appInfo,
  errorScreen,
  labels,
  format,
  cCode,
  dialog,
  topBar,
  sidebar,
  list,
  detail,
  statusBar,
  shortcuts,
  scan,
  edit,
  settings,
  toast,
} as const;

export type Messages = typeof ja;
