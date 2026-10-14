# 0009: Tauri との統合 — コマンド、イベント、設定、バックアップ、書誌取得の組み込み

## 状態

採用

## 日付

2026-10-14

## 背景

フェーズ 1 の部品（コア `crates/core`、書誌取得 `crates/metadata`、フロントの基盤 `src/`）はできている。これを `src-tauri` でつなぎ、フロントのモックを本物の IPC に置き換える。この記録は、ライブラリ側の統合の取り決めと、書誌取得（`src-tauri/src/metadata/`。設計は 0010）の組み込み（§11）である。

## 決定

### 1. エラーの種類（0002 の拡張）

`ErrorKind` は次の 12 種類（IPC では snake_case）。`AppError` に対応する variant がある。

| kind | 使いどころ |
|---|---|
| `db_unavailable` | 起動時（や復元の途中）に DB を開けなかった。DB に触るコマンドはすべてこれを返す |
| `schema_too_new` | DB がこのアプリより新しい版で作られている |
| `database` | SQLite の操作の失敗、マイグレーションの失敗、`local_meta` の欠落・破損 |
| `internal` | アプリの不具合（Op・HLC の不整合、JSON の変換失敗、バックグラウンド処理の失敗） |
| `validation` | 入力が不正。message に項目名を含む（例: `入力が不正です（rating）: …`） |
| `not_found` | 書籍・保管場所・タグなどが見つからない |
| `conflict` | 今の状態ではできない（本が残る保管場所の削除、循環する移動、完全削除した本の復元） |
| `invalid_backup` | バックアップとして使えないファイル |
| `network` | ネットワークの失敗（書誌取得側が使う） |
| `provider` | 書誌の取得元が、エラーや想定外の応答を返した（同上） |
| `io` | ファイルの読み書きの失敗 |
| `keychain` | OS のキーチェーンの失敗（書誌取得側が使う） |

- `From<bibcontainer_core::Error>` の対応: `Validation`→`validation`、`NotFound`→`not_found`、`Conflict`→`conflict`、`InvalidBackup`→`invalid_backup`、`Io`・`InvalidPath`→`io`、`SchemaTooNew`→`schema_too_new`、`Json`・`InvalidHlc`・`InvalidOp`・`ClockBehind`→`internal`、そのほか（SQLite、マイグレーション、メタ情報）→`database`。
- メッセージは日本語。`validation`・`not_found`・`conflict`・`invalid_backup` は、コアが作った文をそのまま出す。`network`・`provider`・`io`・`keychain` は、前置きの文に原因を続ける。
- `src/i18n/ja/errors.ts` の `errorScreen.hints` に、全種類の案内文がある（`ErrorKind` を増やすと型エラーになる）。

### 2. コアの型を IPC で使う

- `FieldChange`、`Origin`、`UpdateOptions` に `Serialize`・`Deserialize`・`TS` を付けた。JSON の形は次のとおり（camelCase）。
  - `FieldChange`: `{ field: string, value: unknown, origin: Origin }`。`value` は任意の JSON で、消すときは null を明示する（`value` を省いたものは受け付けない）。
  - `Origin`: `{ kind: "manual" } | { kind: "barcode" } | { kind: "import" } | { kind: "provider", name: string }`。
  - `UpdateOptions`: `{ overrideManual: boolean }`。省いた項目は既定値（false）。
- `Origin` の Rust 側は `Provider(String)` のまま。`serde(from, into)` は ts-rs が警告を出すので使わず、JSON 上の形（`OriginRepr`）への変換を手で書いた。TS の型は `#[ts(type = …)]` で指定している。

### 3. イベント `library-changed`

- 名前 `library-changed`、ペイロード `{ books: string[], others: boolean }`（`LibraryChanged`）。`src-tauri/src/events.rs` の `emit_library_changed(app, books, others)` で出す。
- 蔵書を変えるコマンドは、成功したあとに出す。実際に変わらなかったとき（同じ値の保存、既にゴミ箱の本）は出さない。
  - `books`: 変わった本の ID。`others`: 保管場所・タグ・シリーズの一覧（件数や名前）も変わった。
  - 書籍の作成・更新: `others` は、タグ（`tags`）かシリーズ（`series_id`）を変えたときだけ true。
  - `modify_tags`、`trash_books`、`restore_books`、`purge_books`: true（タグ・シリーズの件数が変わるため）。
  - 保管場所・タグ・シリーズのコマンド: true。本の行が変わるもの（`rename_tag`、`rename_series`）は、その本の ID も入れる（シリーズ名は `ListRow.seriesName` に入っているため、コアに `book_ids_in_series` を足した）。
  - `restore_backup`: 開き直したあとに `{ books: [], others: true }`。
