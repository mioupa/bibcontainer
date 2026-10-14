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

## 構成
- `crates/core`（`bibcontainer-core`）: Tauri に依存しないコア。DB・マイグレーション・HLC・Op、書籍・保管場所・タグ・シリーズの操作、検索、書き出し、バックアップ、バーコードの判定。
- `crates/metadata`（`bibcontainer-metadata`）: 書誌の取得・正規化・統合、書影の変換。DB には触らない。
- `src-tauri`（`bibcontainer-app`）: アプリの殻。ウィンドウ、ログ、DB 接続（`database.rs`。復元の再オープンもここ）、IPC コマンド（`commands/` に機能ごと）、イベント（`events.rs`）、端末ごとの設定（`settings.rs`）、日次バックアップ（`backups.rs`）、書誌取得のつなぎ（`src-tauri/src/metadata/`。取得元の設定、API キー（OS のキーチェーン）、生の応答のキャッシュ、登録フォームの下書き、取得待ちの自動再取得、書影の取り込みと `cover` URI スキーム）。
- `src`: React フロントエンド。パスエイリアスは `@/` → `src/`。
- `src/bindings/`: ts-rs の生成物。手で編集しない。コミットする。
- フロントから Rust を呼ぶのは `src/lib/ipc.ts` だけ。コンポーネントから `invoke` を直接呼ばない。
- UI の文字列は `src/i18n/ja/` に機能ごとのファイル（`common`・`list`・`scan`・`edit`・`settings`・`errors`）で置き、`src/i18n/ja.ts` が 1 つの `ja` にまとめる。コンポーネントに日本語を直接書かない。
- IPC のエラーは `{ kind, message }`（`AppError`。種類は 12。0009 の 1）。DB に触るコマンドは async にして `Database::run` 経由で呼ぶ。
- データを変えるコマンドは、成功したあとに `events::emit_library_changed` を呼ぶ。フロントは `library-changed` を受けて、変わった行だけ取り直す（`src/features/library/sync.ts`）。
- フロントのデータの取得・変更は `src/features/library/api.ts` を通す。Tauri の外（ブラウザの `pnpm dev`）では、モック（`src/dev/`）で動く。

## 開発コマンド
- `pnpm install`: 依存を入れる。
- `pnpm tauri dev`: アプリを開発モードで起動する。
- `pnpm check`: lint・format・typecheck・Vitest と、`cargo fmt`・`clippy -D warnings`・`cargo test --workspace` をまとめて実行する。コミット前に通す。
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
- Rust は `rust-toolchain.toml` で版を固定している（手元と CI で clippy の lint をそろえるため）。上げるときは `.github/workflows/ci.yml` の `dtolnay/rust-toolchain` の版も合わせる。
