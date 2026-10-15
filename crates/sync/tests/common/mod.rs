//! 統合テストの共通部品。端末（DB・時計・書影のフォルダ）と、同期フォルダの実装（メモリ／ローカル）を作る。
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use bibcontainer_core::schema::Entity;
use bibcontainer_core::{ManualClock, Op, db, dump_canonical, meta, record_change_committed};
use bibcontainer_sync::engine::{DeviceInfo, SyncEngine, SyncOptions};
use bibcontainer_sync::format::sha256_hex;
use bibcontainer_sync::layout::cover_local_path;
use bibcontainer_sync::{ConnDb, LocalFolder, MemoryFolder, SyncTransport, TransportResult};
use serde_json::{Value, json};
use tempfile::TempDir;

pub const T0: i64 = 1_791_966_600_000;
pub const MIN: i64 = 60 * 1000;
pub const DAY: i64 = 24 * 60 * MIN;

pub const BOOK_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a01";
pub const BOOK_2: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a02";
pub const BOOK_3: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a03";
pub const TAG_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0b01";
pub const SERIES_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0c01";
pub const LOC_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0d01";
pub const SHELF_1: &str = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0e01";

/// n 番目の ID（本）。
pub fn book_id(n: usize) -> String {
    format!("0192f3a0-6c1e-7cc3-9b1a-{n:012x}")
}

/// 同期フォルダの実装（メモリ上、またはローカルのフォルダ）。
pub enum Backend {
    Memory(MemoryFolder),
    Local(TempDir),
}

impl Backend {
    pub fn memory() -> Self {
        Backend::Memory(MemoryFolder::new())
    }

    pub fn local() -> Self {
        Backend::Local(TempDir::new().unwrap())
    }

