// 画面をまたいで使う文言（アプリ名、列挙値の表示名、値の整形、ショートカット名、通知）。
import type { ShortcutId } from "@/features/shortcuts/shortcuts";
import type { MetaStatus, Ownership, ReadStatus, WishPriority } from "@/types/contracts";

const countFormat = new Intl.NumberFormat("ja-JP");
/** 件数や金額を 3 桁区切りの文字列にする（各ファイルの文言関数で使う）。 */
export const n = (value: number) => countFormat.format(value);

export const readStatus = {
  unread: "未読",
  reading: "読書中",
  read: "読了",
  paused: "中断",
} satisfies Record<ReadStatus, string>;

export const app = {
  title: "bibcontainer",
  tagline: "蔵書管理",
  loading: "読み込み中…",
} as const;

export const appInfo = {
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
} as const;

// 列挙値の表示名（一覧・詳細・サイドバーで共通）
export const labels = {
  readStatus,
  ownership: {
    owned: "所有",
    wanted: "欲しい本",
    released: "手放した",
  } satisfies Record<Ownership, string>,
  wishPriority: {
    high: "高",
    mid: "中",
    low: "低",
  } satisfies Record<WishPriority, string>,
  metaStatus: {
    ok: "取得済み",
    pending: "取得待ち",
    partial: "一部取得",
    failed: "取得失敗",
  } satisfies Record<MetaStatus, string>,
} as const;

// 値の整形に使う語（単位と区切り）
export const format = {
  yen: (amount: string) => `${amount}円`,
  /** 本体価格に税込価格を併記する（FR-CONF-05）。 */
  withTaxIncluded: (base: string, taxIncluded: string) => `${base}（税込 ${taxIncluded}）`,
  listSeparator: "、",
} as const;

export const cCode = {
  undefinedContent: (code: string) => `未定義（${code}）`,
  nameSeparator: "・",
} as const;

// ショートカットの名前（ツールチップや設定画面で使う）
export const shortcuts = {
  newBook: "手動登録",
  scanRegister: "スキャン登録",
  focusSearch: "検索欄へ移動",
  toggleDetail: "詳細ペインの表示切替",
  edit: "選んだ本を編集",
  trash: "ゴミ箱へ移動",
  undo: "元に戻す",
  openSettings: "設定",
} satisfies Record<ShortcutId, string>;

export const toast = {
  notImplemented: (what: string) => `「${what}」はまだ実装していません`,
  /** 失敗の理由がわからないとき（Rust が用意した原因があれば、そちらを出す）。 */
  failed: "操作に失敗しました",
  undo: "元に戻す",
  trashed: (count: number) => `${n(count)} 冊をゴミ箱へ移しました`,
  restored: (count: number) => `${n(count)} 冊をゴミ箱から戻しました`,
  purged: (count: number) => `${n(count)} 冊を完全に削除しました`,
  bulkChanged: (count: number) => `${n(count)} 冊を変更しました`,
  nothingChanged: "変更する本はありませんでした",
  // 作品の手動のまとめ・外し（0026）。まとめるの冊数は対象の本の数、外す・戻すの冊数は変わった本の数
  worksMerged: (count: number) => `${n(count)} 冊を 1 つの作品にまとめました`,
  worksExcluded: (count: number) => `${n(count)} 冊を作品から外しました`,
  worksReset: (count: number) => `${n(count)} 冊を自動の判定に戻しました`,
  /** モックの「1 つの作品にまとめる」で、対象が 2 冊に満たないとき（Rust の検証と同じ文言）。 */
  worksNeedTwo: "2 冊以上を選んでください",
  exported: (count: number) => `${n(count)} 冊を書き出しました`,
  /** ブラウザで開いた確認用の画面（Tauri の外）では、ファイルを書き出せない。 */
  exportUnavailable: "この画面では、ファイルへ書き出せません",
  purchased: (count: number) =>
    `${n(count)} 冊を購入済み（所有）にしました。購入日が空なら、今日の日付が入ります`,
  tagsChanged: (count: number) => `${n(count)} 冊のタグを変更しました`,
  refetch: {
    /** 使える取得元が 1 つもない（全部無効、または API キーがない）。 */
    noProviders:
      "使える取得元がありません。設定で取得元を有効にしてください（Google Books は API キーが必要です）",
    /** 通信できず、1 冊も取得できなかった。 */
    offline: "通信できませんでした。接続を確かめて、もう一度試してください",
    /** 結果の要約。0 件の項目は出さない。 */
    result: (stats: {
      updated: number;
      unchanged: number;
      offline: number;
      skipped: number;
      failed: number;
      keptManualFields: number;
    }) => {
      const parts = [
        `${n(stats.updated)} 冊を更新しました`,
        stats.unchanged > 0 ? `${n(stats.unchanged)} 冊は変更なし` : null,
        stats.keptManualFields > 0
          ? `手で直した ${n(stats.keptManualFields)} 項目は上書きしませんでした`
          : null,
        stats.offline > 0 ? `${n(stats.offline)} 冊は通信できませんでした` : null,
        stats.skipped > 0 ? `${n(stats.skipped)} 冊は対象外です（ISBN がないなど）` : null,
        stats.failed > 0 ? `${n(stats.failed)} 冊は失敗しました` : null,
      ];
      return parts.filter((part) => part !== null).join("、");
    },
  },
  /** ブラウザで開いた確認用の画面（Tauri の外）で、Rust が要る操作をしたとき。 */
  mockUnavailable: "この画面（モックデータ）では使えません。アプリ本体で操作してください",
} as const;

// ファイルの選択・保存先の指定ダイアログ（lib/dialog.ts に渡す）
export const dialog = {
  export: {
    title: "蔵書を書き出す",
    csvFilter: "CSV ファイル",
    jsonFilter: "JSON ファイル",
    /** 既定のファイル名（拡張子なし）。日付は呼び出し側が付ける。 */
    defaultName: "bibcontainer",
  },
  isbnList: { title: "ISBN の一覧を選ぶ", filter: "テキストファイル" },
  backup: { title: "バックアップを選ぶ", filter: "バックアップ（SQLite）" },
  directory: { title: "保存先のフォルダを選ぶ" },
} as const;
