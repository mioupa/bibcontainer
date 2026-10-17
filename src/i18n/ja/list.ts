// メイン画面（上部バー、サイドバー、一覧、詳細ペイン、ステータスバー）の文言。
import type { ColumnId } from "@/features/list/columnMeta";
import { n, readStatus } from "./common";

export const topBar = {
  searchLabel: "蔵書を検索",
  searchPlaceholder: "書名・著者・出版社・ISBN で検索",
  scanRegister: "スキャン登録",
  newBook: "手動登録",
  settings: "設定",
  toggleSidebar: "サイドバーの表示切替",
  toggleDetail: "詳細ペインの表示切替",
} as const;

// 検索欄で ISBN を確定したとき（FR-SCAN-09）
export const isbnSearch = {
  registerTitle: "登録しますか？",
  registerDescription: (isbn13: string) =>
    `ISBN ${isbn13} の本は、まだ登録されていません。スキャン登録の画面で登録しますか？`,
  register: "登録する",
  cancel: "キャンセル",
  foundInTrash: (title: string) => `「${title}」はゴミ箱にあります`,
} as const;

export const sidebar = {
  ariaLabel: "絞り込み",
  clearFilters: "絞り込みを解除",
  library: {
    heading: "ライブラリ",
    all: "すべて",
    owned: "所有",
    wanted: "欲しい本",
    released: "手放した",
    trash: "ゴミ箱",
  },
  readStatus: { heading: "読書状況", ...readStatus },
  locations: { heading: "保管場所", unset: "保管場所なし" },
  tags: { heading: "タグ", unset: "タグなし" },
  auto: {
    heading: "自動分類",
    content: "内容",
    format: "発行形態",
    publisher: "出版社",
    author: "著者",
    year: "発行年",
    meta: "取得待ち・情報不足",
    metaPending: "取得待ち",
    metaIncomplete: "情報不足",
    unset: "未設定",
    showingTop: (shown: number, total: number) => `上位 ${n(shown)} 件を表示（全 ${n(total)} 件）`,
  },
  expand: "開く",
  collapse: "閉じる",
  // 保管場所・タグの管理（作成・名前の変更・移動・削除・色）
  manage: {
    menuLabel: (name: string) => `「${name}」の操作`,
    cancel: "キャンセル",
    locations: {
      add: "保管場所を追加",
      addChild: "子の保管場所を追加…",
      rename: "名前を変更…",
      move: "移動…",
      delete: "削除…",
      createTitle: "保管場所を追加",
      createRoot: "最上位に、新しい保管場所を作ります。",
      createChild: (parent: string) => `「${parent}」の下に、新しい保管場所を作ります。`,
      nameLabel: "名前",
      namePlaceholder: "例: 書斎の本棚",
      create: "追加",
      renameTitle: "保管場所の名前を変更",
      renameDescription: (name: string) => `「${name}」の新しい名前を入力します。`,
      renameConfirm: "変更",
      moveTitle: "保管場所を移動",
      moveDescription: (name: string) => `「${name}」の移動先を選びます。`,
      moveTarget: "移動先",
      moveRoot: "（最上位）",
      moveConfirm: "移動",
      deleteTitle: "保管場所を削除しますか？",
      deleteDescription: (name: string) => `「${name}」を削除します。`,
      deleteBlocked:
        "子の保管場所か、この場所にある本（ゴミ箱の本を含む）があるため削除できません。先に、子の保管場所と本を移動してください。",
      deleteConfirm: "削除",
      created: (name: string) => `保管場所「${name}」を追加しました`,
      renamed: "名前を変更しました",
      moved: "移動しました",
      deleted: "保管場所を削除しました",
      emptyName: "名前を入力してください",
    },
    tags: {
      rename: "名前を変更…",
      color: "色を設定…",
      renameTitle: "タグの名前を変更",
      renameDescription: (name: string, hasChildren: boolean) =>
        hasChildren
          ? `「${name}」と、その下のタグ（${name}/…）の名前がまとめて変わります。`
          : `「${name}」の名前を変更します。`,
      nameLabel: "新しい名前",
      renameConfirm: "変更",
      renamed: (count: number) => `${n(count)} 冊のタグの名前を変更しました`,
      colorTitle: "タグの色",
      colorDescription: (name: string) => `「${name}」の色を選びます。`,
      colorNone: "色なし",
      colorSet: "色を設定しました",
      emptyName: "名前を入力してください",
      invalidName: "「/」で始まる・終わる・続く名前は使えません",
      sameName: "名前が変わっていません",
    },
  },
} as const;

/** 巻の範囲の文字列（`1〜7`）。片方しかなければそれだけ、同じ値ならその 1 つ、どちらもなければ空。 */
function groupVolumeRange(first: string | null, last: string | null): string {
  if (first === null || first === "") return last ?? "";
  if (last === null || last === "" || last === first) return first;
  return `${first}〜${last}`;
}