- フロントは `features/library/sync.ts` で受ける。変わった行だけ `get_rows` で取り直して TanStack Query のキャッシュに差し込み（`mergeRows`）、`others` が true なら保管場所・タグ・シリーズを取り直す。検索結果の ID は、本が変わると古くなるので、まとめて取り直す。通知は順に処理する（古い取得結果が新しい結果を上書きしない）。
- **バックアップの復元は、行が丸ごと入れ替わる**。`books` が空のイベントでは全行を取り直せないので、呼び出し側（設定画面）が復元の完了後に `reloadLibrary`（全クエリの取り直し）を呼ぶ。

### 4. コマンド（`src-tauri/src/commands/`）

すべて async（DB に触るもの）で、`Database::run` 経由（書誌取得は `MetadataState` 経由で、同じ `Database::run` を使う）。バーコードの 3 つは計算だけなので同期。引数のキーは、Rust の引数名（snake_case）を camelCase にしたもの。フロントからは `src/lib/ipc.ts` の `ipc.*` だけで呼ぶ。

| モジュール | コマンド |
|---|---|
| `mod.rs` | `app_info` |
| `library`（参照） | `list_books`、`get_rows(ids)`、`get_book_detail(id)`、`search_books(query)`、`find_by_isbn(isbn)`、`purchase_store_suggestions`、`pending_book_ids` |
| `books`（書籍の変更） | `create_book(changes)`、`create_books(books)`、`update_book(id, changes, options?)`、`update_books(ids, changes, options?)`、`modify_tags(ids, add, remove)`、`trash_books(ids)`、`restore_books(ids)`、`purge_books(ids)` |
| `organize`（保管場所・タグ・シリーズ） | `list_locations`、`create_location(name, parentId)`、`rename_location(id, name)`、`move_location(id, newParentId)`、`reorder_locations(parentId, orderedIds)`、`delete_location(id)`、`list_tags`、`rename_tag(old, new)`、`set_tag_color(name, color)`、`list_series`、`find_or_create_series(name)`、`rename_series(id, name)` |
| `barcode` | `classify_code(raw)`、`c_code_table`、`describe_c_code(code)` |
| `settings` | `get_scan_settings`・`set_scan_settings`、`get_display_settings`・`set_display_settings`、`get_list_settings`・`set_list_settings`、`get_price_settings`・`set_price_settings`、`get_backup_settings`・`set_backup_settings` |
| `io` | `export_csv(path, ids?)`、`export_json(path)`、`parse_isbn_list(path)` |
| `backup` | `list_backups`、`manual_backup`、`restore_backup(path)` |
| `system` | `open_logs_dir`、`open_backup_dir` |
| `metadata/commands.rs`（書誌取得。0010） | `get_provider_settings`・`set_provider_settings`、`list_providers`、`test_provider(id)`、`set_api_key(provider, key)`・`delete_api_key`・`has_api_key`、`fetch_metadata(isbn13, secondTier?)`、`create_books_from_drafts(drafts)`、`refetch_books(ids, overrideManual)`、`trigger_pending_refetch`、`import_cover_file(path)`・`import_cover_bytes`・`import_cover_url(url)` |

