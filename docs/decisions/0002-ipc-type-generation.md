# 0002: IPC の型を ts-rs で生成する

## 状態

採用

## 日付

2026-10-14

## 背景

Rust のコマンドが受け取る引数と返す値の型を、TypeScript 側にも手で書くと、片方だけ変えて食い違う事故が起きる。要件書（5 章）は「tauri-specta または ts-rs で Rust の型から TypeScript の型を自動生成する」としていた。

調べた時点（2026 年 10 月）では、tauri-specta は Tauri 2 対応版が 2.0.0-rc.25 のままで、安定版が出ていない。

## 決定

- **ts-rs 12 系**を使う。IPC でやり取りする構造体には `#[derive(TS)]` と `#[ts(export)]` を付ける。
- 出力先は `src/bindings/`。リポジトリ直下の `.cargo/config.toml` で `TS_RS_EXPORT_DIR = "src/bindings"` を指定し、`cargo test` を実行すると書き出される。あわせて `TS_RS_LARGE_INT = "number"` とし、`i64`・`u64` は `bigint` ではなく `number` で出力する（JSON では number で届くため）。
- 生成されたファイルは**コミットする**。手で編集しない。ESLint と Prettier の対象外にする。
- フロントエンドからは `src/lib/ipc.ts` の型付きラッパー（`ipc.appInfo()` など）だけを使い、コンポーネントから `invoke` を直接呼ばない。コマンド名ごとの引数と戻り値の型は、ここの `CommandMap` で対応づける。
- コマンドを増やす手順は次のとおり。
  1. Rust 側で型を作って `#[ts(export)]` を付け、`#[tauri::command]` の関数を書く。
  2. `lib.rs` の `generate_handler!` に登録する。
  3. `cargo test --workspace` で `src/bindings/` を更新し、差分をコミットする。
  4. `ipc.ts` の `CommandMap` と `ipc` にエントリを足す。
  - DB に触るコマンドは async にして、`Database::run` 経由で呼ぶ（SQLite の呼び出しはブロックするので、メインスレッドでは実行しない）。
- **フィールド名は camelCase** にする。IPC に使う構造体には `#[serde(rename_all = "camelCase")]` を付け、ts-rs もそれに従う（例: `AppInfo.deviceId`）。
- **コマンドのエラーは `{ kind, message }`** で返す。
  - Rust 側は `thiserror` の `AppError`（`DbUnavailable`、`SchemaTooNew`、`Database`、`Internal`）で、`Serialize` を手書きして `{ kind, message }` にする。TypeScript には `AppError`（`src/bindings/AppError.ts`）と `ErrorKind`（`"db_unavailable" | "schema_too_new" | "database" | "internal"`）が出る。（フェーズ 1 で、`validation`・`not_found`・`conflict`・`invalid_backup`・`network`・`provider`・`io`・`keychain` を足して 12 種類になった。0009 の 1 を参照。）
  - `kind` は画面の出し分け用、`message` は Rust 側が日本語で用意した原因の説明。
  - `ipc.ts` は invoke の失敗を `IpcError`（`kind` と `message` を持つ `Error`）に変換して投げる。`{ kind, message }` の形でない失敗（未登録のコマンドなど）は `kind = "internal"` にする。
  - エラーの種類を増やすときは `ErrorKind` に足す。`src/i18n/ja.ts` の `errorScreen.hints` は `ErrorKind` の全種類を要求するので、案内文を書かないと型エラーになる。
- **CI で生成物の差分を検出する**。`cargo test --workspace` の前に `src/bindings/*.ts` をいったん消し、後で `git status --porcelain -- src/bindings` が空であることを確かめる。コミット漏れ、手での編集、Rust で消した型のファイルの残りを検出できる。

## 理由

- tauri-specta は RC のままで、API が変わる可能性がある。コマンドの登録方法にも入り込むため、後から外しにくい。
- ts-rs は「型を TypeScript に書き出す」だけに役割が絞られている。Tauri のバージョンに依存せず、コアライブラリ（Tauri 非依存）の型にもそのまま使える。
- 型は自動生成され、`ipc.ts` が生成された型を参照する。そのため引数や戻り値のフィールドが食い違えば `pnpm typecheck` で見つかる。

## 影響・今後

- コマンド名の文字列（`"app_info"` など）は `ipc.ts` に手で書く。Rust 側の関数名との一致は、型からは検査されない。コマンドが増えたら、一致を確かめるテストを足すかどうかを考える。
- エラーの `kind` が増えて、種類ごとに追加のデータ（たとえば `SchemaTooNew` の `found` と `supported`）をフロントで使いたくなったら、`AppError` に項目を足す形に広げる。今は `message` の文字列だけで足りている。
- tauri-specta が安定版になり、コマンド名まで含めて生成したくなったら、置き換えを検討する。`src/bindings/` と `ipc.ts` の境界を保っておけば、影響はこの 2 か所に収まる。