    pub fn name(&self) -> &'static str {
        match self {
            Backend::Memory(_) => "memory",
            Backend::Local(_) => "local",
        }
    }

    /// 端末 `viewer` から見たトランスポート。
    pub fn transport(&self, viewer: &str) -> Box<dyn SyncTransport> {
        match self {
            Backend::Memory(m) => Box::new(m.transport(viewer)),
            Backend::Local(dir) => {
                Box::new(LocalFolder::new(dir.path().join("bibcontainer-library")))
            }
        }
    }

    /// ライブラリのフォルダの絶対パス（ローカルのみ）。
    pub fn local_root(&self) -> Option<PathBuf> {
        match self {
            Backend::Local(dir) => Some(dir.path().join("bibcontainer-library")),
            Backend::Memory(_) => None,
        }
    }

    /// 全ファイルのパス（相対）。
    pub fn paths(&self) -> Vec<String> {
        match self {
            Backend::Memory(m) => m.paths(),
            Backend::Local(dir) => {
                let root = dir.path().join("bibcontainer-library");
                let mut out = Vec::new();
                walk(&root, &root, &mut out);
                out.sort();
                out
            }
        }
    }

    pub fn read_raw(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Backend::Memory(m) => m.read_raw(path),
            Backend::Local(dir) => {
                std::fs::read(dir.path().join("bibcontainer-library").join(path)).ok()
            }
        }
    }

    /// ほかの誰かが書いたことにして、ファイルを置く。
    pub fn write_raw(&self, path: &str, data: &[u8]) {
        match self {
            Backend::Memory(m) => m.write_raw(path, data, "external"),
            Backend::Local(dir) => {
                let p = dir.path().join("bibcontainer-library").join(path);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, data).unwrap();
            }
        }
    }

    pub fn remove_raw(&self, path: &str) {
        match self {
            Backend::Memory(m) => m.remove_raw(path),
            Backend::Local(dir) => {
                let _ = std::fs::remove_file(dir.path().join("bibcontainer-library").join(path));
            }
        }
    }

    /// 衝突コピー（`… (1).jsonl`）を作る。
    pub fn make_conflict_copy(&self, path: &str) -> Option<String> {
        match self {
            Backend::Memory(m) => m.make_conflict_copy(path),
            Backend::Local(_) => {
                let data = self.read_raw(path)?;
                let (dir, name) = path.rsplit_once('/')?;
                let (stem, ext) = name.split_once('.')?;
                let copy = format!("{dir}/{stem} (1).{ext}");
                self.write_raw(&copy, &data);
                Some(copy)
            }
        }
    }

    /// ファイルの更新時刻を決める（ミリ秒）。
    pub fn set_mtime(&self, path: &str, ms: i64) {
        match self {
            Backend::Memory(m) => m.set_mtime(path, ms),
            Backend::Local(dir) => {
                let p = dir.path().join("bibcontainer-library").join(path);
                let f = std::fs::File::options().write(true).open(p).unwrap();
                f.set_modified(
                    std::time::UNIX_EPOCH
                        + std::time::Duration::from_millis(u64::try_from(ms).unwrap()),
                )
                .unwrap();
            }
        }
    }

    /// メモリ上の「今」を進める・決める（ローカルでは何もしない）。
    pub fn set_now_ms(&self, ms: i64) {
        if let Backend::Memory(m) = self {
            m.set_now_ms(ms);
        }
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap();
            out.push(
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}

/// 端末 1 台（DB、時計、書影のフォルダ、同期フォルダへの窓）。
pub struct Dev {
    pub name: String,
    pub db: ConnDb,
    pub clock: ManualClock,
    pub covers: TempDir,
    pub transport: Box<dyn SyncTransport>,
    pub info: DeviceInfo,
    /// 作った DB ファイルのフォルダ（ファイルの DB のときだけ）。
    pub dir: Option<TempDir>,
}

impl Dev {
    /// メモリ上の DB の端末。
    pub fn new(name: &str, backend: &Backend) -> Self {
        Self::with_db(name, backend, db::open_in_memory().unwrap(), None)
    }

    /// ファイルの DB の端末（バックアップの復元のテスト用）。
    pub fn new_on_disk(name: &str, backend: &Backend) -> Self {
        let dir = TempDir::new().unwrap();
        let (conn, _) = db::open(
            &dir.path().join("library.sqlite3"),
            &dir.path().join("backups"),
        )
        .unwrap();
        Self::with_db(name, backend, conn, Some(dir))
    }

    fn with_db(
        name: &str,
        backend: &Backend,
        conn: rusqlite::Connection,
        dir: Option<TempDir>,
    ) -> Self {
        Dev {
            name: name.to_owned(),
            db: ConnDb::new(conn),
            clock: ManualClock::new(T0),
            covers: TempDir::new().unwrap(),
            transport: backend.transport(name),
            info: DeviceInfo {
                name: name.to_owned(),
                os: "test".to_owned(),
                app_version: "0.1.0".to_owned(),
            },
            dir,
        }
    }

    pub fn engine(&self) -> SyncEngine<'_, ConnDb, dyn SyncTransport> {
        SyncEngine::new(
            &self.db,
            &*self.transport,
            &self.clock,
            self.covers.path(),
            self.info.clone(),
        )
    }

    pub fn engine_with(&self, options: SyncOptions) -> SyncEngine<'_, ConnDb, dyn SyncTransport> {
        self.engine().with_options(options)
    }

    pub fn id(&self) -> String {
        meta::device_id(&self.db.lock()).unwrap()
    }

    pub fn dump(&self) -> Value {
        dump_canonical(&self.db.lock()).unwrap()
    }

    /// 時計を 1 ミリ秒進めてから、変更を 1 件記録する。
    pub fn set(&self, ent: Entity, id: &str, fields: Value) -> Op {
        self.clock.advance(1);
        let set = match fields {
            Value::Object(m) => m.into_iter().collect::<Vec<_>>(),
            other => panic!("オブジェクトを渡すこと: {other}"),
        };
        record_change_committed(&mut self.db.lock(), &self.clock, ent, id, set).unwrap()
    }

    pub fn set_book(&self, id: &str, fields: Value) -> Op {
        self.set(Entity::Book, id, fields)
    }

    /// 本を作る（書名・作成日時など）。
    pub fn add_book(&self, id: &str, title: &str) -> Op {
        self.set_book(
            id,
            json!({"title": title, "created_at": "2026-10-14T00:00:00.000Z", "ownership": "owned"}),
        )
    }

    /// 書影を手元に作る（`<covers>/<xx>/<hash>.jpg`）。ハッシュを返す。
    pub fn make_cover(&self, content: &[u8]) -> String {
        let hash = sha256_hex(content);
        let path = cover_local_path(self.covers.path(), &hash);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        hash
    }

    pub fn has_cover(&self, hash: &str) -> bool {
        cover_local_path(self.covers.path(), hash).is_file()
    }

    pub fn pending(&self) -> u64 {
        bibcontainer_core::sync_state::pending_count(&self.db.lock()).unwrap()
    }

    pub fn count(&self, table: &str) -> i64 {
        self.db
            .lock()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    pub fn book_title(&self, id: &str) -> Option<String> {
        self.db
            .lock()
            .query_row("SELECT title FROM books WHERE id = ?1", [id], |r| r.get(0))
            .ok()
    }

    pub fn book_col(&self, id: &str, col: &str) -> Value {
        self.db
            .lock()
            .query_row(
                &format!("SELECT {col} FROM books WHERE id = ?1"),
                [id],
                |r| {
                    Ok(match r.get_ref(0)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(i) => json!(i),
                        rusqlite::types::ValueRef::Real(f) => json!(f),
                        rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                        rusqlite::types::ValueRef::Blob(_) => Value::Null,
                    })
                },
            )
            .unwrap()
    }
}

