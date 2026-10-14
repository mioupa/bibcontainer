//! 端末ごとの設定（`local_settings`）。同期しない。
//!
//! キーごとに JSON の値を持つ。列の配置、取得元の優先順位、スキャン設定など、何を入れるかは
//! アプリ側が決める。API キーは入れない（OS のキーチェーンに置く。`docs/decisions/0005` §8）。

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::error::Result;

/// 設定の値を読む。無ければ `None`。壊れた JSON は `None` として扱う。
pub fn get(conn: &Connection, key: &str) -> Result<Option<Value>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value_json FROM local_settings WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// 設定の値を書く（あれば上書き）。
pub fn set(conn: &Connection, key: &str, value: &Value) -> Result<()> {
    conn.execute(
        "INSERT INTO local_settings(key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![key, serde_json::to_string(value)?],
    )?;
    Ok(())
}

/// 設定を消す（既定の値に戻す）。
pub fn remove(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM local_settings WHERE key = ?1", [key])?;
    Ok(())
}

/// すべての設定（キー順）。壊れた JSON のキーは含めない。
pub fn all(conn: &Connection) -> Result<BTreeMap<String, Value>> {
    let mut stmt = conn.prepare("SELECT key, value_json FROM local_settings ORDER BY key")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(k, v)| serde_json::from_str(&v).ok().map(|v| (k, v)))
        .collect())
}
