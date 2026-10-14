# 0008: フロントエンドの基盤（UI 部品・テーマ・一覧・スキャン入力・ペアリング）

## 状態

採用（データの取得は暫定。モックで動く。統合時の差し替え箇所は「影響・今後」に一覧する）

## 日付

2026-10-14

## 背景

フェーズ 1 の画面を作る前に、フロントエンドの土台が要る。Rust 側の IPC（`list_books` など）は並行して作っている最中で、まだ使えない。そこで、バックエンドなしで動く基盤を先に作り、データは `ListRow`（0005 §4）と `ScanCode`（0005 §5）の形どおりのダミーで供給する。統合のときに、取得口だけを IPC に差し替えられる形にしておく。

## 決定

### 1. ディレクトリ構成

```
src/
  main.tsx, App.tsx              入口。Tauri の中では app_info で DB を確かめ、外（ブラウザの pnpm dev）ではモックで動く
  index.css                      Tailwind とテーマ（Catppuccin）
  types/contracts.ts             ListRow・ScanCode などの暫定の型（統合時に bindings に置き換える）
  i18n/ja.ts                     UI の文字列（コンポーネントに日本語を直接書かない）
  components/ui/                 shadcn/ui の部品（Radix ベース）。手で直してよい（scroll-area と sonner は手を入れた）
  components/layout/             上部バー・ステータスバー・ツールチップ
  components/BookCover.tsx       書影と、書影がないときの仮の表紙
  features/
    library/                     取得（api.ts・hooks.ts）、絞り込み・件数（filter.ts）、並べ替え（sort.ts）、ツリー（trees.ts）
    list/                        一覧の表（BookTable.tsx）、列の定義、セル、行の選択（selection.ts）
    sidebar/, detail/, layout/   サイドバー、詳細ペイン、メイン画面（MainScreen.tsx）
    scan/                        スキャン入力（capture.ts・useScanCapture.ts）、ペアリング（pairing.ts・usePairing.ts）、通知音（sound.ts）
    shortcuts/                   ショートカットの定義（shortcuts.ts）と、受けるフック
    settings/                    設定ダイアログ（暫定。テーマとスキャン入力・通知音だけ）
  stores/                        Zustand のストア
  lib/                           platform・theme・cover・cCode・format・persist・runtime などの小さな部品
  data/cCodeTable.ts             付録 A と同じ名称の C コード表（Tauri の外のモックとテスト用。`cCodeTable.test.ts` が Rust の表・付録 A と照合する）
  dev/                           ダミーデータ（mockRows.ts・mockBackend.ts）
```

- 純粋なロジック（絞り込み・並べ替え・選択・スキャンの組み立て・ペアリング・ショートカットの判定）は、React や DOM に触れないファイルに分け、Vitest でテストする。DOM に触れるのは `useScanCapture.ts`、`dom.ts`、`useShortcuts.ts` など薄いフックだけ。
- 型の import は、`ListRow`・`ScanCode`・`Location`・`CCodeTable` を `src/types/contracts.ts` 経由に限る。`src/bindings/` は直接 import しない。

### 2. 状態管理の分け方

| 種類 | 置き場所 | 保存 |
|---|---|---|
| バックエンドのデータ（全書籍、保管場所、Cコード表、検索結果の ID） | TanStack Query（`features/library/hooks.ts`）。取得は `api.ts` の裏に隠す | しない |
| 絞り込み・検索語（`viewStore`）、行の選択（`selectionStore`） | Zustand | しない（起動のたびに「すべて」から始める） |
| 列の状態（並べ替え・表示・並び順・幅。`tableStore`）、サイドバーと詳細ペインの開閉（`layoutStore`）、テーマ・スキャン入力・通知音（`settingsStore`） | Zustand の `persist` | 端末ごと。今は localStorage（`lib/persist.ts`） |
| 絞り込み後の行・件数 | `useLibraryView()` が TanStack Query の結果から `useMemo` で導く（画面の最上位で 1 回だけ呼ぶ） | しない |

