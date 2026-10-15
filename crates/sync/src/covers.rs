//! 書影の受け渡し（0013 §4・§5）。
//!
//! 書影の本体は端末ローカルの `covers/<先頭2文字>/<sha256>.jpg` に置く（表示はここから行う）。
//! 同期フォルダの `covers/` は、端末間で受け渡すための写し。ファイル名が内容の SHA-256 なので、
//! どの端末が書いても同じ内容になる（FR-SYNC-04 の例外）。既にあれば書かない。
//! 受け取るときは、中身の SHA-256 が名前と合うことを確かめる（同期の途中のファイルを使わない）。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::db::SyncDb;
use crate::engine::SyncEngine;
use crate::error::Result;
use crate::format::sha256_hex;
use crate::layout::{
    cover_local_path, cover_remote_dir, cover_remote_path, is_valid_cover_hash,
    parse_cover_file_name,
};
use crate::transport::{SyncTransport, TransportError, list_or_empty, new_tmp_name};

/// 手元にない書影の数（形式の正しいハッシュだけ数える）。
pub(crate) fn count_missing(covers_dir: &Path, hashes: &[String]) -> u64 {
    let n = hashes
        .iter()
        .filter(|h| is_valid_cover_hash(h) && !cover_local_path(covers_dir, h).is_file())
        .count();
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// 端末ローカルの書影を、一時ファイルに書いてから名前を変えて置く。既にあれば何もしない。
pub(crate) fn write_local_cover(
    covers_dir: &Path,
    hash: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let path = cover_local_path(covers_dir, hash);
    if path.is_file() {
        return Ok(());
    }
    let parent = path.parent().expect("書影のパスには親がある");
    fs::create_dir_all(parent)?;
    let tmp = parent.join(new_tmp_name());
    let write = || -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    match fs::rename(&tmp, &path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            // 同時に同じ書影を置いた相手が先に改名したなら、それでよい。
            if path.is_file() { Ok(()) } else { Err(e) }
        }
    }
}

impl<D: SyncDb, T: SyncTransport + ?Sized> SyncEngine<'_, D, T> {
    /// 書影を同期フォルダへ写す（既にあるものは写さない）。写した数を返す。
    ///
    /// 手元にない書影（他端末の書影の取り込みが終わっていない、など）は、警告を出して飛ばす。
    pub(crate) fn upload_covers(&self, hashes: impl IntoIterator<Item = String>) -> Result<usize> {
        let mut by_prefix: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for h in hashes {
            if is_valid_cover_hash(&h) {
                by_prefix.entry(h[..2].to_owned()).or_default().insert(h);
            }
        }
        let mut uploaded = 0;
        for (prefix, wanted) in by_prefix {
            let existing: BTreeSet<String> =
                list_or_empty(self.transport, &cover_remote_dir(&prefix))?
                    .into_iter()
                    .map(|e| e.name)
                    .collect();
            for hash in wanted {
                if existing.contains(&format!("{hash}.jpg")) {
                    continue;
                }
                let local = cover_local_path(&self.covers_dir, &hash);
                match fs::read(&local) {
                    Ok(bytes) if sha256_hex(&bytes) == hash => {
                        self.transport
                            .write_atomic(&cover_remote_path(&hash), &bytes)?;
                        uploaded += 1;
                    }
                    Ok(_) => tracing::warn!(
                        hash,
                        "手元の書影の中身がハッシュと合わないので、写さなかった"
                    ),
                    Err(e) => {
                        tracing::warn!(hash, error = %e, "手元に書影がないので、写さなかった")
                    }
                }
            }
        }
        Ok(uploaded)
    }

    /// 本が参照している書影のうち、手元にないものを同期フォルダから写す。
    ///
    /// 同期フォルダの `covers/<xx>/` を一覧して、実際にあるものだけを（上限の数まで）読む。まだ届いていない
    /// 書影を何度も読みにいかず、届いていない書影のせいで、あとの書影が後回しにならないようにするため。
    pub(crate) fn fetch_missing_covers(&self) -> Result<CoverFetch> {
        let hashes = self
            .db
            .with_conn(|c| bibcontainer_core::sync_state::referenced_cover_hashes(c))?;
        let missing: Vec<String> = hashes
            .into_iter()
            .filter(|h| is_valid_cover_hash(h) && !cover_local_path(&self.covers_dir, h).is_file())
            .collect();
        let mut out = CoverFetch::default();
        if missing.is_empty() {
            return Ok(out);
        }
        let mut by_prefix: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
        for h in &missing {
            by_prefix.entry(&h[..2]).or_default().push(h);
        }
        let mut available: Vec<&String> = Vec::new();
        for (prefix, wanted) in by_prefix {
            let listed: BTreeSet<String> =
                match list_or_empty(self.transport, &cover_remote_dir(prefix)) {
                    Ok(entries) => entries
                        .into_iter()
                        .filter(|e| !e.is_dir && e.size > 0)
                        .map(|e| e.name)
                        .collect(),
                    Err(e) => {
                        // フォルダが使えないなど。残りも同じ理由で失敗するので、ここで切り上げる。
                        out.error = Some(e);
                        break;
                    }
                };
            available.extend(
                wanted
                    .into_iter()
                    .filter(|h| listed.contains(&format!("{h}.jpg"))),
            );
        }
        let limit = self.options.max_cover_fetch.max(1);
        for hash in available.iter().take(limit) {
            match self.transport.read(&cover_remote_path(hash)) {
                Ok(bytes) if sha256_hex(&bytes) == **hash => {
                    match write_local_cover(&self.covers_dir, hash, &bytes) {
                        Ok(()) => {
                            out.fetched += 1;
                            out.fetched_hashes.push((*hash).clone());
                        }
                        Err(e) => {
                            tracing::warn!(hash = %hash, error = %e, "書影を手元に保存できなかった");
                        }
                    }
                }
                // 同期の途中のファイル。次の回にもう一度確かめる。
                Ok(_) => {
                    tracing::debug!(hash = %hash, "同期フォルダの書影がまだ途中のようなので、あとで確かめる")
                }
                Err(e) if e.is_not_found() => {}
                Err(e) => {
                    out.error = Some(e);
                    break;
                }
            }
        }
        out.deferred = available.len().saturating_sub(limit);
        out.still_missing =
            u64::try_from(missing.len().saturating_sub(out.fetched)).unwrap_or(u64::MAX);
        Ok(out)
    }
}

/// 書影の取得の結果。
#[derive(Debug, Default)]
pub(crate) struct CoverFetch {
    /// 同期フォルダから写した数。
    pub fetched: usize,
    /// 同期フォルダから写した書影のハッシュ（写した順。`fetched` と同じ数）。
    pub fetched_hashes: Vec<String>,
    /// 上限のため次の回に回した、同期フォルダにある書影の数。
    pub deferred: usize,
    /// まだ手元にない書影の数。
    pub still_missing: u64,
    /// 最初のエラー。
    pub error: Option<TransportError>,
}

/// 書影のファイル名（`<hash>.jpg`）からハッシュを取り出す（整理で使う）。
pub(crate) fn hash_of_cover_name(name: &str) -> Option<&str> {
    parse_cover_file_name(name)
}