const listBase = {
  ariaLabel: "蔵書の一覧",
  loading: "蔵書を読み込み中…",
  empty: "条件に合う本がありません。",
  emptyHint: "絞り込みや検索語を変えてみてください。",
  columnPicker: {
    button: "列",
    heading: "表示する列と並び順",
    hint: "チェックで表示・非表示、矢印で並び順を変えます。",
    moveUp: (name: string) => `${name}を上（左）へ動かす`,
    moveDown: (name: string) => `${name}を下（右）へ動かす`,
    reset: "列を既定に戻す",
  },
  // 表示中の絞り込み結果の書き出し（FR-IO-01）
  exportShown: {
    label: (count: number) => `表示中の ${n(count)} 冊を CSV に書き出す`,
  },
  // 列の見出し。ColumnId を増やすと、ここも書かないと型エラーになる。
  columns: {
    cover: "書影",
    title: "書名",
    volume: "巻",
    authors: "著者",
    publisher: "出版社",
    publishedOn: "発行日",
    cCode: "内容",
    location: "保管場所",
    readStatus: "読書状況",
    rating: "評価",
    createdAt: "登録日",
    isbn13: "ISBN",
    series: "シリーズ",
    listPrice: "本体価格",
    purchasedOn: "購入日",
    purchasePrice: "購入価格",
    purchaseStore: "購入店",
    ndc: "NDC",
    format: "発行形態",
    audience: "販売対象",
    pages: "ページ数",
    tags: "タグ",
    ownership: "所有状態",
    readFinishedOn: "読了日",
  } satisfies Record<ColumnId, string>,
  sortedAscending: "昇順",
  sortedDescending: "降順",
  sortHint: "クリックで並べ替え、Shift+クリックで複数列",
  // 表示の形（表・グリッド。FR-LIST-07）
  viewSwitch: {
    ariaLabel: "表示の形",
    table: "表",
    tableHint: "表で見る",
    grid: "グリッド",
    gridHint: "書影のグリッドで見る",
  },
  // グリッドには列の見出しがないので、並べ替えはメニューで選ぶ
  grid: {
    ariaLabel: "蔵書のグリッド",
  },
  // 巻をまとめた表示（FR-LIST-08）。同じ作品の巻を 1 つの行（グリッドは 1 枚のカード）にまとめる
  group: {
    /** 上部バーの切替ボタンの文字と、その説明。 */
    toggle: "巻をまとめる",
    toggleHint: "同じ作品の巻を 1 つにまとめて表示する",
    /** まとめた行・カードの、開閉のボタンの名前（aria-label）。 */
    expand: "開く",
    collapse: "畳む",
    /** 巻の数（例: 7 冊）。 */
    count: (count: number) => `${n(count)} 冊`,
    /** 巻の範囲（例: 1〜7）。片方しかなければそれだけ、同じ値ならその 1 つ、どちらもなければ空。 */
    volumeRange: groupVolumeRange,
    /** グリッドのカードの 2 行目（例: 7 冊 · 1〜7）。 */
    gridMeta: (count: number, first: string | null, last: string | null) => {
      const range = groupVolumeRange(first, last);
      return range === "" ? `${n(count)} 冊` : `${n(count)} 冊 · ${range}`;
    },
  },
  sortMenu: {
    /** 引き金のボタンの文字。いまの並べ替えの先頭の項目と向きを出す（例: 並べ替え: 書名 ↑）。 */
    button: (column: string, descending: boolean) =>
      `並べ替え: ${column} ${descending ? "↓" : "↑"}`,
    buttonNone: "並べ替え: なし",
    columnHeading: "並べ替える項目",
    directionHeading: "並べ替えの向き",
  },
  resizeColumn: (name: string) => `${name}の幅を変更`,
  contextMenu: {
    edit: "編集",
    // 一括変更は、項目ごとのサブメニューから値を選ぶ（読書状況・所有状態）
    bulkEdit: "一括変更",
    bulkReadStatus: "読書状況",
    bulkOwnership: "所有状態",
    bulkRating: "評価",
    ratingNone: "評価なし",
    bulkLocation: "保管場所を変更…",
    bulkAddTags: "タグを追加…",
    bulkRemoveTags: "タグを外す…",
    markPurchased: "購入済みにする",
    refetch: "書誌を再取得…",
    export: "CSV に書き出し…",
    trash: "ゴミ箱へ移動",
    // ゴミ箱を開いているときのメニュー
    restore: "ゴミ箱から戻す",
    purge: "完全に削除…",
  },
  // 完全に削除する前の確認
  purgeConfirm: {
    title: "完全に削除しますか？",
    description: (count: number) =>
      `${n(count)} 冊を完全に削除します。この操作は取り消せません。他の端末にも伝わります。`,
    confirm: "完全に削除",
    cancel: "キャンセル",
  },
  deleted: "ゴミ箱",
} as const;