- 保存する 3 つのストアは、保存された値が壊れていても、範囲に収めて読み込む（未知の列、範囲外の幅、3〜30 秒の外のタイムアウトなど）。
- 絞り込みは、ライブラリ・読書状況・保管場所・タグ・自動分類の 5 グループで、各グループは 0 か 1 つを選ぶ。グループどうしは AND。件数は「そのグループ自身の選択を外し、ほかのグループの選択と検索結果を掛けた」冊数（ファセット件数）で、1 回の走査で全グループぶんを数える。
- 「N 冊中 M 冊を表示」の N は、選んでいるライブラリの冊数（検索とほかの絞り込みを掛ける前）、M は掛けた後の冊数。ゴミ箱を選ぶと、ゴミ箱の冊数が N になる。

### 3. テーマの仕組み（FR-CONF-04）

- 配色は Catppuccin（ライトは Latte、ダークは Mocha）。`src/index.css` に `--ctp-*`（配色の原本）を置き、shadcn/ui のトークン（`--background`、`--primary` など）を `var(--ctp-*)` で引く。`<html class="dark">` で Mocha の値に切り替わる。
- 色は 16 進で書く（oklch などに頼らない）。`color-mix()` は Tailwind v4 が不透明度の指定（`bg-primary/15` など）で出すが、`@supports` で守られている。
- 設定は「OS に連動／ライト／ダーク」。`lib/theme.ts` の純粋関数 `resolveTheme` で実際のテーマを決め、`useThemeSync()` が `<html>` の class と `color-scheme` を更新する。「OS に連動」は `matchMedia` の変化に追随する。最初の描画の前に `main.tsx` が適用して、ライトの画面が一瞬見えるのを避ける（CSP でインラインスクリプトを使えないため）。
- sonner（トースト）は shadcn の既定（next-themes）を外し、自前のテーマに合わせた。
- フォントは OS の日本語フォント（`system-ui`、ヒラギノ、Yu Gothic UI、Meiryo UI）。Web フォントは同梱しない。
- 新しい CSS の機能は使わない。ただし Tailwind v4 自体が Safari 16.4 以上を前提にするので、macOS 13.3 以降が下限になる（0001 の決定どおり）。自分で書いた CSS では、ネイティブの CSS ネスト、コンテナクエリ、`dvh`、`field-sizing`、`text-wrap` を使っていない。ビルド後の CSS を grep して、これらが現れず、`oklch()`（Tailwind の既定の色。4 か所）、`color-mix()`（`@supports` で守られている）、`@property`、`@layer`、`:is()`・`:where()`・`:has()`（shadcn の部品が使う）が現れることを確かめた。

### 4. UI 部品

- shadcn/ui（radix-vega）の button・input・dialog・dropdown-menu・context-menu・tabs・select・checkbox・tooltip・popover・separator・scroll-area・sonner を入れた。`components.json` に設定がある。部品を足すときは `pnpm dlx shadcn@latest add <名前>` を使う。
- 次の点は、生成されたものから手で直した。
  - `cn` は、shadcn の初期設定が入れる新しいパッケージ `cn` ではなく、定番の `clsx` と `tailwind-merge`（`lib/utils.ts`）を使う。部品の import もそこへ向けた。
  - `shadcn` パッケージ（`shadcn/tailwind.css` を import する）は入れず、必要な data 属性のバリアント（`data-open` など 9 つ）と `no-scrollbar` を `index.css` に写した。`@import "shadcn/tailwind.css"` は相対色構文（`oklch(from …)`）を含む。
  - Web フォント（Inter）は外した。
  - scroll-area は、内側の div が `display: table` になって長い文字が折り返されないため、`block` に固定した。
  - ESLint の `react-refresh/only-export-components` は `src/components/ui/` だけ無効にした（部品とバリアント定義を同じファイルから export するため）。
- `tsconfig.json` に `paths`（`@/*`）を足した。shadcn の CLI が、ルートの tsconfig からエイリアスを読むため（ないと `@/` というディレクトリを作ってしまう）。

### 5. 画面の骨組み（9.1）

- 上部バー（検索欄、スキャン登録、手動登録、同期状態の枠、設定、開閉ボタン）、サイドバー（幅 16rem）、一覧、詳細ペイン（幅 20〜24rem）、ステータスバーの 3 ペイン構成。サイドバーと詳細ペインは折りたため、開閉は端末ごとに保存する。
- 同期状態は枠だけで、「同期: 未設定」を固定で出す。
- 編集・登録・再取得・書き出し・ゴミ箱への移動は、まだコールバックから「未実装」の通知を出すだけ。

