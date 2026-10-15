//! フロントへ送るイベント。
//!
//! データが変わったら `library-changed` を出す（0005 §4）。フロントは変わった本の行だけを
//! `get_rows` で取り直し、`others` が true のときは保管場所・タグ・シリーズの一覧も取り直す。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use ts_rs::TS;

/// 蔵書の変更を知らせるイベントの名前。
pub const LIBRARY_CHANGED: &str = "library-changed";

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
}
