# 0010: 書誌取得のアプリへの統合（設定・キー・キャッシュ・下書き・再取得・cover スキーム）

## 状態

採用（組み込み済み。`generate_handler!`・setup・URI スキームの登録・CSP・`ipc.ts` は 0009 §11。以下の「影響・今後」の手順は、そのとおりに実施した）

## 日付

2026-10-14

## 背景

`crates/metadata`（0007）は、DB にも Tauri にも依存せず、取得元への問い合わせ・正規化・統合・書影の変換だけを行う。これをアプリ（`src-tauri`）から使うには、0005 §3 のとおり、取得結果の保存・生の応答のキャッシュ・取得待ちの再取得・設定・キーチェーンを `src-tauri` がつなぐ必要がある。0007 の「実装する側（`src-tauri`）への申し送り」と、0005（§3、§7、§8、§9）、0006（コアの `FieldChange`・`Origin`・`create_book_in`・`update_book_in`・`find_or_create_series_in`）に従って、`src-tauri/src/metadata/` を実装した。

あわせて、ユーザーの判断（2026-10-14）を `crates/metadata` に反映した（詳細は 0007 の「改訂」）。

- NDL の書影 API は使わない。`NdlThumbnail` 取得元と `ProviderId::NdlThumbnail`・`MetaSource::NdlThumbnail` を削除した。取得元は `openbd`・`ndl`・`google` の 3 つ。書影の優先順位は openBD > Google。
- Google Books は API キーを使う（ユーザーがキーを取得して、設定画面から入れる）。キーがなければ Google を使わない。`GoogleBooks` の認証を `Required` にした。
- openBD は使い続ける。
- TLS の暗号プロバイダを aws-lc-rs から ring にした（Windows のビルドで NASM・CMake が要ることがあるため）。`cargo tree --workspace --target all -i aws-lc-sys` が「見つからない」になることを確かめた。

## 決定

### 1. 構成（`src-tauri/src/metadata/`）

| ファイル | 内容 |
|---|---|
| `state.rs` | `MetadataState`（managed state。`Arc<MetadataState>`）。設定とキーから `MetadataService` を組み立てて持つ（`Active`）。`active()` は最初に使うときに組み立て、`reload()` で作り直す |
| `settings.rs` | 取得元の設定（`local_settings` のキー `providers`） |
| `keychain.rs` | API キー（keyring）。`KeyStore` トレイト（本番は `OsKeyStore`、テストは `MemoryKeyStore`）と、読んだ結果を覚える `ApiKeys` |
| `cache.rs` | `DbRawCache`（`RawCache` の実装。`provider_cache`） |
| `fetch.rs` | `fetch_metadata`（取得 → 書影 → 取得状態 → 下書き） |
| `draft.rs` | `BookDraft`、取得状態の決め方（`decide_status`） |
| `register.rs` | `create_books_from_drafts` |
| `refetch.rs` | 再取得、取得待ちの自動再取得 |
| `providers.rs` | 取得元の一覧、接続テスト、キーの操作、設定の保存 |
| `covers.rs` | 書影の取り込み |
| `cover_protocol.rs` | `cover` URI スキーム |
| `volume.rs` | 巻次から並べ替え用の数値を作る |
| `commands.rs` | IPC コマンド（処理は `MetadataState` のメソッド。ここは受け渡しだけ） |

- 通信の層は `TransportFactory`（User-Agent から `Transport` を作る関数）で差し替えられる。本番は `ReqwestClient`（rustls + ring）、テストは偽の HTTP。蔵書の変更の通知は `Notifier`（本番は `events::emit_library_changed`、テストは記録）。
- キーチェーンには、最初の取得（または一覧・キーの操作）まで触らない。起動を、キーチェーンの確認で止めないため。

### 2. 取得元の設定（`local_settings` の `providers`）

```ts
type ProviderSettings = {
  enabled: { openbd?: boolean; ndl?: boolean; google?: boolean };  // 利用者の選択
  priorities: MetaPriorities;                                       // { groups: { title: ["openbd","ndl","google"], ... } }
  contactUrl: string;                                               // User-Agent の連絡先
};
```

