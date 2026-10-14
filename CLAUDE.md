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
既存の記録を読んでから作業すること（0001 構成、0002 IPC の型、0003 DB、0004 HLC と Op）。

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
- `crates/core`（`bibcontainer-core`）: Tauri に依存しないコア。DB・マイグレーション・HLC・Op。
- `src-tauri`（`bibcontainer-app`）: アプリの殻。ウィンドウ、IPC コマンド（`commands.rs`）、DB 接続（`database.rs`）、ログ。
- `src`: React フロントエンド。パスエイリアスは `@/` → `src/`。
- `src/bindings/`: ts-rs の生成物。手で編集しない。コミットする。
- フロントから Rust を呼ぶのは `src/lib/ipc.ts` だけ。コンポーネントから `invoke` を直接呼ばない。
- UI の文字列は `src/i18n/ja.ts` に置く。コンポーネントに日本語を直接書かない。
- IPC のエラーは `{ kind, message }`（`AppError`）。DB に触るコマンドは async にして `Database::run` 経由で呼ぶ。

## 開発コマンド
- `pnpm install`: 依存を入れる。
- `pnpm tauri dev`: アプリを開発モードで起動する。
- `pnpm check`: lint・format・typecheck・Vitest と、`cargo fmt`・`clippy -D warnings`・`cargo test --workspace` をまとめて実行する。コミット前に通す。
- `cargo test -p bibcontainer-core`: コアだけを速くテストする。
- bindings の再生成: `cargo test --workspace`（`#[ts(export)]` の型が `src/bindings/` に出る）。差分が出たらコミットする。CI は差分があると落ちる。
- `pnpm tauri build --bundles app`: macOS の .app を作る（`target/release/bundle/macos/`）。
- `pnpm format`: Prettier で整形する。

## データの場所（macOS）
- DB: `~/Library/Application Support/io.github.mioupa.bibcontainer/library.sqlite3`（`backups/` に移行前のバックアップ）
- ログ: `~/Library/Logs/io.github.mioupa.bibcontainer/`（日次、14 ファイルまで）
- Windows の DB は `%LOCALAPPDATA%\io.github.mioupa.bibcontainer\`、ログはその下の `logs\`。

## この Mac での注意
- macOS 27 + Xcode beta では、リリースビルドで rustc の strip が proc-macro の dylib を壊す。ルートの `Cargo.toml` の `[profile.release.build-override] strip = "none"` が回避策なので、消さない（0001 参照）。
- TypeScript は 6.0 系に固定している（typescript-eslint が 6.1 未満まで）。上げないこと。
