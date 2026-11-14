//! CSV 取り込み（`csv_import`）のテスト。DB を使うもの。値の読み替えなどの単体テストは `src/csv_import/tests.rs`。

mod common;
mod lib_common;

use bibcontainer_core::csv_import::{
    ColumnMapping, CsvEncoding, CsvImportOptions, ImportTarget, IssueReason, RowStatus, execute,
    plan, preview, write_rejected, write_rejected_to_file,
};
use bibcontainer_core::export::export_csv;
use bibcontainer_core::ids::new_entity_id;
use bibcontainer_core::library::{self, FieldChange};
use bibcontainer_core::normalize::isbn13_check_digit;
use bibcontainer_core::{Entity, db, list_rows};
use rusqlite::Connection;
use serde_json::{Value, json};
use tempfile::tempdir;

use lib_common::*;

/// 12 桁の数字から ISBN-13 を作る。
fn isbn(n: u64) -> String {
    let first12 = format!("978{n:09}");
    format!("{first12}{}", isbn13_check_digit(&first12).unwrap())
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// 行（欄の配列）から CSV の文字列を作る（LF 区切り）。
fn csv_text(rows: &[Vec<&str>]) -> String {
    rows.iter()
        .map(|r| r.iter().map(|c| quote(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// 推測した対応づけで取り込む指定。
fn suggested_options(bytes: &[u8]) -> CsvImportOptions {
    let p = preview(bytes, None, true).unwrap();
    CsvImportOptions {
        encoding: None,
        has_header: true,
        mapping: p.suggested,
    }
}

fn book_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM books", [], |r| r.get(0))
        .unwrap()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn run(fx: &mut Fx, csv: &str) -> bibcontainer_core::csv_import::CsvImportResult {
    let opts = suggested_options(csv.as_bytes());
    execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts).unwrap()
}

#[test]
fn plan_does_not_write_to_the_database() {
    let mut fx = Fx::new();
    add(
        &mut fx,
        &[("title", json!("既存")), ("isbn13", json!(isbn(1)))],
    );
    let ops = outbox_len(&fx.conn);
    let books = book_count(&fx.conn);
    let csv = csv_text(&[
        vec!["ISBN", "書名", "シリーズ", "保管場所"],
        vec![&isbn(2), "新刊", "新シリーズ", "新しい棚 > 上段"],
        vec![&isbn(1), "既存", "", ""],
    ]);
    let opts = suggested_options(csv.as_bytes());
    let p = plan(&fx.conn, csv.as_bytes(), &opts).unwrap();
    assert_eq!((p.to_import, p.skipped.registered_isbn), (1, 1));
    assert_eq!(p.new_series, ["新シリーズ"]);
    assert_eq!(p.new_locations, ["新しい棚", "新しい棚 > 上段"]);
    assert_eq!(outbox_len(&fx.conn), ops);
    assert_eq!(book_count(&fx.conn), books);
    assert_eq!(count(&fx.conn, "series"), 0);
    assert_eq!(count(&fx.conn, "locations"), 0);
}

#[test]
fn classifies_rows_in_order_with_every_skip_kind() {
    let mut fx = Fx::new();
    let a = add(
        &mut fx,
        &[("title", json!("所有本")), ("isbn13", json!(isbn(10)))],
    );
    add(
        &mut fx,
        &[
            ("title", json!("欲しい本")),
            ("isbn13", json!(isbn(11))),
            ("ownership", json!("wanted")),
        ],
    );
    add(
        &mut fx,
        &[
            ("title", json!("手放した本")),
            ("isbn13", json!(isbn(12))),
            ("ownership", json!("released")),
        ],
    );
    let trashed = add(
        &mut fx,
        &[("title", json!("ゴミ箱の本")), ("isbn13", json!(isbn(13)))],
    );
    library::trash_books(&mut fx.conn, &env(fx.clock), std::slice::from_ref(&trashed)).unwrap();
    let fresh = new_entity_id();
    let (i20, i21, i22, i23, i24) = (isbn(20), isbn(21), isbn(22), isbn(23), isbn(24));
    let (i10, i11, i12, i13) = (isbn(10), isbn(11), isbn(12), isbn(13));

    let csv = csv_text(&[
        vec!["ISBN", "書名", "ID", "ゴミ箱"],
        vec![&i20, "新規\n(2 行にまたがる)", "", ""], // 行 2-3
        vec![&i10, "x", "", ""],
        vec![&i11, "x", "", ""],
        vec![&i12, "x", "", ""],
        vec![&i13, "ゴミ箱の本と同じ ISBN", "", ""],
        vec![&i20, "ファイルの中の重複", "", ""],
        vec!["", "ID 一致", &a, ""],
        vec!["", "ゴミ箱の ID 一致", &trashed, ""],
        vec!["9.78478E+12", "エラー", "", ""],
        vec!["", "", "", "はい"],
        vec![&i21, "ゴミ箱の行", "", "はい"],
        vec![&i22, "新規 ID あり", &fresh, ""],
        vec!["", "同じ ID の 2 つ目", &fresh, ""],
        vec![&i23, "警告つき", "not-an-id", ""],
        vec![&i24, "ゴミ箱の欄が読めない", "", "たぶん"],
    ]);
    let opts = suggested_options(csv.as_bytes());
    let p = plan(&fx.conn, csv.as_bytes(), &opts).unwrap();
    assert_eq!(p.total_rows, 15);
    assert_eq!(p.to_import, 5, "{p:#?}"); // 20, 13(ゴミ箱の本), 22, 23, 24
    assert_eq!(p.to_fetch, 5);
    assert_eq!(p.skipped.registered_isbn, 3);
    assert_eq!(p.skipped.same_id, 2);
    assert_eq!(p.skipped.duplicate_in_file, 2);
    assert_eq!(p.skipped.trashed_row, 1);
    assert_eq!(p.errors, 2);
    assert_eq!(p.warned_rows, 2);

    let issue = |line: u32| {
        p.issues
            .iter()
            .find(|i| i.line == line)
            .unwrap_or_else(|| panic!("行 {line} がない: {:#?}", p.issues))
    };
    assert_eq!(issue(4).status, RowStatus::Skip);
    assert_eq!(
        issue(4).reasons,
        [IssueReason::RegisteredIsbn {
            existing_title: "所有本".into()
        }]
    );
    assert_eq!(
        issue(5).reasons,
        [IssueReason::RegisteredIsbn {
            existing_title: "欲しい本".into()
        }]
    );
    assert_eq!(
        issue(6).reasons,
        [IssueReason::RegisteredIsbn {
            existing_title: "手放した本".into()
        }]
    );
    assert!(
        p.issues.iter().all(|i| i.line != 7),
        "ゴミ箱の本の ISBN は登録済みではない"
    );
    assert_eq!(
        issue(8).reasons,
        [IssueReason::DuplicateInFile { first_line: 2 }]
    );
    assert_eq!(issue(9).reasons, [IssueReason::SameId { trashed: false }]);
    assert_eq!(issue(10).reasons, [IssueReason::SameId { trashed: true }]);
    assert_eq!(issue(11).status, RowStatus::Error);
    assert_eq!(
        issue(11).reasons,
        [IssueReason::InvalidIsbn {
            value: "9.78478E+12".into(),
            excel_number: true
        }]
    );
    assert_eq!(
        issue(12).reasons,
        [IssueReason::NoTitleOrIsbn],
        "エラーはゴミ箱の判定より先"
    );
    assert_eq!(issue(13).reasons, [IssueReason::TrashedRow]);
    assert_eq!(
        issue(15).reasons,
        [IssueReason::DuplicateInFile { first_line: 14 }],
        "ID の重複"
    );
    assert_eq!(issue(16).status, RowStatus::Import);
    assert!(matches!(
        &issue(16).reasons[..],
        [IssueReason::UnreadableValue {
            target: ImportTarget::Id,
            ..
        }]
    ));
    assert_eq!(issue(17).status, RowStatus::Import);
    assert!(matches!(
        &issue(17).reasons[..],
        [IssueReason::UnreadableValue {
            target: ImportTarget::Trashed,
            ..
        }]
    ));
    assert_eq!(issue(4).isbn13.as_deref(), Some(i10.as_str()));
    assert_eq!(issue(4).title.as_deref(), Some("x"));

    // 実行しても同じ（取り込んだのは 5 冊）
    let before = book_count(&fx.conn);
    let r = execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts).unwrap();
    assert_eq!(
        (r.imported, r.fetch_queued, r.skipped, r.errors),
        (5, 5, 8, 2)
    );
    assert_eq!(book_count(&fx.conn), before + 5);
    assert_eq!(r.book_ids.len(), 5);
    // もう一度取り込むと、ISBN のある行は登録済みとして飛ばす。ID は書き込まないので、ISBN のない行
    // （さっきはファイルの中の重複として飛ばした「同じ ID の 2 つ目」）だけが取り込まれる。
    let again = execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts).unwrap();
    assert_eq!(again.imported, 1);
    assert_eq!(book_count(&fx.conn), before + 6);
}

#[test]
fn id_column_is_only_for_duplicate_detection_and_never_written() {
    let mut fx = Fx::new();
    let id = new_entity_id();
    let csv = csv_text(&[vec!["ID", "書名"], vec![&id, "本"]]);
    let r = run(&mut fx, &csv);
    assert_eq!(r.imported, 1);
    assert_ne!(r.book_ids[0], id);
    assert_eq!(count(&fx.conn, "books"), 1);
}

#[test]
fn resolves_locations_by_path_unique_leaf_or_creates_the_missing_tail() {
    let mut fx = Fx::new();
    let env = env(fx.clock);
    let home = library::create_location(&mut fx.conn, &env, "自宅", None).unwrap();
    let study = library::create_location(&mut fx.conn, &env, "書斎", Some(&home)).unwrap();
    let shelf_a = library::create_location(&mut fx.conn, &env, "棚A", Some(&study)).unwrap();
    let parents_home = library::create_location(&mut fx.conn, &env, "実家", None).unwrap();
    let shelf_a2 =
        library::create_location(&mut fx.conn, &env, "棚a", Some(&parents_home)).unwrap();

    let rows = [
        "自宅 > 書斎 > 棚A",        // 全体が一致
        "書斎",                     // 名前が 1 つだけ
        "棚Ａ",                     // 2 か所にある（全角・大文字小文字は同じとみなす）→ 根に作る
        "実家＞押入れ",             // 途中まで一致 → 実家の下に作る
        "自宅 > 書斎 > 棚B > 上段", // 途中まで一致
        "押入れ",                   // 根に合うものがなく、既存にもない → 根に作る
        "棚A",                      // 上で作った根の「棚A」に入る
        " > ",                      // 空
    ];
    let mut table = vec![vec!["書名", "保管場所"]];
    let titles: Vec<String> = (0..rows.len()).map(|i| format!("本{i}")).collect();
    for (t, r) in titles.iter().zip(rows) {
        table.push(vec![t.as_str(), r]);
    }
    let csv = csv_text(&table);
    let opts = suggested_options(csv.as_bytes());
    let p = plan(&fx.conn, csv.as_bytes(), &opts).unwrap();
    assert_eq!(
        p.new_locations,
        [
            "棚A",
            "実家 > 押入れ",
            "自宅 > 書斎 > 棚B",
            "自宅 > 書斎 > 棚B > 上段",
            "押入れ"
        ]
    );
    let ops_before = outbox_len(&fx.conn);
    let r = execute(&mut fx.conn, &env, csv.as_bytes(), &opts).unwrap();
    assert_eq!(r.created_locations, 5);
    assert_eq!(count(&fx.conn, "locations"), 10);
    assert!(outbox_len(&fx.conn) >= ops_before + 5 + 8);

    let infos = library::list_locations(&fx.conn).unwrap();
    let path_of = |book: usize| -> Option<String> {
        let loc = column(&fx.conn, &r.book_ids[book], "location_id");
        loc.as_str()
            .map(|id| infos.iter().find(|l| l.id == id).unwrap().path.clone())
    };
    let loc_id = |book: usize| column(&fx.conn, &r.book_ids[book], "location_id");
    assert_eq!(loc_id(0), json!(shelf_a));
    assert_eq!(loc_id(1), json!(study));
    assert_eq!(path_of(2).as_deref(), Some("棚A"));
    assert_ne!(loc_id(2), json!(shelf_a));
    assert_ne!(loc_id(2), json!(shelf_a2));
    assert_eq!(path_of(3).as_deref(), Some("実家 > 押入れ"));
    assert_eq!(path_of(4).as_deref(), Some("自宅 > 書斎 > 棚B > 上段"));
    assert_eq!(path_of(5).as_deref(), Some("押入れ"));
    assert_eq!(loc_id(6), loc_id(2), "作った根の棚に入る");
    assert!(loc_id(7).is_null());
    // 親が子より先に作られている（子の親が、子より前に作られた）
    let ids: Vec<&str> = infos.iter().map(|l| l.id.as_str()).collect();
    for l in &infos {
        if let Some(p) = &l.parent_id {
            assert!(ids.contains(&p.as_str()));
        }
    }
}

#[test]
fn series_are_matched_by_normalized_name_and_new_ones_are_deduplicated() {
    let mut fx = Fx::new();
    let existing =
        library::find_or_create_series(&mut fx.conn, &env(fx.clock), "シリーズ A").unwrap();
    let csv = csv_text(&[
        vec!["書名", "シリーズ"],
        vec!["1", "ｼﾘｰｽﾞ ａ"],
        vec!["2", "新シリーズ"],
        vec!["3", "新シリーズ"],
        vec!["4", "ＮＥＷ"],
        vec!["5", "new"],
        vec!["6", ""],
    ]);
    let opts = suggested_options(csv.as_bytes());
    let p = plan(&fx.conn, csv.as_bytes(), &opts).unwrap();
    assert_eq!(p.new_series, ["新シリーズ", "ＮＥＷ"]);
    let r = execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts).unwrap();
    assert_eq!(r.created_series, 2);
    assert_eq!(count(&fx.conn, "series"), 3);
    let sid = |i: usize| column(&fx.conn, &r.book_ids[i], "series_id");
    assert_eq!(sid(0), json!(existing));
    assert_eq!(sid(1), sid(2));
    assert_eq!(sid(3), sid(4));
    assert_ne!(sid(1), sid(3));
    assert!(sid(5).is_null());
}

#[test]
fn execute_writes_import_origin_pending_status_ops_and_created_at() {
    let mut fx = Fx::new();
    let ops_before = outbox_len(&fx.conn);
    let csv = csv_text(&[
        vec![
            "ISBN",
            "書名",
            "著者",
            "タグ",
            "発行日",
            "登録日時",
            "巻次",
            "評価",
            "メモ",
        ],
        vec![
            &isbn(30),
            "ISBN あり",
            "夏目漱石（著）; 山田",
            "a, b",
            "2020年5月",
            "2020-01-02T03:04:05.678Z",
            "第3巻",
            "5",
            "m",
        ],
        vec!["", "ISBN なし", "", "", "", "", "", "", ""],
        vec!["4065192900", "", "", "", "", "", "", "0", ""],
    ]);
    let opts = suggested_options(csv.as_bytes());
    let r = execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts).unwrap();
    assert_eq!(
        (r.imported, r.fetch_queued, r.skipped, r.errors),
        (3, 2, 0, 0)
    );
    assert_eq!(outbox_len(&fx.conn), ops_before + 3);
    let [a, b, c] = [&r.book_ids[0], &r.book_ids[1], &r.book_ids[2]];

    let src = sources(&fx.conn, a);
    for f in [
        "isbn13",
        "title",
        "authors",
        "published_on",
        "volume",
        "volume_sort",
    ] {
        assert_eq!(src[f], json!("import"), "{f}");
    }
    assert_eq!(column(&fx.conn, a, "meta_status"), json!("pending"));
    assert_eq!(
        column(&fx.conn, b, "meta_status"),
        json!("ok"),
        "ISBN のない本は取得待ちにしない"
    );
    assert_eq!(column(&fx.conn, c, "meta_status"), json!("pending"));
    assert_eq!(
        column(&fx.conn, c, "isbn13"),
        json!("9784065192900"),
        "ISBN-10 は 13 桁にする"
    );
    assert_eq!(column(&fx.conn, a, "published_on"), json!("2020-05"));
    assert_eq!(column(&fx.conn, a, "rating"), json!(5));
    assert!(column(&fx.conn, c, "rating").is_null(), "評価 0 は空");
    assert_eq!(
        column(&fx.conn, a, "created_at"),
        json!("2020-01-02T03:04:05.678Z")
    );
    assert_eq!(column(&fx.conn, a, "volume_sort"), json!(3.0));
    assert_eq!(
        ops_for(&fx.conn, Entity::Book, a)[0].set["created_at"],
        json!("2020-01-02T03:04:05.678Z")
    );
    // 登録日時の指定がなければ現在時刻
    assert_eq!(
        column(&fx.conn, b, "created_at"),
        json!("2026-10-14T08:30:00.000Z")
    );
    let detail = bibcontainer_core::get_book_detail(&fx.conn, a).unwrap();
    assert_eq!(detail.tags, ["a", "b"]);
    assert_eq!(detail.authors.len(), 2);
    assert_eq!(detail.authors[0].role.as_deref(), Some("著"));
    // 取り込み時の「今日」の補正（読了日）は create の既存の規則のまま
    assert_eq!(list_rows(&fx.conn).unwrap().len(), 3);
}