- 既定: 3 つとも有効、優先順位は 0007 §5 の表（NDL 書影を除いたもの）、連絡先は `https://github.com/mioupa/bibcontainer`。
- 読み出し: 欠けた項目・型が違う値・知らない取得元や項目・壊れた JSON を、既定値で補う。優先順位は、設定の順（重複・未知の値を除く）に、設定にない既定の取得元を末尾に足して、全部の行をそろえて返す。以前の設定に `ndl_thumbnail` があっても、捨てるだけで動く。
- 保存: 連絡先を検べる（`http(s)://` で始まる ASCII の表示できる文字だけ。空白・制御文字・括弧は不可、200 文字まで。User-Agent のヘッダーに入るため）。不正なら `validation`。
- 「有効」は利用者の選択。実際に使うか（`active`）は別で、**有効、かつ、認証が必須ならキーがある** もの。Google はキーがなければ、有効にしていても使わない（キーなしは 429 のため）。キーを入れれば、設定を触らずに使われ始める。設定の保存では、キーのない Google を有効にしても拒否しない（フロントが設定を丸ごと読み書きしても、キーなしで保存が失敗しないように）。

### 3. API キー（OS のキーチェーン）

- keyring 4（`v1` 機能。macOS: Keychain、Windows: 資格情報マネージャー）。サービス名は `io.github.mioupa.bibcontainer`、アカウント名は取得元 ID（`google`）。
- コマンド: `set_api_key(provider, key)`、`delete_api_key(provider)`、`has_api_key(provider)`。**キーを返すコマンドは作らない。** キーは Rust の中だけで読み、`GoogleBooks::new(Some(key))` に渡す。
- 入力: 前後の空白を除き、空・512 バイト超・空白や制御文字を含むものは `validation`（メッセージにキーを入れない）。キーを使わない取得元（openBD・NDL）への保存は `validation`、`has_api_key` は `false`。
- ログ・設定・DB・エラーメッセージにキーを出さない（`Secret` は `Debug`・`Display` で `***`）。URL のログは、`crates/metadata` が `key=` を伏せる。
- 読んだキーはプロセスの間メモリに持つ（呼ぶたびにキーチェーンへ問い合わせて、macOS の許可の確認を出さないため）。保存・削除のたびに更新し、`MetadataService` を作り直す。キーチェーンの読み出しに失敗したときは、警告を出してキーなしとして続ける（openBD・NDL は動く）。
- 実際のキーチェーンを使うテスト（`os_keychain_roundtrip`）は `#[ignore]`。`cargo test -p bibcontainer-app os_keychain -- --ignored` で、書いて・読んで・消す。**この作業では、キーチェーンに触れない環境（`User interaction is not allowed`）だったため、実機では未確認。** そのエラーは `keychain`（`AppError::Keychain`）として返ることは確かめた。

### 4. 生の応答のキャッシュ（`provider_cache`）

`DbRawCache` が `RawCache` を実装する（0007 §3 の対応表どおり）。

| `CacheEntry` | 列 |
|---|---|
| 取得元（`ProviderId::as_str()`） | `provider` |
| ISBN-13 | `isbn13` |
| `fetched_at`（UNIX 秒） | `fetched_at`（RFC 3339、UTC、秒まで。例 `2027-01-15T08:00:00Z`。読むときは数字だけの文字列も UNIX 秒として読む） |
| `found` | `status`（真 `ok`、偽 `not_found`） |
| `body` | `payload` |

- 書き込みは `ON CONFLICT(provider, isbn13) DO UPDATE`（上書き）。時刻・状態・本文を読めない行は「無い」ものとして扱い、取り直させる。DB を開けていないときは `CacheError`（取得は止めない）。
- 有効期限の判断は `MetadataService` が行う（30 日、該当なしは 1 日）。通信エラー・HTTP エラー・解析エラーは保存されない。

### 5. 取得（`fetch_metadata(isbn13, secondTier?)`）

戻り値: `{ merged: MetaMerged, draft: BookDraft, reports: ProviderReport[], offline: boolean, coverHash: string | null }`。DB には書かない。

