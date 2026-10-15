//! メモリ上のトランスポート（テスト用）。
//!
//! 1 つの `MemoryFolder`（共有のフォルダ）に、端末ごとの `MemoryTransport`（その端末から見た窓）をつなぐ。
//! 同期サービスのふるまいを再現できる。
//!
//! - **遅れて見える**: ほかの端末が書いたファイルは、`DeliveryPolicy::Hidden` なら届くまで見えない。
//! - **途中まで見える**: `DeliveryPolicy::Partial` なら、先頭の一部だけが見える。
//! - **衝突コピー**: `MemoryFolder::make_conflict_copy` が `… (1).jsonl` のようなコピーを作る。
//! - **使えない**: `set_unavailable`（フォルダが見つからない）、`set_fail_writes`（容量不足など）。
//!
//! 自分が書いたファイルは、すぐ自分に見える。届いていないファイルの一覧と、届ける操作
//! （`pending`・`deliver`・`set_delivery`）は、テストが「どの順で届くか」を決めるために使う。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::transport::{
    FileEntry, SyncTransport, TransportError, TransportErrorKind, TransportResult, split_path,
};

/// 別の端末が書いたファイルが、どう見えるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// 全部見える。
    Full,
    /// まだ見えない（一覧にも出ない）。
    Hidden,
    /// 先頭の `n` バイトだけが見える。
    Partial(usize),
}

/// ほかの端末が新しく書いたファイルを、最初にどう見せるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryPolicy {
    /// すぐ全部見える。
    Immediate,
    /// `deliver` するまで見えない。
    Hidden,
    /// 先頭の `percent`％だけ見える（`deliver` で全部になる）。
    Partial { percent: u8 },
}

#[derive(Debug, Clone)]
struct StoredFile {
    data: Vec<u8>,
    writer: String,
    mtime_ms: i64,
}

#[derive(Debug, Default)]
struct State {
    files: BTreeMap<String, StoredFile>,
    dirs: BTreeSet<String>,
    viewers: BTreeSet<String>,
    policies: BTreeMap<String, DeliveryPolicy>,
    delivery: BTreeMap<(String, String), Delivery>,
    now_ms: i64,
    unavailable: BTreeSet<String>,
    fail_writes: BTreeMap<String, TransportErrorKind>,
}

impl State {
    fn state_of(&self, viewer: &str, path: &str) -> Delivery {
        self.delivery
            .get(&(viewer.to_owned(), path.to_owned()))
            .copied()
            .unwrap_or(Delivery::Full)
    }

    /// 端末 `viewer` から見えるファイルの中身。
    fn view(&self, viewer: &str, path: &str) -> Option<Vec<u8>> {
        let f = self.files.get(path)?;
        if f.writer == viewer {
            return Some(f.data.clone());
        }
        match self.state_of(viewer, path) {
            Delivery::Full => Some(f.data.clone()),
            Delivery::Hidden => None,
            Delivery::Partial(n) => Some(f.data[..n.min(f.data.len())].to_vec()),
        }
    }

    /// ファイルが書かれた（置き換えられた）ので、ほかの端末ごとの見え方を決め直す。
    fn reset_delivery(&mut self, path: &str, len: usize, writer: &str) {
        let viewers: Vec<String> = self.viewers.iter().cloned().collect();
        for v in viewers {
            let key = (v.clone(), path.to_owned());
            if v == writer {
                self.delivery.remove(&key);
                continue;
            }
            match self
                .policies
                .get(&v)
                .copied()
                .unwrap_or(DeliveryPolicy::Immediate)
            {
                DeliveryPolicy::Immediate => {
                    self.delivery.remove(&key);
                }
                DeliveryPolicy::Hidden => {
                    self.delivery.insert(key, Delivery::Hidden);
                }
                DeliveryPolicy::Partial { percent } => {
                    let n = len * usize::from(percent.min(100)) / 100;
                    self.delivery.insert(key, Delivery::Partial(n));
                }
            }
        }
    }

    fn dir_exists(&self, viewer: &str, dir: &str) -> bool {
        if dir.is_empty() || self.dirs.contains(dir) {
            return true;
        }
        let prefix = format!("{dir}/");
        self.files
            .keys()
            .any(|p| p.starts_with(&prefix) && self.view(viewer, p).is_some())
    }
}

/// 共有のフォルダ。`transport` で端末ごとの窓を作る。
#[derive(Debug, Clone, Default)]
pub struct MemoryFolder {
    inner: Arc<Mutex<State>>,
}

impl MemoryFolder {
    /// 空のフォルダ。
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 端末 `viewer` から見た窓を作る。同じ名前で何度作っても、同じ端末として扱う。
    pub fn transport(&self, viewer: &str) -> MemoryTransport {
        self.lock().viewers.insert(viewer.to_owned());
        MemoryTransport {
            folder: self.clone(),
            viewer: viewer.to_owned(),
        }
    }