#[test]
fn a_failure_in_the_middle_rolls_everything_back() {
    let mut fx = Fx::new();
    fx.conn
        .execute_batch(
            "CREATE TRIGGER fail_on_title BEFORE UPDATE ON books WHEN NEW.title = 'FAIL'
             BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .unwrap();
    let ops = outbox_len(&fx.conn);
    let csv = csv_text(&[
        vec!["書名", "シリーズ", "保管場所"],
        vec!["ok1", "S1", "棚 > 段"],
        vec!["ok2", "S2", "別の棚"],
        vec!["FAIL", "S3", "三つ目"],
        vec!["ok4", "", ""],
    ]);
    let opts = suggested_options(csv.as_bytes());
    let err = execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts);
    assert!(err.is_err());
    assert_eq!(count(&fx.conn, "books"), 0);
    assert_eq!(count(&fx.conn, "series"), 0);
    assert_eq!(count(&fx.conn, "locations"), 0);
    assert_eq!(outbox_len(&fx.conn), ops, "Op も残らない");
    // 失敗の原因を除けば、同じファイルが取り込める
    fx.conn.execute_batch("DROP TRIGGER fail_on_title").unwrap();
    let r = execute(&mut fx.conn, &env(fx.clock), csv.as_bytes(), &opts).unwrap();
    assert_eq!(r.imported, 4);
}

