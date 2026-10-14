-- field_clocks を WITHOUT ROWID のテーブルに作り直す（docs/decisions/0006 §11）。
--
-- 通常のテーブルは、行本体（rowid をキーにした B 木）と、主キーの索引（別の B 木）の両方に
-- (entity, entity_id, field) を持つ。field_clocks は 1 冊あたり数十行あり、5,000 冊で約 25 万行になるので、
-- 同じキーを 2 回持つ分が DB の大きさの約 3 割を占めていた。WITHOUT ROWID にすると、主キーの B 木が
-- 行本体を兼ねるので、キーは 1 回だけになる。
--
-- 列・主キー・値は変えない（同期の形式、Op、HLC の比較にも触れない）。新しいテーブルを作り、
-- 主キーの順に写し、古いものを消して名前を付け替える。古いテーブルの領域は DB ファイルの空きページに
-- なるので、ランナーが、空きが大きいときにマイグレーションのあとで VACUUM する（migrate.rs）。
-- マイグレーションランナーがトランザクションを張るので、BEGIN / COMMIT は書かない。

CREATE TABLE field_clocks_new (
  entity TEXT NOT NULL, entity_id TEXT NOT NULL, field TEXT NOT NULL, hlc TEXT NOT NULL,
  PRIMARY KEY (entity, entity_id, field)
) WITHOUT ROWID;

-- 主キーの順に入れると、ページがほぼ満杯に詰まる。
INSERT INTO field_clocks_new (entity, entity_id, field, hlc)
  SELECT entity, entity_id, field, hlc FROM field_clocks ORDER BY entity, entity_id, field;

DROP TABLE field_clocks;
ALTER TABLE field_clocks_new RENAME TO field_clocks;