// 一括変更のダイアログ（保管場所・タグ）
export const bulkDialog = {
  location: {
    title: "保管場所を変更",
    description: (count: number) => `選んだ ${n(count)} 冊の保管場所を、まとめて変更します。`,
    label: "保管場所",
    none: "（保管場所なし）",
    apply: "変更する",
  },
  addTags: {
    title: "タグを追加",
    description: (count: number) => `選んだ ${n(count)} 冊に、タグを追加します。`,
    label: "追加するタグ",
    apply: "追加する",
  },
  removeTags: {
    title: "タグを外す",
    description: (count: number) =>
      `選んだ ${n(count)} 冊から外すタグを選びます。「研究」を選んでも、「研究/SiC」は外れません。`,
    label: "外すタグ",
    empty: "選んだ本には、タグが付いていません。",
    apply: "外す",
  },
  cancel: "キャンセル",
} as const;

// 書誌の再取得（FR-META-13、AT-16）
export const refetchDialog = {
  title: "書誌を再取得しますか？",
  description: (count: number) => `選んだ ${n(count)} 冊の書誌情報を、取得元から取り直します。`,
  overrideLabel: "手で直した項目も上書きする",
  keepHint: "オフのままなら、手で直した項目は上書きされません（既定）。",
  overrideHint: "オンにすると、手で直した項目も取得元の値に置き換わります。",
  confirm: "再取得",
  cancel: "キャンセル",
} as const;

export const detail = {
  ariaLabel: "詳細",
  empty: "本を選ぶと、ここに詳細が表示されます。",
  multiple: (count: number) => `${n(count)} 冊を選択中`,
  multipleHint: "右クリックで、まとめて変更・削除・書き出しができます。",
  edit: "編集",
  refetch: "書誌を再取得",
  markPurchased: "購入済みにする",
  noValue: "—",
  loadFailed: "詳細を読み込めませんでした",
  sections: {
    basic: "基本",
    classification: "分類・保管",
    reading: "読書",
    purchase: "購入",
    meta: "取得状態",
  },
  fields: {
    title: "書名",
    titleKana: "ヨミ",
    subtitle: "副題",
    authors: "著者",
    series: "シリーズ",
    volume: "巻次",
    publisher: "出版社",
    publishedOn: "発行日",
    isbn13: "ISBN",
    cCode: "Cコード",
    ndc: "NDC",
    listPrice: "本体価格（税抜）",
    refPriceIncl: "参考税込価格",
    pages: "ページ数",
    ownership: "所有状態",
    wishPriority: "優先度",
    location: "保管場所",
    tags: "タグ",
    readStatus: "読書状況",
    readFinishedOn: "読了日",
    rating: "評価",
    purchasedOn: "購入日",
    purchasePrice: "購入価格（税込）",
    purchaseStore: "購入店",
    metaStatus: "書誌情報",
    createdAt: "登録日",
    sizeText: "大きさ",
    language: "言語",
    description: "内容紹介",
    readStartedOn: "読み始めた日",
    review: "感想",
    purchaseCond: "新品・中古",
    purchaseMemo: "購入メモ",
    memo: "メモ",
    manualFields: "手で直した項目",
  },
  purchaseCond: { new: "新品", used: "中古" },
  /** 手で直した項目（`manualFields`、Op 上の項目名）の表示名。 */
  fieldNames: {
    isbn13: "ISBN",
    title: "書名",
    title_kana: "書名のヨミ",
    subtitle: "副書名",
    series_id: "シリーズ",
    volume: "巻次",
    volume_sort: "巻次（並べ替え用）",
    authors: "著者",
    publisher: "出版社",
    published_on: "発行日",
    c_code: "Cコード",
    ndc: "NDC",
    list_price: "本体価格",
    ref_price_incl: "参考税込価格",
    pages: "ページ数",
    size_text: "大きさ",
    language: "言語",
    description: "内容紹介",
    cover_hash: "書影",
  } as Record<string, string>,
} as const;

export const statusBar = {
  shown: (total: number, shown: number) => `${n(total)} 冊中 ${n(shown)} 冊を表示`,
  selected: (count: number) => `${n(count)} 冊を選択`,
  mockData: "モックデータ",
} as const;

// 一覧まわりの文言。検索欄の ISBN の確認、一括変更・再取得のダイアログは、ここにまとめる。
export const list = { ...listBase, isbnSearch, bulkDialog, refetchDialog } as const;