- `create_book`・`create_books` は、依頼の一覧にはなかったが、登録画面が必ず要る薄い対応なので足した。
- `update_book(s)` の `options` は省ける。1 冊でも失敗した `update_books`・`create_books`・`modify_tags` などは全体を取り消す（コアの仕様）。
- `parse_isbn_list`（FR-IO-03）: 1 行に 1 つ。前後の空白と空行を除き、各行を `classify_code` と同じ規則で判定した `ScanCode[]` を返す。BOM を除き、改行は LF・CRLF・CR のどれでもよい。2段目のコードが混ざっていても、そのまま `secondTier` で返す（ISBN との組み合わせは、フロントが連続登録と同じ流れで行う）。8 MB を超えるファイルは `validation`。
- 書き出し先（`export_csv`・`export_json`）は絶対パスで、フォルダでなく、親のフォルダがあること（`validation`）。
- ファイルの選択と保存先の指定は、tauri-plugin-dialog の JS API。`src/lib/dialog.ts` に薄いラッパー（`chooseSavePath`・`chooseOpenFile`・`chooseDirectory`）を置いた。権限は `dialog:allow-open`・`dialog:allow-save`（`capabilities/default.json`）。フォルダを開くコマンドは、Rust 側で tauri-plugin-opener を使うので、JS 用の権限は足していない。
- コマンドの追加手順は 0002 のとおり（`lib.rs` の `generate_handler!` と `ipc.ts` の `CommandMap`）。
- `AppHandle` を受けるコマンドは、`AppHandle<R: Runtime>` のジェネリックにした（`emit_library_changed` も同様）。Tauri の `MockRuntime` で、フロントが送るのと同じ JSON の引数から呼ぶテスト（`commands/ipc_tests.rs`）を書くため。引数のキー（camelCase、省略できる `options` など）、`FieldChange` の JSON、戻り値の形、エラーの `{ kind, message }`、`library-changed` のペイロードを、ここで確かめている。実行環境（Wry）のコマンドを足すときは、ジェネリックにしなくてもよい（その場合は、このテストの対象外になる）。

### 5. 設定（端末ごと、`local_settings`）

- 型付きのセクションごとに、`get_*_settings`・`set_*_settings`。`set` は範囲に収めて保存し、保存した値を返す。保存は `local_settings` のキー（`scan`・`display`・`list`・`price`・`backup`）に JSON で行う。`providers` は書誌取得側が持つ。
- どのセクションも、serde の既定値で読む（`src-tauri/src/settings.rs`）。
  - 保存された JSON に項目が欠けていれば、既定値で補う（入れ子の項目も）。
  - 型の合わない項目（`"timeoutSec": "ten"`）は、その項目だけ既定値に戻し、ほかの項目は生かす。形そのものが違うとき・壊れた JSON は、セクション全体を既定値にする。
  - 範囲のある値は範囲に収める（読むときも保存するときも）。未知の項目は読み捨てる。

| セクション | 項目と既定値 |
|---|---|
| `scan` | `terminator`: `enter`（`enter`／`tab`／`none`）、`timeoutSec`: 10（3〜30）、`keyIntervalMs`: 30（5〜200）、`idleCommitMs`: 100（30〜1000）、`stripLeading`・`stripTrailing`: 0（0〜4）、`sounds`: `success`・`error`・`attention` それぞれ `{ enabled: true, volume: 0.6 }`（音量 0〜1）、`muted`: false |
| `display` | `theme`: `system`（`system`／`light`／`dark`）、`fontFamily`: null（OS の日本語フォント。引用符・波括弧・セミコロン・制御文字を含む名前は捨てる）、`fontSize`: 16（px、12〜24。画面全体の基準の大きさ） |
| `list` | `columnVisibility`: {}、`columnOrder`: []、`columnSizing`: {}（幅 16〜2000）、`sorting`: `[{ id: "title", desc: false }]`（最大 8、列 ID の重複と空は除く）、`sidebarOpen`・`detailOpen`: true |
| `price` | `showTaxIncluded`: false、`taxRatePercent`: 10（0〜100） |
| `backup` | `directory`: 空（既定の場所）、`dailyKeep`: 7（1〜365）、`weeklyKeep`: 4（1〜52） |

- `backup.directory` の扱い: 保存時に空を渡すと既定の場所（`app_local_data_dir/backups`）に戻る。既定の場所と同じパスを渡したときも空として保存する。読んだときは、既定でも実際の絶対パスを入れて返す（画面に出せるように）。相対パスは保存できない（`validation`）。読むときに相対パスなら、既定の場所に戻す。
- `list` は、フロントの `tableStore`（並べ替え・列の表示と幅）と `layoutStore`（サイドバー・詳細ペインの開閉）が分け合う。`columnOrder` は `tableStore` が持つ（「列」メニューと設定の「表示」の矢印で並べ替える）。

### 6. 日次バックアップ（FR-IO-05、0005 §9）

