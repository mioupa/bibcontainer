# 0005: フェーズ1の構成と、層の間の取り決め

## 状態

採用

## 日付

2026-10-14

## 背景

フェーズ1（ローカルで使える最小版）は、データ層・書誌取得・画面と範囲が広く、複数の作業を並行して進める。層の間で受け渡すデータの形と、処理をどこに置くかを先に決めておかないと、後でつなぐときに食い違う。あわせて、フェーズ0の後にユーザーと合意した 2 点（取得元・手修正の記録を項目単位で同期する、復元した DB には新しい端末 ID を振る）を記録する。

## 決定

### 1. 取得元と手修正の記録は、項目ごとに別の同期キーにする（ユーザー合意済み）

- 8.2 の案では `field_sources_json` と `manual_fields_json` を 1 つの値として同期するが、それだと端末 A で書名、端末 B で出版社を手で直したとき、片方の記録が LWW で消える（FR-SYNC-05 に反する）。
- Op のキーを項目ごとに分ける。
  - `field_sources.<項目名>`: 値は取得元の文字列（`barcode`、`openbd`、`ndl`、`google`、`manual` など）。null で消す。
  - `manual_fields.<項目名>`: 値は `true`。null で消す。
- `field_clocks` の `field` には、このドット付きのキーをそのまま入れる。項目ごとに時計を持つので、別の項目の記録はどちらも残る。
- 列は残す。`field_sources_json` はオブジェクト（`{"title":"openbd"}`）、`manual_fields_json` は項目名の配列（辞書順）で、どちらも Op から組み立てる保存形式である。
- 項目全体のキー（`field_sources`、`manual_fields`）は Op では使わない。

### 2. バックアップから復元した DB には、新しい端末 ID を振る（ユーザー合意済み）

- 復元前の端末が書き出した Op と、`(dev, seq)` が重ならないようにする。復元時に `device_id` を作り直し、seq のカウンタを 0 に戻す。
- 復元した DB の outbox に残る未書き出しの Op（古い端末 ID のもの）をどう扱うかは、フェーズ2で決める。

### 3. 処理の置き場所

| 処理 | 置き場所 | 理由 |
|---|---|---|
| バーコードの判定・検証・ISBN 変換・2段目の分解・Cコード表 | `crates/core` の `barcode` | 一元化する。ISBN 一覧の取り込みや検索でも使う |
| 2 段のペアリング（状態遷移とタイムアウト） | フロントエンドの純粋な reducer ＋フック | 残り秒数の表示、音、フォームと密に結びつくため。判定は IPC の `classify_code` で Rust に任せる |
| 書籍・シリーズ・保管場所・タグの操作、検索、一覧用の行、書き出し、バックアップ | `crates/core` の `library` など | 同期対象の変更は必ず `record_change` を通す |
| 書誌の取得・正規化・統合、書影の変換 | 新しいクレート `crates/metadata`（`bibcontainer-metadata`） | ネットワークと画像処理の依存をコアから切り離す。DB には触らない |
| 取得結果の保存、生の応答のキャッシュ、取得待ちの再取得、日次バックアップ、設定、キーチェーン | `src-tauri` | 各クレートをつなぐ層 |

- `crates/metadata` はコアに依存しない。取得元の生の応答を保存する場所は、トレイト（`RawCache` など）として外から渡す。`src-tauri` がコアの `provider_cache` テーブルで実装する。
- IPC で使う型は、`crates/core` と `crates/metadata` でも `#[derive(TS)]` を付けてよい（ts-rs は Tauri に依存しない）。

### 4. 一覧のデータの流れ

- 起動時に `list_books` で、全書籍の一覧用の行（`ListRow`）をフロントへ一度に渡す（完全削除したものは除く。ゴミ箱のものは含める）。サイドバーの絞り込み、並べ替え、件数の集計はフロントで行う。5,000〜20,000 冊なら、毎回 Rust に問い合わせるより速い。
- 検索は Rust で行う。`search_books(query)` は一致した ID の一覧を返し、フロントは表示中の絞り込みと掛け合わせる。
- データが変わったら、Rust はイベント `library-changed`（`{ books: string[], others: boolean }`）を出す。フロントは変わった行だけ `get_rows(ids)` で取り直す。`others` が true なら保管場所・タグ・シリーズの一覧も取り直す。
- 並べ替え用のキー（ヨミがあればヨミ、なければ書名を正規化したもの）は Rust で作り、`ListRow` に入れる。

`ListRow` の形（IPC、camelCase）:

