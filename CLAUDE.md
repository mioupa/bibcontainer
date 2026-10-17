# bibcontainer

蔵書管理アプリ（Tauri 2 + Rust + React/TypeScript）。バーコードで登録でき、書誌情報を自動取得し、複数端末（Mac・Windows）の蔵書を Google Drive 上のフォルダ経由で同期する。

## 要件書
`docs/requirements.md`。長いので `@` でインポートせず、必要な節だけ読むこと。
- 0 章 作業指示（最初に読む） / 7 章 機能要件 / 8 章 データ設計（スキーマ、同期フォルダ、Op の形式）
- 9 章 画面要件 / 10 章 非機能要件 / 11 章 外部 API メモ / 12 章 開発フェーズと受け入れ基準
- 13 章 未決事項 / 付録 A Cコード表 / 付録 B バーコードのテスト用データ

作業は 12 章のフェーズ順に進め、受け入れ基準をテストで確認してから次へ進む。

## 決定記録
`docs/decisions/NNNN-*.md`。見出しは 状態／日付／背景／決定／理由／影響・今後。
設計上の判断や要件書との差異はここに書く。要件書の修正が必要なときは、直接直さずユーザーに提案する。
既存の記録を読んでから作業すること。
- 0001 構成、0002 IPC の型とエラー、0003 DB、0004 HLC と Op
- 0005 フェーズ 1 の層の取り決め（一覧の流れ、`library-changed`、バーコードの判定、設定、書影）
- 0006 コアのデータ層（書籍・保管場所・タグ・シリーズの操作、検索、書き出し、バックアップ）
- 0007 書誌取得（取得元の仕様と統合の規則）、0008 フロントの基盤（UI 部品、テーマ、一覧、スキャン入力）
- 0009 Tauri との統合（コマンドの一覧、イベント、設定のセクション、エラーの種類、定期バックアップ、復元）
- 0010 書誌取得のアプリへの統合（取得元の設定、API キー、キャッシュ、下書き、再取得、`cover` スキーム。組み込みは 0009 §11）
- 0011 メイン画面と登録・編集フォーム（保存の規則、候補、書影の差し替え、検索欄の ISBN、ショートカット）
- 0012 スキャン登録画面と設定画面（ペアリングの効果の処理、重複と欲しい本、連続登録、ISBN 一覧の取り込み）
- 0013 フェーズ 2 の同期の設計（同期フォルダの構成、セグメント、スナップショット、設定の流れ）、0014 同期エンジン（`crates/sync` の API、DB の操作、取り込み済みの範囲、組み込みの手順は §11）
- 0015 同期の画面（上部バーの表示、設定の「同期」の節、モック）、0016 同期のアプリへの組み込み（いつ動かすか、状態の組み立て、設定・置き換え・再接続・復元の流れ、スレッドとロック）
- 0017 Open Library の取得元（Google Books の ISBN 検索の障害への対応。検索 API → 版の JSON、書影 ID の URL、優先順位は最後）、0018 Google Books の ISBN 検索の回避策（Dynamic Links → `volumes/<ID>`）、0019 洋書の著者名（NDL の `foaf:name`、`名 姓` の形、英語の責任表示からの役割。0007 §4 を一部改める）
- 0020 統合の優先順位を ISBN の国番号で 2 組に（978-4 と ISBN なしは和書、それ以外は洋書など。設定の `priorities` は `{ japanese, foreign }`）
- 0021 実機で見つかった取得結果の誤りの修正（巻次の製本の種類・レーベル番号、再取得で古い取得元の値を消す `clear_stale`、書影の縦横比、大文字だけの著者名）
- 0022 フェーズ 3 の範囲・順番・進め方（楽天 → キーワード検索 → 一括再取得 → グリッド → 巻をまとめた表示 → CSV 取り込み → カメラ。元に戻す・スマート本棚・集計はあと回し。AT は各セッションで案を作って承認を得る）
- 画面どうしの受け渡しは `src/stores/screenStore.ts`（画面の切り替え）と `src/stores/editorStore.ts`（フォームを開く）だけで行う