#[test]
fn bad_options_and_encodings_are_reported_as_errors() {
    let mut fx = Fx::new();
    let csv = b"a,b\n1,2\n";
    let m = |column, target| ColumnMapping { column, target };
    let bad = CsvImportOptions {
        encoding: None,
        has_header: true,
        mapping: vec![m(0, ImportTarget::Title), m(1, ImportTarget::Title)],
    };
    assert!(plan(&fx.conn, csv, &bad).is_err());
    assert!(execute(&mut fx.conn, &env(fx.clock), csv, &bad).is_err());
    let oob = CsvImportOptions {
        mapping: vec![m(2, ImportTarget::Title)],
        ..bad.clone()
    };
    assert!(plan(&fx.conn, csv, &oob).is_err());
    let undecodable = CsvImportOptions {
        mapping: vec![],
        ..bad
    };
    assert!(plan(&fx.conn, b"\xFF\x81 \n", &undecodable).is_err());
    assert!(write_rejected(b"\xFF\x81 \n", &undecodable, &mut Vec::new()).is_err());
}

#[test]
fn imports_shift_jis_files() {
    let mut fx = Fx::new();
    let text = "書名,著者\r\n①髙橋のかな漢字,山田（著）\r\n";
    let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(text);
    let p = preview(&bytes, None, true).unwrap();
    assert_eq!(p.encoding, CsvEncoding::ShiftJis);
    let opts = CsvImportOptions {
        encoding: Some(p.encoding),
        has_header: true,
        mapping: p.suggested,
    };
    let r = execute(&mut fx.conn, &env(fx.clock), &bytes, &opts).unwrap();
    assert_eq!(r.imported, 1);
    assert_eq!(
        column(&fx.conn, &r.book_ids[0], "title"),
        json!("①髙橋のかな漢字")
    );
}

