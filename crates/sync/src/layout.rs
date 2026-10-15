//! 同期フォルダの中のパスと名前の規則（0013 §2）。
//!
//! ```text
//! bibcontainer-library/
//! ├─ library.json
//! ├─ devices/<device_id>.json
//! ├─ ops/<device_id>/<seq_from:010>-<seq_to:010>.jsonl
//! ├─ covers/<先頭2文字>/<sha256>.jpg
//! └─ snapshots/<device_id>/<ms15>-<counter5>.jsonl.gz
//! ```

use std::path::{Path, PathBuf};

use bibcontainer_core::ids::is_valid_device_id;

use crate::transport::TMP_PREFIX;

/// ユーザーが選んだフォルダの下に作るフォルダの名前。
pub const LIBRARY_DIR_NAME: &str = "bibcontainer-library";
/// ライブラリの情報のファイル。
pub const LIBRARY_FILE: &str = "library.json";
/// 端末の情報のフォルダ。
pub const DEVICES_DIR: &str = "devices";
/// 変更ログのフォルダ。
pub const OPS_DIR: &str = "ops";
/// 書影のフォルダ。
pub const COVERS_DIR: &str = "covers";
/// スナップショットのフォルダ。
pub const SNAPSHOTS_DIR: &str = "snapshots";

/// `devices/<device_id>.json`
pub fn device_file(dev: &str) -> String {
    format!("{DEVICES_DIR}/{dev}.json")
}

/// `devices/` の中のファイル名から端末 ID を取り出す。端末 ID の書式でなければ `None`
/// （`d-xxx (1).json` のような衝突コピーを除く）。
pub fn parse_device_file_name(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".json")?;
    (is_valid_device_id(stem) && !name.starts_with(TMP_PREFIX)).then_some(stem)
}

/// `ops/<device_id>`
pub fn ops_dir(dev: &str) -> String {
    format!("{OPS_DIR}/{dev}")
}

/// セグメントのファイル名（`0000000001-0000000250.jsonl`）。
pub fn segment_name(from: i64, to: i64) -> String {
    format!("{from:010}-{to:010}.jsonl")
}

/// `ops/<device_id>/<セグメントのファイル名>`
pub fn segment_path(dev: &str, from: i64, to: i64) -> String {
    format!("{}/{}", ops_dir(dev), segment_name(from, to))
}

/// セグメントのファイル名（`^\d{10}-\d{10}\.jsonl$`）から seq の範囲を取り出す。
/// 一致しない名前、`from` が 1 未満、`to` が `from` 未満のものは `None`。
pub fn parse_segment_name(name: &str) -> Option<(i64, i64)> {
    let stem = name.strip_suffix(".jsonl")?;
    let (from, to) = stem.split_once('-')?;
    let digits = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(from, 10) || !digits(to, 10) {
        return None;
    }
    let (from, to) = (from.parse::<i64>().ok()?, to.parse::<i64>().ok()?);
    (from >= 1 && to >= from).then_some((from, to))
}

/// `snapshots/<device_id>`
pub fn snapshots_dir(dev: &str) -> String {
    format!("{SNAPSHOTS_DIR}/{dev}")
}

/// スナップショットのファイル名（`<ms15>-<counter5>.jsonl.gz`）。HLC の端末 ID を除いた部分で、
/// 名前の文字列順が HLC の順と一致する。`:` を使えない Windows のため、HLC そのものにはしない。
pub fn snapshot_name(ms: i64, counter: u32) -> String {
    format!("{ms:015}-{counter:05}.jsonl.gz")
}

/// `snapshots/<device_id>/<スナップショットのファイル名>`
pub fn snapshot_path(dev: &str, name: &str) -> String {
    format!("{}/{name}", snapshots_dir(dev))
}

/// スナップショットのファイル名（`^\d{15}-\d{5}\.jsonl\.gz$`）から (ms, counter) を取り出す。
pub fn parse_snapshot_name(name: &str) -> Option<(i64, u32)> {
    let stem = name.strip_suffix(".jsonl.gz")?;
    let (ms, counter) = stem.split_once('-')?;
    let digits = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(ms, 15) || !digits(counter, 5) {
        return None;
    }
    Some((ms.parse().ok()?, counter.parse().ok()?))
}

/// 書影のハッシュとして正しいか（小文字の 16 進 64 桁）。パスを作る前の検査。
pub fn is_valid_cover_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 同期フォルダの中の書影のパス（`covers/<先頭2文字>/<hash>.jpg`）。`hash` は検査済みであること。
pub fn cover_remote_path(hash: &str) -> String {
    format!("{COVERS_DIR}/{}/{hash}.jpg", &hash[..2])
}

/// 同期フォルダの中の書影のフォルダ（`covers/<先頭2文字>`）。
pub fn cover_remote_dir(prefix: &str) -> String {
    format!("{COVERS_DIR}/{prefix}")
}

/// 端末ローカルの書影のパス（`<covers_dir>/<先頭2文字>/<hash>.jpg`。`bibcontainer-metadata` の `cover_path` と同じ）。
pub fn cover_local_path(covers_dir: &Path, hash: &str) -> PathBuf {
    covers_dir.join(&hash[..2]).join(format!("{hash}.jpg"))
}

/// 書影のファイル名（`<hash>.jpg`）からハッシュを取り出す。
pub fn parse_cover_file_name(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".jpg")?;
    is_valid_cover_hash(stem).then_some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_names() {
        assert_eq!(segment_name(1, 250), "0000000001-0000000250.jsonl");
        assert_eq!(
            parse_segment_name("0000000001-0000000250.jsonl"),
            Some((1, 250))
        );
        for bad in [
            "0000000001-0000000250 (1).jsonl",
            "0000000001-0000000250.jsonl.tmp",
            ".tmp-abc",
            "1-250.jsonl",
            "0000000000-0000000005.jsonl",
            "0000000005-0000000001.jsonl",
            "000000000a-0000000250.jsonl",
        ] {
            assert_eq!(parse_segment_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn snapshot_names_sort_like_hlc() {
        let a = snapshot_name(1_760_412_345_678, 0);
        let b = snapshot_name(1_760_412_345_678, 12);
        let c = snapshot_name(1_760_412_345_679, 0);
        assert_eq!(a, "001760412345678-00000.jsonl.gz");
        assert!(a < b && b < c);
        assert_eq!(parse_snapshot_name(&b), Some((1_760_412_345_678, 12)));
        assert_eq!(parse_snapshot_name("x.jsonl.gz"), None);
        assert!(!a.contains(':'));
    }

    #[test]
    fn device_and_cover_names() {
        assert_eq!(parse_device_file_name("d-7f3a2c.json"), Some("d-7f3a2c"));
        assert_eq!(parse_device_file_name("d-7f3a2c (1).json"), None);
        assert_eq!(parse_device_file_name(".tmp-1.json"), None);
        let h = "ab".repeat(32);
        assert!(is_valid_cover_hash(&h));
        assert_eq!(cover_remote_path(&h), format!("covers/ab/{h}.jpg"));
        assert!(!is_valid_cover_hash(&h.to_uppercase()));
        assert_eq!(parse_cover_file_name(&format!("{h}.jpg")), Some(h.as_str()));
        assert_eq!(parse_cover_file_name(&format!("{h} (1).jpg")), None);
    }
}
