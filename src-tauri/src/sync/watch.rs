//! 同期フォルダのファイル監視（FR-SYNC-06。`notify` クレート）。
//!
//! ライブラリのフォルダ（`bibcontainer-library`）を再帰で監視し、他の端末の変更になりうる通知が来たら
//! コールバックを呼ぶ（まとめる 2 秒は `schedule.rs` が行う）。Google Drive のストリーミングなどの仮想
//! ドライブは通知が届かないことがあるので、60 秒ごとの取り込みも続ける。
//!
//! 自分の書き込み（自分の `ops/<自分>`・`snapshots/<自分>`・`devices/<自分>.json`）と、書き込み途中の
//! `.tmp-*`、OS が作る隠しファイル、`covers/`（書影は、それを参照するセグメントが届いたときの取り込みで
//! 写す。自分の書影の写しが大量の通知になるのを避ける）の通知は、取り込みのきっかけにしない。

use std::path::{Component, Path, PathBuf};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// 監視している間、持っておく（落とすと監視をやめる）。
pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
}

/// `root`（`bibcontainer-library`）の監視を始める。`on_change` は、取り込みのきっかけになる通知のたびに、
/// 監視のスレッドから呼ばれる（すぐ戻ること）。フォルダがない・監視できないときは `Err`。
pub fn start(
    root: &Path,
    self_id: &str,
    on_change: impl Fn() + Send + 'static,
) -> notify::Result<FolderWatcher> {
    let filter = EventFilter::new(root, self_id);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        match event {
            Ok(event) if filter.is_relevant(&event) => on_change(),
            Ok(_) => {}
            // 監視が壊れたときなど。60 秒ごとの取り込みが続くので、警告だけにする。
            Err(err) => tracing::warn!(error = %err, "同期フォルダの監視でエラーが起きました"),
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(FolderWatcher { _watcher: watcher })
}

/// 通知を、取り込みのきっかけにするか決める。
#[derive(Debug, Clone)]
pub struct EventFilter {
    roots: Vec<PathBuf>,
    self_id: String,
}

impl EventFilter {
    pub fn new(root: &Path, self_id: &str) -> Self {
        // macOS の FSEvents は、リンクをたどった実パス（`/var` → `/private/var` など）で通知する。
        let mut roots = vec![root.to_path_buf()];
        if let Ok(real) = std::fs::canonicalize(root)
            && real != root
        {
            roots.push(real);
        }
        Self {
            roots,
            self_id: self_id.to_owned(),
        }
    }

    /// 通知が、他の端末の変更の知らせになりうるか。
    pub fn is_relevant(&self, event: &Event) -> bool {
        // 開く・閉じるだけ（読み取り）の通知は、変更ではない。
        if matches!(event.kind, EventKind::Access(_)) {
            return false;
        }
        event.paths.is_empty() || event.paths.iter().any(|p| self.path_is_relevant(p))
    }

    fn path_is_relevant(&self, path: &Path) -> bool {
        let relative = self
            .roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok());
        // ライブラリのフォルダの中でないものは、念のため知らせる（フォルダそのものの作成・削除など）
        let Some(relative) = relative else {
            return true;
        };
        let parts: Vec<String> = relative
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let Some(last) = parts.last() else {
            return true;
        };
        if parts.iter().any(|p| p.starts_with('.'))
            || last.eq_ignore_ascii_case("desktop.ini")
            || last.eq_ignore_ascii_case("thumbs.db")
        {
            return false;
        }
        match parts.first().map(String::as_str) {
            Some("covers") => false,
            Some("ops" | "snapshots") if parts.get(1) == Some(&self.self_id) => false,
            Some("devices")
                if parts.get(1).map(String::as_str) == Some(&format!("{}.json", self.self_id)) =>
            {
                false
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use notify::event::{AccessKind, CreateKind, ModifyKind};

    use super::*;

    fn filter() -> EventFilter {
        EventFilter::new(Path::new("/lib/bibcontainer-library"), "d-me")
    }

    fn created(path: &str) -> Event {
        Event::new(EventKind::Create(CreateKind::File)).add_path(PathBuf::from(path))
    }

    #[test]
    fn other_devices_files_are_relevant() {
        let f = filter();
        for path in [
            "/lib/bibcontainer-library/ops/d-other/0000000001-0000000003.jsonl",
            "/lib/bibcontainer-library/snapshots/d-other/001-00001.jsonl.gz",
            "/lib/bibcontainer-library/devices/d-other.json",
            "/lib/bibcontainer-library/library.json",
            "/lib/bibcontainer-library/ops",
            "/lib/bibcontainer-library",
        ] {
            assert!(f.is_relevant(&created(path)), "{path}");
        }
    }

    #[test]
    fn my_own_files_temp_files_hidden_files_and_covers_are_not() {
        let f = filter();
        for path in [
            "/lib/bibcontainer-library/ops/d-me/0000000001-0000000003.jsonl",
            "/lib/bibcontainer-library/ops/d-me/.tmp-1234",
            "/lib/bibcontainer-library/snapshots/d-me/001-00001.jsonl.gz",
            "/lib/bibcontainer-library/devices/d-me.json",
            "/lib/bibcontainer-library/devices/.tmp-abcd",
            "/lib/bibcontainer-library/ops/d-other/.tmp-9",
            "/lib/bibcontainer-library/covers/ab/abcdef.jpg",
            "/lib/bibcontainer-library/.DS_Store",
            "/lib/bibcontainer-library/ops/desktop.ini",
            "/lib/bibcontainer-library/ops/d-other/Thumbs.db",
        ] {
            assert!(!f.is_relevant(&created(path)), "{path}");
        }
    }

    #[test]
    fn reads_are_not_changes_and_unknown_paths_are_reported() {
        let f = filter();
        let access = Event::new(EventKind::Access(AccessKind::Read)).add_path(PathBuf::from(
            "/lib/bibcontainer-library/ops/d-other/0000000001-0000000003.jsonl",
        ));
        assert!(!f.is_relevant(&access));
        // 中身の変更・名前の変更は、変更
        let modified = Event::new(EventKind::Modify(ModifyKind::Any)).add_path(PathBuf::from(
            "/lib/bibcontainer-library/ops/d-other/0000000001-0000000003.jsonl",
        ));
        assert!(f.is_relevant(&modified));
        // ライブラリのフォルダの外のパス・パスがない通知は、念のため知らせる
        assert!(f.is_relevant(&created("/elsewhere/file")));
        assert!(f.is_relevant(&Event::new(EventKind::Any)));
    }

    #[test]
    fn a_real_watcher_reports_other_devices_files_and_not_my_own() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bibcontainer-library");
        std::fs::create_dir_all(root.join("ops/d-other")).unwrap();
        std::fs::create_dir_all(root.join("ops/d-me")).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = start(&root, "d-me", move || {
            let _ = tx.send(());
        })
        .unwrap();

        // 監視を始める前のフォルダ作りの通知（FSEvents は、直前の変更を遅れて知らせることがある）を、先に流す
        std::thread::sleep(Duration::from_millis(1200));
        while rx.try_recv().is_ok() {}

        // 自分のフォルダへの書き込みでは、通知は来ない
        std::fs::write(root.join("ops/d-me/0000000001-0000000001.jsonl"), "x").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(1500)).is_err(),
            "自分の書き込みは知らせない"
        );

        // 他の端末のフォルダへの書き込みは、来る（監視の開始直後は取りこぼしうるので、数回書く）
        let mut seen = false;
        for i in 0..10 {
            std::fs::write(
                root.join(format!("ops/d-other/{:010}-{:010}.jsonl", i + 1, i + 1)),
                "x",
            )
            .unwrap();
            if rx.recv_timeout(Duration::from_millis(1000)).is_ok() {
                seen = true;
                break;
            }
        }
        assert!(seen, "他の端末のファイルの通知が届かなかった");
    }

    #[test]
    fn watching_a_missing_folder_fails() {
        let dir = tempfile::tempdir().unwrap();
        assert!(start(&dir.path().join("missing"), "d-me", || {}).is_err());
    }
}
