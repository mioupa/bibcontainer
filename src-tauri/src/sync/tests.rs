//! 同期の結合テスト。アプリ（`Harness`。DB とコントローラと MockRuntime のウィンドウ）を 2 つ作って、1 つの同期フォルダで
//! つなぎ、コマンドの経路（`create_book` → 書き出し → もう一方で取り込み → `list_books`）を通す。
//!
//! スケジューラ（タイマー）は始めず、スケジューラが呼ぶ周期（`run_due`）とコマンドを直接呼ぶ。タイマーそのものは
//! `schedule.rs` の単体テストと、ビルドしたアプリでの確認（0016）で見る。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bibcontainer_sync::format::sha256_hex;
use bibcontainer_sync::layout::cover_local_path;
use serde_json::{Value, json};
use tauri::async_runtime::block_on;

use super::schedule::Due;
use super::settings::SyncSettings;
use crate::commands::ipc_tests::{Harness, create, manual};
use crate::settings;

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn library_root(folder: &Path) -> PathBuf {
    folder.join("bibcontainer-library")
}

fn setup(h: &Harness, folder: &Path, mode: &str) -> Value {
    h.ok(
        "setup_sync",
        json!({ "path": path_str(folder), "mode": mode }),
    )
}

fn inspect(h: &Harness, folder: &Path) -> Value {
    h.ok("inspect_sync_folder", json!({ "path": path_str(folder) }))
}

fn status(h: &Harness) -> Value {
    h.ok("get_sync_status", json!({}))
}

fn new_book(h: &Harness, title: &str) -> String {
    create(h, vec![manual("title", json!(title))])
}

fn titles(h: &Harness) -> Vec<String> {
    let mut titles: Vec<String> = h
        .ok("list_books", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["title"].as_str().unwrap().to_owned())
        .collect();
    titles.sort();
    titles
}

/// スケジューラが、最後の変更の 5 秒後に行う書き出し。
fn export(h: &Harness) {
    block_on(h.sync.run_due(Due {
        export: true,
        ..Due::default()
    }));
}

/// スケジューラが、ファイル監視・前面に来たとき・60 秒ごとに行う取り込み。
fn import(h: &Harness) {
    block_on(h.sync.run_due(Due {
        import: true,
        ..Due::default()
    }));
}

/// バックアップの保存先を、このアプリの一時フォルダの `backups` にする（既定の場所は、MockRuntime の
/// アプリのデータフォルダ＝実際のユーザーのフォルダになってしまうため）。
fn pin_backup_dir(h: &Harness) -> PathBuf {
    let dir = h.dir.join("backups");
    let value = json!({ "directory": path_str(&dir) });
    block_on(
        h.db.run(move |conn, _| Ok(bibcontainer_core::settings::set(conn, "backup", &value)?)),
    )
    .unwrap();
    dir
}

