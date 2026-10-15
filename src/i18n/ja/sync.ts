// 同期（フェーズ 2）の文言。上部バーの表示と、設定の「同期」の節。
export const sync = {
  // ---- 上部バーの表示（9.1） ----
  indicator: {
    label: "同期",
    loading: "…",
    disabled: "未設定",
    syncing: "同期中…",
    /** 設定済みで、まだ一度も書き出し・取り込みをしていないとき */
    waiting: "待機中",
    pending: (count: string) => `未書き出し ${count} 件`,
    error: "エラー",
    mismatch: "ライブラリが違います",
    updateRequired: "更新が必要",
    skewBadge: "時計のずれ",
    openLabel: (text: string) => `同期: ${text}（同期の設定を開く）`,
  },
  /** 状態の名前（ツールチップと、設定の「同期の状態」） */
  states: {
    disabled: "同期フォルダが未設定です",
    idle: "待機中",
    syncing: "同期中",
    error: "エラー",
    mismatch: "ライブラリが一致しません（同期を止めています）",
    updateRequired: "アプリの更新が必要です（同期を止めています）",
  },
  tooltip: {
    folder: "フォルダ",
    lastExport: "最終書き出し",
    lastImport: "最終取り込み",
    pending: "未書き出し",
    missingCovers: "届いていない書影",
    never: "まだありません",
    count: (count: string) => `${count} 件`,
    clickHint: "押すと、設定の「同期」を開きます",
  },

  // ---- 設定の「同期」の節 ----
  section: {
    description:
      "複数の端末（Mac と Windows など）の蔵書を、Google Drive などの同期フォルダを介してそろえます。同期フォルダを設定しなくても、この端末だけで全機能を使えます。",
    loading: "同期の状態を読み込み中…",
    loadFailed: "同期の状態を読み込めませんでした",
  },

  // 未設定のとき: フォルダを選ぶ → 中身に応じた手順
  setup: {
    heading: "同期フォルダを設定する",
    intro:
      "Google Drive for desktop などで同期しているフォルダを選びます。選んだフォルダの下に「bibcontainer-library」というフォルダを作ります（そのフォルダ自体を選んでもかまいません）。ほかの端末でも、同じフォルダを選んでください。",
    chooseFolder: "同期フォルダを選ぶ…",
    dialogTitle: "同期フォルダを選ぶ",
    inspecting: "フォルダの中身を調べています…",
    inspectFailed: "フォルダの中身を調べられませんでした",
    rechoose: "別のフォルダを選ぶ",
    working: "設定しています…（本や書影が多いと、時間がかかります）",
    chosenFolder: "選んだフォルダ",
    libraryFolder: "同期に使うフォルダ",
    done: "同期を設定しました",
    devicesHeading: "このフォルダにいる端末",
    devicesNone: "まだ端末はありません",
    devicesHint:
      "これまでにこのフォルダを使った端末です。見覚えのある端末が並んでいるか確かめてください。",
    empty: {
      title: "新しいライブラリを作る",
      description:
        "このフォルダには、まだライブラリがありません。新しいライブラリを作って、この端末の蔵書を書き出します（手元に本がなければ、空のライブラリになります）。ほかの端末は、同じフォルダを選んで参加できます。",
      action: "新しいライブラリを作る",
    },
    join: {
      title: "このライブラリに参加する",
      description:
        "このフォルダには、すでにライブラリがあります。この端末の蔵書は空なので、ライブラリの内容を取り込みます（本や書影が多いと、時間がかかります）。",
      action: "このライブラリに参加する",
    },
    chooseMode: {
      title: "このライブラリに参加します。この端末にある本は、どうしますか？",
      description:
        "このフォルダには、すでにライブラリがあり、この端末にも蔵書があります。次のどちらかを選んでください。",
      merge: {
        title: "統合する",
        description:
          "この端末の本と、ライブラリの本を、どちらもすべて残します。同じ本をそれぞれで別々に登録していた場合は、同じ本が 2 冊になることがあります（あとで片方を消せます）。",
        action: "統合する",
      },
      replace: {
        title: "置き換える",
        description:
          "この端末の蔵書を消して（先にバックアップを取ります）、ライブラリの内容にします。この端末にだけある本は、ライブラリになければなくなります。",
        action: "置き換える…",
      },
    },
    replaceConfirm: {
      title: "この端末の蔵書を置き換えますか？",
      description:
        "この端末の蔵書をすべて消して、ライブラリの内容にします。消す前に、この端末のデータをバックアップとして残します（設定の「バックアップ」から復元できます）。置き換えると、この端末の端末 ID は新しくなります。",
      confirm: "置き換える",
      cancel: "キャンセル",
    },
    otherLibrary: {
      title: "別のライブラリのフォルダです",
      description:
        "この端末は、すでに別のライブラリに属しています。選んだフォルダは、それとは別のライブラリなので、同期しません（別の蔵書が混ざるのを防ぐためです）。フォルダを間違えていないか確かめてください。",
      hint: "この端末の蔵書を、選んだライブラリと統合したり置き換えたりしたいときは、この端末の同期を解除してから、選び直します。手元のデータと同期フォルダの中身は消えません。",
      action: "同期を解除してから、選び直す",
    },
    notWritable: {
      title: "このフォルダには書き込めません",
      description:
        "書き込む権限がないか、フォルダが読み取り専用です。Google Drive の共有フォルダで「閲覧者」になっていないか、容量がいっぱいでないかを確かめて、別のフォルダを選んでください。",
    },
    updateRequired: {
      title: "アプリの更新が必要です",
      description:
        "このフォルダのライブラリは、このアプリより新しい版で作られています。アプリを更新してから、もう一度選んでください。",
    },
  },

  // 設定済みのとき
  status: {
    heading: "同期の状態",
    folder: "同期フォルダ",
    state: "状態",
    lastExport: "最終書き出し",
    lastImport: "最終取り込み",
    pending: "未書き出しの変更",
    missingCovers: "届いていない書影",
    never: "まだありません",
    count: (count: string) => `${count} 件`,
    errorLabel: "直近のエラー",
    libraryId: "ライブラリ ID",
  },
  actions: {
    syncNow: "今すぐ同期",
    syncing: "同期中…",
    syncNowDone: "同期しました",
    leave: "同期を解除…",
  },
  leaveConfirm: {
    title: "同期を解除しますか？",
    description:
      "この端末の同期フォルダの設定を消して、同期を止めます。この端末の蔵書と、同期フォルダの中身は、消えません。解除すると、この端末の変更はほかの端末に届かなくなり、ほかの端末の変更も取り込まれなくなります。あとで同じフォルダを選び直せば、同期を再開できます。",
    confirm: "同期を解除する",
    cancel: "キャンセル",
    done: "同期を解除しました",
  },
  deviceName: {
    heading: "この端末",
    label: "端末名",
    hint: "同期フォルダの端末の一覧に出る名前です。既定はこの Mac / PC のホスト名です。",
    save: "名前を変更",
    saved: "端末名を変更しました",
    deviceId: "端末 ID",
  },
  devices: {
    heading: "同期フォルダの端末",
    description: "同期フォルダに、変更を書き出したことのある端末です。",
    self: "この端末",
    lastExport: "最終書き出し",
    never: "まだ書き出していません",
    empty: "端末の情報はまだありません",
    appVersion: (version: string) => `v${version}`,
  },
  covers: {
    heading: "書影の整理",
    description:
      "どの本からも使われていない書影のうち、30 日より前のものを、この端末と同期フォルダの両方から消します。手動のときだけ行います。",
    button: "使われていない書影を整理…",
    confirm: {
      title: "使われていない書影を整理しますか？",
      description:
        "どの本からも使われていない書影で、30 日より前のものを、この端末と同期フォルダの両方から消します。消した書影は戻せません。ほかの端末が使っている書影は、消しません。",
      confirm: "整理する",
      cancel: "キャンセル",
    },
    result: (removed: string) =>
      removed === "0" ? "整理する書影はありませんでした" : `${removed} 件の書影を整理しました`,
  },

  // 警告（FR-SYNC-10／11、更新が必要、エラー）
  warnings: {
    mismatch: {
      title: "このフォルダは、別のライブラリです",
      description:
        "同期フォルダのライブラリ ID が、この端末のライブラリと一致しません。誤って別のライブラリのフォルダを指定したか、フォルダの中身が入れ替わった可能性があります。同期は止めていて、この端末のデータは変わりません。フォルダを確かめて、「同期を解除」してから選び直してください。",
    },
    updateRequired: {
      title: "アプリの更新が必要です",
      description:
        "同期フォルダのライブラリは、このアプリより新しい版で作られています。アプリを更新するまで、同期は止まります（この端末だけでは、これまでどおり使えます）。",
    },
    skew: {
      title: (deviceName: string, duration: string) =>
        `端末 ${deviceName} の時計が ${duration}進んでいます`,
      description:
        "時計がずれていると、変更の新しさの判定が狂うことがあります。その端末の日付と時刻を、自動設定にして合わせてください。",
    },
    error: {
      title: "同期でエラーが起きました",
      description:
        "この端末での操作は、そのまま続けられます。原因を取り除くと、自動でやり直します（「今すぐ同期」でもやり直せます）。",
    },
  },

  // Google Drive の使い方（8.3）
  drive: {
    heading: "Google Drive で同期するときの注意",
    items: [
      "ストリーミングモードでは、ファイルが必要になるまでダウンロードされません。ライブラリのフォルダ（bibcontainer-library）を右クリックして「オフラインで使用可能」にするか、ミラーリングモードを使ってください。",
      "変更がほかの端末に届くまでには、Google Drive のアップロードとダウンロードの時間（数秒〜数分）がかかります。すぐに反映されなくても、故障ではありません。",
      "Windows の仮想ドライブ（G: など）では、ファイルの変更の通知が届かないことがあります。そのため、60 秒ごとの確認も続けています。",
      "この端末の DB（library.sqlite3）は同期フォルダに置きません。同期フォルダに置くのは、変更のログと書影だけです（5,000 冊で、ログは数 MB、書影は 250〜500 MB が目安です）。",
    ],
    examplesHeading: "同期フォルダの場所の例",
    examples: [
      { os: "macOS", path: "~/Library/CloudStorage/GoogleDrive-（メールアドレス）/マイドライブ/" },
      { os: "Windows", path: "G:\\マイドライブ\\" },
    ],
    examplesNote:
      "実際の場所は、Google Drive for desktop の設定によって変わります。端末ごとにパスが違っていてかまいません（同じライブラリのフォルダを選べば同期します）。",
  },

  // 開発用のパネル（モックで動いているときだけ。pnpm dev の開発中に出る）
  dev: {
    heading: "開発用: 同期のモック",
    description: "Rust の同期エンジンに差し替えるまでの、画面の確認用です。製品版には出ません。",
    scenarios: {
      disabled: "未設定",
      idle: "完了",
      syncing: "同期中",
      error: "エラー",
      mismatch: "不一致",
      updateRequired: "更新が必要",
      skew: "時計のずれ",
      pending: "未書き出しあり",
    },
    scenarioLabel: "状態",
    inspectLabel: "フォルダを選んだときの判定",
    inspectKinds: {
      empty: "空",
      library: "ライブラリあり",
      otherLibrary: "別のライブラリ",
      notWritable: "書き込めない",
      updateRequired: "更新が必要",
    },
    localHasData: "この端末に蔵書のデータがある",
    play: "同期の様子を再生",
  },
} as const;