1. ISBN を ISBN-13 にそろえる（ISBN-10・ハイフン・全角も可）。読めなければ通信せず `validation`。
2. 実際に使う取得元に並行して問い合わせる（`FetchRequest`。優先順位は設定のもの、2段目の Cコード・本体価格は `secondTier`）。キャッシュを使う。
3. 書影の URL があれば `acquire_cover`（優先順位の順に試す）で取得して、`app_local_data_dir/covers` に保存する。
4. 取得状態を決める（§7）。下書きにする（§6）。

通信できない（オフライン）ときも失敗にはしない。ISBN と 2段目の情報だけの下書き（`meta_status = pending`）を返すので、そのまま保存でき、あとで自動で再取得される（FR-SCAN-16、AT-11）。書影は保存されるが、下書きを本として保存しなければ孤立したファイルになる（フェーズ 3 の整理の対象）。

### 6. `BookDraft`

```ts
type DraftValue<T> = { value: T; origin: Origin };      // Origin はコアの形: { kind: "manual"|"barcode"|"import" } | { kind: "provider", name: string }
type BookDraft = {
  fields: DraftFields;                                   // キーは書籍の Op の項目名（snake_case）。値がない項目はキーごとない
  seriesName: { name: string; origin: Origin } | null;   // シリーズは名前で持つ
};
// DraftFields: isbn13, title, title_kana, subtitle, authors(Author[]), publisher, published_on, c_code, ndc,
//   list_price, ref_price_incl, pages, size_text, language, description, volume, volume_sort, cover_hash, meta_status
```

- `origin`: **ISBN と、2段目の Cコード・本体価格は `barcode`。** それ以外は採用された取得元（`{ kind: "provider", name: "openbd" }` など）。書影（`cover_hash`）は、実際に画像を取れた取得元。
- `volume_sort` は巻次（`volume`）から作る（`巻1`・`第12巻`・`巻一`・`第十二巻`・`上`/`中`/`下` など。`volume.rs`）。読み取れなければ入れない。由来は巻次と同じ。
- `meta_status` も項目の 1 つ（`{ value: "ok"|"pending"|"partial"|"failed", origin }`）。コアは記録を付けない項目なので `origin` に意味はない（`provider: "bibcontainer"` を入れてある）。こうしておくと、`fields` の各項目を `FieldChange` にするだけで、取得状態も保存される（取得待ちの本が取得待ちのまま保存される。AT-11）。
- 変換: `Object.entries(draft.fields).map(([field, v]) => ({ field, value: v.value, origin: v.origin }))`（Rust 側の `DraftFields::to_changes` と同じ）。フォームで直した項目は、フロントが `origin: { kind: "manual" }` に差し替える。

### 7. 取得状態（`meta_status`）の規則（`decide_status`）

書誌で通信する取得元（`ProviderReport.network`）の結果だけで決める。

| 状況 | 状態 |
|---|---|
| 取得元を 1 つも使わなかった（全部無効、またはキーがなくて使えない） | `pending`（何も試していない。取得元を有効にすれば取り直す） |
| 通信する取得元がすべて通信エラー（`offline`） | `pending` |
| 1 つも取れず、すべて「該当なし」 | `failed`（再取得は手動） |
| 1 つも取れず、エラー（HTTP・解析、または一部だけの通信エラー）がある | `pending`（あとで取り直す） |
| 取れたが、一部の取得元がエラー、または書名が取れていない | `partial` |
| 取れて、エラーなし | `ok` |

- 書影の取得で、試した URL がすべて通信エラー（`cover.offline`）のときは、`ok`・`partial` も `pending` にする（書影をあとで取り直すため）。ダミー画像・HTTP エラーは通信エラーではないので、`pending` にしない。
- 取れた取得元が一部だけ「該当なし」でも、エラーがなければ `ok`（収録されていない取得元があるのは普通のため）。
- 再取得では、すでに書誌がある本（`ok`・`partial`）は、今回たまたま取れなかった（`failed`・`pending`）だけでは状態を悪くしない（`status_after_refetch`）。

### 8. `create_books_from_drafts(drafts)`

