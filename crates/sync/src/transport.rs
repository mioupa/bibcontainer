//! 同期フォルダへの読み書き（`SyncTransport`）。FR-SYNC-12。
//!
//! エンジンはフォルダの中身を、この trait だけを通して読み書きする。初版の実装は `LocalFolder`
//! （`std::fs`）。テスト用に、ファイルが遅れて見える・途中まで見える・衝突コピーができる、を
//! 再現できるメモリ上の実装（`MemoryFolder`）もある。将来は Google Drive API の実装を足せる。
//!
//! パスは、ライブラリのフォルダ（`bibcontainer-library/`）からの相対パスで、区切りは常に `/`。
//! `..`・先頭の `/`・`\`・`:` を含むパスは使えない。

use std::fmt;
use std::io;

/// フォルダの中の 1 つの項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// 名前（パスの最後の部分）。
    pub name: String,
    /// フォルダか。
    pub is_dir: bool,
    /// ファイルの大きさ（バイト）。フォルダは 0。
    pub size: u64,
    /// 更新時刻（UNIX ミリ秒）。分からなければ `None`。
    pub modified_ms: Option<i64>,
}

/// 同期フォルダの操作の失敗の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// ファイルやフォルダがない。
    NotFound,
    /// 同期フォルダそのものが見つからない（未接続、Google Drive が起動していない、など）。
    Unavailable,
    /// 権限がない（読み取り専用、他のプロセスが使用中、など）。
    PermissionDenied,
    /// 空き容量が足りない。
    StorageFull,
    /// そのほか。
    Other,
}

/// 同期フォルダの操作の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    /// 種類。
    pub kind: TransportErrorKind,
    /// 対象のパス（相対パス）。
    pub path: String,
    /// 詳細（OS のメッセージなど）。
    pub message: String,
}

impl TransportError {
    /// 種類とパスから作る。
    pub fn new(
        kind: TransportErrorKind,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            path: path.into(),
            message: message.into(),
        }
    }

    /// `std::io::Error` から作る。
    pub fn from_io(path: &str, err: &io::Error) -> Self {
        let kind = match err.kind() {
            io::ErrorKind::NotFound => TransportErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => TransportErrorKind::PermissionDenied,
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => {
                TransportErrorKind::StorageFull
            }
            _ => TransportErrorKind::Other,
        };
        Self::new(kind, path, err.to_string())
    }

    /// 見つからないエラーか。
    pub fn is_not_found(&self) -> bool {
        self.kind == TransportErrorKind::NotFound
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            TransportErrorKind::NotFound => "見つかりません",
            TransportErrorKind::Unavailable => {
                "同期フォルダが見つかりません（Google Drive などが接続されているか確認してください）"
            }
            TransportErrorKind::PermissionDenied => "アクセスする権限がありません",
            TransportErrorKind::StorageFull => "空き容量が足りません",
            TransportErrorKind::Other => "操作に失敗しました",
        };
        write!(f, "{what}: {}（{}）", self.path, self.message)
    }
}

impl std::error::Error for TransportError {}

/// トランスポートの結果。
pub type TransportResult<T> = std::result::Result<T, TransportError>;

/// 同期フォルダ（ライブラリのフォルダ）への読み書き。
///
/// どの実装も、複数の端末が同じフォルダを同時に使うことを前提にする。自分が書いたファイルは、
/// 書いたあとすぐ自分に見える。ほかの端末が書いたファイルは、遅れて見えたり、途中まで見えたりしうる
/// （エンジンが、終端レコードとハッシュで確かめる）。
pub trait SyncTransport {
    /// フォルダの中身を一覧する（再帰しない）。フォルダがなければ `NotFound`。
    fn list(&self, dir: &str) -> TransportResult<Vec<FileEntry>>;

    /// ファイルを読む。なければ `NotFound`。
    fn read(&self, path: &str) -> TransportResult<Vec<u8>>;

    /// ファイルを書く。同じフォルダに一時ファイル（`.tmp-<乱数>`）として書いてから名前を変えるので、
    /// 読む側に書きかけのファイルは見えない。親のフォルダがなければ作る。同じ名前があれば置き換える。
    fn write_atomic(&self, path: &str, data: &[u8]) -> TransportResult<()>;

