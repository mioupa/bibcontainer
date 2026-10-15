# 0015: 同期の画面（上部バーの表示、設定の「同期」の節、モックと差し替え箇所）

## 状態

採用（モックで動く。Rust の同期エンジンへの差し替え箇所は「影響・今後」に一覧する）

## 日付

2026-10-15

## 背景

フェーズ 2 の同期エンジン（0013）は `crates/sync` で別に作っている。画面（上部バーの同期の表示 9.1、設定の同期の節 FR-SYNC-08〜11・13・FR-CONF-01）は、エンジンを待たずに作り、後の組み込みで IPC に差し替えられるようにする。そのため、0013 §9 の型とコマンドの形に合わせた暫定の型と呼び出し口を置き、中身はメモリ上のモックにした。

## 決定

### 1. 構成

| 場所 | 役割 |
|---|---|
| `src/features/sync/types.ts` | `SyncStatus`・`SyncDevice`・`FolderInspection`・`SetupMode` などの暫定の型（0013 §9 と同じ形。camelCase）と、呼び出し口の型 `SyncApi`。**画面・フック・モックは、この型をここから import する**（`src/bindings/` を直接 import しない） |
| `src/features/sync/api.ts` | 呼び出し口。`getSyncStatus`・`inspectSyncFolder(path)`・`setupSync(path, mode)`・`leaveSync()`・`syncNow()`・`setDeviceName(name)`・`gcCovers()`・`onSyncStatus(handler)`・`usingMockSync()`。**どのバックエンドを使うかを決めるのは `selectBackend()` の 1 か所だけ** |
| `src/dev/mockSync.ts` | モック。`SyncApi` と同じ形の `api` と、開発・テスト用の `control`（状態の切り替え、フォルダの判定の切り替え、遅延、リセット）。`createMockSync()` は独立した状態を作れる。アプリ全体では `mockSync` を 1 つ使う |
| `src/dev/SyncDevPanel.tsx` | 開発用のパネル。設定の同期の節の下に、開発中（`import.meta.env.DEV` かつテスト以外かつモックで動いているとき）だけ出る。製品版のビルドにはコードが入らない（文言は `ja` に残る） |
| `src/features/sync/hooks.ts` | `useSyncStatus()`（状態の取得）、`useSyncStatusSubscription()`（`sync-status` の購読。`AppShell` が 1 回だけ呼ぶ）、`useSyncActions()`（設定・解除・今すぐ同期・端末名・書影の整理の操作） |
| `src/features/sync/display.ts`・`format.ts`・`setupPlan.ts` | 純粋な関数。表示の規則（`describeSync`）、時刻・時計のずれ・OS 名の整形、設定の手順の分岐（`planSetup`） |
| `src/features/sync/SyncIndicator.tsx` | 上部バーの表示。`TopBar` が置く |
| `src/features/sync/SyncSetup.tsx`・`SyncConfigured.tsx`・`DeviceNameGroup.tsx`・`DeviceList.tsx`・`DriveNotes.tsx`・`Notice.tsx`・`ConfirmDialog.tsx`・`folderPicker.ts` | 設定の節の部品 |
| `src/features/settings/sections/SyncSection.tsx` | 節の組み立て。未設定なら `SyncSetup`、設定済みなら `SyncConfigured`。端末名（`DeviceNameGroup`）と Google Drive の注意（`DriveNotes`）は、どちらのときも出す |
| `src/i18n/ja/sync.ts` | 文言（`ja.sync`）。上部バーの表示の文言は `ja.topBar` から `ja.sync.indicator` に移し、仮の `ja.settings.sync` は消した |

### 2. 状態の持ち方