    /// 更新時刻に使う「今」（UNIX ミリ秒）を決める。
    pub fn set_now_ms(&self, ms: i64) {
        self.lock().now_ms = ms;
    }

    /// 更新時刻に使う「今」を進める。
    pub fn advance_ms(&self, delta: i64) {
        self.lock().now_ms += delta;
    }

    /// フォルダにある全ファイルのパス（見え方に関係なく）。
    pub fn paths(&self) -> Vec<String> {
        self.lock().files.keys().cloned().collect()
    }

    /// ファイルの中身（見え方に関係なく）。
    pub fn read_raw(&self, path: &str) -> Option<Vec<u8>> {
        self.lock().files.get(path).map(|f| f.data.clone())
    }

    /// `writer` が書いたことにしてファイルを置く（ほかの端末への見え方は、各端末の方針に従う）。
    pub fn write_raw(&self, path: &str, data: &[u8], writer: &str) {
        let mut s = self.lock();
        let mtime_ms = s.now_ms;
        s.files.insert(
            path.to_owned(),
            StoredFile {
                data: data.to_vec(),
                writer: writer.to_owned(),
                mtime_ms,
            },
        );
        s.reset_delivery(path, data.len(), writer);
    }

    /// ファイルを消す（見え方に関係なく）。
    pub fn remove_raw(&self, path: &str) {
        let mut s = self.lock();
        s.files.remove(path);
        s.delivery.retain(|(_, p), _| p != path);
    }

    /// 保存されている中身を、先頭の `len` バイトだけにする（元のファイルが途中までしか届かなかった状態）。
    pub fn truncate_raw(&self, path: &str, len: usize) {
        if let Some(f) = self.lock().files.get_mut(path) {
            f.data.truncate(len);
        }
    }

    /// ファイルの更新時刻を決める。
    pub fn set_mtime(&self, path: &str, ms: i64) {
        if let Some(f) = self.lock().files.get_mut(path) {
            f.mtime_ms = ms;
        }
    }

    /// 同期サービスが作る衝突コピー（`<名前> (1).<拡張子>`）を作る。コピーの名前を返す。
    /// 元のファイルと同じ中身で、どの端末にも「同期サービスが書いたもの」として届く。
    pub fn make_conflict_copy(&self, path: &str) -> Option<String> {
        let data = self.read_raw(path)?;
        let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
        let (stem, ext) = match name.split_once('.') {
            Some((s, e)) => (s, format!(".{e}")),
            None => (name, String::new()),
        };
        let copy_name = format!("{stem} (1){ext}");
        let copy_path = if dir.is_empty() {
            copy_name.clone()
        } else {
            format!("{dir}/{copy_name}")
        };
        self.write_raw(&copy_path, &data, "sync-service");
        Some(copy_path)
    }
}

/// ある端末から見た共有フォルダ。
#[derive(Debug, Clone)]
pub struct MemoryTransport {
    folder: MemoryFolder,
    viewer: String,
}

impl MemoryTransport {
    /// この窓の端末名。
    pub fn viewer(&self) -> &str {
        &self.viewer
    }

    /// 共有のフォルダ。
    pub fn folder(&self) -> &MemoryFolder {
        &self.folder
    }

    /// ほかの端末が新しく書くファイルの見え方の方針を決める（既定は `Immediate`）。
    pub fn set_policy(&self, policy: DeliveryPolicy) {
        self.folder
            .lock()
            .policies
            .insert(self.viewer.clone(), policy);
    }

    /// まだ全部は見えていないファイル（パス、今の見え方）。
    pub fn pending(&self) -> Vec<(String, Delivery)> {
        let s = self.folder.lock();
        s.files
            .iter()
            .filter(|(_, f)| f.writer != self.viewer)
            .filter_map(|(p, _)| {
                let d = s.state_of(&self.viewer, p);
                (d != Delivery::Full).then(|| (p.clone(), d))
            })
            .collect()
    }

    /// ファイルの見え方を決める。
    pub fn set_delivery(&self, path: &str, delivery: Delivery) {
        let mut s = self.folder.lock();
        let key = (self.viewer.clone(), path.to_owned());
        if delivery == Delivery::Full {
            s.delivery.remove(&key);
        } else {
            s.delivery.insert(key, delivery);
        }
    }

    /// ファイルを全部見えるようにする。
    pub fn deliver(&self, path: &str) {
        self.set_delivery(path, Delivery::Full);
    }

    /// 届いていないファイルをすべて全部見えるようにする。
    pub fn deliver_all(&self) {
        let mut s = self.folder.lock();
        let viewer = self.viewer.clone();
        s.delivery.retain(|(v, _), _| *v != viewer);
    }