/// 参加の記録（ライブラリ ID）はあるが、同期フォルダの設定がない状態にする（設定の途中で止まったあと、など）。
fn forget_folder(h: &Harness) {
    block_on(h.db.run(|conn, _| {
        let mut current: SyncSettings = settings::load(conn)?;
        current.folder = None;
        settings::save(conn, current)?;
        Ok(())
    }))
    .unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn wait_until(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "{what} が間に合わなかった");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn manual_backups(h: &Harness) -> Vec<Value> {
    h.ok("list_backups", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["kind"] == "manual")
        .cloned()
        .collect()
}

#[test]
fn before_setup_the_status_is_disabled_with_the_default_device_name() {
    let h = Harness::new();
    let st = status(&h);
    assert_eq!(st["state"], "disabled");
    assert_eq!(st["folder"], Value::Null);
    assert_eq!(st["libraryId"], Value::Null);
    assert_eq!(st["deviceName"], "テスト端末");
    assert!(!st["deviceId"].as_str().unwrap().is_empty());
    assert_eq!(st["pendingOps"], 0);
    assert_eq!(st["devices"], json!([]));
    assert_eq!(st["error"], Value::Null);
    assert!(!h.sync.is_configured());
}

#[test]
fn inspect_resolves_the_library_folder_and_leaves_nothing_behind() {
    let h = Harness::new();
    let folder = tempfile::tempdir().unwrap();
    let found = inspect(&h, folder.path());
    assert_eq!(found["kind"], "empty");
    assert_eq!(
        found["libraryFolder"],
        path_str(&library_root(folder.path()))
    );
    assert_eq!(found["libraryId"], Value::Null);
    assert_eq!(found["localHasData"], false);
    assert_eq!(found["localLibraryId"], Value::Null);
    assert_eq!(found["devices"], json!([]));
    assert!(
        !library_root(folder.path()).exists(),
        "調べただけでは、フォルダを作らない"
    );

    // 絶対パスだけ受け付ける
    for body in [json!({ "path": "relative/dir" }), json!({ "path": "  " })] {
        assert_eq!(h.err_kind("inspect_sync_folder", body), "validation");
    }
    assert_eq!(
        h.err_kind("setup_sync", json!({ "path": "", "mode": "create" })),
        "validation"
    );
    // 手元にデータがあれば、そう伝える
    new_book(&h, "本");
    assert_eq!(inspect(&h, folder.path())["localHasData"], true);
}

#[test]
fn books_registered_on_one_app_appear_on_the_other_through_the_command_path() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    let b = Harness::new();

    // A: 新しいライブラリを作る（手元は空）
    let st = setup(&a, folder.path(), "create");
    assert_eq!(st["state"], "idle");
    assert_eq!(
        st["folder"],
        path_str(&library_root(folder.path())),
        "選んだフォルダの下の bibcontainer-library を使う"
    );
    let library_id = st["libraryId"].as_str().unwrap().to_owned();
    assert!(library_root(folder.path()).join("library.json").is_file());
    assert!(a.sync.is_configured());

    // B: ライブラリが見つかり、手元が空なので、参加する
    let found = inspect(&b, folder.path());
    assert_eq!(found["kind"], "library");
    assert_eq!(found["libraryId"], library_id.as_str());
    assert_eq!(found["localHasData"], false);
    let devices = found["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["isSelf"], false);
    assert_eq!(devices[0]["name"], "テスト端末");
    let st_b = setup(&b, folder.path(), "join");
    assert_eq!(st_b["libraryId"], library_id.as_str());
    assert_eq!(st_b["state"], "idle");
    assert_ne!(st_b["deviceId"], st["deviceId"]);
    assert_eq!(st_b["devices"].as_array().unwrap().len(), 2);

    // A: 本を登録する → 未書き出しの Op がたまり、書き出すと 0 になる
    let id = new_book(&a, "吾輩は猫である");
    assert!(status(&a)["pendingOps"].as_u64().unwrap() > 0);
    export(&a);
    let st = status(&a);
    assert_eq!(st["pendingOps"], 0);
    assert!(st["lastExportAt"].is_string());
    assert_eq!(st["state"], "idle");

    // B: 取り込む → 一覧に出て、library-changed が届く
    b.take_events();
    import(&b);
    assert_eq!(titles(&b), ["吾輩は猫である"]);
    assert_eq!(b.take_events(), [json!({ "books": [id], "others": false })]);
    let st_b = status(&b);
    assert!(st_b["lastImportAt"].is_string());
    assert_eq!(st_b["pendingOps"], 0);

    // 逆向き: B の変更を、「今すぐ同期」（取り込み＋書き出し）で届け、A も「今すぐ同期」で受け取る
    b.ok(
        "update_book",
        json!({ "id": id, "changes": [manual("title", json!("坊っちゃん"))] }),
    );
    let after = b.ok("sync_now", json!({}));
    assert_eq!(after["state"], "idle");
    assert_eq!(after["pendingOps"], 0);
    a.take_events();
    let after = a.ok("sync_now", json!({}));
    assert_eq!(after["state"], "idle");
    assert_eq!(titles(&a), ["坊っちゃん"]);
    assert_eq!(a.take_events(), [json!({ "books": [id], "others": false })]);

    // 同期した結果が、sync-status のイベントでも届いている
    let events = a.status_events.lock().unwrap().clone();
    let last = events.last().expect("sync-status が届く");
    assert_eq!(last["state"], "idle");
    assert_eq!(last["folder"], path_str(&library_root(folder.path())));
    assert_eq!(last["pendingOps"], 0);
}

#[test]
fn a_new_controller_on_the_same_database_sees_the_same_sync_state() {
    // 設定は DB（local_settings と local_meta）にあるので、コントローラを作り直しても（アプリを再起動しても）状態は同じ
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    setup(&a, folder.path(), "create");
    new_book(&a, "本");
    export(&a);
    let fresh = crate::sync::build_controller(
        a.app_handle(),
        std::sync::Arc::clone(&a.db),
        crate::sync::SyncConfig {
            covers_dir: a.dir.join("covers"),
            default_backup_dir: a.dir.join("backups"),
            os: "testos".to_owned(),
            app_version: "0.1.0".to_owned(),
            default_device_name: "テスト端末".to_owned(),
        },
        std::sync::Arc::new(bibcontainer_core::SystemClock),
    );
    let st = block_on(fresh.status()).unwrap();
    assert_eq!(st.folder, Some(path_str(&library_root(folder.path()))));
    assert_eq!(st.pending_ops, 0);
    // 再起動直後の同期（起動時の周期）で、同期フォルダの端末の一覧が入る
    assert!(block_on(fresh.sync_now()).is_ok());
    assert_eq!(block_on(fresh.status()).unwrap().devices.len(), 1);
}

#[test]
fn a_device_with_data_cannot_just_join_and_merge_keeps_both() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    setup(&a, folder.path(), "create");
    new_book(&a, "ライブラリの本");
    export(&a);

    let d = Harness::new();
    new_book(&d, "D だけの本");
    let found = inspect(&d, folder.path());
    assert_eq!(found["kind"], "library");
    assert_eq!(found["localHasData"], true);
    // そのままの参加は断る。何も変わらない
    assert_eq!(
        d.err_kind(
            "setup_sync",
            json!({ "path": path_str(folder.path()), "mode": "join" })
        ),
        "conflict"
    );
    assert_eq!(status(&d)["state"], "disabled");
    assert_eq!(titles(&d), ["D だけの本"]);

    // 統合: 両方残り、ライブラリの側にも D の本が届く
    let st = setup(&d, folder.path(), "joinMerge");
    assert_eq!(st["state"], "idle");
    assert_eq!(titles(&d), ["D だけの本", "ライブラリの本"]);
    import(&a);
    assert_eq!(titles(&a), ["D だけの本", "ライブラリの本"]);
}

