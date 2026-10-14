-- 初期スキーマ（docs/requirements.md 8.2 をもとに docs/decisions/0003, 0004 の調整を加えたもの）。
-- マイグレーションランナーがトランザクションを張るので、BEGIN / COMMIT は書かない。

-- ===== 同期対象 =====
-- 他端末の Op は欠番ありで届くため、id 以外の NOT NULL 列にはすべて DEFAULT を付ける。
-- 作成 Op より先に部分更新 Op が届いても、INSERT OR IGNORE INTO t(id) VALUES (?) で行を作れるようにするため。
-- 同期対象テーブル間に外部キー制約は作らない（参照先がまだ届いていないことがある）。

CREATE TABLE books (
  id                 TEXT NOT NULL PRIMARY KEY,           -- UUIDv7
  isbn13             TEXT,                                -- ISBN のない本は NULL
  title              TEXT NOT NULL DEFAULT '',
  title_kana         TEXT,
  subtitle           TEXT,
  series_id          TEXT,
  volume             TEXT,                                -- 巻次（表示用）
  volume_sort        REAL,                                -- 巻次（並べ替え用）
  authors_json       TEXT NOT NULL DEFAULT '[]',          -- [{"name":"","kana":"","role":"著"}]
  publisher          TEXT,
  published_on       TEXT,                                -- YYYY / YYYY-MM / YYYY-MM-DD
  c_code             TEXT,                                -- 4 桁
  ndc                TEXT,
  list_price         INTEGER,                             -- 本体価格（税抜・円）
  ref_price_incl     INTEGER,                             -- 参考の税込価格（楽天など）
  pages              INTEGER,
  size_text          TEXT,                                -- 例: "15cm"
  language           TEXT,
  description        TEXT,
  cover_hash         TEXT,                                -- 書影の SHA-256
  ownership          TEXT NOT NULL DEFAULT 'owned',       -- owned / wanted / released
  wish_priority      TEXT,                                -- high / mid / low
  location_id        TEXT,
  tags_json          TEXT NOT NULL DEFAULT '[]',          -- ["研究/SiC", "小説"]
  read_status        TEXT NOT NULL DEFAULT 'unread',      -- unread / reading / read / paused
  read_started_on    TEXT,
  read_finished_on   TEXT,
  rating             INTEGER,                             -- 1〜5
  review             TEXT,
  purchased_on       TEXT,
  purchase_store     TEXT,
  purchase_price     INTEGER,                             -- 実際に払った額（税込・円）
  purchase_cond      TEXT,                                -- new / used
  purchase_memo      TEXT,
  memo               TEXT,
  field_sources_json TEXT NOT NULL DEFAULT '{}',          -- {"title":"openbd","list_price":"barcode"}
  manual_fields_json TEXT NOT NULL DEFAULT '[]',          -- 手で直した項目名
  meta_status        TEXT NOT NULL DEFAULT 'ok',          -- ok / pending / partial / failed
  extra_json         TEXT NOT NULL DEFAULT '{}',          -- 新しい版の端末から届いた未知の項目を保持
  deleted            INTEGER NOT NULL DEFAULT 0,          -- ゴミ箱
  purged             INTEGER NOT NULL DEFAULT 0,          -- 完全削除（行は残す）
  created_at         TEXT NOT NULL DEFAULT '',
  updated_at         TEXT NOT NULL DEFAULT ''             -- 端末ローカルの派生値（同期しない）
);
CREATE INDEX books_isbn13 ON books(isbn13);

CREATE TABLE series (
  id         TEXT NOT NULL PRIMARY KEY,
  name       TEXT NOT NULL DEFAULT '',
  name_kana  TEXT,
  memo       TEXT,
  extra_json TEXT NOT NULL DEFAULT '{}',
  deleted    INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE locations (
  id         TEXT NOT NULL PRIMARY KEY,
  name       TEXT NOT NULL DEFAULT '',
  parent_id  TEXT,
  sort_order REAL,
  extra_json TEXT NOT NULL DEFAULT '{}',
  deleted    INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT ''
);

-- name は UNIQUE にしない。2 台が同じ名前のタグを別 ID で作ると、取り込み時に制約違反で適用できなくなる。
-- 重複の扱いはフェーズ 1 で決める。
CREATE TABLE tags (
  id         TEXT NOT NULL PRIMARY KEY,
  name       TEXT NOT NULL DEFAULT '',
  color      TEXT,
  extra_json TEXT NOT NULL DEFAULT '{}',
  deleted    INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT ''
);
CREATE INDEX tags_name ON tags(name);

CREATE TABLE shelves (
  id         TEXT NOT NULL PRIMARY KEY,
  name       TEXT NOT NULL DEFAULT '',
  query_json TEXT NOT NULL DEFAULT '{}',
  sort_order REAL,
  extra_json TEXT NOT NULL DEFAULT '{}',
  deleted    INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT ''
);

-- ===== 同期の管理（端末ローカル） =====
CREATE TABLE field_clocks (                       -- 項目ごとの最終更新時刻（HLC）。field は Op 上の項目名
  entity TEXT NOT NULL, entity_id TEXT NOT NULL, field TEXT NOT NULL, hlc TEXT NOT NULL,
  PRIMARY KEY (entity, entity_id, field)
);
CREATE TABLE outbox (                             -- 自分の変更の記録。seq は Op の seq そのもの（アプリが明示的に入れる）
  seq INTEGER PRIMARY KEY, hlc TEXT NOT NULL, op_json TEXT NOT NULL, flushed INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE applied_segments (                   -- 取り込み済みの他端末のセグメント
  device_id TEXT NOT NULL, seq_from INTEGER NOT NULL, seq_to INTEGER NOT NULL,
  PRIMARY KEY (device_id, seq_from)
);
CREATE TABLE local_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- device_id, hlc_last, op_seq_last, 将来 library_id など

-- ===== 端末ローカルの補助 =====
CREATE TABLE provider_cache (
  provider TEXT NOT NULL, isbn13 TEXT NOT NULL, fetched_at TEXT NOT NULL,
  status TEXT NOT NULL, payload TEXT, PRIMARY KEY (provider, isbn13)
);

-- books_fts と book_tags（検索用の派生索引）は、検索と一緒にフェーズ 1 で設計する。
