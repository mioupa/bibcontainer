//! 書き出し（CSV・JSON）のテスト。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

mod common;
mod lib_common;

use bibcontainer_core::export::{
    JSON_FORMAT, JSON_FORMAT_VERSION, UTF8_BOM, csv_headers, export_csv, export_csv_to_file,
    export_json, export_json_to_file, write_csv,
};
use bibcontainer_core::library::{self, FieldChange};
use bibcontainer_core::schema::Entity;
use serde_json::{Value, json};
use tempfile::tempdir;

use lib_common::*;

/// 最小の CSV パーサ（引用符、`""`、引用符内の改行に対応）。
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {
                assert_eq!(chars.next(), Some('\n'), "改行は CRLF");
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, '\n') => panic!("引用符の外に LF だけの改行がある"),
            (false, c) => field.push(c),
        }
    }
    assert!(
        field.is_empty() && row.is_empty(),
        "最後の行も CRLF で終わる"
    );
    rows
}

fn csv_of(conn: &rusqlite::Connection, ids: Option<&[String]>) -> (Vec<u8>, Vec<Vec<String>>) {
    let bytes = export_csv(conn, ids).unwrap();
    assert_eq!(&bytes[..3], UTF8_BOM, "UTF-8 の BOM");
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    let rows = parse_csv(&text);
    (bytes, rows)
}

fn col(rows: &[Vec<String>], name: &str) -> usize {
    rows[0]
        .iter()
        .position(|h| h == name)
        .unwrap_or_else(|| panic!("{name} 列がない"))
}

struct Library {
    fx: Fx,
    ids: Vec<String>,
}

fn library_fixture() -> Library {
    let mut fx = Fx::new();
    let e = env(fx.clock);
    let home = library::create_location(&mut fx.conn, &e, "自宅", None).unwrap();
    let study = library::create_location(&mut fx.conn, &e, "書斎の本棚", Some(&home)).unwrap();
    let series = library::find_or_create_series(&mut fx.conn, &e, "岩波文庫").unwrap();
    let a = add(
        &mut fx,
        &[
            ("title", json!("吾輩は猫である")),
            ("title_kana", json!("ワガハイハネコデアル")),
            ("isbn13", json!("9784003101117")),
            ("series_id", json!(series)),
            ("volume", json!("上")),
            ("volume_sort", json!(1)),
            (
                "authors",
                json!([{"name": "夏目漱石", "role": "著"}, {"name": "校注者"}]),
            ),
            ("publisher", json!("岩波書店")),
            ("published_on", json!("1990-01")),
            ("c_code", json!("0193")),
            ("list_price", json!(700)),
            ("pages", json!(500)),
            ("location_id", json!(study)),
            ("tags", json!(["小説", "研究/SiC"])),
            ("read_status", json!("read")),
            ("rating", json!(5)),
            ("review", json!("面白い。\n2 行目、\"引用\"あり")),
            ("purchase_store", json!("紀伊國屋")),
            ("purchase_price", json!(770)),
            ("purchase_cond", json!("used")),
            ("memo", json!("カンマ, を含む")),
        ],
    );
    let b = add(
        &mut fx,
        &[
            ("title", json!("欲しい本")),
            ("ownership", json!("wanted")),
            ("wish_priority", json!("high")),
        ],
    );
    let c = add_titled(&mut fx, "ゴミ箱の本");
    library::trash_books(&mut fx.conn, &e, &[c.clone()]).unwrap();
    let d = add_titled(&mut fx, "完全削除する本");
    library::purge_books(&mut fx.conn, &e, &[d]).unwrap();
    Library {
        fx,
        ids: vec![a, b, c],
    }
}