#[test]
fn replacing_backs_up_rebuilds_the_database_and_keeps_the_device_settings() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    setup(&a, folder.path(), "create");
    new_book(&a, "ライブラリの本");
    export(&a);

    let c = Harness::new();
    let backups = pin_backup_dir(&c);
    new_book(&c, "C だけの本");
    c.ok(
        "set_scan_settings",
        json!({ "settings": { "muted": true } }),
    );
    c.ok("set_device_name", json!({ "name": "書斎の Mac" }));
    let device_before = status(&c)["deviceId"].clone();
    let found = inspect(&c, folder.path());
    assert_eq!(found["kind"], "library");
    assert_eq!(found["localHasData"], true);

    c.take_events();
    let st = setup(&c, folder.path(), "joinReplace");
    assert_eq!(st["state"], "idle");
    assert_eq!(titles(&c), ["ライブラリの本"], "手元の本は置き換わる");
    assert_ne!(st["deviceId"], device_before, "新しい端末 ID");

    // 端末ごとの設定は、新しい DB に引き継がれる（同期の設定の端末名も、バックアップの保存先も）
    assert_eq!(c.ok("get_scan_settings", json!({}))["muted"], true);
    assert_eq!(st["deviceName"], "書斎の Mac");
    // 置き換え前の DB が、手動バックアップとして残っている
    let made = manual_backups(&c);
    assert_eq!(made.len(), 1, "{made:?}");
    assert!(
        made[0]["path"]
            .as_str()
            .unwrap()
            .starts_with(&path_str(&backups))
    );
    let backup = rusqlite::Connection::open_with_flags(
        made[0]["path"].as_str().unwrap(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let old_titles: Vec<String> = backup
        .prepare("SELECT title FROM books")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(old_titles, ["C だけの本"]);

    // 画面は全部取り直す（DB が入れ替わった）
    assert!(
        c.take_events()
            .contains(&json!({ "books": [], "others": true }))
    );
    // 参加が済んでいるので、そのあとは普通に同期できる
    new_book(&c, "置き換えたあとの本");
    export(&c);
    import(&a);
    assert_eq!(titles(&a), ["ライブラリの本", "置き換えたあとの本"]);
}

/// 置き換えで DB を作り直したあとに、設定が途中で失敗したとき（ここでは、同期フォルダの `devices/` に書けない）。
/// 画面は取り直す（DB は入れ替わっている）。参加は記録されているので、フォルダも覚えて、次の同期が続きから行う。
#[cfg(unix)]
#[test]
fn a_failure_after_the_replacement_still_reloads_the_library_and_the_next_sync_continues() {
    use std::os::unix::fs::PermissionsExt;

    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    setup(&a, folder.path(), "create");
    new_book(&a, "ライブラリの本");
    export(&a);

    let c = Harness::new();
    pin_backup_dir(&c);
    new_book(&c, "C だけの本");
    let devices = library_root(folder.path()).join("devices");
    let writable = std::fs::metadata(&devices).unwrap().permissions();
    std::fs::set_permissions(&devices, std::fs::Permissions::from_mode(0o555)).unwrap();

    c.take_events();
    let failed = c.call(
        "setup_sync",
        json!({ "path": path_str(folder.path()), "mode": "joinReplace" }),
    );
    // 以降の確認で、フォルダを消せるよう、先に書き込めるように戻す
    std::fs::set_permissions(&devices, writable).unwrap();
    let err = failed.unwrap_err();
    assert_eq!(err["kind"], "io", "{err}");

    assert!(
        c.take_events()
            .contains(&json!({ "books": [], "others": true })),
        "DB は入れ替わったので、画面は取り直す"
    );
    assert_eq!(
        titles(&c),
        Vec::<String>::new(),
        "手元の本は置き換わっている"
    );
    assert_eq!(
        manual_backups(&c).len(),
        1,
        "置き換え前の DB はバックアップに残っている"
    );
    let st = status(&c);
    assert_eq!(st["folder"], path_str(&library_root(folder.path())));
    assert_eq!(st["state"], "error", "{st}");
    assert!(c.sync.is_configured());

    // 書き込めるようになったら、次の同期が続きから行う
    let st = c.ok("sync_now", json!({}));
    assert_eq!(st["state"], "idle");
    assert_eq!(titles(&c), ["ライブラリの本"]);
}

#[test]
fn replace_is_refused_before_touching_the_database_when_the_folder_cannot_be_joined() {
    let a = Harness::new();
    let backups = pin_backup_dir(&a);
    new_book(&a, "手元の本");
    let device_before = status(&a)["deviceId"].clone();

    // ライブラリのないフォルダでは、置き換えない（DB を作り直したあとで断ると、手元が空になる）
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        a.err_kind(
            "setup_sync",
            json!({ "path": path_str(empty.path()), "mode": "joinReplace" })
        ),
        "conflict"
    );

    // 別のライブラリに参加している端末が、さらに別のライブラリのフォルダで頼んでも、どの方法でも断る。
    // まず A をライブラリ X に参加させ、画面が未設定の状態（参加の記録だけが残っている）にする
    let x = tempfile::tempdir().unwrap();
    setup(&Harness::new(), x.path(), "create");
    setup(&a, x.path(), "joinMerge");
    forget_folder(&a);
    let y = tempfile::tempdir().unwrap();
    setup(&Harness::new(), y.path(), "create");
    let found = inspect(&a, y.path());
    assert_eq!(found["kind"], "otherLibrary");
    assert_eq!(found["localHasData"], true);
    for mode in ["join", "joinMerge", "joinReplace", "create"] {
        assert_eq!(
            a.err_kind(
                "setup_sync",
                json!({ "path": path_str(y.path()), "mode": mode })
            ),
            "conflict",
            "{mode}"
        );
    }
    assert_eq!(titles(&a), ["手元の本"]);
    assert_eq!(
        status(&a)["deviceId"],
        device_before,
        "端末 ID は変わらない"
    );
    assert_eq!(
        inspect(&a, x.path())["localLibraryId"],
        found["localLibraryId"],
        "参加の記録は変わらない"
    );
    assert!(manual_backups(&a).is_empty(), "バックアップも作らない");
    assert!(!backups.exists());
}