## 守ること
- 次の 3 点は設計の根幹。変えたいときは実装前にユーザーに確認する。
  - SQLite を同期フォルダに置かない。
  - すべてのデータ変更を Op として記録する。
  - 秘密情報（API キーなど）をソース・リポジトリ・同期フォルダに置かない。
- 同期対象テーブル（books / series / locations / tags / shelves）は直接 UPDATE せず、必ず `store::record_change`（1 件だけなら `record_change_committed`）で変更する。
- 他端末の Op を取り込むときは `observe_hlc` を呼ぶ（呼ばないと次のローカル変更が `ClockBehind` になる）。
- 適用済みのマイグレーションは編集しない。`crates/core/migrations/` に新しい番号のファイルを足し、`MIGRATIONS` に登録する。
- 外部 API の仕様は、実装前に公式ドキュメントで確認する。
- 仕様が曖昧なときは、推測で機能を足さずユーザーに質問する。

## コミット・push と指示の扱い（2026-10-16 にユーザーが許可）
- ローカルコミットと main への push は、作業の区切りごとに、確認なしでしてよい。`pnpm check` を通してから行う。force push・履歴の書き換えはしない。push したら CI（macOS・Windows）の結果を確かめ、落ちたら直す。
- ユーザーが渡す作業の指示文は、実装やデータと食い違う点、もっとよい方法が分かった点を、必要に応じて変えて進めてよい。変えた点と理由は、報告と決定記録に書く。
- 要件書（`docs/requirements.md`）は、これまでどおり直接直さず、修正案をユーザーに見せてから反映する。

## 構成
- `crates/core`（`bibcontainer-core`）: Tauri に依存しないコア。DB・マイグレーション・HLC・Op、書籍・保管場所・タグ・シリーズの操作、検索、書き出し、バックアップ、バーコードの判定。
- `crates/sync`（`bibcontainer-sync`）: 同期エンジン。Tauri に依存しない。同期フォルダ（`SyncTransport`。初版は `LocalFolder`）への Op・スナップショット・書影の書き出しと取り込み、初めての設定・解除、手入れ。DB には `SyncDb` 経由で短く触る。画面とやり取りする型（`SyncStatus` など）は `src/bindings/` に出る。
- `crates/metadata`（`bibcontainer-metadata`）: 書誌の取得・正規化・統合、書影の変換。DB には触らない。
- `src-tauri`（`bibcontainer-app`）: アプリの殻。ウィンドウ、ログ、DB 接続（`database.rs`。復元の再オープンもここ）、IPC コマンド（`commands/` に機能ごと）、イベント（`events.rs`）、端末ごとの設定（`settings.rs`）、日次バックアップ（`backups.rs`）、書誌取得のつなぎ（`src-tauri/src/metadata/`。取得元の設定、API キー（OS のキーチェーン）、生の応答のキャッシュ、登録フォームの下書き、取得待ちの自動再取得、書影の取り込みと `cover` URI スキーム）、同期のつなぎ（`src-tauri/src/sync/`。`SyncController` が、取り込み・書き出し・手入れを動かす時機（起動時・ウィンドウが前面に来たとき・ファイル監視・60 秒ごと・最後の変更の 5 秒後・終了時・復元のあと）、状態（`sync-status`）、設定・解除・置き換え・再接続、端末名、書影の整理を受け持つ。同期フォルダの場所と端末名は `local_settings` の `sync`）。
- `src`: React フロントエンド。パスエイリアスは `@/` → `src/`。
- `src/bindings/`: ts-rs の生成物。手で編集しない。コミットする。
- フロントから Rust を呼ぶのは `src/lib/ipc.ts` だけ。コンポーネントから `invoke` を直接呼ばない。
- UI の文字列は `src/i18n/ja/` に機能ごとのファイル（`common`・`list`・`scan`・`edit`・`settings`・`errors`）で置き、`src/i18n/ja.ts` が 1 つの `ja` にまとめる。コンポーネントに日本語を直接書かない。
- IPC のエラーは `{ kind, message }`（`AppError`。種類は 12。0009 の 1）。DB に触るコマンドは async にして `Database::run` 経由で呼ぶ。
- データを変えるコマンドは、成功したあとに `events::emit_library_changed` を呼ぶ（同期の書き出しの合図も兼ねる）。他の端末から取り込んだ変更を知らせるときは `emit_remote_library_changed`（合図を送らない）。同期で書影があとから届いたときは `covers-arrived`（`{ hashes }`。画面は `BookCover` が、版つきの URL で読み込み直す）。フロントは `library-changed` を受けて、変わった行だけ取り直す（`src/features/library/sync.ts`）。
- フロントのデータの取得・変更は `src/features/library/api.ts` を通す。同期は `src/features/sync/api.ts`（どちらを使うかは `selectBackend()` の 1 か所）。どちらも、Tauri の外（ブラウザの `pnpm dev`）では、モック（`src/dev/`）で動く。