#[test]
fn csv_has_bom_crlf_japanese_headers_and_all_fields() {
    let lib = library_fixture();
    let (bytes, rows) = csv_of(&lib.fx.conn, None);
    // ヘッダーは日本語の項目名
    assert_eq!(rows[0], csv_headers());
    for h in [
        "ID",
        "ISBN",
        "書名",
        "書名のヨミ",
        "著者",
        "出版社",
        "発行日",
        "Cコード",
        "保管場所",
        "タグ",
        "読書状況",
        "評価",
        "購入店",
        "ゴミ箱",
    ] {
        assert!(rows[0].iter().any(|x| x == h), "{h}");
    }
    // 全項目（同期する項目は列のどれかに出る）。どの行も列数が同じ
    assert!(rows[0].len() >= 38, "{}", rows[0].len());
    assert!(rows.iter().all(|r| r.len() == rows[0].len()));
    // 完全削除した本は含まない。ゴミ箱の本は含む
    assert_eq!(rows.len(), 1 + 3);
    // 改行は CRLF だけ（引用符の外に LF 単体はない。parse_csv が検査する）
    assert!(bytes.windows(2).any(|w| w == b"\r\n"));

    let a = &rows[1];
    let get = |name: &str| a[col(&rows, name)].as_str();
    assert_eq!(get("ID"), lib.ids[0]);
    assert_eq!(get("書名"), "吾輩は猫である");
    assert_eq!(get("ISBN"), "9784003101117");
    assert_eq!(get("シリーズ"), "岩波文庫");
    assert_eq!(get("著者"), "夏目漱石（著）; 校注者");
    assert_eq!(get("タグ"), "小説; 研究/SiC");
    assert_eq!(get("保管場所"), "自宅 > 書斎の本棚");
    assert_eq!(get("本体価格"), "700");
    assert_eq!(get("Cコード"), "0193");
    assert_eq!(get("発行日"), "1990-01");
    assert_eq!(get("所有状態"), "所有");
    assert_eq!(get("読書状況"), "読了");
    assert_eq!(get("読了日"), TODAY);
    assert_eq!(get("評価"), "5");
    assert_eq!(get("新品・中古"), "中古");
    assert_eq!(get("購入店"), "紀伊國屋");
    assert_eq!(get("購入価格"), "770");
    assert_eq!(get("ゴミ箱"), "");
    assert_eq!(get("取得状態"), "取得済み");
    assert_eq!(
        get("感想"),
        "面白い。\n2 行目、\"引用\"あり",
        "改行・引用符・カンマは引用符で囲んで残る"
    );
    assert_eq!(get("メモ"), "カンマ, を含む");
    assert!(get("登録日時").ends_with('Z'));

    let b = &rows[2];
    assert_eq!(b[col(&rows, "所有状態")], "欲しい本");
    assert_eq!(b[col(&rows, "欲しい本の優先度")], "高");
    let c = &rows[3];
    assert_eq!(c[col(&rows, "ゴミ箱")], "はい");
}

#[test]
fn csv_can_be_limited_to_the_given_ids_in_the_given_order() {
    let lib = library_fixture();
    let want = vec![
        lib.ids[1].clone(),
        "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned(),
        lib.ids[0].clone(),
        lib.ids[1].clone(),
    ];
    let (_, rows) = csv_of(&lib.fx.conn, Some(&want));
    let ids: Vec<&str> = rows[1..].iter().map(|r| r[0].as_str()).collect();
    assert_eq!(
        ids,
        [lib.ids[1].as_str(), lib.ids[0].as_str()],
        "渡した順、重複と存在しない ID は飛ばす"
    );

    // 空の指定は、ヘッダーだけ
    let (_, rows) = csv_of(&lib.fx.conn, Some(&[]));
    assert_eq!(rows.len(), 1);
    let mut out = Vec::new();
    let n = write_csv(&lib.fx.conn, Some(&lib.ids[..2]), &mut out).unwrap();
    assert_eq!(n, 2);
}

#[test]
fn csv_fields_that_could_be_formulas_get_a_leading_quote() {
    let mut fx = Fx::new();
    let titles = [
        "=HYPERLINK(\"http://example.com\",\"x\")",
        "+1+2",
        "-3",
        "@SUM(A1)",
        "安全な書名=1",
    ];
    let ids: Vec<String> = titles.iter().map(|t| add_titled(&mut fx, t)).collect();
    let (_, rows) = csv_of(&fx.conn, Some(&ids));
    let title = col(&rows, "書名");
    let got: Vec<&str> = rows[1..].iter().map(|r| r[title].as_str()).collect();
    assert_eq!(
        got,
        [
            "'=HYPERLINK(\"http://example.com\",\"x\")",
            "'+1+2",
            "'-3",
            "'@SUM(A1)",
            "安全な書名=1",
        ],
        "数式の先頭になりうる文字で始まる欄だけ、先頭に ' を付ける"
    );
}