#[test]
fn choosing_the_same_library_at_another_path_only_moves_the_folder() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = Harness::new();
    let backups = pin_backup_dir(&a);
    let st = setup(&a, first.path(), "create");
    let library_id = st["libraryId"].as_str().unwrap().to_owned();
    let device = st["deviceId"].clone();
    new_book(&a, "本");
    export(&a);

    // 同期フォルダが別のパスから見えるようになった（Windows のドライブ文字が変わった、など）
    copy_dir(&library_root(first.path()), &library_root(second.path()));
    forget_folder(&a);
    assert_eq!(status(&a)["state"], "disabled");

    let found = inspect(&a, second.path());
    assert_eq!(found["kind"], "library");
    assert_eq!(found["libraryId"], library_id.as_str());
    assert_eq!(found["localLibraryId"], library_id.as_str());
    assert_eq!(found["localHasData"], true);

    // 参加し直す（統合でも置き換えでもなく、場所の付け替え）。DB もスナップショットも作らない
    let st = setup(&a, second.path(), "join");
    assert_eq!(st["state"], "idle");
    assert_eq!(st["folder"], path_str(&library_root(second.path())));
    assert_eq!(st["libraryId"], library_id.as_str());
    assert_eq!(st["deviceId"], device, "端末 ID は変わらない");
    assert_eq!(st["pendingOps"], 0);
    assert_eq!(titles(&a), ["本"]);
    let own = library_root(second.path())
        .join("snapshots")
        .join(device.as_str().unwrap());
    assert!(!own.exists(), "スナップショットは書かない");

    // 置き換えを頼まれても、同じライブラリなら DB は作り直さない
    forget_folder(&a);
    let st = setup(&a, second.path(), "joinReplace");
    assert_eq!(st["deviceId"], device);
    assert_eq!(titles(&a), ["本"]);
    assert!(!backups.exists(), "バックアップも作らない");

    // 新しい場所を読み書きしている
    let b = Harness::new();
    setup(&b, second.path(), "join");
    assert_eq!(titles(&b), ["本"]);
    new_book(&b, "B の本");
    export(&b);
    import(&a);
    assert_eq!(titles(&a), ["B の本", "本"]);
}

