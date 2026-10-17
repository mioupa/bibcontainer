-- books に work_id（作品の手動の指定）を足す。
--
-- NULL は自動（一覧の巻のまとめを、書名と著者から決める）、'none' は作品に入れない、
-- それ以外は手動でまとめた作品の ID（同じ ID の本が 1 つの作品になる）。同期する項目で、
-- 検索には使わない。
-- マイグレーションランナーがトランザクションを張るので、BEGIN / COMMIT は書かない。

ALTER TABLE books ADD COLUMN work_id TEXT;

-- この項目を知らない版のアプリは、届いた work_id を未知の項目として extra_json に入れている。
-- その値を列へ移す。時計は field_clocks に work_id の名前で入っているので、そのまま使う
-- （docs/decisions/0004）。文字列か null のものだけ移し、ほかの型の値は extra_json に残す。
UPDATE books
   SET work_id = json_extract(extra_json, '$.work_id'),
       extra_json = json_remove(extra_json, '$.work_id')
 WHERE CASE WHEN json_valid(extra_json)
            THEN json_type(extra_json, '$.work_id') IN ('text', 'null')
            ELSE 0 END;
