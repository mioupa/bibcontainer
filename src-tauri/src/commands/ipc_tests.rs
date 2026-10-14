//! コマンドを、フロントが送るのと同じ JSON の引数から呼んで確かめるテスト（Tauri の MockRuntime）。
//!
//! 確かめること: 引数のキー（camelCase）、省略できる引数、`FieldChange` の JSON、戻り値の形、
//! エラーの `{ kind, message }`、変更後の `library-changed`。

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

use crate::database::{DB_FILE_NAME, Database};
use crate::error::AppError;

struct Harness {
    _dir: tempfile::TempDir,
    dir: std::path::PathBuf,
    // App を落とすと WebView も消えるので、テストの間は持っておく
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    events: Arc<Mutex<Vec<Value>>>,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join(DB_FILE_NAME), &dir.path().join("backups"));
        Self::with_db(dir, db)
    }

    fn failed() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::failed(AppError::DbUnavailable {
            reason: "開けない".into(),
        });
        Self::with_db(dir, db)
    }

    fn with_db(dir: tempfile::TempDir, db: Database) -> Self {
        let app = mock_builder()
            .manage(Arc::new(db))
            .invoke_handler(tauri::generate_handler![
                super::library::list_books,
                super::library::get_rows,
                super::library::get_book_detail,
                super::library::search_books,
                super::library::find_by_isbn,
                super::library::purchase_store_suggestions,
                super::library::pending_book_ids,
                super::books::create_book,
                super::books::create_books,
                super::books::update_book,
                super::books::update_books,
                super::books::modify_tags,
                super::books::trash_books,
                super::books::restore_books,
                super::books::purge_books,
                super::organize::list_locations,
                super::organize::create_location,
                super::organize::rename_location,
                super::organize::move_location,
                super::organize::reorder_locations,
                super::organize::delete_location,
                super::organize::list_tags,
                super::organize::rename_tag,
                super::organize::set_tag_color,
                super::organize::list_series,
                super::organize::find_or_create_series,
                super::organize::rename_series,
                super::barcode::classify_code,
                super::barcode::c_code_table,
                super::barcode::describe_c_code,
                super::settings::get_scan_settings,
                super::settings::set_scan_settings,
                super::settings::get_display_settings,
                super::settings::set_display_settings,
                super::settings::get_list_settings,
                super::settings::set_list_settings,
                super::settings::get_price_settings,
                super::settings::set_price_settings,
                super::io::export_csv,
                super::io::export_json,
                super::io::parse_isbn_list,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        app.listen(crate::events::LIBRARY_CHANGED, move |event| {
            let payload: Value = serde_json::from_str(event.payload()).unwrap();
            sink.lock().unwrap().push(payload);
        });
        Harness {
            dir: dir.path().to_owned(),
            _dir: dir,
            _app: app,
            webview,
            events,
        }
    }

    /// コマンドを呼ぶ。成功は `Ok(戻り値)`、失敗は `Err({ kind, message })`。
    fn call(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            // アプリのページの URL は OS で違う（WebView2 は http://tauri.localhost）。違うと権限の確認で拒否される
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.webview, request).map(|b| b.deserialize::<Value>().unwrap())
    }

    fn ok(&self, cmd: &str, body: Value) -> Value {
        self.call(cmd, body)
            .unwrap_or_else(|err| panic!("{cmd} が失敗した: {err}"))
    }

    fn err_kind(&self, cmd: &str, body: Value) -> String {
        let err = self
            .call(cmd, body)
            .expect_err(&format!("{cmd} は失敗するはず"));
        assert!(err["message"].is_string(), "{err}");
        err["kind"].as_str().unwrap().to_owned()
    }

    /// これまでに届いた `library-changed` のペイロードを取り出す（届くのを少し待つ）。
    fn take_events(&self) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_millis(300);
        loop {
            let mut events = self.events.lock().unwrap();
            if !events.is_empty() || Instant::now() >= deadline {
                return std::mem::take(&mut *events);
            }
            drop(events);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

fn manual(field: &str, value: Value) -> Value {
    json!({ "field": field, "value": value, "origin": { "kind": "manual" } })
}

fn create(h: &Harness, changes: Vec<Value>) -> String {
    h.ok("create_book", json!({ "changes": changes }))
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn creating_and_reading_books_with_the_json_the_frontend_sends() {
    let h = Harness::new();
    assert_eq!(h.ok("list_books", json!({})), json!([]));

    let id = create(
        &h,
        vec![
            manual("title", json!("吾輩は猫である")),
            manual("isbn13", json!("9784003101117")),
            manual("tags", json!(["小説", "研究/SiC"])),
            manual(
                "authors",
                json!([{ "name": "夏目漱石", "kana": null, "role": "著" }]),
            ),
        ],
    );
    // タグを含むので、タグ・シリーズの一覧も取り直させる
    assert_eq!(h.take_events(), [json!({ "books": [id], "others": true })]);

    let rows = h.ok("list_books", json!({}));
    let row = &rows[0];
    assert_eq!(row["id"], id.as_str());
    assert_eq!(row["title"], "吾輩は猫である");
    assert_eq!(row["isbn13"], "9784003101117");
    assert_eq!(row["tags"], json!(["小説", "研究/SiC"]));
    assert_eq!(row["authors"][0]["name"], "夏目漱石");
    assert_eq!(row["ownership"], "owned");
    assert!(row["sortTitle"].is_string(), "camelCase のキー");

    assert_eq!(
        h.ok("get_rows", json!({ "ids": [id, "存在しない"] }))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let detail = h.ok("get_book_detail", json!({ "id": id }));
    assert_eq!(detail["title"], "吾輩は猫である");
    assert_eq!(detail["fieldSources"]["title"], "manual");
    assert_eq!(h.ok("search_books", json!({ "query": "猫" })), json!([id]));
    assert_eq!(
        h.ok("search_books", json!({ "query": "存在しない語" })),
        json!([])
    );
    let dup = h.ok("find_by_isbn", json!({ "isbn": "978-4-00-310111-7" }));
    assert_eq!(dup[0]["id"], id.as_str());
    assert_eq!(dup[0]["deleted"], false);
}

#[test]
fn updating_with_and_without_options_and_the_manual_edit_protection() {
    let h = Harness::new();
    let id = create(&h, vec![manual("title", json!("元の書名"))]);
    h.take_events();

    // options は省ける。書いた項目が返り、library-changed が出る（タグ・シリーズではないので others は false）
    let report = h.ok(
        "update_book",
        json!({ "id": id, "changes": [manual("publisher", json!("岩波書店"))] }),
    );
    assert_eq!(
        report,
        json!({ "id": id, "written": ["publisher"], "skippedManual": [] })
    );
    assert_eq!(h.take_events(), [json!({ "books": [id], "others": false })]);

    // 同じ値の保存は、何も書かず、通知も出さない
    let report = h.ok(
        "update_book",
        json!({ "id": id, "changes": [manual("publisher", json!("岩波書店"))], "options": { "overrideManual": false } }),
    );
    assert_eq!(report["written"], json!([]));
    assert!(h.take_events().is_empty());

    // 手で直した項目は、取得元の値で上書きしない。overrideManual なら上書きする（FR-META-07）
    let provider = json!({ "field": "title", "value": "取得した書名", "origin": { "kind": "provider", "name": "openbd" } });
    let report = h.ok(
        "update_books",
        json!({ "ids": [id], "changes": [provider.clone()] }),
    );
    assert_eq!(report[0]["skippedManual"], json!(["title"]));
    assert!(h.take_events().is_empty());
    let report = h.ok(
        "update_books",
        json!({ "ids": [id], "changes": [provider], "options": { "overrideManual": true } }),
    );
    assert_eq!(report[0]["written"], json!(["title"]));
    assert_eq!(
        h.ok("get_book_detail", json!({ "id": id }))["fieldSources"]["title"],
        "openbd"
    );
}

#[test]
fn bulk_tag_changes_announce_the_changed_books() {
    let h = Harness::new();
    let a = create(&h, vec![manual("title", json!("A"))]);
    let b = create(&h, vec![manual("title", json!("B"))]);
    h.take_events();

    assert_eq!(
        h.ok(
            "modify_tags",
            json!({ "ids": [a, b], "add": ["x"], "remove": [] })
        ),
        json!([a, b])
    );
    assert_eq!(
        h.take_events(),
        [json!({ "books": [a, b], "others": true })]
    );
}

#[test]
fn trash_restore_and_purge() {
    let h = Harness::new();
    let a = create(&h, vec![manual("title", json!("A"))]);
    h.take_events();

    assert_eq!(h.ok("trash_books", json!({ "ids": [a] })), json!([a]));
    assert_eq!(h.take_events(), [json!({ "books": [a], "others": true })]);
    assert_eq!(h.ok("get_rows", json!({ "ids": [a] }))[0]["deleted"], true);
    // 既にゴミ箱の本は変わらず、通知も出ない
    assert_eq!(h.ok("trash_books", json!({ "ids": [a] })), json!([]));
    assert!(h.take_events().is_empty());

    assert_eq!(h.ok("restore_books", json!({ "ids": [a] })), json!([a]));
    assert_eq!(h.ok("get_rows", json!({ "ids": [a] }))[0]["deleted"], false);

    assert_eq!(h.ok("purge_books", json!({ "ids": [a] })), json!([a]));
    assert_eq!(h.ok("get_rows", json!({ "ids": [a] })), json!([]));
    // 完全削除した本は、戻せない・変更できない
    assert_eq!(
        h.err_kind("restore_books", json!({ "ids": [a] })),
        "conflict"
    );
    assert_eq!(
        h.err_kind(
            "update_book",
            json!({ "id": a, "changes": [manual("title", json!("x"))] })
        ),
        "conflict"
    );
}

#[test]
fn errors_come_back_as_kind_and_message() {
    let h = Harness::new();
    let id = create(&h, vec![manual("title", json!("A"))]);

    assert_eq!(
        h.err_kind(
            "update_book",
            json!({ "id": id, "changes": [manual("rating", json!(9))] })
        ),
        "validation"
    );
    let err = h
        .call(
            "update_book",
            json!({ "id": id, "changes": [manual("rating", json!(9))] }),
        )
        .unwrap_err();
    assert!(
        err["message"].as_str().unwrap().contains("rating"),
        "項目名が入る: {err}"
    );
    assert_eq!(
        h.err_kind("get_book_detail", json!({ "id": "無い" })),
        "not_found"
    );
    assert_eq!(
        h.err_kind(
            "update_book",
            json!({ "id": "無い", "changes": [manual("title", json!("x"))] })
        ),
        "not_found"
    );
    // 1 冊でも失敗したら全体を取り消す
    assert_eq!(
        h.err_kind(
            "update_books",
            json!({ "ids": [id, "無い"], "changes": [manual("title", json!("書き換え"))] })
        ),
        "not_found"
    );
    assert_eq!(h.ok("get_book_detail", json!({ "id": id }))["title"], "A");
    assert_eq!(
        h.err_kind(
            "create_book",
            json!({ "changes": [manual("no_such_field", json!(1))] })
        ),
        "validation"
    );
}

#[test]
fn locations_tags_and_series() {
    let h = Harness::new();
    let home = h.ok(
        "create_location",
        json!({ "name": "自宅", "parentId": null }),
    );
    let study = h.ok(
        "create_location",
        json!({ "name": "書斎", "parentId": home }),
    );
    let attic = h.ok(
        "create_location",
        json!({ "name": "屋根裏", "parentId": null }),
    );
    assert_eq!(h.take_events().len(), 3);

    let list = h.ok("list_locations", json!({}));
    assert_eq!(list[1]["path"], "自宅 > 書斎");
    assert_eq!(list[1]["depth"], 1);

    h.ok(
        "move_location",
        json!({ "id": study, "newParentId": attic }),
    );
    h.ok("move_location", json!({ "id": study, "newParentId": null }));
    h.ok(
        "reorder_locations",
        json!({ "parentId": null, "orderedIds": [attic, home, study] }),
    );
    h.ok("rename_location", json!({ "id": home, "name": "実家" }));
    assert_eq!(
        h.err_kind("move_location", json!({ "id": home, "newParentId": home })),
        "conflict"
    );

    // 本があると、保管場所は削除できない
    let book = create(
        &h,
        vec![
            manual("title", json!("本")),
            manual("location_id", json!(attic)),
        ],
    );
    assert_eq!(
        h.err_kind("delete_location", json!({ "id": attic })),
        "conflict"
    );
    h.ok(
        "update_book",
        json!({ "id": book, "changes": [manual("location_id", Value::Null)] }),
    );
    h.ok("delete_location", json!({ "id": attic }));
    h.take_events();

    // タグ: `old`・`new` のキー。子孫もまとめて変わる。本の ID が通知に入る
    h.ok(
        "update_book",
        json!({ "id": book, "changes": [manual("tags", json!(["研究", "研究/SiC"]))] }),
    );
    h.take_events();
    assert_eq!(
        h.ok("rename_tag", json!({ "old": "研究", "new": "研究2" })),
        json!([book])
    );
    assert_eq!(
        h.take_events(),
        [json!({ "books": [book], "others": true })]
    );
    assert_eq!(
        h.ok("get_rows", json!({ "ids": [book] }))[0]["tags"],
        json!(["研究2", "研究2/SiC"])
    );
    h.ok(
        "set_tag_color",
        json!({ "name": "研究2", "color": "#3366ff" }),
    );
    let tags = h.ok("list_tags", json!({}));
    assert_eq!(tags[0]["name"], "研究2");
    assert_eq!(tags[0]["color"], "#3366ff");
    assert_eq!(
        h.err_kind("rename_tag", json!({ "old": "無い", "new": "x" })),
        "not_found"
    );

    // シリーズ: 名前を変えると、その本の行（seriesName）が変わるので、本の ID も通知する
    let series = h.ok("find_or_create_series", json!({ "name": "岩波文庫" }));
    h.ok(
        "update_book",
        json!({ "id": book, "changes": [manual("series_id", json!(series))] }),
    );
    h.take_events();
    h.ok(
        "rename_series",
        json!({ "id": series, "name": "岩波文庫（青）" }),
    );
    assert_eq!(
        h.take_events(),
        [json!({ "books": [book], "others": true })]
    );
    assert_eq!(
        h.ok("get_rows", json!({ "ids": [book] }))[0]["seriesName"],
        "岩波文庫（青）"
    );
    assert_eq!(h.ok("list_series", json!({}))[0]["bookCount"], 1);
}

#[test]
fn barcode_commands_return_the_documented_shapes() {
    let h = Harness::new();
    assert_eq!(
        h.ok("classify_code", json!({ "raw": "4003101111" })),
        json!({ "kind": "isbn13", "raw": "4003101111", "isbn13": "9784003101117", "isbn10": "4003101111" })
    );
    let second = h.ok("classify_code", json!({ "raw": "1920193007008" }));
    assert_eq!(second["kind"], "secondTier");
    assert_eq!(second["cCode"], "0193");
    assert_eq!(second["listPrice"], 700);
    assert_eq!(
        h.ok("classify_code", json!({ "raw": "9784003101118" }))["reason"],
        "checksum"
    );

    let table = h.ok("c_code_table", json!({}));
    assert_eq!(table["audience"][0], json!({ "code": "0", "name": "一般" }));
    assert!(table["content"].as_array().unwrap().len() > 50);
    assert_eq!(
        h.ok("describe_c_code", json!({ "code": "0193" }))["code"],
        "0193"
    );
    assert_eq!(
        h.ok("describe_c_code", json!({ "code": "ab" })),
        Value::Null
    );
}

#[test]
fn settings_round_trip_with_partial_input_and_clamping() {
    let h = Harness::new();
    let defaults = h.ok("get_scan_settings", json!({}));
    assert_eq!(defaults["terminator"], "enter");
    assert_eq!(defaults["timeoutSec"], 10);
    assert_eq!(
        defaults["sounds"]["success"],
        json!({ "enabled": true, "volume": 0.6 })
    );

    // 一部の項目だけでも保存できる。範囲外は収め、返る値は保存した値
    let saved = h.ok(
        "set_scan_settings",
        json!({ "settings": { "timeoutSec": 99, "muted": true } }),
    );
    assert_eq!(saved["timeoutSec"], 30);
    assert_eq!(saved["muted"], true);
    assert_eq!(saved["keyIntervalMs"], 30);
    assert_eq!(h.ok("get_scan_settings", json!({})), saved);

    let list = h.ok(
        "set_list_settings",
        json!({ "settings": { "sorting": [{ "id": "authors", "desc": true }], "sidebarOpen": false } }),
    );
    assert_eq!(list["sidebarOpen"], false);
    assert_eq!(list["detailOpen"], true);
    assert_eq!(
        h.ok("get_list_settings", json!({}))["sorting"][0]["id"],
        "authors"
    );

    assert_eq!(
        h.ok("get_display_settings", json!({})),
        json!({ "theme": "system", "fontFamily": null, "fontSize": 16 })
    );
    h.ok(
        "set_display_settings",
        json!({ "settings": { "theme": "dark", "fontSize": 18 } }),
    );
    assert_eq!(h.ok("get_display_settings", json!({}))["theme"], "dark");
    assert_eq!(
        h.ok("get_price_settings", json!({})),
        json!({ "showTaxIncluded": false, "taxRatePercent": 10 })
    );
    h.ok(
        "set_price_settings",
        json!({ "settings": { "showTaxIncluded": true } }),
    );
    assert_eq!(
        h.ok("get_price_settings", json!({}))["showTaxIncluded"],
        true
    );
    // 型の違う値は、コマンドの引数として受け付けない
    assert!(
        h.call(
            "set_scan_settings",
            json!({ "settings": { "terminator": "gamepad" } })
        )
        .is_err()
    );
}

#[test]
fn export_and_isbn_list_commands_use_absolute_paths() {
    let h = Harness::new();
    let a = create(
        &h,
        vec![
            manual("title", json!("=SUM(A1)")),
            manual("isbn13", json!("9784003101117")),
        ],
    );
    create(&h, vec![manual("title", json!("B"))]);

    let csv = h.path("蔵書.csv");
    assert_eq!(h.ok("export_csv", json!({ "path": csv })), json!(2));
    let bytes = std::fs::read(&csv).unwrap();
    assert_eq!(&bytes[..3], b"\xEF\xBB\xBF");
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("'=SUM(A1)"), "数式注入の対策: {text}");

    assert_eq!(
        h.ok(
            "export_csv",
            json!({ "path": h.path("一部.csv"), "ids": [a] })
        ),
        json!(1)
    );
    h.ok("export_json", json!({ "path": h.path("全部.json") }));
    let json_text = std::fs::read_to_string(h.path("全部.json")).unwrap();
    assert!(json_text.contains("bibcontainer-library"));

    assert_eq!(
        h.err_kind("export_csv", json!({ "path": "相対.csv" })),
        "validation"
    );
    assert_eq!(
        h.err_kind("export_json", json!({ "path": h.path("無い/x.json") })),
        "validation"
    );

    let list = h.path("isbn.txt");
    std::fs::write(&list, "9784003101117\n\n 1920193007008 \nabc\n").unwrap();
    let codes = h.ok("parse_isbn_list", json!({ "path": list }));
    let kinds: Vec<&str> = codes
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["isbn13", "secondTier", "invalid"]);
    assert_eq!(
        h.err_kind("parse_isbn_list", json!({ "path": h.path("無い.txt") })),
        "io"
    );
}

#[test]
fn every_database_command_reports_db_unavailable_when_the_db_could_not_be_opened() {
    let h = Harness::failed();
    for (cmd, body) in [
        ("list_books", json!({})),
        ("get_rows", json!({ "ids": [] })),
        ("create_book", json!({ "changes": [] })),
        ("trash_books", json!({ "ids": [] })),
        ("list_locations", json!({})),
        ("get_scan_settings", json!({})),
        (
            "export_csv",
            json!({ "path": Path::new("/tmp").join("x.csv").to_string_lossy() }),
        ),
    ] {
        assert_eq!(h.err_kind(cmd, body), "db_unavailable", "{cmd}");
    }
    // 計算だけのコマンドは、DB がなくても動く
    assert_eq!(
        h.ok("classify_code", json!({ "raw": "9784003101117" }))["kind"],
        "isbn13"
    );
}
