-- フェーズ 1: 端末ごとの設定と、検索・並べ替え用の派生データ（docs/decisions/0005, 0006）。
-- どちらも端末ローカルで、同期しない。
-- 派生データは、同期対象テーブル（books / series）の内容から Rust 側（derived.rs）が作る。
-- マイグレーションでは入れ物だけを作り、中身はマイグレーション後の rebuild_derived で埋める。

CREATE TABLE local_settings (                     -- 端末ごとの設定（列の配置、取得元の設定など）
  key TEXT NOT NULL PRIMARY KEY, value_json TEXT NOT NULL
);

-- 派生データの管理表。書籍 1 冊につき 1 行。idx が books_fts の rowid になる。
CREATE TABLE book_idx (
  idx          INTEGER PRIMARY KEY,
  book_id      TEXT NOT NULL UNIQUE,
  sort_title   TEXT NOT NULL,                     -- ヨミがあればヨミ、なければ書名を正規化したもの
  sort_authors TEXT NOT NULL                      -- 先頭の著者のヨミ（なければ名前）を正規化したもの
);

-- tags_json から作る索引（タグの件数、名前の変更で使う）。
CREATE TABLE book_tags (
  book_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (book_id, tag)
) WITHOUT ROWID;
CREATE INDEX book_tags_tag ON book_tags(tag);

-- 全文検索。正規化（NFKC → 小文字 → カタカナをひらがな）した文字列を入れる。
-- 3 文字以上の検索語は trigram で、2 文字以下は同じ列に対する LIKE で探す。
CREATE VIRTUAL TABLE books_fts USING fts5(
  title, title_kana, authors, publisher, series, tags, memo, isbn,
  tokenize = 'trigram'
);

-- 検索・一覧・重複チェックで引く外部キー相当の列に索引を足す。
CREATE INDEX books_series_id ON books(series_id);
CREATE INDEX books_location_id ON books(location_id);

-- バックアップから復元したとき、古い端末 ID で outbox に残っていた未書き出しの Op の退避先。
-- 新しい端末 ID では seq が 1 から振り直されるので、outbox（seq が主キー）には残せない。
-- 扱いはフェーズ 2 で決める（docs/decisions/0006）。
CREATE TABLE outbox_restored (
  dev TEXT NOT NULL, seq INTEGER NOT NULL, hlc TEXT NOT NULL, op_json TEXT NOT NULL,
  PRIMARY KEY (dev, seq)
);