- 状態 `SyncStatus` は TanStack Query のキャッシュ（キー `["sync_status"]`、`staleTime: Infinity`）に置く。更新は 2 つの経路: `sync-status` イベント（`useSyncStatusSubscription`）と、操作（`setupSync`・`leaveSync`・`syncNow`・`setDeviceName`）の戻り値。どちらもキャッシュを置き換える。
- 取得の途中でイベントが届いたときに、あとから戻ってきた古い取得結果で上書きしないよう、イベント・操作の戻り値を受けるたびに数えて、取得の前後で数が変わっていれば、キャッシュの値を使う。
- 本が取り込みで変わったときの一覧の更新は、既存の `library-changed`（`features/library/sync.ts`）が行う。同期の画面は扱わない。ただし、**置き換え（`joinReplace`）で設定したあとだけ**、DB が作り直されるので、復元（0009 §3）と同じく `reloadLibrary`（全クエリの取り直し）を呼ぶ。

### 3. 上部バーの表示（9.1）

「同期: ＜アイコン＞ ＜文字＞」の小さなボタン。押すと `showSettings("sync")` で設定の同期の節を開く。ツールチップに詳細（状態、フォルダ、最終書き出し・最終取り込み、未書き出し、届いていない書影、エラー、時計のずれ。0 のものは出さない）と「押すと、設定の「同期」を開きます」。`data-testid="sync-status"`、`data-kind`（見た目の種類）、`data-tone`（色の調子）を付けた。`role="status"` は、ステータスバーが使っているので付けない。

| 状態（`SyncStatus`） | 種類（`data-kind`） | 文字 | 色の調子 |
|---|---|---|---|
| 取得前 | `loading` | `…` | muted |
| `disabled` | `disabled` | 未設定 | muted |
| `syncing` | `syncing` | 同期中…（アイコンが回る。`motion-safe`） | info |
| `idle`、未書き出し 0 | `synced` | 最終の取り込み・書き出しの**新しい方**の時刻。今日なら `12:04`、前の日以前なら `10/14 23:05`。どちらもなければ「待機中」 | success |
| `idle`、未書き出し 1 以上 | `pending` | 未書き出し N 件 | warning |
| `error` | `error` | エラー | danger（文字も赤） |
| `mismatch` | `mismatch` | ライブラリが違います | danger |
| `updateRequired` | `updateRequired` | 更新が必要 | danger |

- 時計のずれ（`clockSkew`）がある設定済みの状態には、状態にかかわらず時計の警告アイコンを添える（読み上げの名前にも「時計のずれ」を入れる）。
- アクセシブルな名前は「同期: 12:04（同期の設定を開く）」の形で、見えている文字を含める。
- 「未書き出しあり」は、書き出しの 5 秒待ち（0013 §8）の間も出るので、警告ではなく注意の色（warning）にとどめた。

### 4. 設定の「同期」の節

**未設定のとき**（`SyncSetup`）: 説明と「同期フォルダを選ぶ…」。フォルダを選ぶ（`lib/dialog.ts` の `chooseDirectory`。ブラウザには無いので、モックの固定のパスを選んだことにする）と `inspectSyncFolder` で調べ、選んだフォルダと実際に使うフォルダ（`libraryFolder`）、手順、見つかった端末の一覧（名前・OS・版・最終書き出し）を出す。手順は `planSetup(inspection)` で決まる。

| `kind` | 手元のデータ | 手順 | `SetupMode` |
|---|---|---|---|
| `empty` | あり・なし | 「新しいライブラリを作る」（手元のデータを書き出す、と説明） | `create` |
| `library` | なし | 「このライブラリに参加する」 | `join` |
| `library` | あり | 「統合する」（両方の本がすべて残り、同じ本が 2 冊になることがある）か「置き換える…」（この端末の蔵書を消して、先にバックアップを取り、ライブラリの内容にする）を選ばせる。**置き換えだけ確認ダイアログ**（バックアップ、端末 ID が新しくなること）を挟む | `joinMerge`／`joinReplace` |
| `otherLibrary` | あり・なし | 警告（AT-S8。`role="alert"`）。同期しない。新規・参加・統合・置き換えは出さない。「同期を解除してから、選び直す」ボタン（確認ダイアログ → `leaveSync` → **同じフォルダをもう一度調べる**） | — |
| `notWritable` | — | 理由（権限・読み取り専用・容量・Drive の共有の権限）と、別のフォルダを選ぶ案内 | — |
| `updateRequired` | — | 理由（新しい版で作られている）とアプリを更新する案内 | — |