#[test]
fn csv_has_the_manual_work_column_right_after_the_volume_sort_column() {
    const WORK: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
    let mut fx = Fx::new();
    let auto = add_titled(&mut fx, "自動");
    let none = add(
        &mut fx,
        &[("title", json!("まとめない")), ("work_id", json!("none"))],
    );
    let manual_work = add(
        &mut fx,
        &[("title", json!("手動の作品")), ("work_id", json!(WORK))],
    );
    // 他端末から届いた値は検証を通らない。数式になりうる値も、ほかの列と同じに無害化する。
    let remote = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a99";
    common::apply(
        &mut fx.conn,
        &common::remote_op(
            common::DEV_B,
            T0 + 1000,
            0,
            Entity::Book,
            remote,
            json!({ "title": "届いた本", "work_id": "=1+1" }),
        ),
    );

    let ids = [
        auto.clone(),
        none.clone(),
        manual_work.clone(),
        remote.to_owned(),
    ];
    let (_, rows) = csv_of(&fx.conn, Some(&ids));
    let headers = csv_headers();
    let at = headers.iter().position(|h| *h == "作品（手動）").unwrap();
    assert_eq!(headers[at - 1], "巻次（並べ替え用）", "直前の列");
    assert_eq!(headers[at + 1], "著者", "直後の列");
    assert_eq!(col(&rows, "作品（手動）"), at);

    let value = |i: usize| rows[1 + i][at].as_str();
    assert_eq!(value(0), "", "自動（NULL）は空");
    assert_eq!(value(1), "まとめない", "none は日本語で");
    assert_eq!(value(2), WORK, "手動の作品は ID のまま");
    assert_eq!(
        value(3),
        "'=1+1",
        "数式の先頭の文字は、ほかの列と同じに無害化する"
    );
}

#[test]
fn csv_file_is_written_atomically() {
    let lib = library_fixture();
    let dir = tempdir().unwrap();
    let path = dir.path().join("蔵書.csv");
    std::fs::write(&path, "古い内容").unwrap();
    let n = export_csv_to_file(&lib.fx.conn, &path, None).unwrap();
    assert_eq!(n, 3);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], UTF8_BOM);
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["蔵書.csv"], "一時ファイルは残らない");

    // 書けないときは、既存のファイルを壊さない
    let bad = dir.path().join("no-such-dir").join("x.csv");
    assert!(export_csv_to_file(&lib.fx.conn, &bad, None).is_err());
}