// ---- 書き出し → 取り込み ----

/// 書き出しの CSV を、欄の配列にする。
fn parse(bytes: &[u8]) -> Vec<Vec<String>> {
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap();
    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(body)
        .records()
        .map(|r| r.unwrap().iter().map(str::to_owned).collect())
        .collect()
}

fn varied_library() -> (Fx, String) {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let series = library::find_or_create_series(&mut fx.conn, &e, "テストシリーズ").unwrap();
    let home = library::create_location(&mut fx.conn, &e, "自宅", None).unwrap();
    let shelf = library::create_location(&mut fx.conn, &e, "書斎", Some(&home)).unwrap();
    let work = new_entity_id();
    let _full = add(
        &mut fx,
        &[
            ("isbn13", json!(isbn(100))),
            ("title", json!("吾輩は猫である")),
            ("title_kana", json!("わがはいはねこである")),
            ("subtitle", json!("副題")),
            ("series_id", json!(series)),
            ("volume", json!("第3巻")),
            ("work_id", json!(work)),
            (
                "authors",
                json!([{"name": "夏目 漱石", "role": "著"}, {"name": "山田太郎", "role": "訳"}, {"name": "無役"}]),
            ),
            ("publisher", json!("講談社")),
            ("published_on", json!("2020-05")),
            ("c_code", json!("0093")),
            ("ndc", json!("913")),
            ("list_price", json!(1200)),
            ("ref_price_incl", json!(1320)),
            ("pages", json!(320)),
            ("size_text", json!("A5")),
            ("language", json!("ja")),
            ("description", json!("一行目\n二行目, \"引用\"")),
            ("ownership", json!("owned")),
            ("location_id", json!(shelf)),
            ("tags", json!(["小説", "名作"])),
            ("read_status", json!("read")),
            ("read_started_on", json!("2026-01-01")),
            ("read_finished_on", json!("2026-02-03")),
            ("rating", json!(4)),
            ("review", json!("よかった")),
            ("purchased_on", json!("2025-12-24")),
            ("purchase_store", json!("丸善")),
            ("purchase_price", json!(1500)),
            ("purchase_cond", json!("used")),
            ("purchase_memo", json!("帯つき")),
            ("memo", json!("=1+1 の本")),
        ],
    );
    fx.clock.advance(60_000);
    add(
        &mut fx,
        &[
            ("title", json!("欲しい本")),
            ("ownership", json!("wanted")),
            ("wish_priority", json!("high")),
            ("work_id", json!("none")),
            ("isbn13", json!(isbn(101))),
        ],
    );
    fx.clock.advance(60_000);
    add(&mut fx, &[("title", json!("書名だけ"))]);
    fx.clock.advance(60_000);
    add(
        &mut fx,
        &[
            ("title", json!("-先頭がマイナス")),
            ("memo", json!("+プラス,\"引用\"")),
        ],
    );
    fx.clock.advance(60_000);
    let gone = add(
        &mut fx,
        &[("title", json!("ゴミ箱の本")), ("isbn13", json!(isbn(102)))],
    );
    let e = env(fx.clock);
    library::trash_books(&mut fx.conn, &e, std::slice::from_ref(&gone)).unwrap();
    (fx, gone)
}