- `empty` を手元のデータの有無にかかわらず「作る」にするのは、0013 §7 のとおり（手元にデータがあればエンジンがスナップショットを書く）。
- 設定中（`setupSync` が戻るまで）は、手順のボタンと「別のフォルダを選ぶ」を押せなくし、「設定しています…」を出す。失敗したら理由を通知し、手順に残る。成功したら、状態が `disabled` でなくなるので、設定済みの画面に切り替わる。
- 解除したあとの再判定は、`otherLibrary` が「ライブラリあり」に変わる（手元のライブラリ ID が消えるため）ことを前提にしている。

**設定済みのとき**（`SyncConfigured`）: 上から、警告、同期の状態、端末の一覧、書影の整理。

- 警告（`role="alert"`）: ライブラリの不一致（danger。「同期を解除してから選び直してください」）、アプリの更新が必要（danger）、エラー（danger。直近のエラーの文と「今すぐ同期でやり直せる」）、時計のずれ（warning。「端末 ○○ の時計が N 分進んでいます」。名前がなければ端末 ID。60 分以上は「2 時間 30 分」、1 日以上は「1 日 3 時間」の形）。
- 状態: フォルダのパス、状態、最終書き出し・最終取り込み（`2026-10-15 12:04`。なければ「まだありません」）、未書き出しの件数、届いていない書影の数、直近のエラー、ライブラリ ID。
- 「今すぐ同期」（同期中、不一致、更新が必要のあいだは押せない）、「同期を解除…」（確認ダイアログ: 手元のデータと同期フォルダの中身は消えないこと、解除するとこの端末の変更が届かなくなること、あとで同じフォルダを選び直せること）。
- 端末の一覧: 名前、OS（`macos` → macOS など）、版、最終書き出し。自分には「この端末」の印。
- 「使われていない書影を整理…」: 確認ダイアログ（30 日より前、この端末と同期フォルダの両方から消す、戻せない、ほかの端末が使っているものは消さない）→ `gcCovers` → 件数を節の中（`data-testid="gc-result"`）と通知に出す。0 件なら「整理する書影はありませんでした」。

**どちらのときも**:

- 端末名（`DeviceNameGroup`）: 入力欄と「名前を変更」。前後の空白を除き、空・変更なしは保存できない。Enter でも保存する（日本語入力の変換を確定する Enter では保存しない）。上限 64 文字（モックの検査と合わせた暫定。下の未解決）。端末 ID も出す。
- Google Drive の注意（`DriveNotes`、8.3）: ストリーミングモードならライブラリのフォルダを「オフラインで使用可能」にするかミラーリングモードを使うこと、反映に数秒〜数分かかること、仮想ドライブでは通知が届かないので 60 秒ごとの確認も続けていること、DB は同期フォルダに置かないこと（容量の目安）、Mac と Windows の同期フォルダの場所の例。

**バックアップの復元の確認ダイアログ**（`BackupSection`）: 同期が設定済み（`state` が `disabled` 以外）のときだけ、「この端末は同期中です。復元したあとも、ほかの端末の変更をもう一度取り込むので、ライブラリ全体が過去に戻るわけではありません（この端末のデータを直す、という意味になります）。」を足した（0013 §10）。

### 5. モック

- 見本の状態（`MOCK_SYNC_SCENARIOS`）: 未設定、完了、同期中、エラー、不一致、更新が必要、時計のずれ、未書き出しあり。URL の `?mockSync=error` で最初の状態を決める（知らない値は未設定）。開発用のパネル（`SyncDevPanel`）で、実行中にも切り替えられる。切り替えると `sync-status` が届く。
- フォルダを選んだときの判定（`inspect`）の見本: 空・ライブラリあり・別のライブラリ・書き込めない・更新が必要と、「この端末に蔵書のデータがある」。既定は「ライブラリあり」＋「データあり」（いちばん手順が多い）。
- `setupSync`: 判定と合わない方法（例: 手元にデータがあるのに `join`）は `validation`、同期できないフォルダや設定済みでの再設定は `conflict` で断る（画面の不具合に気づけるように、エンジンと同じくらい厳しくした）。設定中は `syncing` の状態を出し（既定 0.9 秒）、終わると完了にする。
- `syncNow`: `syncing`（既定 1.5 秒）→ `idle`。時刻を今にして、未書き出しとエラーを消す（モックでは、エラーのまま再試行で直る）。未設定・不一致・更新が必要では `conflict`。
- `leaveSync`: 未設定に戻す。`otherLibrary` の判定は、`library`（手元のライブラリ ID なし）に変える。
- `gcCovers`: 見本の件数（既定 3）を返し、次からは 0。
- 本物の同期はしない。DB も同期フォルダも触らない。

