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

/// パーセントエンコードを戻す（テストの偽の HTTP が、URL の検索語を読むため）。
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Open Library のまとめた検索（`search.json?q=isbn:(A OR B …)&…`）の URL から、ISBN の一覧を取る。
/// まとめた検索でなければ `None`。
pub fn open_library_batch_isbns(url: &str) -> Option<Vec<String>> {
    let q = url.split("/search.json?").nth(1)?;
    let q = q.split('&').find_map(|p| p.strip_prefix("q="))?;
    let q = percent_decode(q);
    let inner = q.strip_prefix("isbn:(")?.strip_suffix(')')?;
    Some(inner.split(" OR ").map(str::to_string).collect())
}

/// まとめた検索への応答を、1 冊ずつの検索のフィクスチャ（`openlibrary_search_<ISBN>.json`。なければ該当なし）の
/// `docs` をつなげて作る。
pub fn open_library_batch_body(isbns: &[String]) -> String {
    let mut docs: Vec<serde_json::Value> = Vec::new();
    for isbn in isbns {
        if let Ok(text) =
            std::fs::read_to_string(fixture_path(&format!("openlibrary_search_{isbn}.json")))
        {
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            docs.extend(v["docs"].as_array().cloned().unwrap_or_default());
        }
    }
    serde_json::json!({ "numFound": docs.len(), "start": 0, "docs": docs }).to_string()
}
