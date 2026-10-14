//! `local_meta`（端末ローカルのキー・値）の読み書き。
//!
//! HLC と seq のカウンタは、変更を記録するのと同じトランザクションで読み書きする。
//! そうすると、再起動後に壁時計が戻っても HLC が逆行せず、ロールバックすればカウンタも戻る。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::hlc::HlcState;
use crate::ids::{is_valid_device_id, new_device_id};

/// 端末 ID。初回起動時に一度だけ生成し、以後は読むだけ。
pub const KEY_DEVICE_ID: &str = "device_id";
/// 最後に採番した（または観測した）HLC の (ms, counter)。
pub const KEY_HLC_LAST: &str = "hlc_last";
/// 最後に採番した Op の seq。outbox の行を消しても再利用しないよう、カウンタで持つ。
pub const KEY_OP_SEQ_LAST: &str = "op_seq_last";

/// 値を読む。
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM local_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

/// 値を書く（あれば上書き）。
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO local_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// 端末 ID を返す。まだなければ生成して保存する。
///
/// `INSERT OR IGNORE` なので、同時に呼ばれても最初に書かれた値だけが残る。
pub fn ensure_device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = get(conn, KEY_DEVICE_ID)? {
        return validated_device_id(id);
    }
    conn.execute(
        "INSERT OR IGNORE INTO local_meta(key, value) VALUES (?1, ?2)",
        params![KEY_DEVICE_ID, new_device_id()],
    )?;
    device_id(conn)
}

/// 保存済みの端末 ID を読む。なければ `MissingMeta`。
pub fn device_id(conn: &Connection) -> Result<String> {
    match get(conn, KEY_DEVICE_ID)? {
        Some(id) => validated_device_id(id),
        None => Err(Error::MissingMeta(KEY_DEVICE_ID)),
    }
}

fn validated_device_id(id: String) -> Result<String> {
    if is_valid_device_id(&id) {
        Ok(id)
    } else {
        Err(Error::CorruptMeta {
            key: KEY_DEVICE_ID,
            value: id,
        })
    }
}

/// 前回の HLC の値を読む。まだなければ `HlcState::ZERO`。
pub fn load_hlc_state(conn: &Connection) -> Result<HlcState> {
    match get(conn, KEY_HLC_LAST)? {
        None => Ok(HlcState::ZERO),
        Some(s) => s.parse().map_err(|_| Error::CorruptMeta {
            key: KEY_HLC_LAST,
            value: s,
        }),
    }
}

/// HLC の値を保存する。
pub fn save_hlc_state(conn: &Connection, state: HlcState) -> Result<()> {
    set(conn, KEY_HLC_LAST, &state.to_string())
}

/// 最後に採番した seq を読む。まだなければ 0。
pub fn load_op_seq(conn: &Connection) -> Result<i64> {
    match get(conn, KEY_OP_SEQ_LAST)? {
        None => Ok(0),
        Some(s) => s
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0)
            .ok_or(Error::CorruptMeta {
                key: KEY_OP_SEQ_LAST,
                value: s,
            }),
    }
}

/// 最後に採番した seq を保存する。
pub fn save_op_seq(conn: &Connection, seq: i64) -> Result<()> {
    set(conn, KEY_OP_SEQ_LAST, &seq.to_string())
}