- 各要素は `{ changes: FieldChange[], seriesName?: { name, origin } | null }`。1 つのトランザクションで、シリーズを `find_or_create_series_in`（正規化した名前で探して、なければ作る）で解決して `series_id`（由来は `seriesName.origin`）にし、`create_book_in` で作る。1 冊でも失敗したら全体（シリーズも）を取り消す。空のシリーズ名は無視する。
- 巻次（`volume`）があって `volume_sort` がなければ、巻次から作る（由来は巻次と同じ）。
- 成功したら `library-changed`（本の ID の一覧、シリーズを解決したなら `others = true`）を出す。新しい本の ID を渡した順に返す。手動登録（ISBN なし）も同じ経路。

### 9. 再取得

**`refetch_books(ids, overrideManual)`**（FR-META-13）。キャッシュを使わず取り直す。戻り値は `RefetchSummary`（件数と、本ごとの `RefetchItem { id, outcome, metaStatus, written, skippedManual, message }`。`outcome` は `updated`・`unchanged`・`offline`・`skipped`・`error`）。

1. 本の ISBN と、**2段目由来（`field_sources` が `barcode`）の Cコード・本体価格**を読む。後者は最優先の値として統合に渡すので、取得元の値に置き換わらない。取得元由来・手修正の値は渡さない（手修正の値は、下の 4 でコアが守る）。
2. 取得する。最初の 1 冊で通信できるかを確かめ、全部通信エラーなら残りは試さずに `offline` とする（NDL・Google は 1 件ずつリトライしながら取るので、全冊を試すと長くかかる）。以降は 20 冊ずつ（進捗が DB に残る単位）。
3. 書影がまだない本だけ、書影を取得する。
4. `update_book_in` を、取得元の由来（`Provider`）で呼ぶ。手で直した項目は、`overrideManual` が偽なら上書きされない（コアが守る。AT-16）。真なら上書きして、その項目の手修正の記録を外す。組になる項目は一緒に扱う: 巻次が手修正済みなら `volume_sort` も書かない。シリーズ（`series_id`）が手修正済みなら、シリーズを探さない（作らない）。ISBN は書かない。取得状態も更新する（§7）。
5. 本ごとに 1 つのトランザクション。1 冊の失敗は `error` として記録し、ほかの本は続ける。書き換えた本があれば `library-changed` を出す。

全取得元が通信エラー（`offline`）のときは何も書かない（Op も通知も出さない）。取得待ちの本は取得待ちのまま次回に回る。ISBN がない・完全削除した・見つからない本、使える取得元がない場合（`noActiveProviders`）は `skipped`。

**取得待ち（`pending`）の自動再取得**（0005 §9、AT-11）

- 起動の 5 秒後から 5 分ごとに `run_pending_refetch` を呼ぶ（`start_background`。`setup` が始める）。取得待ちの本がなければ DB を 1 回引くだけ。
- `trigger()`（ウィンドウが前面に来たときに呼ぶ）は、すぐ戻り、裏で 1 回試す。前回の試行から 30 秒たっていなければ何もしない（前面に来るたびに通信しないため）。
- 取得待ちの再取得は、キャッシュを使う（`force_refresh` なし。該当なしは 1 日）。
- **同時に 2 つ走らない**: `refetch_lock`（tokio の Mutex）。手動の `refetch_books` は待ち、自動の `run_pending_refetch` は走っていれば見送る（`None`）。

### 10. 取得元の一覧と接続テスト

- `list_providers()` → `ProviderStatus[]`: 能力情報（`ProviderInfo`: 表示名・認証・取れる項目・レート制限・クレジット表記）と同じ階層に、`enabled`（設定）、`hasKey`、`active`（実際に使う）を足したもの。クレジット表記は FR-CONF-07 の「このアプリについて」にそのまま使う。
- `test_provider(id)` → `ProviderTestResult { provider, isbn13, outcome, title, publisher, cCode, hasCover, elapsedMs }`: 既知の ISBN `9784780802047` を、キャッシュを使わずに取得する。無効にしている取得元でも試せる。認証が必須でキーがなければ `validation`。通信できないときは、エラーではなく `outcome`（`networkError` など）で返す。