### 6. 一覧の表（FR-LIST-01／02、9.5）

- TanStack Table v9（`useTable` と機能の登録）で、列の定義・表示・並び順・幅・並べ替えの状態を持ち、TanStack Virtual で行を仮想スクロールする。
- 行は絞り込み・並べ替え済みの `ListRow[]` をそのまま渡し、**Table の行モデルは作らない**。20,000 行の行モデルの生成は、測ると 12.7ms（初回）かかり、絞り込みのたびに無駄になる。セルは `features/list/cells.tsx` が列 id ごとに描く。
- 並べ替えは `features/library/sort.ts` の純粋関数。Table には `manualSorting` を指定し、見出しのクリックと Shift+クリックの複数列の操作だけを使う（最初のクリックは常に昇順。降順始まりにすると Table が先頭行の値の型を調べにいって行モデルを作ってしまうため、`sortDescFirst: false`）。
  - 書名は `sortTitle`、著者は `sortAuthors`。ほかの文字列の列は、UTF-16 のコード単位で比べる（`Intl.Collator` は 2 万行では遅い）。値のない行は、昇順でも降順でも最後。同じ値のときは、書名 → 巻次 → 元の並びで決める。
- 列の並び順（FR-LIST-01）は `tableStore.columnOrder`（空なら既定の順。Rust の `list.columnOrder` に保存）。「列」メニュー（`ColumnPicker`。矢印のボタンへ Tab で移れるよう、メニューではなくポップオーバー）と、設定の「表示」（どちらも `ColumnList`）の、上・下の矢印で 1 つずつ動かす。載っていない列（あとから増えた列）は末尾に並ぶ。見出しのドラッグでは動かせない。
- 幅の変更はドラッグをやめたときに確定する（`columnResizeMode: "onEnd"`）。途中の移動のたびに保存しない。ダブルクリックで既定の幅に戻る。
- 行の選択は `features/list/selection.ts` の純粋関数: クリック、⌘（macOS）／Ctrl（Windows）+クリックで 1 行ずつ、Shift+クリックで範囲（⌘／Ctrl+Shift で足す）。キーボードは、矢印、Shift+矢印、Home、End、PageUp、PageDown、⌘／Ctrl+A、Esc、Enter（編集を開くコールバック）、ゴミ箱への移動。日本語入力の変換中のキーは無視する。右クリックで、編集・一括変更・再取得・書き出し・ゴミ箱へ移動のメニューを出す（行の外では出さない）。
- 内容（Cコード）列の名称は、Cコード表を props で受け取って引く（`lib/cCode.ts`）。表にない内容は「未定義（xx）」。

### 7. サイドバーの絞り込み（FR-LIST-04）

- ライブラリ（すべて／所有／欲しい本／手放した／ゴミ箱）、読書状況、保管場所（ツリー。親を選ぶと子孫の本も含む）、タグ（`/` で階層。親を選ぶと子も含む）、自動分類（内容の大分類 → 中分類、発行形態、出版社、著者、発行年、取得待ち・情報不足）。
- 自動分類のうち、数が多い出版社と著者は、件数の多い順に上位 100 件だけを出す（選んでいる項目は必ず残す）。「取得待ち」は `metaStatus` が pending、「情報不足」は partial または failed。
- Cコードがない本、出版社がない本などは、「未設定」の項目（値が空文字のキー）で絞れる。
- `deleted` の本は、ゴミ箱を選んだときだけ出す。検索結果の ID の集合は、すべての絞り込みに掛け合わせる。

### 8. スキャン入力（FR-SCAN-07／08／10／12）

