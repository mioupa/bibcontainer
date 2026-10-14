//! フロントエンドから呼ばれる IPC コマンド。
//!
//! - 引数・戻り値の型は ts-rs で `src/bindings/` に TypeScript として出力する
//!   （`cargo test` で生成される）。フロント側の対応表は `src/lib/ipc.ts`。
//! - 戻り値のエラーは `AppError`（`{ kind, message }` として届く）。
//! - DB に触るコマンドは async にし、`Database::run` 経由で別スレッドで実行する。

use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::database::Database;
use crate::error::AppError;
use bibcontainer_core::{HlcState, meta, migrate, pending_ops};

/// アプリと DB の状態（動作確認と診断用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    /// この端末の ID。
    pub device_id: String,
    /// DB のスキーマ版（`PRAGMA user_version`）。
    pub schema_version: u32,
    /// DB ファイルのパス。
    pub db_path: String,
    /// HLC の前回値（`{ミリ秒}:{カウンタ}`）。まだ変更を記録していなければ null。
    pub hlc_last: Option<String>,
    /// outbox の、まだ同期フォルダへ書き出していない Op の件数。
    pub pending_ops: u32,
}

impl AppInfo {
    fn collect(
        name: String,
        version: String,
        conn: &Connection,
        db_path: &Path,
    ) -> Result<Self, AppError> {
        let hlc = meta::load_hlc_state(conn)?;
        Ok(Self {
            name,
            version,
            device_id: meta::device_id(conn)?,
            schema_version: migrate::user_version(conn)?,
            db_path: db_path.to_string_lossy().into_owned(),
            hlc_last: (hlc != HlcState::ZERO).then(|| hlc.to_string()),
            pending_ops: u32::try_from(pending_ops(conn)?.len()).unwrap_or(u32::MAX),
        })
    }
}

#[tauri::command]
pub async fn app_info(app: AppHandle, db: State<'_, Arc<Database>>) -> Result<AppInfo, AppError> {
    tracing::debug!("app_info が呼ばれました");
    let package = app.package_info();
    let name = package.name.clone();
    let version = package.version.to_string();
    db.run(move |conn, path| AppInfo::collect(name, version, conn, path))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bibcontainer_core::{Entity, SystemClock, ids::new_entity_id, record_change_committed};
    use serde_json::json;

    #[test]
    fn collects_the_state_of_a_fresh_db_and_counts_pending_ops() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("library.sqlite3");
        let (mut conn, _) = bibcontainer_core::open(&db_path, &dir.path().join("backups")).unwrap();

        let info =
            AppInfo::collect("bibcontainer".into(), "0.1.0".into(), &conn, &db_path).unwrap();
        assert_eq!(info.name, "bibcontainer");
        assert_eq!(info.schema_version, migrate::LATEST_VERSION);
        assert_eq!(info.device_id, meta::device_id(&conn).unwrap());
        assert_eq!(info.db_path, db_path.to_string_lossy());
        assert_eq!(info.hlc_last, None);
        assert_eq!(info.pending_ops, 0);

        record_change_committed(
            &mut conn,
            &SystemClock,
            Entity::Book,
            &new_entity_id(),
            [("title".to_string(), json!("本"))],
        )
        .unwrap();

        let info =
            AppInfo::collect("bibcontainer".into(), "0.1.0".into(), &conn, &db_path).unwrap();
        assert!(info.hlc_last.is_some());
        assert_eq!(info.pending_ops, 1);
    }

    #[test]
    fn app_info_serializes_with_camel_case_keys() {
        let info = AppInfo {
            name: "bibcontainer".into(),
            version: "0.1.0".into(),
            device_id: "d-0123456789ab".into(),
            schema_version: 1,
            db_path: "/tmp/library.sqlite3".into(),
            hlc_last: None,
            pending_ops: 0,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["deviceId"], "d-0123456789ab");
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["dbPath"], "/tmp/library.sqlite3");
        assert!(json["hlcLast"].is_null());
        assert_eq!(json["pendingOps"], 0);
    }
}
