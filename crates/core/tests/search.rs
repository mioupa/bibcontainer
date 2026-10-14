//! 検索用の派生データ（books_fts など）と、`search_books` のテスト。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod common;
mod lib_common;

use bibcontainer_core::library::{self, FieldChange, UpdateOptions};
use bibcontainer_core::{
    LATEST_VERSION, MIGRATIONS, derived, normalize_for_search, open_with_migrations, rows,
    search_books,
};
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;

use lib_common::*;

fn search(fx: &Fx, q: &str) -> Vec<String> {
    search_books(&fx.conn, q).unwrap()
}

fn fts_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM books_fts", [], |r| r.get(0))
        .unwrap()
}

fn idx_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM book_idx", [], |r| r.get(0))
        .unwrap()
}

/// 書名・著者・出版社などを持つ本を数冊登録した DB。
fn library_fixture() -> (Fx, Vec<String>) {
    let mut fx = Fx::new();
    let sosekis = add(
        &mut fx,
        &[
            ("title", json!("吾輩は猫である")),
            ("title_kana", json!("ワガハイハネコデアル")),
            (
                "authors",
                json!([{"name": "夏目漱石", "kana": "ナツメソウセキ", "role": "著"}]),
            ),
            ("publisher", json!("岩波書店")),
            ("isbn13", json!("9784003101117")),
            ("tags", json!(["小説", "研究/SiC"])),
            ("memo", json!("Kindle 版もある。100% 好き_です")),
        ],
    );
    let rust = add(
        &mut fx,
        &[
            ("title", json!("プログラミング Rust")),
            ("subtitle", json!("Fast, Safe Systems Development")),
            (
                "authors",
                json!([{"name": "Jim Blandy"}, {"name": "Jason Orendorff", "role": "著"}]),
            ),
            ("publisher", json!("オライリー・ジャパン")),
            ("isbn13", json!("9784873119786")),
            ("tags", json!(["技術"])),
        ],
    );
    let fullwidth = add(
        &mut fx,
        &[
            ("title", json!("ＡＢＣ入門")),
            ("publisher", json!("ｶﾀｶﾅ出版")),
            ("isbn13", json!("9784003101117")),
        ],
    );
    (fx, vec![sosekis, rust, fullwidth])
}

#[test]
fn matches_across_fields_and_normalizes_the_query_the_same_way() {
    let (fx, ids) = library_fixture();
    let (soseki, rust, abc) = (&ids[0], &ids[1], &ids[2]);

    // 書名（3 文字以上は FTS）
    assert_eq!(search(&fx, "吾輩は"), [soseki.clone()]);
    // ひらがな・カタカナの区別なし（ヨミと書名）
    assert_eq!(search(&fx, "わがはい"), [soseki.clone()]);
    assert_eq!(search(&fx, "ワガハイ"), [soseki.clone()]);
    assert_eq!(search(&fx, "ぷろぐらみんぐ"), [rust.clone()]);
    // 英字の大文字小文字・全角半角の区別なし
    assert_eq!(search(&fx, "RUST"), [rust.clone()]);
    assert_eq!(search(&fx, "ｒｕｓｔ"), [rust.clone()]);
    assert_eq!(search(&fx, "abc"), [abc.clone()]);
    assert_eq!(search(&fx, "ＡＢＣ"), [abc.clone()]);
    // 半角カナで入っている出版社を全角カナ・ひらがなで探せる
    assert_eq!(search(&fx, "カタカナ"), [abc.clone()]);
    assert_eq!(search(&fx, "かたかな"), [abc.clone()]);
    // 著者（名前とヨミ）、出版社、副書名、タグ、メモ
    assert_eq!(search(&fx, "夏目漱石"), [soseki.clone()]);
    assert_eq!(search(&fx, "なつめそうせき"), [soseki.clone()]);
    assert_eq!(search(&fx, "Orendorff"), [rust.clone()]);
    assert_eq!(search(&fx, "岩波書店"), [soseki.clone()]);
    assert_eq!(search(&fx, "Safe Systems"), [rust.clone()]);
    assert_eq!(search(&fx, "技術"), [rust.clone()]);
    assert_eq!(search(&fx, "研究/SiC"), [soseki.clone()]);
    assert_eq!(search(&fx, "kindle"), [soseki.clone()]);
    // 部分一致（語の途中）
    assert_eq!(search(&fx, "猫である"), [soseki.clone()]);
    assert_eq!(search(&fx, "ログラミ"), [rust.clone()]);
}