    /// 以後の操作がすべて「同期フォルダが見つからない」で失敗するようにする。
    pub fn set_unavailable(&self, unavailable: bool) {
        let mut s = self.folder.lock();
        if unavailable {
            s.unavailable.insert(self.viewer.clone());
        } else {
            s.unavailable.remove(&self.viewer);
        }
    }

    /// 以後の書き込み（と削除・フォルダ作成）が、指定の種類で失敗するようにする。`None` で元に戻す。
    pub fn set_fail_writes(&self, kind: Option<TransportErrorKind>) {
        let mut s = self.folder.lock();
        match kind {
            Some(k) => {
                s.fail_writes.insert(self.viewer.clone(), k);
            }
            None => {
                s.fail_writes.remove(&self.viewer);
            }
        }
    }

    fn check_read(&self, s: &State, path: &str) -> TransportResult<()> {
        if s.unavailable.contains(&self.viewer) {
            return Err(TransportError::new(
                TransportErrorKind::Unavailable,
                path,
                "メモリ上のフォルダが使えない",
            ));
        }
        if split_path(path).is_none() {
            return Err(TransportError::new(
                TransportErrorKind::Other,
                path,
                "使えないパス",
            ));
        }
        Ok(())
    }

    fn check_write(&self, s: &State, path: &str) -> TransportResult<()> {
        self.check_read(s, path)?;
        if let Some(kind) = s.fail_writes.get(&self.viewer) {
            return Err(TransportError::new(*kind, path, "書き込みを失敗させる設定"));
        }
        Ok(())
    }
}

impl SyncTransport for MemoryTransport {
    fn list(&self, dir: &str) -> TransportResult<Vec<FileEntry>> {
        let s = self.folder.lock();
        self.check_read(&s, dir)?;
        if !s.dir_exists(&self.viewer, dir) {
            return Err(TransportError::new(
                TransportErrorKind::NotFound,
                dir,
                "フォルダがない",
            ));
        }
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut out: BTreeMap<String, FileEntry> = BTreeMap::new();
        for (path, f) in &s.files {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            let Some(visible) = s.view(&self.viewer, path) else {
                continue;
            };
            match rest.split_once('/') {
                Some((child, _)) => {
                    out.entry(child.to_owned()).or_insert_with(|| FileEntry {
                        name: child.to_owned(),
                        is_dir: true,
                        size: 0,
                        modified_ms: None,
                    });
                }
                None => {
                    out.insert(
                        rest.to_owned(),
                        FileEntry {
                            name: rest.to_owned(),
                            is_dir: false,
                            size: visible.len() as u64,
                            modified_ms: Some(f.mtime_ms),
                        },
                    );
                }
            }
        }
        for d in &s.dirs {
            if let Some(rest) = d.strip_prefix(&prefix)
                && !rest.is_empty()
            {
                let child = rest.split('/').next().unwrap_or(rest);
                out.entry(child.to_owned()).or_insert_with(|| FileEntry {
                    name: child.to_owned(),
                    is_dir: true,
                    size: 0,
                    modified_ms: None,
                });
            }
        }
        Ok(out.into_values().collect())
    }

    fn read(&self, path: &str) -> TransportResult<Vec<u8>> {
        let s = self.folder.lock();
        self.check_read(&s, path)?;
        s.view(&self.viewer, path).ok_or_else(|| {
            TransportError::new(TransportErrorKind::NotFound, path, "ファイルがない")
        })
    }

    fn write_atomic(&self, path: &str, data: &[u8]) -> TransportResult<()> {
        let mut s = self.folder.lock();
        self.check_write(&s, path)?;
        if path.is_empty() {
            return Err(TransportError::new(
                TransportErrorKind::Other,
                path,
                "空のパス",
            ));
        }
        let mtime_ms = s.now_ms;
        s.files.insert(
            path.to_owned(),
            StoredFile {
                data: data.to_vec(),
                writer: self.viewer.clone(),
                mtime_ms,
            },
        );
        s.reset_delivery(path, data.len(), &self.viewer);
        Ok(())
    }

    fn exists(&self, path: &str) -> TransportResult<bool> {
        let s = self.folder.lock();
        self.check_read(&s, path)?;
        Ok(s.view(&self.viewer, path).is_some())
    }

    fn delete(&self, path: &str) -> TransportResult<()> {
        let mut s = self.folder.lock();
        self.check_write(&s, path)?;
        s.files.remove(path);
        s.delivery.retain(|(_, p), _| p != path);
        Ok(())
    }

    fn create_dir_all(&self, dir: &str) -> TransportResult<()> {
        let mut s = self.folder.lock();
        self.check_write(&s, dir)?;
        let mut acc = String::new();
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(part);
            s.dirs.insert(acc.clone());
        }
        Ok(())
    }

    fn check_writable(&self) -> TransportResult<()> {
        let s = self.folder.lock();
        self.check_write(&s, "")
    }
}