- 起動の 5 秒後（起動直後の一覧の読み込みと、DB 全体のコピーが接続を取り合わないよう、少し待つ）と、そのあと 1 時間ごとに、コアの `run_scheduled_backup_keeping` を、設定の保存先・世代数で呼ぶ（`src-tauri/src/backups.rs`）。実際に作るのは、最後の日次から 24 時間以上たっているときだけ（コアが判断する）。週次・ローテーションもコアの仕様どおり。
- 世代数を指定できるよう、コアに `Keep { daily, weekly }` と `run_scheduled_backup_keeping`・`rotate_backups_keeping` を足した（既存の関数は既定の 7・4 で呼ぶ）。
- 失敗しても止めない。ログに出して、次の周期で試す。スリープなどで時間が飛んでも、取りこぼしを連続で取り返さない（`MissedTickBehavior::Delay`）。DB を開けていない間は、エラーをログに出すだけ。

### 7. 復元と、接続の開き直し

- `Database` の中身を `Mutex<Slot>`（`Open(Connection)` か `Failed(AppError)`）にした。`run` のシグネチャは変えていない。`failure()` は `&AppError` ではなく所有した値を返す（テスト専用）。
- `Database::restore(バックアップ, 退避先)` の流れ: 接続の Mutex を取る → 接続を閉じる → コアの `restore_backup`（検証、新しい端末 ID、現在の DB を `pre-restore-…` で退避、置き換え）→ `open` で開き直す → Mutex を放す。復元中の `run` は、その間待たされ、新しい接続で実行される。
- 復元に失敗したとき（検証で弾かれたときなど）は、現在の DB のまま開き直して、エラーを返す。開き直せなかったときは、その理由を保持し（以後のコマンドが同じエラーを返す）、そのエラーを返す。
- DB を最初から開けなかった場合（`schema_too_new` など）も、バックアップから復元できる。
- 退避先は、設定の保存先（読めなければ既定の場所）。`restore_backup` コマンドは、成功したあとに `library-changed`（`books: []`、`others: true`）を出す。

### 8. コアの小さな修正

- **CSV の数式注入対策**: `=`、`+`、`-`、`@`、タブ、CR で始まる欄は、先頭に `'` を付ける（OWASP の推奨）。ヘッダーも同じ関数を通るが、日本語の項目名なので影響しない。付けたあと、`,`・`"`・改行を含む欄は引用符で囲む。0006 の「影響・今後」の該当項目は、これで対応した。
- **タグの階層の改名**: `rename_tag("研究", "研究2")` で、`研究` と、子孫（`研究/…`）がまとめて変わる（`研究/SiC` → `研究2/SiC`）。本の `tags` と tag エンティティ（色など）の両方、1 つのトランザクション。
  - `研究室` や `旧研究/SiC` のように、`/` で区切られていない・途中に含むだけの名前は対象外。
  - 変更先に同名のタグがあれば、本の `tags` は重複を除き、tag エンティティは変更先の方を残して古い方を削除する（0006 の動きを、子孫のタグにも適用）。
  - `研究` → `研究/sub` のように自分の子孫の下へ改名しても、もとの名前に二重には適用しない（改名の対象は、操作の前の名前で決める）。
  - 親そのもののタグが無く、子だけがあっても改名できる。対象がひとつも無ければ `not_found`。
  - 0006 の「ツリーの親を変えたとき、子も変えるか」は、変える、で決めた。

### 9. フロントの差し替え（0008 の一覧に対応）

| 0008 の差し替え箇所 | 対応 |
|---|---|
| `types/contracts.ts` | `ListRow`・`Author`・`ScanCode`・`LocationInfo` を bindings の再エクスポートに替えた。`Ownership` などは `ListRow` の項目から取る。`Location` は `LocationInfo` の部分集合（`id`・`name`・`parentId`・`sortOrder`）にして、`list_locations` の戻り値がそのまま渡せる。`CCodeTable` は名称を引く形（コード → 名称）のまま残し、`api.ts` の `toCCodeLookup` で bindings の配列から変換する |
| `features/library/api.ts` | Tauri の中は IPC、外（ブラウザの `pnpm dev`）はモック。どちらを使うかは呼ぶたびに `inTauri()` で決める。`USING_MOCK_DATA` は関数 `usingMockData()` にした。モックは、ゴミ箱への移動・復元・完全削除・1 項目の一括変更も、メモリ上で動かす |
| `library-changed` の受け取り | `features/library/sync.ts`（上の 3） |
| `lib/ipc.ts` | `CommandMap` と `ipc.*` に全コマンドを足した。`onLibraryChanged` でイベントを購読する |
| `lib/persist.ts` | `settingsStorage` を、Tauri の中は IPC、外は localStorage に切り替える StateStorage にした（下の 10） |
| `features/layout/MainScreen.tsx` | ゴミ箱への移動（元に戻すボタン付きの通知）、ゴミ箱から戻す、完全削除（確認ダイアログつき）、一括変更（右クリックメニューのサブメニュー: 読書状況・所有状態）、CSV の書き出し（選んだ本を、保存先をダイアログで選んで）をつないだ。編集・登録・再取得は「未実装」の通知のまま |
| `trees.ts` の `buildLocationPaths` | 残した（モックでも使う。結果は `LocationInfo.path` と同じ） |