#[test]
fn shorter_terms_use_like_and_all_terms_must_match() {
    let (fx, ids) = library_fixture();
    let (soseki, rust, abc) = (&ids[0], &ids[1], &ids[2]);

    // 1 文字・2 文字は LIKE
    assert_eq!(search(&fx, "猫"), [soseki.clone()]);
    assert_eq!(search(&fx, "漱石"), [soseki.clone()]);
    assert_eq!(search(&fx, "技"), [rust.clone()]);
    assert_eq!(search(&fx, "入門"), [abc.clone()]);
    // どれかの列に入っていればよい（列をまたいでも AND）
    assert_eq!(search(&fx, "猫 岩波"), [soseki.clone()]);
    assert_eq!(search(&fx, "猫 小説 岩波書店"), [soseki.clone()]);
    assert_eq!(search(&fx, "猫 技術"), Vec::<String>::new());
    // 全角スペースで区切ってもよい
    assert_eq!(search(&fx, "吾輩は\u{3000}岩波"), [soseki.clone()]);
    // 長い語と短い語の組み合わせ
    assert_eq!(search(&fx, "rust ジ"), [rust.clone()]);
    assert_eq!(search(&fx, "rust 猫"), Vec::<String>::new());
    // 一致なし
    assert!(search(&fx, "存在しない書名").is_empty());
    // 空の検索語は全冊（ID 順）
    let mut all = ids.clone();
    all.sort();
    assert_eq!(search(&fx, ""), all);
    assert_eq!(search(&fx, "   "), all);
}

#[test]
fn isbn_search_ignores_hyphens_and_accepts_isbn10() {
    let (fx, ids) = library_fixture();
    let mut both_soseki = vec![ids[0].clone(), ids[2].clone()];
    both_soseki.sort();
    assert_eq!(search(&fx, "9784003101117"), both_soseki);
    assert_eq!(search(&fx, "978-4-00-310111-7"), both_soseki);
    assert_eq!(
        search(&fx, "４００３１０１１１１"),
        both_soseki,
        "ISBN-10 を全角で"
    );
    assert_eq!(search(&fx, "4003101111"), both_soseki);
    assert_eq!(search(&fx, "4-00-310111-1"), both_soseki);
    assert_eq!(search(&fx, "9784873119786"), [ids[1].clone()]);
    assert_eq!(search(&fx, "4873119782"), [ids[1].clone()]);
    // 途中までの ISBN
    assert_eq!(search(&fx, "978487311"), [ids[1].clone()]);
    // 書名と ISBN の組み合わせ
    assert_eq!(search(&fx, "978-4-00-310111-7 猫"), [ids[0].clone()]);
    // 間違ったチェックデジット
    assert!(search(&fx, "9784873119787").is_empty());
}