```ts
type Author = { name: string; kana: string | null; role: string | null };
type ListRow = {
  id: string;
  isbn13: string | null;
  title: string;
  titleKana: string | null;
  subtitle: string | null;
  volume: string | null;
  volumeSort: number | null;
  seriesId: string | null;
  seriesName: string | null;
  authors: Author[];
  publisher: string | null;
  publishedOn: string | null;      // YYYY / YYYY-MM / YYYY-MM-DD
  cCode: string | null;            // 4 桁
  ndc: string | null;
  listPrice: number | null;        // 本体価格（税抜）
  pages: number | null;
  ownership: "owned" | "wanted" | "released";
  wishPriority: "high" | "mid" | "low" | null;
  locationId: string | null;
  tags: string[];
  readStatus: "unread" | "reading" | "read" | "paused";
  readFinishedOn: string | null;
  rating: number | null;
  purchasedOn: string | null;
  purchasePrice: number | null;
  purchaseStore: string | null;
  coverHash: string | null;
  metaStatus: "ok" | "pending" | "partial" | "failed";
  deleted: boolean;
  createdAt: string;
  sortTitle: string;               // 並べ替え用
  sortAuthors: string;             // 並べ替え用（先頭の著者）
};
```

### 5. バーコードの判定結果

`classify_code(raw)` が返す形（IPC）。ISBN-10 の入力は ISBN-13 に変換して `isbn13` として返す。

```ts
type ScanCode =
  | { kind: "isbn13"; raw: string; isbn13: string; isbn10: string | null }
  | { kind: "secondTier"; raw: string; cCode: string; listPrice: number | null }
  | { kind: "magazine"; raw: string }
  | { kind: "otherJan"; raw: string }
  | { kind: "invalid"; raw: string; reason: "checksum" | "format" };
```

Cコードの名称は `c_code_table()` で一度だけ取得し、フロントで引く。

### 6. 検索用の正規化と派生データ

- 正規化: NFKC → 英字の小文字化 → カタカナをひらがなに。検索語にも同じ処理をする。
- `books_fts`（FTS5、trigram）と `book_tags` など、検索と並べ替えのための派生データは、端末ローカルのテーブルに持つ（同期しない）。`apply_op` で書籍・シリーズが変わったときに更新するので、他端末の Op を取り込んだときも最新になる。
- 3 文字以上の検索語は FTS5 で、2 文字以下は LIKE で探す。空白で区切った語はすべて満たすもの（AND）を返す。ISBN はハイフンを除き、ISBN-10 は ISBN-13 に直して照合する。

### 7. 書影

- 書影の本体は `app_local_data_dir/covers/<先頭2文字>/<SHA-256>.jpg` に置く（同期フォルダへの書き出しはフェーズ2）。
- サムネイル（長辺 160px・320px）はキャッシュフォルダに、必要になったときに作る。→ 2026-10-15 に、一覧用の 80px を足した（一覧は 80、詳細ペインとフォームは 320。0010 §14）。
- WebView へは独自の URI スキーム `cover` で渡す（例: macOS は `cover://localhost/<hash>?size=160`、Windows は `http://cover.localhost/<hash>?size=160`）。URL はフロントの `coverUrl(hash, size)` で組み立て、CSP の `img-src` に加える。
- 書影がないときの仮の表紙（書名と著者を描いたもの）は、フロントのコンポーネントで描く。

### 8. 設定と秘密情報

- 端末ごとの設定は、コアの DB に新しく作る `local_settings` テーブル（キー → JSON）に置く。同期しない。
- API キー（Google Books など）は keyring クレートで OS のキーチェーンに置く。DB・ログ・設定には書かない。
- User-Agent の連絡先は、既定で `https://github.com/mioupa/bibcontainer` とする（設定で変えられる）。

### 9. そのほか

- 通知音は Web Audio API で合成する（音声ファイルを同梱しない）。
- 取得待ちの本の再取得は Rust 側の定期処理で行う。取得待ちの本がある間は 5 分ごとと、ウィンドウが前面に来たときに試す。
- 日次バックアップは、起動時と 1 時間ごとに「前回から 24 時間以上たっているか」を確かめて取る。

## 理由

- 項目単位の同期キーにすると、既存の LWW の仕組み（`field_clocks`）をそのまま使える。保存形式の列を残すので、表示や書き出しのコードは単純なままにできる。
- ペアリングを Rust に置くと、残り秒数の表示や音のために状態をイベントで頻繁に送ることになる。判定だけを Rust に置き、状態遷移は純粋な関数にしてテストする方が単純である。
- 一覧をフロントに丸ごと持つと、絞り込みと並べ替えに IPC の往復がなくなり、性能目標（検索 100ms、スクロール 60fps）を満たしやすい。

## 影響・今後

- 要件書 8.2 のスキーマ案（`field_sources_json`、`manual_fields_json`）の記述は、本記録の内容に合わせて修正を提案する。
- フェーズ2では、書影を同期フォルダにも書き出し、そこからも読めるようにする。`cover` スキームの実装をその前提で作る。
