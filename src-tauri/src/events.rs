//! フロントへ送るイベント。
//!
//! データが変わったら `library-changed` を出す（0005 §4）。フロントは変わった本の行だけを
//! `get_rows` で取り直し、`others` が true のときは保管場所・タグ・シリーズの一覧も取り直す。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use ts_rs::TS;

/// 蔵書の変更を知らせるイベントの名前。
pub const LIBRARY_CHANGED: &str = "library-changed";

/// 書影が手元に届いたことを知らせるイベントの名前。
pub const COVERS_ARRIVED: &str = "covers-arrived";

/// 1 つの `covers-arrived` に入れるハッシュの数の上限（最初の同期で数千個届いても、1 つのイベントを大きくしない）。
pub const COVERS_ARRIVED_CHUNK: usize = 500;

/// `library-changed` のペイロード。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryChanged {
    /// 変わった（作られた・消えた・戻った）本の ID。
    pub books: Vec<String>,
    /// 本以外（保管場所・タグ・シリーズ）や、DB 全体が変わった。
    pub others: bool,
}

/// `covers-arrived` のペイロード（同期フォルダから写した書影）。
///
/// 本の行が先に届き、書影があとから届くことがある（同期フォルダに書影のファイルがまだない）。そのとき画面は、
/// 書影が読み込めずに仮の表紙を出している。届いた書影のハッシュを知らせて、読み込み直させる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CoversArrived {
    /// 手元に届いた書影のハッシュ（`cover_hash`。小文字の 16 進 64 桁）。
    pub hashes: Vec<String>,
}

/// `library-changed` を出す。送れなくても処理は止めず、ログに残す。
///
/// 自分の変更を知らせるものなので、同期の書き出しの合図（最後の変更から 5 秒後に書き出す）も送る。
/// 他の端末から取り込んだ変更を知らせるときは、`emit_remote_library_changed`（合図を送らない）を使う。
pub fn emit_library_changed<R: Runtime>(app: &AppHandle<R>, books: Vec<String>, others: bool) {
    send(app, books, others);
    crate::sync::notify_local_change(app);
}

/// 他の端末から取り込んだ変更（と、同期の置き換えによる DB の入れ替え）を `library-changed` で知らせる。
/// 書き出しの合図は送らない（取り込んだ変更を、また書き出さないため）。
pub fn emit_remote_library_changed<R: Runtime>(
    app: &AppHandle<R>,
    books: Vec<String>,
    others: bool,
) {
    send(app, books, others);
}

/// 同期フォルダから書影を写したことを `covers-arrived` で知らせる（`COVERS_ARRIVED_CHUNK` 個ずつ）。
pub fn emit_covers_arrived<R: Runtime>(app: &AppHandle<R>, hashes: Vec<String>) {
    for chunk in hashes.chunks(COVERS_ARRIVED_CHUNK) {
        let payload = CoversArrived {
            hashes: chunk.to_vec(),
        };
        if let Err(err) = app.emit(COVERS_ARRIVED, &payload) {
            tracing::warn!(error = %err, "covers-arrived を送れませんでした");
        }
    }
}

fn send<R: Runtime>(app: &AppHandle<R>, books: Vec<String>, others: bool) {
    let payload = LibraryChanged { books, others };
    if let Err(err) = app.emit(LIBRARY_CHANGED, &payload) {
        tracing::warn!(error = %err, "library-changed を送れませんでした");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_has_the_documented_shape() {
        let payload = LibraryChanged {
            books: vec!["a".into(), "b".into()],
            others: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "books": ["a", "b"], "others": true })
        );
        assert_eq!(LIBRARY_CHANGED, "library-changed");
    }

    #[test]
    fn covers_arrived_has_the_documented_shape_and_name() {
        let payload = CoversArrived {
            hashes: vec!["ab".repeat(32)],
        };
        assert_eq!(
            serde_json::to_value(&payload).unwrap(),
            serde_json::json!({ "hashes": ["ab".repeat(32)] })
        );
        assert_eq!(COVERS_ARRIVED, "covers-arrived");
    }
}