#[test]
fn an_empty_folder_while_attached_to_a_library_is_refused_without_creating_anything() {
    let first = tempfile::tempdir().unwrap();
    let a = Harness::new();
    let library_id = setup(&a, first.path(), "create")["libraryId"].clone();
    new_book(&a, "本");
    forget_folder(&a);

    let empty = tempfile::tempdir().unwrap();
    let found = inspect(&a, empty.path());
    assert_eq!(found["kind"], "empty");
    assert_eq!(found["localLibraryId"], library_id);

    let err = a
        .call(
            "setup_sync",
            json!({ "path": path_str(empty.path()), "mode": "create" }),
        )
        .unwrap_err();
    assert_eq!(err["kind"], "conflict");
    assert!(
        err["message"].as_str().unwrap().contains("同期を解除"),
        "{err}"
    );
    assert!(
        !library_root(empty.path()).exists(),
        "新しいライブラリを作らない"
    );
    assert_eq!(
        inspect(&a, empty.path())["localLibraryId"],
        library_id,
        "参加の記録は変わらない"
    );
}

#[test]
fn leaving_keeps_the_data_and_the_folder_and_the_same_library_can_be_merged_back() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    let library_id = setup(&a, folder.path(), "create")["libraryId"].clone();
    new_book(&a, "本 1");
    export(&a);
    let files_before = std::fs::read_dir(library_root(folder.path()).join("ops"))
        .unwrap()
        .count();

    let st = a.ok("leave_sync", json!({}));
    assert_eq!(st["state"], "disabled");
    assert_eq!(st["folder"], Value::Null);
    assert!(!a.sync.is_configured());
    assert_eq!(titles(&a), ["本 1"], "手元のデータはそのまま");
    assert_eq!(
        std::fs::read_dir(library_root(folder.path()).join("ops"))
            .unwrap()
            .count(),
        files_before,
        "同期フォルダの中身もそのまま"
    );
    // 解除したあとの変更は、書き出されない（未設定なので）
    new_book(&a, "離れている間の本");
    assert_eq!(
        status(&a)["pendingOps"],
        0,
        "未設定の状態は、件数を出さない"
    );
    assert_eq!(a.err_kind("sync_now", json!({})), "conflict");

    // 解除すると、同じフォルダは「ライブラリあり（手元にデータあり）」として調べ直せる
    let found = inspect(&a, folder.path());
    assert_eq!(found["kind"], "library");
    assert_eq!(found["libraryId"], library_id);
    assert_eq!(found["localLibraryId"], Value::Null);
    assert_eq!(found["localHasData"], true);
    setup(&a, folder.path(), "joinMerge");
    assert_eq!(titles(&a), ["本 1", "離れている間の本"]);

    // 別のライブラリが残っているとき、解除すれば、その同期フォルダの判定は「別のライブラリ」でなくなる
    a.ok("leave_sync", json!({}));
    assert_eq!(inspect(&a, folder.path())["kind"], "library");
}