- 右クリックメニューの「一括変更」は、ダイアログではなく、読書状況と所有状態のサブメニューにした（`update_books` に、手入力の由来で 1 項目を書く）。複数の項目をまとめて変える画面は、編集フォームの段階で作る。
- ゴミ箱を開いている間は、メニューの「ゴミ箱へ移動」が「ゴミ箱から戻す」と「完全に削除…」に変わる。⌘⌫（Delete）はゴミ箱の中では何もしない。

### 10. 設定のフロントでの保存

- zustand の persist は、キー名ごとに `{ state, version }` を保存する。Tauri の中では、その保存先をセクションに変える（`lib/settingsBridge.ts` が変換、`lib/persist.ts` が IPC）。対応は次のとおり。
  - `bibcontainer.settings`（テーマ・フォント・文字サイズ・スキャン・通知音）… `display` と `scan`
  - `bibcontainer.table`（並べ替え・列の表示と幅）と `bibcontainer.layout`（サイドバー・詳細ペインの開閉）… `list`（もう一方の項目を残して重ねる。並び順は table の項目）
- 書き込みは「最新の値だけ」にまとめる（書いている最中に来た値は、最新だけを次に書く）。スライダーなど、短い間隔で何度も変わる設定で、IPC と DB への書き込みを溜めないため。
- 読み込みは非同期なので、`main.tsx` は、Tauri の中ではストアの読み込み（最大 1 秒）を待ってから描く（ライトの画面が一瞬見えるのを避ける）。読み込めなかったとき（DB を開けないなど）は、既定の設定で進み、書き込みはしない。
- 通知音の設定は、種類ごとの有効・音量と、全体のミュートに変えた（`SoundSettings = { muted, kinds }`）。再生器（`sound.ts`）は、1 回ぶんの `{ volume, muted }` を受け取るままで、`playSound` が `effectiveSound` で求める。設定ダイアログ（暫定）の音量は、3 種類をそろえて変える。種類ごとの画面は、設定画面の段階で作る。
- フォントと文字サイズ（FR-CONF-04）は、設定ストアが持ち、`useThemeSync` が `<html>` に反映する（文字サイズは `font-size`、フォントは `--font-sans`）。操作する画面は、設定画面の段階で作る。

### 11. 書誌取得の組み込み（0010 の手順）

- `lib.rs`
  - `tauri::Builder::default()` を `metadata::register_cover_protocol(...)` に通した（`cover` URI スキーム）。
  - `.setup` で、`Arc<Database>` を managed state に入れたあとに `metadata::setup(app.handle())` を呼ぶ。失敗はログに出すだけで、起動は続ける（書誌取得だけが使えない）。成功すると、`Arc<MetadataState>` が managed state に入り、取得待ちの自動再取得が、起動の 5 秒後から 5 分ごとに動く。
  - `generate_handler!` に `metadata::commands::` の 14 コマンドを足した。
  - `RunEvent::WindowEvent` の `Focused(true)` で `metadata::trigger_pending_refetch(app)` を呼ぶ（30 秒以内の再実行は metadata 側で見送る）。