#[test]
fn special_characters_are_searched_literally_and_never_break_the_query() {
    let (fx, ids) = library_fixture();
    // LIKE のワイルドカード（1〜2 文字の語）は文字そのものとして扱う
    assert_eq!(search(&fx, "%"), [ids[0].clone()], "メモの「100%」だけ");
    assert_eq!(
        search(&fx, "_"),
        [ids[0].clone()],
        "メモの「好き_です」だけ"
    );
    assert_eq!(search(&fx, "0%"), [ids[0].clone()]);
    assert!(search(&fx, "\\").is_empty());
    // FTS の演算子や記号（3 文字以上）も文字として扱う。エラーにならない
    for q in [
        "\"",
        "\"\"\"",
        "a\"b\"c",
        "foo OR bar",
        "foo AND",
        "NOT x",
        "NEAR(a b)",
        "a*",
        "*abc*",
        "col:abc",
        "title:吾輩は",
        "-abc",
        "(abc",
        "abc)",
        "'; DROP TABLE books; --",
        "abc'def",
    ] {
        let r = search_books(&fx.conn, q);
        assert!(r.is_ok(), "{q:?}: {r:?}");
    }
    assert!(search(&fx, "NOT x").is_empty());
    // 引用符で囲んでも、中身が文字として一致する
    assert_eq!(search(&fx, "\"吾輩は\""), Vec::<String>::new());
    assert_eq!(search(&fx, "Fast,"), [ids[1].clone()], "記号を含む語");
    assert_eq!(count(&fx.conn, "books"), 3, "テーブルは無事");
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn the_index_follows_local_edits_trash_and_purge() {
    let (mut fx, ids) = library_fixture();
    let soseki = &ids[0];
    assert_eq!(fts_rows(&fx.conn), 3);
    assert_eq!(idx_rows(&fx.conn), 3);

    // 書名を変えると、検索結果も変わる
    edit(
        &mut fx,
        soseki,
        &[("title", json!("坊っちゃん")), ("title_kana", json!(null))],
    );
    assert!(search(&fx, "吾輩は").is_empty());
    assert_eq!(search(&fx, "坊っちゃん"), [soseki.clone()]);
    assert_eq!(fts_rows(&fx.conn), 3, "行が増えない");

    // タグの変更は book_tags にも反映される
    edit(&mut fx, soseki, &[("tags", json!(["随筆"]))]);
    let tags: Vec<String> = fx
        .conn
        .prepare("SELECT tag FROM book_tags WHERE book_id = ?1")
        .unwrap()
        .query_map([soseki], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(tags, ["随筆"]);
    assert_eq!(search(&fx, "随筆"), [soseki.clone()]);
    assert!(search(&fx, "研究/SiC").is_empty());

    // ゴミ箱の本は検索対象に残る
    library::trash_books(&mut fx.conn, &env(fx.clock), &[soseki.clone()]).unwrap();
    assert_eq!(search(&fx, "坊っちゃん"), [soseki.clone()]);

    // 完全削除した本は外れる
    library::purge_books(&mut fx.conn, &env(fx.clock), &[soseki.clone()]).unwrap();
    assert!(search(&fx, "坊っちゃん").is_empty());
    assert_eq!(fts_rows(&fx.conn), 2);
    assert_eq!(idx_rows(&fx.conn), 2);
    assert_eq!(count(&fx.conn, "book_tags"), 1, "技術");
}

#[test]
fn remote_ops_and_series_renames_update_the_index_too() {
    let mut fx = Fx::new();
    // 他端末から届いた Op（順序に関わらず、適用後に最新になる）
    let remote = common::book_op(
        common::DEV_B,
        5_000,
        json!({"title": "遠隔の本", "authors": [{"name": "遠藤周作", "kana": "エンドウシュウサク"}],
               "series_id": common::SERIES_1}),
    );
    common::apply(&mut fx.conn, &remote);
    assert_eq!(search(&fx, "遠藤"), [common::BOOK_1.to_owned()]);
    assert_eq!(
        search(&fx, "えんどうしゅうさく"),
        [common::BOOK_1.to_owned()]
    );
    assert_eq!(fts_rows(&fx.conn), 1);

    // シリーズの行は後から届く
    let series = common::remote_op(
        common::DEV_B,
        6_000,
        0,
        bibcontainer_core::Entity::Series,
        common::SERIES_1,
        json!({"name": "ゴースト・シリーズ"}),
    );
    common::apply(&mut fx.conn, &series);
    assert_eq!(search(&fx, "ゴースト"), [common::BOOK_1.to_owned()]);

    // シリーズ名の変更は、そのシリーズの本の検索に反映される
    let renamed = common::remote_op(
        common::DEV_A,
        7_000,
        0,
        bibcontainer_core::Entity::Series,
        common::SERIES_1,
        json!({"name": "改名後"}),
    );
    common::apply(&mut fx.conn, &renamed);
    assert!(search(&fx, "ゴースト").is_empty());
    assert_eq!(search(&fx, "改名後"), [common::BOOK_1.to_owned()]);

    // 自分の操作でも同じ
    library::rename_series(
        &mut fx.conn,
        &env(fx.clock),
        common::SERIES_1,
        "Ｎｅｗ Name",
    )
    .unwrap();
    assert_eq!(search(&fx, "new name"), [common::BOOK_1.to_owned()]);
    assert!(search(&fx, "改名後").is_empty());
}

#[test]
fn rebuild_recreates_the_derived_data_from_the_books() {
    let (mut fx, ids) = library_fixture();
    fx.conn
        .execute_batch("DELETE FROM books_fts; DELETE FROM book_idx; DELETE FROM book_tags;")
        .unwrap();
    assert!(search(&fx, "吾輩は").is_empty());

    assert_eq!(derived::rebuild(&mut fx.conn).unwrap(), 3);
    assert_eq!(search(&fx, "吾輩は"), [ids[0].clone()]);
    assert_eq!(fts_rows(&fx.conn), 3);
    assert_eq!(count(&fx.conn, "book_tags"), 3);

    // 二度作り直しても同じ
    assert_eq!(derived::rebuild(&mut fx.conn).unwrap(), 3);
    assert_eq!(fts_rows(&fx.conn), 3);

    // 版が合っていれば ensure_built は何もしない。合っていなければ作り直す
    assert!(!derived::ensure_built(&mut fx.conn).unwrap());
    fx.conn
        .execute(
            "UPDATE local_meta SET value = 'old' WHERE key = ?1",
            [derived::KEY_DERIVED_VERSION],
        )
        .unwrap();
    assert!(derived::ensure_built(&mut fx.conn).unwrap());
    assert!(!derived::ensure_built(&mut fx.conn).unwrap());
}

#[test]
fn opening_a_phase0_database_builds_the_derived_data_after_migrating() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("lib.sqlite3");
    let backups = dir.path().join("backups");

    // フェーズ 0 の DB（0001 だけ適用済みで、本が入っている）
    {
        let (conn, _) = open_with_migrations(&db_path, &backups, &MIGRATIONS[..1]).unwrap();
        conn.execute(
            "INSERT INTO books(id, title, authors_json, tags_json, isbn13, memo)
             VALUES (?1, '昔の本', '[{\"name\":\"古い著者\"}]', '[\"旧\"]', '9784003101117', 'メモ')",
            [common::BOOK_1],
        )
        .unwrap();
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    let (conn, report) = open_with_migrations(&db_path, &backups, MIGRATIONS).unwrap();
    assert_eq!(report.from_version, 1);
    assert_eq!(report.to_version, LATEST_VERSION);
    assert!(report.backup_path.is_some(), "移行前のバックアップ");
    assert_eq!(search_books(&conn, "昔の本").unwrap(), [common::BOOK_1]);
    assert_eq!(search_books(&conn, "古い著者").unwrap(), [common::BOOK_1]);
    assert_eq!(search_books(&conn, "4003101111").unwrap(), [common::BOOK_1]);
    let list = rows::list_rows(&conn).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].sort_title, "昔の本");
}

#[test]
fn normalization_is_what_the_index_stores() {
    let (fx, _) = library_fixture();
    // 索引には正規化した文字列が入る
    let title: String = fx
        .conn
        .query_row(
            "SELECT title FROM books_fts WHERE title LIKE '%abc%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, normalize_for_search("ＡＢＣ入門"));
    assert_eq!(title, "abc入門");
}

#[test]
fn a_book_changed_without_search_fields_keeps_a_complete_index() {
    // 検索に関係ない項目だけの変更でも、索引に行が無ければ作る
    let mut fx = Fx::new();
    let ids = add(&mut fx, &[("title", json!("T"))]);
    fx.conn
        .execute_batch("DELETE FROM books_fts; DELETE FROM book_idx;")
        .unwrap();
    library::update_book(
        &mut fx.conn,
        &env(fx.clock),
        &ids,
        &[FieldChange::manual("rating", json!(3))],
        UpdateOptions::default(),
    )
    .unwrap();
    assert_eq!(idx_rows(&fx.conn), 1);
}