#[test]
fn json_export_has_format_version_first_and_every_entity_with_clocks() {
    let mut lib = library_fixture();
    let e = env(lib.fx.clock);
    library::set_tag_color(&mut lib.fx.conn, &e, "小説", Some("red")).unwrap();

    let export = export_json(&lib.fx.conn, T0).unwrap();
    let text = serde_json::to_string(&export).unwrap();
    assert!(
        text.starts_with(&format!(
            "{{\"format\":\"{JSON_FORMAT}\",\"version\":{JSON_FORMAT_VERSION},\"exportedAt\":\"2026-10-14T08:30:00.000Z\""
        )),
        "形式名と版が先頭: {}",
        &text[..120]
    );
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], json!("bibcontainer-library"));
    assert_eq!(v["version"], json!(1));
    assert_eq!(v["schemaVersion"], json!(bibcontainer_core::LATEST_VERSION));
    assert!(v["deviceId"].as_str().unwrap().starts_with("d-"));

    // ゴミ箱・完全削除を含む全エンティティ
    assert_eq!(v["books"].as_array().unwrap().len(), 4);
    assert_eq!(v["locations"].as_array().unwrap().len(), 2);
    assert_eq!(v["series"].as_array().unwrap().len(), 1);
    assert_eq!(v["tags"].as_array().unwrap().len(), 1);
    assert_eq!(v["shelves"].as_array().unwrap().len(), 0);

    // 本: ID、項目の値、項目ごとの HLC
    let book = v["books"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == json!(lib.ids[0]))
        .unwrap();
    assert_eq!(book["values"]["title"], json!("吾輩は猫である"));
    assert_eq!(book["values"]["tags"], json!(["小説", "研究/SiC"]));
    assert_eq!(book["values"]["authors"][0]["name"], json!("夏目漱石"));
    assert_eq!(book["values"]["field_sources"]["title"], json!("manual"));
    assert!(book["values"]["manual_fields"].as_array().unwrap().len() > 3);
    assert_eq!(book["values"]["deleted"], json!(false));
    assert!(book["extra"].is_object());
    let clocks = book["clocks"].as_object().unwrap();
    for key in [
        "title",
        "tags",
        "field_sources.title",
        "manual_fields.title",
        "created_at",
    ] {
        let hlc = clocks[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} の HLC"));
        assert!(bibcontainer_core::Hlc::parse(hlc).is_ok(), "{key}: {hlc}");
    }
    // 項目ごとの HLC は field_clocks と一致する
    let op_hlc = ops_for(&lib.fx.conn, Entity::Book, &lib.ids[0])[0]
        .hlc
        .clone();
    assert_eq!(clocks["title"], json!(op_hlc));

    // ゴミ箱と完全削除
    let trashed = v["books"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == json!(lib.ids[2]))
        .unwrap();
    assert_eq!(trashed["values"]["deleted"], json!(true));
    assert_eq!(trashed["values"]["purged"], json!(false));
    let purged: Vec<&Value> = v["books"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["values"]["purged"] == json!(true))
        .collect();
    assert_eq!(purged.len(), 1);
    assert_eq!(purged[0]["values"]["title"], json!(""));

    // 同期する全項目の値が出ている
    for def in Entity::Book.fields() {
        assert!(book["values"].get(def.name).is_some(), "{}", def.name);
    }
}

#[test]
fn json_export_to_file_is_pretty_valid_json() {
    let lib = library_fixture();
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.json");
    export_json_to_file(&lib.fx.conn, lib.fx.clock, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("{\n  \"format\": \"bibcontainer-library\",\n  \"version\": 1,"));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["books"].as_array().unwrap().len(), 4);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "一時ファイルは残らない"
    );
}

#[test]
fn json_export_values_can_be_replayed_as_ops() {
    // 書き出した値と時計から、別の DB に同じ状態を作れる（形式の完全性の確認）
    let lib = library_fixture();
    let v = serde_json::to_value(export_json(&lib.fx.conn, T0).unwrap()).unwrap();
    let mut other = common::mem_db();
    for (ent, key) in [
        (Entity::Series, "series"),
        (Entity::Location, "locations"),
        (Entity::Tag, "tags"),
        (Entity::Book, "books"),
    ] {
        for row in v[key].as_array().unwrap() {
            let id = row["id"].as_str().unwrap();
            let values = row["values"].as_object().unwrap();
            // 項目ごとの HLC で 1 項目ずつ Op にする
            for (field, hlc) in row["clocks"].as_object().unwrap() {
                // 取得元・手修正の記録は、保存形式（オブジェクト・配列）から項目ごとの値を取り出す。
                // 無ければ、時計が残っている（消した）ことを表す null
                let value = match field.as_str() {
                    f if f.starts_with("field_sources.") => values["field_sources"]
                        .get(&f["field_sources.".len()..])
                        .cloned()
                        .unwrap_or(Value::Null),
                    f if f.starts_with("manual_fields.") => {
                        let name = &f["manual_fields.".len()..];
                        if values["manual_fields"]
                            .as_array()
                            .unwrap()
                            .contains(&json!(name))
                        {
                            json!(true)
                        } else {
                            Value::Null
                        }
                    }
                    f => values[f].clone(),
                };
                let hlc = bibcontainer_core::Hlc::parse(hlc.as_str().unwrap()).unwrap();
                let mut op =
                    common::remote_op(hlc.device(), hlc.ms(), hlc.counter(), ent, id, json!({}));
                op.set.insert(field.clone(), value);
                common::apply(&mut other, &op);
            }
        }
    }
    assert_eq!(
        bibcontainer_core::dump_canonical(&lib.fx.conn).unwrap()["books"],
        bibcontainer_core::dump_canonical(&other).unwrap()["books"]
    );
    let _ = FieldChange::manual("x", json!(1));
}