## 開発コマンド
- `pnpm install`: 依存を入れる。
- `pnpm tauri dev`: アプリを開発モードで起動する。
- `pnpm check`: lint・format・typecheck・Vitest と、`cargo fmt`・`clippy -D warnings`・`cargo test --workspace` をまとめて実行する。コミット前に通す。
- CI（`.github/workflows/ci.yml`）: 検査・テストの `check` と、アプリのビルド（成果物にインストーラー）の `build` が、macOS・Windows で並行に走る。`docs/` と Markdown だけの変更では走らない（その書式はローカルの `pnpm check` で確かめる）。
- `cargo test -p bibcontainer-core`: コアだけを速くテストする。
- bindings の再生成: `cargo test --workspace`（`#[ts(export)]` の型が `src/bindings/` に出る）。差分が出たらコミットする。CI は差分があると落ちる。
- `pnpm tauri build --bundles app`: macOS の .app を作る（`target/release/bundle/macos/`）。
- `pnpm format`: Prettier で整形する。

## データの場所（macOS）
- DB: `~/Library/Application Support/io.github.mioupa.bibcontainer/library.sqlite3`（`backups/` に移行前・日次・週次・手動のバックアップ。保存先は設定で変えられる）
- ログ: `~/Library/Logs/io.github.mioupa.bibcontainer/`（日次、14 ファイルまで）
- Windows の DB は `%LOCALAPPDATA%\io.github.mioupa.bibcontainer\`、ログはその下の `logs\`。

## この Mac での注意
- macOS 27 + Xcode beta では、リリースビルドで rustc の strip が proc-macro の dylib を壊す。ルートの `Cargo.toml` の `[profile.release.build-override] strip = "none"` が回避策なので、消さない（0001 参照）。
- TypeScript は 6.0 系に固定している（typescript-eslint が 6.1 未満まで）。上げないこと。
- Rust は `rust-toolchain.toml` で版を固定している（手元と CI で clippy の lint をそろえるため）。上げるときは `.github/workflows/ci.yml` の `dtolnay/rust-toolchain` の版（`check` と `build` の 2 か所）も合わせる。

## Windows で落ちやすいテスト（手元の Mac では気づけない。CI で確かめる）
- 絶対パスを書くときは `/tmp/...` などではなく `std::env::temp_dir()` や `tempfile` を使う（Windows はドライブ名がないと絶対パスにならない）。
- IPC のテストの呼び出し元 URL は、Windows では `http://tauri.localhost`（macOS は `tauri://localhost`）。
- `src-tauri/build.rs` はマニフェストを全ターゲットに埋め込んでいる（ダイアログのプラグインのため。無いとテストの実行ファイルが `STATUS_ENTRYPOINT_NOT_FOUND` で起動しない）。消さない。
- ネットワークの時間を測るテストは、Windows の TCP が接続拒否のあとも数回やり直す（約 2 秒）ことを考える。
- ファイル監視（`notify`）しているフォルダの親は、Windows では名前を変えられない。同期のテストで同期フォルダを動かすときは、先に `SyncController::drop_watcher`（テスト専用）で監視をやめる。`chmod` を使うテストは `#[cfg(unix)]`。
