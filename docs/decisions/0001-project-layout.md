# 0001: プロジェクトの構成（ワークスペース・フロントの配置・Tauri のバージョン）

## 状態

採用

## 日付

2026-10-14

## 背景

要件書（`docs/requirements.md` の 5 章）では、アプリ基盤に Tauri 2.x、バックエンドに Rust、フロントエンドに React + TypeScript + Vite を使うと決めている。DB・同期エンジン・書誌取得といった中心の処理は Rust 側に置く。将来は iOS／Android への展開も視野に入れている。

フェーズ 0 では、これらをどのディレクトリにどう並べるかを先に固めておく必要があった。なお crates.io の `tauri` の最新版は 3.0.0-alpha 系で、まだ安定版ではない。

## 決定

- **Cargo ワークスペース**を作る（ルートの `Cargo.toml`、`resolver = "3"`、`members = ["crates/*", "src-tauri"]`）。`target/` と `Cargo.lock` はルートで共有する。
  - `crates/core`（`bibcontainer-core`）: Tauri に依存しないコアライブラリ。DB、マイグレーション、HLC、変更ログなど。
  - `src-tauri`（`bibcontainer-app`）: アプリの殻。ウィンドウ、IPC コマンド、ログの初期化だけを持ち、実際の処理は core を呼ぶ。
- **フロントエンドはリポジトリ直下**に置く（`package.json`、`index.html`、`vite.config.ts`、`src/`）。Rust のアプリ本体は `src-tauri/`。create-tauri-app の標準構成に合わせた。
- **Tauri は 2.x を使う**。`tauri = "2"`、`tauri-build = "2"`、`@tauri-apps/api` と `@tauri-apps/cli` は `^2`。3.x の alpha は使わない。
- フロントは React 19 + TypeScript（strict）+ Vite + Tailwind CSS v4 + TanStack Query。パスエイリアスは `@/` → `src/`。UI の文字列は `src/i18n/ja.ts` に集める。shadcn/ui、Zustand、TanStack Table などは、必要になったフェーズ 1 で入れる。
- TypeScript は 6.0 系に固定する（`~6.0.3`）。typescript-eslint が TypeScript 6.1 未満までしか対応していないため。
- 品質ツールは、Rust が rustfmt（既定）と clippy（`-D warnings`）、フロントが ESLint（flat config）・Prettier・Vitest。
- `package.json` の scripts は `dev`、`build`、`preview`、`tauri`、`lint`、`format`、`format:check`、`typecheck`、`test`、`check`。`check` は上記すべてと `cargo fmt`・`cargo clippy`・`cargo test` を順に実行する。
- CI は GitHub Actions で、macOS（arm64）と Windows（x64）の両方で検査とビルドを行い、生成物をアップロードする。

## 理由

コアを Tauri から分ける理由は 3 つある。

1. Tauri や WebView をビルドしなくても `cargo test -p bibcontainer-core` だけで速くテストできる。同期エンジンの性質テストのように、大量の試行を回すテストと相性がよい。
2. 業務ロジックと、ウィンドウや IPC といったアプリ固有の配線が、型の上でも分かれる。
3. 将来のモバイル版や、同期だけを行うツールでも同じコアを使い回せる。

Tauri 3.x は alpha で、API が変わる可能性が高い。要件書も 2.x を前提にしている。

## 影響・今後

- `crates/core` に Tauri の型や API を持ち込まない。IPC でやり取りする型は、アプリ側で薄く包むか、ts-rs の対象にする（`0002-ipc-type-generation.md`）。
- Tauri 3 が安定したら、移行するかどうかを別の決定記録で判断する。
- アイコンは `src-tauri/app-icon.svg` を原画にして `pnpm tauri icon src-tauri/app-icon.svg` で作る。モバイル用と Windows ストア用のアイコンは今は使わないので、リポジトリには入れていない。
- 配布物には署名も公証もしていない。配布の方法はフェーズ 4 で決める。
- DB は `app_local_data_dir()` の `library.sqlite3`、移行前のバックアップは同じ場所の `backups/` に置く。macOS では `app_data_dir()` と同じ場所（`~/Library/Application Support/io.github.mioupa.bibcontainer/`）。Windows では `app_data_dir()` が Roaming（`%APPDATA%`）になるため、端末ローカルの `%LOCALAPPDATA%` を使う側を選んだ。
- CSP は `src-tauri/tauri.conf.json` に明示している（`default-src 'self'`、画像は `data:`・`blob:`・`asset:` を許可、IPC 用に `ipc:` と `http://ipc.localhost` を許可）。外部の画像や API をフロントから直接読み込むことはしない（通信は Rust 側で行う）。必要が出たら、ここに追記して広げる。
- ルートの `Cargo.toml` の `[profile.release.build-override] strip = "none"` は、macOS 27 + Xcode 26 beta の環境で proc-macro の dylib が壊れる問題を避けるための設定。正式版のツールチェーンで問題が出なくなったら外してよい。