### 6. テスト（Vitest）

- `display.test.ts`（20）: 状態ごとの上部バーの表示（種類・文字・色・時計のずれ・詳細の行）、時刻の整形、時計のずれの長さ、OS 名、手順の分岐（判定 × 手元のデータの有無の全組み合わせ）。
- `SyncIndicator.test.tsx`（13）: 状態ごとの表示、押すと同期の節を開く、`sync-status` での更新（同期中 → 完了）、購読の解除、ツールチップの詳細。今を固定して時刻を確かめる。
- `SyncSection.test.tsx`（41）: フォルダ選択（キャンセル、ブラウザ、判定の失敗）、手順の分岐（空／参加／統合・置き換え／別のライブラリ／書き込めない／更新が必要）、置き換えの確認（キャンセルで呼ばない、承諾で `joinReplace` と全クエリの取り直し）、解除の確認（設定済みから、別のライブラリの警告から）、設定中の無効化と失敗、設定済みの状態・端末の一覧・自分の印、警告（不一致、更新が必要、時計のずれ、エラー）、今すぐ同期、端末名の変更（保存、Enter、空・変更なし、失敗、未設定のとき）、書影の整理（確認、件数、0 件、失敗）、Google Drive の注意。
- `src/dev/mockSync.test.ts`（18）: 見本の状態、イベント、同期の遷移（時計を進めて）、設定・解除、断る条件、端末名、書影の整理。
- `SettingsScreen.test.tsx`: 同期の節の確認を新しいものに替え、復元の確認ダイアログの同期中の注記（出る・出ない）を足した。

## 理由

- 呼び出し口を `SyncApi` の型にそろえ、選ぶ場所を `selectBackend()` の 1 か所にしたので、統合は「IPC 版を書いて、その 1 行を直す」だけになる。画面・フック・テストは変えずに済む。
- 状態を TanStack Query に置いたのは、イベントと操作の戻り値の両方で更新でき、上部バーと設定の節が同じ値を見られるため。購読を `AppShell` に 1 つだけ置くのは、`library-changed` と同じ理由（画面の切り替えの間にイベントを取りこぼさない）。
- 手順の分岐を純粋な関数（`planSetup`）にしたのは、判定の種類と手元のデータの全組み合わせを、画面なしで確かめるため。
- モックを厳しくした（合わない方法は断る）のは、本物のエンジンに替えたときに初めて分かる不具合を減らすため。
- 「別のライブラリ」の警告に解除のボタンを付けたのは、案内だけだと、設定済みの画面に行けない（未設定の画面にいる）ため。解除してから同じフォルダを調べ直せば、統合・置き換えにそのまま進める。

## 影響・今後

### 統合時に差し替える箇所