/// 取り込みで変わってよい列（ID・書影・取得状態）を除き、ゴミ箱の行を除いて、並べ替えた表。
fn comparable(bytes: &[u8]) -> Vec<Vec<String>> {
    let rows = parse(bytes);
    let header = &rows[0];
    let drop: Vec<usize> = ["ID", "書影", "取得状態"]
        .iter()
        .map(|n| header.iter().position(|h| h == n).unwrap())
        .collect();
    let trash = header.iter().position(|h| h == "ゴミ箱").unwrap();
    let mut out: Vec<Vec<String>> = rows
        .iter()
        .skip(1)
        .filter(|r| r[trash].is_empty())
        .map(|r| {
            r.iter()
                .enumerate()
                .filter(|(i, _)| !drop.contains(i))
                .map(|(_, c)| c.clone())
                .collect()
        })
        .collect();
    out.sort();
    let mut head: Vec<String> = header
        .iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(_, c)| c.clone())
        .collect();
    head.insert(0, String::new());
    out.insert(0, head[1..].to_vec());
    out
}

#[test]
fn export_then_import_round_trips_into_an_empty_library() {
    let (src, _) = varied_library();
    let exported = export_csv(&src.conn, None).unwrap();
    let opts = suggested_options(&exported);
    assert!(opts.mapping.iter().any(|m| m.target == ImportTarget::Id));

    let mut dst = Fx::new();
    let p = plan(&dst.conn, &exported, &opts).unwrap();
    assert_eq!(
        (p.total_rows, p.to_import, p.skipped.trashed_row, p.errors),
        (5, 4, 1, 0)
    );
    assert_eq!(p.warned_rows, 0, "{:#?}", p.issues);
    assert_eq!(p.new_locations, ["自宅", "自宅 > 書斎"]);
    assert_eq!(p.new_series, ["テストシリーズ"]);
    let r = execute(&mut dst.conn, &env(dst.clock), &exported, &opts).unwrap();
    assert_eq!((r.imported, r.fetch_queued), (4, 2));

    let again = export_csv(&dst.conn, None).unwrap();
    let (a, b) = (comparable(&exported), comparable(&again));
    assert_eq!(a.len(), 5, "見出し + 4 冊");
    assert_eq!(a, b);
    // 取り込んだ ISBN の本は取得待ち、書影・取得状態は引き継がない
    let pending: i64 = dst
        .conn
        .query_row(
            "SELECT COUNT(*) FROM books WHERE meta_status = 'pending'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending, 2);
}

#[test]
fn importing_an_export_into_its_own_library_skips_every_row() {
    let (mut src, gone) = varied_library();
    let exported = export_csv(&src.conn, None).unwrap();
    let before = book_count(&src.conn);
    let opts = suggested_options(&exported);
    let p = plan(&src.conn, &exported, &opts).unwrap();
    assert_eq!(p.to_import, 0);
    assert_eq!(p.skipped.trashed_row, 1);
    assert_eq!(p.skipped.same_id, 4);
    // ID の列を使わなくても、ISBN のある行は登録済みとして飛ばす（ゴミ箱の本の行は「ゴミ箱」の列で飛ばす）
    let no_id = CsvImportOptions {
        mapping: opts
            .mapping
            .iter()
            .filter(|m| m.target != ImportTarget::Id)
            .cloned()
            .collect(),
        ..opts.clone()
    };
    let p = plan(&src.conn, &exported, &no_id).unwrap();
    assert_eq!(p.skipped.registered_isbn, 2);
    assert_eq!(p.skipped.trashed_row, 1);
    assert_eq!(p.to_import, 2, "ISBN のない本は ID なしでは区別できない");
    // 「ゴミ箱」の列も使わなければ、ゴミ箱の本の ISBN は登録済みではないので取り込まれる
    let no_trash = CsvImportOptions {
        mapping: no_id
            .mapping
            .iter()
            .filter(|m| m.target != ImportTarget::Trashed)
            .cloned()
            .collect(),
        ..no_id
    };
    let p = plan(&src.conn, &exported, &no_trash).unwrap();
    assert_eq!(p.to_import, 3);
    let r = execute(&mut src.conn, &env(src.clock), &exported, &opts).unwrap();
    assert_eq!(r.imported, 0);
    assert_eq!(book_count(&src.conn), before);
    let _ = gone;
}

#[test]
fn rejected_rows_file_has_header_reason_column_bom_and_crlf_and_can_be_fixed_and_reimported() {
    let mut fx = Fx::new();
    add(
        &mut fx,
        &[("title", json!("登録済み")), ("isbn13", json!(isbn(200)))],
    );
    let ok = isbn(201);
    let csv = csv_text(&[
        vec!["ISBN", "書名", "メモ"],
        vec![&ok, "成功", ""],
        vec!["9.78478E+12", "エクセルの崩れ", "a,\"b\"\nc"],
        vec![&isbn(200), "登録済み(飛ばす)", ""],
        vec!["", "", "=書名も ISBN もない"],
        vec!["978-4-0000-0000-1", "ISBN の誤り", ""],
    ]);
    let opts = suggested_options(csv.as_bytes());
    let mut out = Vec::new();
    let n = write_rejected(csv.as_bytes(), &opts, &mut out).unwrap();
    assert_eq!(n, 3, "エラーの行だけ（飛ばす行は含めない）");
    assert_eq!(&out[..3], b"\xEF\xBB\xBF");
    let text = String::from_utf8(out[3..].to_vec()).unwrap();
    assert!(text.ends_with("\r\n"));
    assert!(
        !text.replace("\r\n", "").replace('\n', "").contains('\r'),
        "引用符の外の改行は CRLF"
    );
    let rows = parse(&out);
    assert_eq!(rows[0], ["ISBN", "書名", "メモ", "取り込めなかった理由"]);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows[1][..3],
        ["9.78478E+12", "エクセルの崩れ", "a,\"b\"\nc"],
        "元の欄はそのまま"
    );
    assert_eq!(
        rows[1][3],
        "ISBN を読めません（9.78478E+12。Excel で数値になり桁が落ちた可能性があります）"
    );
    assert_eq!(rows[2][3], "書名も ISBN もありません");
    assert_eq!(rows[2][2], "'=書名も ISBN もない", "書き出しと同じ数式対策");
    assert_eq!(rows[3][3], "ISBN を読めません（978-4-0000-0000-1）");

    // ファイル版は同じ内容を書く
    let dir = tempdir().unwrap();
    let path = dir.path().join("rejected.csv");
    assert_eq!(
        write_rejected_to_file(csv.as_bytes(), &opts, &path).unwrap(),
        3
    );
    assert_eq!(std::fs::read(&path).unwrap(), out);

    // 直して取り込み直す（理由の列はそのままでよい）
    let fixed_isbn = isbn(202);
    let mut fixed: Vec<Vec<&str>> = rows
        .iter()
        .map(|r| r.iter().map(String::as_str).collect())
        .collect();
    fixed[1][0] = &fixed_isbn;
    fixed[2][1] = "題名を補った";
    fixed[3][0] = "";
    fixed[3][1] = "ISBN を消した";
    let fixed_csv = csv_text(&fixed);
    let r = run(&mut fx, &fixed_csv);
    assert_eq!((r.imported, r.errors), (3, 0));
}