### 11. 書影の取り込み

`import_cover_file(path)`・`import_cover_bytes(bytes)`・`import_cover_url(url)` → 保存した書影のハッシュ。本への設定はしない（フロントが `update_book` で `cover_hash` を `Manual` として書く）。

- ファイル: 20 MiB を超えるものは読まずに拒否する。ファイルでない・無い・読めない → `io`／`validation`。
- バイト列: `invoke("import_cover_bytes", uint8Array)`（バイナリのまま。大きい画像向け）か、`{ bytes: number[] }`。
- URL: http・https だけ。画像でない応答（`Content-Type` が `text/*`・xml・json）は拒否する。見つからない・つながらない → `network`。
- 変換できない画像は `validation`（「画像として読み込めません」など）。変換・保存は 0007 §8 のとおり（長辺 800px 以下の JPEG、SHA-256 のファイル名）。

### 12. `cover` URI スキーム（0005 §7）

- URL: `cover://localhost/<hash>?size=160`（macOS）、`http://cover.localhost/<hash>?size=160`（Windows）。
- `<hash>`: **小文字の 16 進 64 桁だけ**を受け付ける（`is_valid_hash`）。大文字・`..`・`/`・`%`・長さ違いなどは 400。パスの組み立てには、検証したハッシュだけを使う。
- `size`: なし（原寸）、`160`、`320`（`ensure_thumbnail` が `app_cache_dir/thumbs/<長辺>/<先頭 2 文字>/<hash>.jpg` に作ってから返す）。それ以外は 400。
- 応答: 200（`Content-Type: image/jpeg`、`Cache-Control: public, max-age=31536000, immutable`、`X-Content-Type-Options: nosniff`、`Access-Control-Allow-Origin: *`）、400、404（書影がない）、405（GET・HEAD 以外。HEAD は本体なし）、500（読めない・壊れた書影のサムネイル）、503（`MetadataState` がまだない）。エラーは `Cache-Control: no-store`。
- 非同期の URI スキーム（`register_asynchronous_uri_scheme_protocol`）。ファイルの読み込みとサムネイルの生成は `spawn_blocking`。登録は `register_cover_protocol::<R: Runtime>(builder) -> Builder<R>`。
- フェーズ 2 で同期フォルダの書影からも読めるようにするときは、`locate_original` に探す場所を足す。

### 13. 通信の層

- 本番は `ReqwestClient::new(user_agent(版, 連絡先))`（タイムアウト 10 秒、プロキシは OS の設定）。User-Agent は `bibcontainer/<版> (+<連絡先>)`。設定の連絡先を変えると、`MetadataService` を作り直すので、次の取得から変わる。
- `ring` の既定のプロバイダは、`ReqwestClient` を作るときに 1 回だけプロセスの既定として入れる（`http::install_crypto_provider`）。

## 理由

- 設定とキーから `MetadataService` を組み立てて持つのは、取得元の有効・無効、キー、連絡先（User-Agent）が変わったときに、通信の層ごと作り直せば足りるため。優先順位と有効・無効は依頼ごとに渡すので、作り直しは、キーと連絡先が変わったときに要る。
- 下書きの項目名を Op の項目名（snake_case）にし、`meta_status` も項目に入れたのは、フロントが `fields` を `FieldChange` に機械的に直せるようにするため。フロントに「取得状態も足す」という手順を覚えさせると、足し忘れで取得待ちの本が `ok` で保存され、自動再取得が働かない（AT-11 が壊れる）。
- 「有効」と「使う」を分けたのは、Google のキーを入れた瞬間に使われ始めるようにするため。設定を丸ごと保存するフロントの実装で、キーのない Google を有効のまま保存しても失敗しない。
- 2段目由来の値だけを再取得に渡すのは、取得元由来の古い値は新しい取得元の値に更新してよく、手修正の値はコアが守るため。取得元由来の値まで渡すと、取得元の更新が反映されない。
- 最初の 1 冊で通信を確かめるのは、NDL・Google が 1 件ずつ・リトライつきで取るため、オフラインで取得待ちが多いと、各冊が長く待たされるから。
- 手動の再取得でキャッシュを使わないのは「書誌を再取得」（FR-META-13）の意図（最新の値を取り直す）による。取得待ちの自動再取得は、5 分ごとに全取得元へ問い合わせないよう、キャッシュを使う（0007 の申し送り）。
- `cover` のハッシュを小文字の 16 進 64 桁に限るのは、ハッシュをパスの組み立てに使うため（パストラバーサル対策）。大文字を受けないのは、保存するハッシュが常に小文字で、大文字小文字を区別しないファイルシステムでの取り違えを避けるため。

