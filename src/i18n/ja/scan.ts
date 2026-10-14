// スキャン入力・ペアリングの文言。
import type { PairingMessage } from "@/features/scan/pairing";
import { n } from "./common";

export const scan = {
  state: {
    idle: "待機中",
    isbnHeld: (seconds: number) => `ISBN 保留中（残り ${seconds} 秒）`,
    secondHeld: (seconds: number) => `2段目 保留中（残り ${seconds} 秒）`,
  },
  // ペアリングが出すメッセージ（PairingMessage）の文言
  message: (message: PairingMessage): string => {
    switch (message.kind) {
      case "isbnHeld":
        return `ISBN ${message.isbn13} を読みました。2段目を読むか、次の本へ進んでください`;
      case "secondTierHeld":
        return `2段目（${message.cCode}${message.listPrice === null ? "" : `、${n(message.listPrice)}円`}）を読みました。ISBN を読んでください`;
      case "paired":
        return `${message.isbn13} に 2段目（${message.cCode}${message.listPrice === null ? "、価格は不明" : `、${n(message.listPrice)}円`}）を付けて確定しました`;
      case "confirmedWithoutSecondTier":
        return message.reason === "timeout"
          ? `${message.isbn13} を、2段目なしで確定しました（時間切れ）`
          : `${message.isbn13} を、2段目なしで確定しました`;
      case "duplicateIsbn":
        return `${message.isbn13} は、いま読んだ本と同じです。無視しました`;
      case "secondTierReplaced":
        return `古い 2段目を捨てて、新しい 2段目（${message.cCode}）に置き換えました`;
      case "secondTierDiscarded":
        return message.reason === "timeout"
          ? "2段目に対応する ISBN が読まれなかったため、2段目を捨てました"
          : "2段目だけでは 1 冊にならないため、2段目を捨てました";
      case "rejected":
        switch (message.reason) {
          case "magazine":
            return "雑誌は非対応です";
          case "otherJan":
            return "書籍以外は非対応です";
          case "checksum":
            return `${message.raw} は、チェックデジットが一致しないため受け付けられません`;
          case "format":
            return `${message.raw} は、バーコードとして読み取れない形式です`;
        }
    }
  },
  // メイン画面でスキャンを拾ったときの暫定の表示（スキャン登録は次の作業）
  received: (digits: string) => `スキャンを読み取りました: ${digits}`,
  manualInput: {
    label: "ISBN・2段目を手入力",
    placeholder: "数字を入力して Enter（全角も可）",
    submit: "読み取る",
  },

  // ---- スキャン登録画面（9.3） ----
  screen: {
    title: "スキャン登録",
    back: "メイン画面へ戻る",
    browserNotice:
      "ブラウザで開いているため、読み取りと保存はできません（アプリのウィンドウで使ってください）",
    stateLabel: "読み取りの状態",
    mode: {
      label: "登録の方法",
      single: "1 冊ずつ",
      batch: "連続",
      singleHint: "読んだ本の登録フォームを、1 冊ずつ開きます",
      batchHint: "読んだ本を一覧に溜めて、まとめて保存します",
    },
    commit: {
      button: "確定",
      hint: "スペースキー",
      description: "保留中の本を、2段目なしで確定します",
    },
    paused: "フォームや確認を開いている間は、読み取りを止めています",
    editing:
      "入力欄にフォーカスがあります。バーコードを読み取るには、入力欄の外をクリックしてください",
  },
  last: {
    heading: "直前に読んだ本",
    empty: "バーコードを読み取ると、ここに表示します",
    fetching: "書誌を取得中…",
    noTitle: "（書名を取得できていません）",
    isbn: "ISBN",
    cCode: "内容",
    price: "本体価格",
    priceUnknown: "不明",
    noSecondTier: "（2段目は未読）",
  },
  result: {
    owned: (title: string, locationPath: string | null) =>
      `所有済み: ${title} ／ ${locationPath ?? "保管場所なし"}`,
    ownedListed: (title: string, locationPath: string | null) =>
      `所有済み: ${title} ／ ${locationPath ?? "保管場所なし"}（重複として一覧に追加しました。既定では保存しません）`,
    registerSecond: "2 冊目として登録",
    wanted: (title: string) => `欲しい本に登録済みです: ${title}`,
    wantedKept: (title: string) => `欲しい本のままにしました: ${title}`,
    moved: (title: string) => `所有に移しました: ${title}`,
    moveFailed: (title: string) => `所有に移せませんでした: ${title}`,
    fetching: "書誌を取得しています…",
    opened: "登録フォームを開きました",
    added: "一覧に追加しました",
    inBatch: "同じ ISBN が一覧にすでにあります（重複として追加しました。既定では保存しません）",
  },
  wantedDialog: {
    title: "所有に移しますか？",
    description: (title: string) =>
      `「${title}」は欲しい本に登録されています。所有に移しますか？ 購入日には今日の日付が入ります。`,
    confirm: "所有に移す",
    cancel: "移さない",
  },

  // ---- 連続登録（FR-SCAN-14／15） ----
  batch: {
    heading: "連続登録の一覧",
    empty: "読み取った本が、ここに溜まります",
    presets: {
      heading: "全冊に共通の値",
      description: "保存するとき、一覧の本すべてに入れます。入れない項目は空のままにします。",
      location: "保管場所",
      locationNone: "指定しない",
      tags: "タグ",
      tagsPlaceholder: "カンマで区切る（例: 研究/SiC, 小説）",
      ownership: "所有状態",
      ownershipNone: "指定しない",
      purchasedOn: "購入日",
      purchaseStore: "購入店",
    },
    status: {
      loading: "取得中",
      done: "完了",
      partial: "一部取得",
      failed: "取得失敗",
      pending: "取得待ち（オフライン）",
      duplicate: "重複",
    },
    duplicate: {
      owned: (title: string, locationPath: string | null) =>
        `所有済み: ${title} ／ ${locationPath ?? "保管場所なし"}`,
      inBatch: "同じ ISBN が一覧にあります",
    },
    untitled: "（書名なし）",
    include: "保存する",
    includeRow: (title: string) => `${title} を保存の対象にする`,
    edit: "編集",
    editRow: (title: string) => `${title} を編集`,
    remove: "削除",
    removeRow: (title: string) => `${title} を一覧から削除`,
    editedMark: "編集済み",
    saveAll: (count: number) => `まとめて保存（${count} 冊）`,
    saving: "保存中…",
    waiting: (count: number) => `書誌を取得中の本が ${count} 冊あります。終わるまでお待ちください`,
    saved: (count: number) => `${count} 冊を保存しました`,
    pendingSaved: (count: number) =>
      `うち ${count} 冊は取得待ちです。オンラインになると自動で補完します`,
    clear: "一覧を空にする",
    clearConfirm: {
      title: "一覧を空にしますか？",
      description: (count: number) => `一覧の ${count} 冊を、保存せずに取り除きます。`,
      confirm: "空にする",
      cancel: "キャンセル",
    },
    summary: (total: number, included: number) => `${total} 冊（保存する ${included} 冊）`,
    columns: {
      cover: "書影",
      book: "書名",
      status: "状態",
      include: "保存",
      actions: "操作",
    },
  },
  import: {
    button: "ISBN 一覧を読み込む",
    started: (count: number) => `${count} 冊の ISBN を読み込みます`,
    done: (result: { added: number; duplicates: number; skipped: number }) =>
      `一覧に ${result.added} 冊を追加しました（重複 ${result.duplicates} 冊、読み取れなかった行 ${result.skipped} 件）`,
  },
} as const;