- 組み立ては `features/scan/capture.ts` の純粋関数（時刻は引数）。イベントの購読は `useScanCapture` フックで、`window` の keydown を**捕捉フェーズ**で受ける。
- キーは `KeyboardEvent.code`（`Digit0〜9`、`Numpad0〜9`、`Enter`／`NumpadEnter`、`Tab`）だけで見る。`key` と入力欄の値は使わない。`isComposing` の中のイベントも同じに読む。日本語入力がオンでも、`code` は物理キーのままなので変わらない。
- 数字キーと、設定された末尾キーのときだけ既定の動作を止める（`preventDefault` と `stopPropagation`）。英字・記号・矢印などは状態を変えず、止めない（リーダーが付ける文字は無視する）。Ctrl・⌘・Alt 付きのキーはスキャンの一部ではない。押しっぱなしの繰り返しは数えない。
- 末尾キーは Enter／Tab／なし。なしのときは、最後のキーから一定時間（既定 100ms）たつと確定する。先頭・末尾の余分な「数字」は、設定の文字数ぶん捨てる（0〜4）。末尾キーではない Enter・Tab は入力の区切りとして、途中の数字を捨てる。
- 前のキーから間隔が空いたら、途中の数字を捨てて数え直す。この上限が画面で違う。
  - メイン画面（`mainScreen`）: キー間隔の閾値（既定 30ms）。テキスト入力欄にフォーカスがないときだけ。8 桁以上の数字列＋Enter をスキャンとみなす。足りない Enter は奪わず、普通のキー操作として通す（Enter で編集を開く操作を壊さない）。
  - スキャン登録画面（`scanScreen`）: 1 秒。入力欄にフォーカスがなくても拾う。ほかの入力欄（タグなど）にフォーカスがあるときは拾わず、スキャン専用の欄だけ `data-scan-target` を付けて拾う。1 桁以上を渡し、判定（`classify_code`）は Rust に任せる。
- 手入力欄用に、全角の数字（と ISBN-10 の検査数字の全角 Ｘ）を半角にする `toHalfWidthDigits` を用意した。ハイフンや空白の扱いは `classify_code` に任せる。
- WebView が IME の変換を始めてから届くキーの `preventDefault` は、OS によって効かないことがある。読み取りの正しさは `code` に依っているので、それで足りる。実機（macOS の WKWebView と Windows の WebView2）で AT-09 を確かめる。

### 9. ペアリングの reducer（FR-SCAN-05／06）

- `features/scan/pairing.ts` の `pairingReducer(state, event, config)` は純粋関数。入力は `scan`（`ScanCode` と時刻）、`tick`、`commit`（確定キー）。出力は新しい状態と、効果の一覧。
- 状態: `idle`／`isbnHeld`／`secondHeld`。保留中は期限（`deadline`、ミリ秒）を持つ。残り秒数は `remainingSeconds` で切り上げて出す。
- 効果: `startLookup`（ISBN を読んだ時点で書誌取得を始める。2段目を待たない）、`confirm`（ISBN と任意の 2段目と、確定の理由）、`sound`、`message`（文言は `ja.scan.message` が持つ）。reducer は効果を実行せず、`usePairing` フックが、状態の保持・期限のタイマー・残り秒数の時計・効果の実行を受け持つ。
- 要件書の状態遷移図のとおりに作り、次の点を決めた。
  - タイムアウトは、`scan` と `commit` の処理の頭でも必ず検査する。`tick` が遅れても、期限後のスキャンは先に時間切れの処理を済ませてから扱う。
  - エラー（雑誌・一般 JAN・チェックデジット不一致・形式不正）では、状態と期限を変えない（保留中の本を壊さない）。
  - 音は、読んだバーコードごとに 1 回: 受け付けたら成功、同じ ISBN の二度読みと 2段目の置き換えは注意、受け付けないバーコードと 2段目の時間切れはエラー。ISBN 保留の時間切れによる確定は、ユーザーの操作の直後ではないので鳴らさない。確定キーによる確定は成功音。重複（所有済み）の警告音は DB を引かないとわからないので、呼び出し側が足す。
  - 確定キー: ISBN 保留なら 2段目なしで確定。2段目保留なら捨てる（時間切れと同じ扱い）。待機中は何もしない。
- タイムアウト T は 3〜30 秒に収める（既定 10 秒）。

### 10. 通知音（FR-SCAN-11）