/// 端末のエンジンで、取り込み → 書き出し（`sync`）を 1 回行う。
pub fn sync(d: &Dev) -> bibcontainer_sync::SyncReport {
    d.engine()
        .sync(100)
        .unwrap_or_else(|e| panic!("{} の同期に失敗: {e}", d.name))
}

/// 端末のエンジンで、書き出す。
pub fn export(d: &Dev) -> bibcontainer_sync::ExportReport {
    d.engine()
        .export()
        .unwrap_or_else(|e| panic!("{} の書き出しに失敗: {e}", d.name))
}

/// 端末のエンジンで、取り込む（読むべきファイルがなくなるまで）。
pub fn import(d: &Dev) -> bibcontainer_sync::ImportReport {
    d.engine()
        .import_all(100)
        .unwrap_or_else(|e| panic!("{} の取り込みに失敗: {e}", d.name))
}

/// 全端末で書き出しと取り込みを、変化がなくなるまで繰り返す。
pub fn settle(devs: &[&Dev]) {
    for _ in 0..6 {
        for d in devs {
            export(d);
        }
        for d in devs {
            import(d);
        }
    }
}

/// すべての端末の `dump_canonical` が一致するか。
pub fn assert_converged(devs: &[&Dev]) {
    let first = devs[0].dump();
    for d in &devs[1..] {
        assert_eq!(
            first,
            d.dump(),
            "{} と {} の状態が一致しない",
            devs[0].name,
            d.name
        );
    }
}

pub fn fake_result<T>(r: TransportResult<T>) -> T {
    r.unwrap()
}

/// 他端末が書いた Op を作る（テストで外部のセグメントを組み立てる用）。
pub fn foreign_op(dev: &str, seq: i64, ms: i64, ent: Entity, id: &str, set: Value) -> Op {
    let set = match set {
        Value::Object(m) => m.into_iter().collect(),
        other => panic!("オブジェクトを渡すこと: {other}"),
    };
    Op {
        v: 1,
        dev: dev.to_owned(),
        seq,
        hlc: bibcontainer_core::Hlc::new(ms, 0, dev).unwrap().to_string(),
        ent,
        id: id.to_owned(),
        set,
    }
}

/// 他端末のセグメントを同期フォルダに置く。
pub fn put_segment(backend: &Backend, dev: &str, from: i64, to: i64, ops: &[Op]) -> String {
    let path = bibcontainer_sync::layout::segment_path(dev, from, to);
    backend.write_raw(
        &path,
        &bibcontainer_sync::format::encode_segment(ops).unwrap(),
    );
    path
}

/// 行に終端レコードをつける（形式の細工用）。
pub fn sealed(lines: &[String]) -> Vec<u8> {
    let mut body = Vec::new();
    for l in lines {
        body.extend_from_slice(l.as_bytes());
        body.push(b'\n');
    }
    let end = json!({"end": true, "count": lines.len(), "sha256": sha256_hex(&body)});
    body.extend_from_slice(end.to_string().as_bytes());
    body.push(b'\n');
    body
}