#[test]
fn rejected_rows_without_header_have_no_header_row() {
    let csv = "9.78478E+12,x\n,\n";
    let opts = CsvImportOptions {
        encoding: None,
        has_header: false,
        mapping: vec![
            ColumnMapping {
                column: 0,
                target: ImportTarget::Isbn13,
            },
            ColumnMapping {
                column: 1,
                target: ImportTarget::Title,
            },
        ],
    };
    let mut out = Vec::new();
    assert_eq!(write_rejected(csv.as_bytes(), &opts, &mut out).unwrap(), 1);
    let rows = parse(&out);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], "9.78478E+12");
}

// ---- 性能 ----

/// 5,000 行の取り込み（見積もり 2 秒・実行 10 秒以内が目標）。普段は動かさない。
///
/// ```text
/// cargo test --release -p bibcontainer-core --test csv_import -- --ignored --nocapture
/// ```
#[test]
#[ignore = "性能の計測。リリースビルドで --ignored を付けて実行する"]
fn perf_5000_rows_plan_and_execute() {
    use std::time::Instant;
    let dir = tempdir().unwrap();
    let (mut conn, _) =
        db::open(&dir.path().join("lib.sqlite3"), &dir.path().join("backups")).unwrap();
    let clock = bibcontainer_core::SystemClock;
    let env = library::Env::new(&clock);
    // 既存の本 1,000 冊（ISBN の重複確認の対象）
    for i in 0..1000_u64 {
        let changes = vec![
            FieldChange::manual("title", json!(format!("既存 {i}"))),
            FieldChange::manual("isbn13", json!(isbn(i))),
        ];
        library::create_book(&mut conn, &env, &changes).unwrap();
    }
    let mut csv =
        String::from("ISBN,書名,著者,シリーズ,巻次,出版社,発行日,本体価格,保管場所,タグ,メモ\n");
    for i in 0..5000_u64 {
        let n = 500 + i; // 先頭の 500 冊は既存と重なる
        csv.push_str(&format!(
            "{},書名 {i},著者{} （著）; 訳者{},シリーズ{},{},出版社{},2020-0{},{},棚{} > 段{},\"タグ{}, タグ{}\",メモ{i}\n",
            isbn(n), i % 300, i % 70, i % 100, i % 12 + 1, i % 40, i % 9 + 1, 1000 + i, i % 20, i % 5, i % 15, i % 8,
        ));
    }
    let opts = suggested_options(csv.as_bytes());
    let t = Instant::now();
    let p = plan(&conn, csv.as_bytes(), &opts).unwrap();
    let plan_time = t.elapsed();
    let t = Instant::now();
    let r = execute(&mut conn, &env, csv.as_bytes(), &opts).unwrap();
    let exec_time = t.elapsed();
    eprintln!(
        "plan: {plan_time:?} (取り込む {} / 登録済み {}), execute: {exec_time:?} (取り込んだ {}, 保管場所 {}, シリーズ {})",
        p.to_import, p.skipped.registered_isbn, r.imported, r.created_locations, r.created_series
    );
    assert_eq!(p.to_import, 4500);
    assert_eq!(r.imported, 4500);
    assert!(plan_time.as_secs_f64() < 2.0, "{plan_time:?}");
    assert!(exec_time.as_secs_f64() < 10.0, "{exec_time:?}");
    let _: Value = json!(null);
}