- CSP の `img-src` に `cover: http://cover.localhost` を足した。capabilities の追加は要らない（独自のコマンドと URI スキームは書かなくてよい）。
- `metadata/mod.rs` の `#![allow(dead_code)]` と、`register_cover_protocol` の `#[allow(unused_imports)]` を外した。そのため、テスト専用の `MemoryKeyStore` を `#[cfg(test)]` にした。`AppError::Network`・`Keychain` の `allow(dead_code)` も外した。`Provider` だけは使われていない（今の書誌取得は、取得元のエラーを `ProviderReport` で返し、失敗にしない）ので、種類は IPC の取り決めに残して、理由つきの `allow` を付けた。
- フロント
  - `src/lib/ipc.ts` に、型（bindings の `ProviderSettings`・`ProviderStatus`・`ProviderTestResult`・`FetchMetadataResult`・`NewBookDraft`・`RefetchSummary` など）と `ipc.*` を足した。`import_cover_bytes` は、`Uint8Array` を渡すとバイナリのまま送り（`invoke("import_cover_bytes", uint8Array)`）、数値の配列は `{ bytes }` で送る。`CommandMap` の引数の型で表し、`call` はそのまま通す。
  - `src/features/library/drafts.ts`: `draftToChanges(draft)`（`BookDraft.fields` → `FieldChange[]`。取得状態も入る）と、`toNewBookDraft(draft, edits, { seriesName? })`（フォームで直した項目を、同じ項目の下書きの値と置き換えて、`create_books_from_drafts` の 1 冊分にする）。
  - 画面（登録フォーム・スキャン登録・設定の取得元・再取得の操作）は、次の段階。
- `volume_sort` はコアが自動で作る（0006 §2）。`create_books_from_drafts` の巻次の補完は、コアの `create_book_in` に任せた。下書きの `volume_sort`（`BookDraft.fields`）は、コアの `volume::volume_sort` で作る。
- 確認した。
  - ビルドした .app を、`HOME` を scratchpad に向けて起動した。取得待ちの本を DB に 1 冊入れておくと、5 秒後に自動再取得が走り、openBD と NDL から書誌が入り（書名・著者・出版社・Cコードなど、取得元の記録つき）、書影が `covers/` に保存されて、状態が `ok` になった。ログにエラーはない。
  - 実際のネットワークで `fetch_metadata("9784780802047")` 相当（一時的な `#[ignore]` のテスト。確認後に削除）を実行した。openBD から書影をダウンロードし、`cover` スキームのハンドラが、原寸・160・320 のすべてで 200 の JPEG を返した。下書きを本として保存でき、`cover_hash` が一致した。
  - 作業用の `HOME` では、OS のキーチェーンが使えない（`A default keychain could not be found`）。そのため Google のキーの確認は警告になり、openBD と NDL だけで動く。実際のユーザーの環境では確認していない（0010 のとおり）。

## 理由

- 変更の通知を「変わった本の ID」にすると、20,000 冊でも、変更のたびに全件を取り直さずに済む（0005 §4）。タグ・シリーズの件数は、本の変更から変わるので、`others` で一括して知らせる。
- 設定を型付きのセクションにすると、Rust 側で既定値・範囲・補完をテストでき、フロントは壊れた保存内容に左右されない。保存先を StateStorage の差し替えにしたので、既存のストアの定義（キー名、`partialize`、`merge`）をほとんど変えずに済んだ。
- 復元で接続を持ち替える仕組みを `Database` の中に閉じたので、`run` を使う呼び出し側（書誌取得側を含む）は、復元を意識しなくてよい。
- タグの改名を階層に広げたのは、サイドバーが `/` でツリーを作る以上、親の改名で子が取り残される動きは利用者の期待に合わないため。

## 影響・今後

- 要件書は直接直していない。FR-CONF-03 の「音の種類と音量」を、種類ごとの有効・音量と全体のミュートで実装した。
- ファイルの場所（書き出し先・ISBN 一覧・バックアップ）は、フロントが渡す絶対パスを、そのまま使う。WebView は CSP で外部のスクリプトを読まないが、パスを制限する仕組み（スコープ）は入れていない。ファイルを書く・読むコマンドが増えるときは、ダイアログで選んだものに限る仕組みを考える。
- 復元した DB の `outbox_restored` の扱いは、フェーズ 2（0005 §2、0006 §8）。
- モックの `listTags`・`listSeries` は空を返す。`useTags`・`useSeries` を使う画面（タグの管理など）を作るときに、必要ならダミーを足す。
- `restore_backup` のあとの全件の取り直しは、呼び出し側の責任（上の 3）。設定画面で復元を作るときに、`reloadLibrary` を呼ぶこと。
- 実機の確認（WKWebView と WebView2 でのダイアログ、フォルダを開く動作、日本語入力中のスキャン）は未実施。
