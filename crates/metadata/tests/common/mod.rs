//! テスト共通の補助。フィクスチャは `tests/fixtures/` にある（取得元の実際の応答。Google の成功例だけは合成）。

#![allow(dead_code)]

use std::path::PathBuf;

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("フィクスチャ {name} を読めません: {e}"))
}

pub fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name))
        .unwrap_or_else(|e| panic!("フィクスチャ {name} を読めません: {e}"))
}