#[test]
fn device_names_are_validated_saved_and_written_to_the_sync_folder() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    // 未設定のときも変えられる
    let st = a.ok("set_device_name", json!({ "name": "  机の Mac  " }));
    assert_eq!(st["deviceName"], "机の Mac");
    for bad in ["", "   ", &"あ".repeat(65)] {
        assert_eq!(
            a.err_kind("set_device_name", json!({ "name": bad })),
            "validation",
            "{bad:?}"
        );
    }
    assert_eq!(status(&a)["deviceName"], "机の Mac");
    assert_eq!(
        a.ok("set_device_name", json!({ "name": "あ".repeat(64) }))["deviceName"],
        "あ".repeat(64)
    );

    // 設定済みなら、同期フォルダの自分の記録も書き直す
    let st = setup(&a, folder.path(), "create");
    let device = st["deviceId"].as_str().unwrap().to_owned();
    assert_eq!(
        st["deviceName"],
        "あ".repeat(64),
        "設定しても名前は変わらない"
    );
    let st = a.ok("set_device_name", json!({ "name": "家の PC" }));
    assert_eq!(st["deviceName"], "家の PC");
    let file = library_root(folder.path())
        .join("devices")
        .join(format!("{device}.json"));
    let written: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(written["name"], "家の PC");
    let me = st["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["isSelf"] == true)
        .unwrap()
        .clone();
    assert_eq!(me["name"], "家の PC");
}

#[test]
fn an_unavailable_sync_folder_shows_an_error_but_local_work_goes_on() {
    let parent = tempfile::tempdir().unwrap();
    let drive = parent.path().join("drive");
    std::fs::create_dir_all(&drive).unwrap();
    let a = Harness::new();
    setup(&a, &drive, "create");
    new_book(&a, "本 1");
    export(&a);

    // Google Drive が未接続になった（Windows では、監視しているフォルダを含むフォルダは動かせないので、監視をやめる）
    a.sync.drop_watcher();
    let away = parent.path().join("drive-away");
    std::fs::rename(&drive, &away).unwrap();
    let err = a.call("sync_now", json!({})).unwrap_err();
    assert_eq!(err["kind"], "io");
    assert!(
        err["message"].as_str().unwrap().contains("同期フォルダ"),
        "{err}"
    );
    let st = status(&a);
    assert_eq!(st["state"], "error");
    assert!(st["error"].as_str().unwrap().contains("同期フォルダ"));
    assert!(
        !drive.exists() && !library_root(&drive).exists(),
        "空のフォルダを作らない"
    );

    // ローカルの操作は止まらない。未書き出しはたまる
    new_book(&a, "本 2");
    assert_eq!(titles(&a), ["本 1", "本 2"]);
    export(&a);
    assert_eq!(status(&a)["state"], "error");
    assert!(status(&a)["pendingOps"].as_u64().unwrap() > 0);

    // つながったら、書き出されて、エラーが消える
    std::fs::rename(&away, &drive).unwrap();
    let st = a.ok("sync_now", json!({}));
    assert_eq!(st["state"], "idle");
    assert_eq!(st["error"], Value::Null);
    assert_eq!(st["pendingOps"], 0);
    let b = Harness::new();
    setup(&b, &drive, "join");
    assert_eq!(titles(&b), ["本 1", "本 2"]);
}

#[test]
fn a_library_mismatch_is_reported_as_its_own_state() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = Harness::new();
    setup(&a, first.path(), "create");
    let z = Harness::new();
    setup(&z, second.path(), "create");
    // A の同期フォルダの library.json が、別のライブラリのものに入れ替わった
    std::fs::copy(
        library_root(second.path()).join("library.json"),
        library_root(first.path()).join("library.json"),
    )
    .unwrap();
    let err = a.call("sync_now", json!({})).unwrap_err();
    assert_eq!(err["kind"], "conflict");
    let st = status(&a);
    assert_eq!(st["state"], "mismatch");
    assert!(st["error"].as_str().unwrap().contains("別のライブラリ"));
}

