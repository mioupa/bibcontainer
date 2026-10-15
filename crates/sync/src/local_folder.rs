//! ローカルのフォルダ（Google Drive for desktop などが同期するフォルダ）を使うトランスポート。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use crate::layout::{LIBRARY_DIR_NAME, LIBRARY_FILE};
use crate::transport::{
    FileEntry, SyncTransport, TransportError, TransportErrorKind, TransportResult, new_tmp_name,
    split_path,
};

/// `std::fs` で読み書きするトランスポート。`root` が `bibcontainer-library` のフォルダ。
#[derive(Debug, Clone)]
pub struct LocalFolder {
    root: PathBuf,
}

impl LocalFolder {
    /// `root`（`bibcontainer-library` のフォルダ）を使う。フォルダがなくてもよい（書き込み時に作る）。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `bibcontainer-library` のフォルダのパス。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 利用者が選んだフォルダから、実際に使う `bibcontainer-library` のフォルダを決める。
    ///
    /// 選んだフォルダ自体が `bibcontainer-library` という名前か、`library.json` がある場合はそのフォルダ。
    /// そうでなければ、その下の `bibcontainer-library`（なければ作る予定のパス）。
    pub fn resolve_root(selected: &Path) -> PathBuf {
        let named = selected.file_name().is_some_and(|n| n == LIBRARY_DIR_NAME);
        if named || selected.join(LIBRARY_FILE).is_file() {
            selected.to_path_buf()
        } else {
            selected.join(LIBRARY_DIR_NAME)
        }
    }

    fn path_of(&self, rel: &str) -> TransportResult<PathBuf> {
        let parts = split_path(rel)
            .ok_or_else(|| TransportError::new(TransportErrorKind::Other, rel, "使えないパス"))?;
        let mut p = self.root.clone();
        for part in parts {
            p.push(part);
        }
        Ok(p)
    }

    /// I/O エラーを変換する。フォルダ自体がなければ「同期フォルダが見つからない」にする。
    fn error(&self, rel: &str, err: &std::io::Error) -> TransportError {
        let mut e = TransportError::from_io(rel, err);
        // ライブラリのフォルダがまだないだけ（親はある）なら、ふつうの「ない」。親もなければ、
        // Google Drive が未接続など、同期フォルダそのものが使えない。
        if e.kind == TransportErrorKind::NotFound && !self.root.is_dir() && !self.parent_exists() {
            e.kind = TransportErrorKind::Unavailable;
        }
        e
    }

    fn parent_exists(&self) -> bool {
        self.root
            .parent()
            .is_some_and(|p| p.as_os_str().is_empty() || p.is_dir())
    }

    /// ルートがなければ、親があるときだけ作る（Google Drive が未接続のとき、ローカルに空のフォルダを
    /// 作ってしまわないため）。
    fn ensure_root(&self) -> TransportResult<()> {
        if self.root.is_dir() {
            return Ok(());
        }
        if !self.parent_exists() {
            return Err(TransportError::new(
                TransportErrorKind::Unavailable,
                "",
                self.root.display().to_string(),
            ));
        }
        match fs::create_dir(&self.root) {
            Ok(()) => Ok(()),
            Err(_) if self.root.is_dir() => Ok(()),
            Err(e) => Err(TransportError::from_io("", &e)),
        }
    }
}

fn modified_ms(meta: &fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

/// 名前の変更。他のプロセス（同期クライアントやウイルス対策）が掴んでいて一時的に失敗することが
/// あるので、権限エラーは少し待って数回やり直す。
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 5 => {
                attempt += 1;
                thread::sleep(Duration::from_millis(40 * attempt));
            }
            Err(e) => return Err(e),
        }
    }
}

impl SyncTransport for LocalFolder {
    fn list(&self, dir: &str) -> TransportResult<Vec<FileEntry>> {
        let path = self.path_of(dir)?;
        let read = fs::read_dir(&path).map_err(|e| self.error(dir, &e))?;
        let mut out = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| self.error(dir, &e))?;
            let Ok(name) = entry.file_name().into_string() else {
                tracing::warn!(dir, "UTF-8 でない名前のファイルを無視した");
                continue;
            };
            // 消えかけのファイルなどで metadata が取れなければ、その項目は飛ばす。
            let Ok(meta) = entry.metadata() else { continue };
            out.push(FileEntry {
                name,
                is_dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified_ms: modified_ms(&meta),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn read(&self, path: &str) -> TransportResult<Vec<u8>> {
        let p = self.path_of(path)?;
        fs::read(&p).map_err(|e| self.error(path, &e))
    }

    fn write_atomic(&self, path: &str, data: &[u8]) -> TransportResult<()> {
        let target = self.path_of(path)?;
        self.ensure_root()?;
        let parent = target
            .parent()
            .ok_or_else(|| TransportError::new(TransportErrorKind::Other, path, "親がない"))?;
        fs::create_dir_all(parent).map_err(|e| self.error(path, &e))?;
        let tmp = parent.join(new_tmp_name());
        let write = || -> std::io::Result<()> {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(data)?;
            // 名前を変える前に中身をディスクに書く（電源断で空のファイルが残らないように）。
            f.sync_all()?;
            Ok(())
        };
        if let Err(e) = write() {
            let _ = fs::remove_file(&tmp);
            return Err(self.error(path, &e));
        }
        if let Err(e) = rename_with_retry(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            return Err(self.error(path, &e));
        }
        Ok(())
    }

    fn exists(&self, path: &str) -> TransportResult<bool> {
        let p = self.path_of(path)?;
        match fs::metadata(&p) {
            Ok(m) => Ok(m.is_file()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(self.error(path, &e)),
        }
    }

    fn delete(&self, path: &str) -> TransportResult<()> {
        let p = self.path_of(path)?;
        match fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(self.error(path, &e)),
        }
    }

    fn create_dir_all(&self, dir: &str) -> TransportResult<()> {
        let p = self.path_of(dir)?;
        self.ensure_root()?;
        fs::create_dir_all(&p).map_err(|e| self.error(dir, &e))
    }

    fn check_writable(&self) -> TransportResult<()> {
        // 作る前のフォルダを、確かめるためだけに作らない。いちばん近い既存のフォルダに一時ファイルを書く。
        let mut dir: &Path = &self.root;
        while !dir.is_dir() {
            match dir.parent() {
                Some(p) if !p.as_os_str().is_empty() => dir = p,
                _ => {
                    return Err(TransportError::new(
                        TransportErrorKind::Unavailable,
                        "",
                        self.root.display().to_string(),
                    ));
                }
            }
        }
        let probe = dir.join(new_tmp_name());
        let result = fs::write(&probe, b"probe");
        let _ = fs::remove_file(&probe);
        result.map_err(|e| TransportError::from_io("", &e))
    }
}