    /// ファイルがあるか。
    fn exists(&self, path: &str) -> TransportResult<bool>;

    /// ファイルを削除する。なくてもエラーにしない。
    fn delete(&self, path: &str) -> TransportResult<()>;

    /// フォルダを作る（途中のフォルダも作る。あってもよい）。
    fn create_dir_all(&self, dir: &str) -> TransportResult<()>;

    /// このフォルダに書き込めるか（設定の前の確認）。フォルダが無くても、作れるなら `Ok`。
    ///
    /// 既定の実装は、一時ファイルを書いて消す。
    fn check_writable(&self) -> TransportResult<()> {
        let path = format!(".tmp-probe-{}", uuid::Uuid::new_v4().simple());
        self.write_atomic(&path, b"probe")?;
        self.delete(&path)
    }
}

impl<T: SyncTransport + ?Sized> SyncTransport for &T {
    fn list(&self, dir: &str) -> TransportResult<Vec<FileEntry>> {
        (**self).list(dir)
    }
    fn read(&self, path: &str) -> TransportResult<Vec<u8>> {
        (**self).read(path)
    }
    fn write_atomic(&self, path: &str, data: &[u8]) -> TransportResult<()> {
        (**self).write_atomic(path, data)
    }
    fn exists(&self, path: &str) -> TransportResult<bool> {
        (**self).exists(path)
    }
    fn delete(&self, path: &str) -> TransportResult<()> {
        (**self).delete(path)
    }
    fn create_dir_all(&self, dir: &str) -> TransportResult<()> {
        (**self).create_dir_all(dir)
    }
    fn check_writable(&self) -> TransportResult<()> {
        (**self).check_writable()
    }
}

impl<T: SyncTransport + ?Sized> SyncTransport for Box<T> {
    fn list(&self, dir: &str) -> TransportResult<Vec<FileEntry>> {
        (**self).list(dir)
    }
    fn read(&self, path: &str) -> TransportResult<Vec<u8>> {
        (**self).read(path)
    }
    fn write_atomic(&self, path: &str, data: &[u8]) -> TransportResult<()> {
        (**self).write_atomic(path, data)
    }
    fn exists(&self, path: &str) -> TransportResult<bool> {
        (**self).exists(path)
    }
    fn delete(&self, path: &str) -> TransportResult<()> {
        (**self).delete(path)
    }
    fn create_dir_all(&self, dir: &str) -> TransportResult<()> {
        (**self).create_dir_all(dir)
    }
    fn check_writable(&self) -> TransportResult<()> {
        (**self).check_writable()
    }
}

/// 一時ファイルの名前の接頭辞。
pub const TMP_PREFIX: &str = ".tmp-";

/// 新しい一時ファイル名。
pub fn new_tmp_name() -> String {
    format!("{TMP_PREFIX}{}", uuid::Uuid::new_v4().simple())
}

/// パスを検査して、`/` で区切った部品にする。空の部品・`.`・`..`・`\`・`:` を含むものは `None`。
pub fn split_path(path: &str) -> Option<Vec<&str>> {
    if path.is_empty() {
        return Some(Vec::new());
    }
    let parts: Vec<&str> = path.split('/').collect();
    let ok = parts
        .iter()
        .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains(['\\', ':', '\0']));
    ok.then_some(parts)
}

/// フォルダがなくても空として一覧する。
pub fn list_or_empty<T: SyncTransport + ?Sized>(
    transport: &T,
    dir: &str,
) -> TransportResult<Vec<FileEntry>> {
    match transport.list(dir) {
        Err(e) if e.is_not_found() => Ok(Vec::new()),
        other => other,
    }
}

/// ファイルを読む。なければ `None`。
pub fn read_optional<T: SyncTransport + ?Sized>(
    transport: &T,
    path: &str,
) -> TransportResult<Option<Vec<u8>>> {
    match transport.read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.is_not_found() => Ok(None),
        Err(e) => Err(e),
    }
}