#[test]
fn gc_covers_removes_old_unreferenced_covers_and_returns_the_count() {
    let h = Harness::new();
    let covers = h.dir.join("covers");
    let put = |bytes: &[u8], age_days: u64| {
        let hash = sha256_hex(bytes);
        let path = cover_local_path(&covers, &hash);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let modified = std::time::SystemTime::now() - Duration::from_secs(age_days * 24 * 60 * 60);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        path
    };
    let old = put(b"old cover", 31);
    let fresh = put(b"fresh cover", 1);
    assert_eq!(h.ok("gc_covers", json!({})), json!(1));
    assert!(!old.exists());
    assert!(fresh.exists());
    assert_eq!(h.ok("gc_covers", json!({})), json!(0));
}

#[test]
fn flushing_on_exit_exports_pending_changes_once_and_only_when_configured() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    // 未設定なら、何もしない（すぐ戻る）
    let started = Instant::now();
    block_on(a.sync.flush_on_exit(Duration::from_secs(5)));
    assert!(started.elapsed() < Duration::from_secs(2));

    setup(&a, folder.path(), "create");
    let b = Harness::new();
    setup(&b, folder.path(), "join");
    new_book(&a, "終了の直前の本");
    assert!(status(&a)["pendingOps"].as_u64().unwrap() > 0);
    assert!(a.sync.begin_exit(), "最初の終了の要求");
    assert!(!a.sync.begin_exit(), "2 回目は、書き出しのあとの終了");
    block_on(a.sync.flush_on_exit(Duration::from_secs(5)));
    assert_eq!(status(&a)["pendingOps"], 0);
    import(&b);
    assert_eq!(titles(&b), ["終了の直前の本"]);
}

#[test]
fn restoring_a_backup_while_syncing_resumes_with_a_snapshot_and_gets_exported_changes_back() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    pin_backup_dir(&a);
    setup(&a, folder.path(), "create");
    new_book(&a, "バックアップ前の本");
    export(&a);
    let backup = a.ok("manual_backup", json!({}));
    new_book(&a, "バックアップ後の本（書き出し済み）");
    export(&a);
    new_book(&a, "バックアップ後の本（書き出していない）");
    let old_device = status(&a)["deviceId"].clone();

    let report = a.ok(
        "restore_backup",
        json!({ "path": backup["path"].as_str().unwrap() }),
    );
    assert_ne!(report["newDeviceId"], old_device);
    // 復元の直後は、バックアップの時点の本だけ。そのあと同期が自動で始まり、書き出し済みの本が戻る
    wait_until("復元のあとの同期", Duration::from_secs(20), || {
        titles(&a).len() == 2
    });
    assert_eq!(
        titles(&a),
        ["バックアップ前の本", "バックアップ後の本（書き出し済み）"]
    );
    wait_until("状態が落ち着く", Duration::from_secs(10), || {
        let st = status(&a);
        st["state"] == "idle" && st["pendingOps"] == 0
    });
    assert_eq!(status(&a)["deviceId"], report["newDeviceId"]);

    // 復元した端末のスナップショットが書かれていて、ほかの端末が参加すると同じ内容になる
    let b = Harness::new();
    setup(&b, folder.path(), "join");
    assert_eq!(
        titles(&b),
        ["バックアップ前の本", "バックアップ後の本（書き出し済み）"]
    );
}

#[test]
fn operations_wait_for_the_exclusive_lock_that_a_restore_holds() {
    let folder = tempfile::tempdir().unwrap();
    let a = Harness::new();
    setup(&a, folder.path(), "create");
    // バックアップの復元は、この排他のロックを持ったまま DB を置き換える。持っている間、同期の処理は始まらない
    let guard = block_on(a.sync.exclusive());
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let task = {
        let sync = std::sync::Arc::clone(&a.sync);
        let done = std::sync::Arc::clone(&done);
        tauri::async_runtime::spawn(async move {
            let status = sync.leave().await;
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            status
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    assert!(!done.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        status(&a)["state"],
        "idle",
        "ロックを持っている間は、解除も始まらない"
    );
    drop(guard);
    let left = block_on(task).unwrap().unwrap();
    assert_eq!(left.state, bibcontainer_sync::SyncState::Disabled);
}