- Web Audio API で合成する（`features/scan/sound.ts`）。成功（高い 2 音が上がる、サイン波）、エラー（低い音を 2 回、のこぎり波）、注意（中くらいの音を 2 回、三角波）。音量（0〜1。聞こえ方に合わせて 2 乗）とミュートは設定ストア。
- `AudioContext` は、最初のキー入力・クリックで作って `resume()` しておく（`useSoundUnlock`。App が呼ぶ）。音を鳴らすのは、バーコードを読んだあとの IPC の完了のあと（ユーザーの操作の外）なので、そこで初めて作ると、WKWebView の自動再生の制限で止まったままになり、最初の 1 回が鳴らないことがあるため。修飾キーだけの keydown などは操作として数えられないことがあるので、鳴らせる状態（running）になるまで、続く操作でもやり直す。鳴らすときも、止まっていれば再開する。作れない環境でも例外を投げない。

### 11. ショートカット（9.5）

- `features/shortcuts/shortcuts.ts` に定義を集め、`event.code` で判定する（OS ごとに ⌘ と Ctrl、Shift の有無も区別）。手動登録、スキャン登録、検索欄へ移動、詳細ペインの切替、設定、元に戻す（入力欄にフォーカスがあるときは入力欄に譲る）は、`useShortcuts` が受ける（手動登録・スキャン登録・設定は、どの画面でも効くよう App の `useGlobalShortcuts`、検索欄・詳細ペインはメイン画面。0011 §12）。編集（Enter）とゴミ箱へ移動（macOS は ⌘⌫、Windows は Delete）は、一覧にフォーカスがあるときだけ、一覧の `onKeyDown` が同じ判定関数で受ける。
- Tauri の既定のメニューが ⌘ 系のキーを先に取る場合がある。実機で確かめ、取られるものはメニュー側の設定を直す。

### 12. 書影（FR-COVER）

- `coverUrl(hash, size)`（`lib/cover.ts`）が 0005 §7 のとおりに URL を作る。macOS（と Linux）は `cover://localhost/<hash>?size=160`、Windows は `http://cover.localhost/<hash>?size=160`。サイズは 160 と 320。OS は User-Agent で判定する。→ 2026-10-15 に 80 を足し、一覧（`sm`）は 80、詳細ペインとフォーム（`lg`）は 320 にした（`LIST_COVER_SIZE`・`DETAIL_COVER_SIZE`。0010 §14）。
- `BookCover` は、書影がない、または読み込めないとき、`PlaceholderCover`（書名と著者を描いた仮の表紙）にする。色は ID の FNV-1a ハッシュで Catppuccin のアクセント色 14 色から決め、背景に薄く混ぜて本文色の文字を載せる（ライト・ダークのどちらでも読める）。

### 13. モックデータ

- `dev/mockRows.ts` が、種つきの乱数で `ListRow` を作る（有効な ISBN-13、ヨミ、複数の著者、シリーズ、階層タグ、各種の所有状態・読書状況・取得状態、ゴミ箱の本）。冊数は URL の `?mockRows=20000` で変えられる（既定 5,000、最大 100,000）。
- ブラウザで `pnpm dev` を開くと（Tauri の外）、IPC の確認を飛ばしてモックで動く。Tauri の中では、従来どおり `app_info` で DB を確かめ、失敗したらエラー画面を出す。

## 理由

- 純粋な関数に分けると、DOM なしで速くテストでき、後で IPC につないでも壊れにくい。絞り込み・並べ替え・件数の集計は、20,000 行でも最も遅いものが約 10ms で、要件（50ms）に十分収まる（下の測定を参照）。
- 一覧を全件フロントに持つ（0005 §4）ので、サイドバーの操作に IPC の往復がない。行モデルを作らないのは、そのぶんをさらに削るため。
- ペアリングを reducer にすると、AT-05〜08 とタイムアウトを、時刻を渡すだけで再現できる。
- `code` で拾うと、IME・キーボード配列・リーダーの設定（Shift の有無）に左右されない。

## 影響・今後

### 性能の測定（`src/features/library/perf.test.ts`、20,000 行、最速の回）