## 影響・今後

### 組み込み（ライブラリ側の統合の担当）

1. `src-tauri/src/lib.rs`: `tauri::Builder::default()` を `metadata::register_cover_protocol(tauri::Builder::default())` に通す（`.plugin(...)`・`.setup(...)` より前でよい）。
2. `.setup` の中で、`app.manage(Arc<Database>)` のあとに `metadata::setup(app.handle())` を呼ぶ（失敗はログに出すだけにして、起動は続ける）。`setup` が `Arc<MetadataState>` を managed state に入れ、取得待ちの自動再取得を始める。
3. `generate_handler!` に次を足す: `metadata::commands::get_provider_settings`、`set_provider_settings`、`list_providers`、`test_provider`、`set_api_key`、`delete_api_key`、`has_api_key`、`fetch_metadata`、`create_books_from_drafts`、`refetch_books`、`trigger_pending_refetch`、`import_cover_file`、`import_cover_bytes`、`import_cover_url`。
4. ウィンドウが前面に来たとき（`RunEvent::WindowEvent` の `Focused(true)`）に `metadata::trigger_pending_refetch(app_handle)` を呼ぶ。
5. `tauri.conf.json` の CSP の `img-src` に `cover: http://cover.localhost` を足す。
6. `src-tauri/src/metadata/mod.rs` 先頭の `#![allow(dead_code)]` と、`register_cover_protocol` の再エクスポートの `#[allow(unused_imports)]` を外す。
7. capabilities: 足すものはない。独自のコマンドと URI スキームは capabilities に書かなくてよく、書影のファイルを選ぶダイアログの権限（`dialog:allow-open`）は `default.json` にすでにある（`import_cover_file` はパスを受け取るだけで、ファイルの権限は要らない）。
8. フロントの `ipc.ts` に、上のコマンドの型を足す（`src/bindings/` に `ProviderSettings`・`ProviderStatus`・`ProviderTestResult`・`FetchMetadataResult`・`BookDraft`・`DraftFields`・`DraftValue`・`DraftSeries`・`MetaStatus`・`NewBookDraft`・`RefetchSummary`・`RefetchItem`・`RefetchOutcome` が出る）。

### 未解決・今後

- **OS のキーチェーンの動作は実機で確認していない**（この作業の環境では、キーチェーンへの操作が拒否された）。`cargo test -p bibcontainer-app os_keychain -- --ignored` を Mac と Windows で実行する。開発ビルド（署名なし）では、macOS が初回に許可の確認を出す。
- 更新（`update_book`）で巻次を変えたときの `volume_sort` は、コアが自動で作るようにした（0006 §2、0009 §11）。`volume.rs` はコアに移した（`bibcontainer_core::volume::volume_sort`）。
- Google が（キー無効・割り当て切れなどで）エラーを返し続けると、取れた本がすべて `partial` になる。取得元の不調を知らせるのには役立つが、不都合なら、`partial` を「書名が取れていない」だけにし、取得元のエラーは別に知らせる形に変える。
- 書影は、下書きを取得した時点で保存する。本として保存しなかった書影は孤立したファイルになる。整理はフェーズ 3。
- 要件書の修正案（0007 に加えて）: FR-META-02 の NDL（書影 API）の行を削除、書影の既定の優先順位から NDL を削除（FR-META-04 の表）、FR-CONF-02 に「キーがない取得元は有効にしても使わない」を追記。
- Windows のビルド（ring・keyring の windows-native）は、CI の windows-latest で未確認。