| 箇所 | 内容 |
|---|---|
| `src/features/sync/types.ts` | `SyncState`・`SyncDevice`・`SyncStatus`・`FolderInspection`・`SetupMode`（と `ClockSkew`・`FolderKind`）を `src/bindings/` の再エクスポートに置き換える（`export type { SyncStatus } from "@/bindings/SyncStatus";` など）。`SyncApi` はそのまま残す。ts-rs の生成物は `camelCase` の同じ形のはず。違いがあれば、ここで合わせる |
| `src/lib/ipc.ts` | `CommandMap` と `ipc.*` に `get_sync_status`、`inspect_sync_folder({ path })`、`setup_sync({ path, mode })`、`leave_sync`、`sync_now`、`set_device_name({ name })`、`gc_covers` を足す。`onSyncStatus` を `onLibraryChanged` と同じ形で足す（イベント名 `sync-status`、ペイロードは `SyncStatus`） |
| `src/features/sync/api.ts` | `selectBackend()` を `inTauri() ? ipcSyncApi : mockSync.api` に直す。`ipcSyncApi` は `ipc.*` を `SyncApi` の形に包んだもの。**戻り値の形を合わせる**: `setupSync`・`leaveSync`・`syncNow`・`setDeviceName` は `SyncStatus` を返す前提、`gcCovers` は消した件数（`number`）を返す前提。Rust の戻り値が違う（例: 戻り値なし、件数の内訳）なら、ここで包んで合わせる |
| `src/dev/mockSync.ts`・`SyncDevPanel.tsx` | 統合後も、ブラウザの `pnpm dev`（Tauri の外）では使う。IPC に替えたあとは、Tauri の中では `usingMockSync()` が false になり、パネルは出ない |
| `src/features/sync/folderPicker.ts` | Tauri の外のときの固定のパス（`MOCK_FOLDER_PATH`）はそのまま。Tauri の中のダイアログは本物 |
| `src-tauri` | `sync-status` の発行、7 つのコマンド、`capabilities`（フォルダ選択は `dialog:allow-open` が既にある）。CSP の変更は要らない |

- 置き換え（`joinReplace`）のあと、画面は `reloadLibrary` を呼ぶ。Rust 側が `library-changed`（`books: []`、`others: true`）を出すなら二重になるが害はない。
- **製品版の注意**: 統合するまでは、製品版のビルドでも、同期の節は動く（モックの状態に切り替わるだけで、何も同期しない）。リリースの前に必ず統合する。

### 要件書との差異・補足（修正案。直接は直していない）

- 9.1: 上部バーの表示は、「同期: ✓ 12:04」の ✓ をアイコン（チェック）にした。未設定・同期中・未書き出しあり・エラー・不一致・更新が必要の表示と、時計のずれの印を足した。
- FR-SYNC-09: 「別のライブラリに属している」ときの警告に、同期を解除して選び直すボタンを足した（0013 §7 の案内の具体化）。
- FR-CONF-01: 端末名は、同期が未設定のときも変えられる。

### 未解決・申し送り

- 端末名の長さの上限（64 文字）と、`set_device_name` が空の名前を断ること（`validation`）は、モックと画面で仮に決めた。Rust 側と合わせる（`DEVICE_NAME_MAX_LENGTH` は `DeviceNameGroup.tsx`）。
- `gc_covers` の戻り値、`setup_sync` などの戻り値（`SyncStatus` か、イベントだけか）は、Rust 側に合わせて `api.ts` で包む。
- `inspect_sync_folder` の `devices` の `isSelf` は、フォルダの中の端末のうち自分の ID のものに付く前提。設定の手順では、自分が参加済みでなければ付かない。
- `FolderInspection.kind` が `empty` で `localLibraryId` が非 null（手元が別のライブラリに属しているのに、空のフォルダを選んだ）の扱いは、0013 §7 に書かれていない。画面は `kind` に従い、「新しいライブラリを作る」を出す。エンジンが断るなら、`otherLibrary` と同じ扱いにするか、判定の種類を足す。
- Google Drive のパスの例（`~/Library/CloudStorage/GoogleDrive-（メールアドレス）/マイドライブ/`、`G:\マイドライブ\`）は、実機の確認が要る（Drive の設定と表示言語で変わる）。
- 実機（WKWebView・WebView2）での確認と、ライト・ダークでの見た目の確認は未実施（jsdom のテストまで。この作業では、開発サーバーの画面をブラウザで見る確認もできなかった）。警告の囲み（`border-warning/40 bg-warning/10`）の読みやすさ、上部バーの幅（検索欄が狭い窓での収まり）を、実機で見る。
- 同期中の表示のアイコンは `animate-spin`（`motion-safe`）。OS の「視差効果を減らす」では回らない。
- 設定の同期の節は、状態が変わるたびに再描画する。端末の一覧が数十台になることは想定していない。