| 処理 | 時間 |
|---|---|
| 絞り込み（ライブラリ、保管場所、タグ、検索結果の ID との掛け合わせ。多いものでも） | 0.2〜1.0ms |
| 件数の集計（全グループ 1 回の走査） | 3.4〜6.5ms |
| 並べ替え（書名、著者、保管場所、3 列の組み合わせ） | 9.1〜10.1ms |
| 絞り込み → 並べ替え → 件数の集計（サイドバーを 1 回押したとき） | 約 6ms |

開発機（Apple Silicon の Mac）での値。テストは、最速の回が 50ms 未満であることを確かめる。

### 統合時に差し替える箇所

| 箇所 | 内容 |
|---|---|
| `src/types/contracts.ts` | `ListRow`・`ScanCode`・`Author` を `src/bindings/` の型の再エクスポートに置き換える（形は 0005 と一致している）。`Location` は `LocationInfo`（`path`・`depth` を足した上位集合）、`CCodeTable` は bindings が `{ code, name }` の配列なので、ここか `api.ts` で `Record<string, string>` の形に変換する（または `lib/cCode.ts` を配列の引きに直す） |
| `src/features/library/api.ts` | `listBooks`・`listLocations`・`cCodeTable`・`searchBooks` を `ipc.ts` の呼び出しに替え、`USING_MOCK_DATA` を false にする。`library-changed` イベントを受けた行の取り直し（`get_rows`）と、`others` が true のときの保管場所の取り直しもここに足す |
| `src/lib/ipc.ts` | `CommandMap` に `list_books`、`search_books`、`get_rows`、`list_locations`、`c_code_table`、`classify_code` などを足す（この作業では触れていない） |
| `src/lib/persist.ts` | `settingsStorage` を、Rust の `local_settings`（キー → JSON）を読み書きする IPC 版に替える。3 つの persist ストアの設定は、そのまま使える |
| `src/features/library/trees.ts` の `buildLocationPaths` | bindings の `LocationInfo.path` があるので不要にできる（保管場所の列とパスの並べ替えで使っている） |
| `src/features/scan/` のペアリングの呼び出し | `usePairing` の `onEffect` で、`startLookup` を書誌取得の IPC に、`confirm` を一覧・フォームに、`sound` を `playSound` につなぐ。`useScanCapture` の `onScan` から `classify_code` を呼び、結果の `ScanCode` を `submit` に渡す（スキャン登録画面が必要。次の作業） |
| `src/features/layout/MainScreen.tsx` | 「未実装」の通知を、編集フォーム・スキャン登録・ゴミ箱への移動などの実処理に替える。メイン画面でスキャンを拾ったときの暫定の通知（`ja.scan.received`）も同様 |
| `src/features/detail/DetailPane.tsx` | `ListRow` にない項目（メモ・感想・説明・取得元など）は、詳細取得（`BookDetail` が bindings にある）を足して表示する |
| `src-tauri` | `cover` スキームの登録と、CSP の `img-src` への `cover:` と `http://cover.localhost` の追加（このフロントは `coverUrl` で URL を作るだけ） |
| `src/dev/`、`src/data/cCodeTable.ts` | 統合後は、テスト・性能計測の固定データとして残すか、消す |

### 未解決・次の作業

- 実機（WKWebView と WebView2）での、日本語入力がオンのスキャン（AT-09）、⌘ 系ショートカット、書影の URL スキーム、フォントの見た目の確認。この作業では jsdom のテストまでしか確かめていない。
- スキャン登録画面、編集・登録フォーム、設定画面の本体、スマート本棚（FR-LIST-05）、グリッド表示（FR-LIST-07）、取得待ちの再取得の操作は、まだない。
- 設定ダイアログは暫定。テーマ・スキャン入力・通知音だけを変えられる。FR-CONF-04 のフォントと文字サイズは未対応。
- 保管場所の並べ替えは、パスの文字列のコード単位の順（ヨミがないため、日本語は五十音順にならない）。必要なら、並び順（`sortOrder`）で並べるように変える。
- `pnpm build` のチャンクが 500KB を超える警告が出る（約 630KB、gzip で約 190KB）。ローカルで読み込むアプリなので当面は放置し、起動が重くなったら設定画面などを `import()` で分ける。
- TanStack Table は v9 を使っている（行モデルは使わない）。v9 は新しいので、API が変わったら `features/list/columns.ts` と `BookTable.tsx` を直す。
