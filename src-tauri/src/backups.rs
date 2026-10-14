//! バックアップの保存先と、日次バックアップの定期実行（FR-IO-05、0005 §9）。
//!
//! - 保存先と世代数は設定の `backup` セクション。保存先の既定は `app_local_data_dir/backups`。
//! - 起動の数秒後と、そのあと 1 時間ごとに `run_scheduled_backup` を呼ぶ。実際に作るのは、最後の日次から
//!   24 時間以上たっているときだけ（`run_scheduled_backup` が判断する）。
//! - 失敗してもアプリは止めず、ログに出す。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bibcontainer_core::Clock;
use bibcontainer_core::backup::{self, BackupInfo};
use rusqlite::Connection;
use tauri::{AppHandle, Manager, Runtime};
use tokio::time::{MissedTickBehavior, interval};

use crate::database::{BACKUP_DIR_NAME, Database};
use crate::error::AppError;
use crate::settings::{self, BackupSettings};

/// 定期実行の間隔。
const TICK: Duration = Duration::from_secs(60 * 60);
/// 起動してから最初の確認までの待ち。起動直後の一覧の読み込みと、バックアップ（DB 全体のコピー）が
/// 接続を取り合わないようにする。
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// 既定のバックアップの保存先（`app_local_data_dir/backups`）。
pub fn default_backup_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|err| AppError::Io(format!("アプリのデータフォルダを決められません: {err}")))?;
    Ok(dir.join(BACKUP_DIR_NAME))
}

/// 設定に従う保存先と世代数。
pub fn configured(
    conn: &Connection,
    default_dir: &Path,
) -> Result<(PathBuf, backup::Keep), AppError> {
    let settings: BackupSettings = settings::load(conn)?;
    Ok((settings.resolve_dir(default_dir), settings.keep()))
}

/// 設定の保存先・世代数で、日次バックアップを（時期なら）取る。時期でなければ `None`。
pub fn scheduled_backup(
    conn: &Connection,
    default_dir: &Path,
    clock: &dyn Clock,
) -> Result<Option<BackupInfo>, AppError> {
    let (dir, keep) = configured(conn, default_dir)?;
    Ok(backup::run_scheduled_backup_keeping(
        conn, &dir, clock, keep,
    )?)
}

/// 日次バックアップの定期実行を始める。最初は起動の数秒後、以後は 1 時間ごと。
pub fn start_scheduler<R: Runtime>(app: AppHandle<R>, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let default_dir = match default_backup_dir(&app) {
            Ok(dir) => dir,
            Err(err) => {
                tracing::error!(error = %err, "日次バックアップを始められません");
                return;
            }
        };
        tokio::time::sleep(STARTUP_DELAY).await;
        let mut ticker = interval(TICK);
        // スリープなどで時間が飛んでも、取りこぼしを連続で取り返さず、次の周期に合わせる。
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let default_dir = default_dir.clone();
            let result = db
                .run(move |conn, _| {
                    scheduled_backup(conn, &default_dir, &bibcontainer_core::SystemClock)
                })
                .await;
            match result {
                Ok(Some(info)) => tracing::info!(path = %info.path, "日次バックアップを取りました"),
                Ok(None) => tracing::debug!("日次バックアップは、まだ時期ではありません"),
                // DB を開けていない間も止まらず、次の周期で試す。
                Err(err) => tracing::error!(error = %err, "日次バックアップに失敗しました"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use bibcontainer_core::backup::BackupKind;
    use bibcontainer_core::{ManualClock, open_in_memory};
    use serde_json::json;

    use super::*;

    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    const T0: i64 = 1_790_000_000_000;

    #[test]
    fn takes_a_daily_backup_in_the_default_folder_when_due_and_not_before() {
        let dir = tempfile::tempdir().unwrap();
        let default_dir = dir.path().join("backups");
        let conn = open_in_memory().unwrap();
        let clock = ManualClock::new(T0);

        let made = scheduled_backup(&conn, &default_dir, &clock).unwrap();
        assert_eq!(made.map(|b| b.kind), Some(BackupKind::Daily));
        // 1 時間後に呼んでも、まだ作らない（日次は 24 時間あけて作る）
        clock.advance(60 * 60 * 1000);
        assert!(
            scheduled_backup(&conn, &default_dir, &clock)
                .unwrap()
                .is_none()
        );
        // 24 時間たてば、また作る
        clock.advance(DAY_MS);
        assert!(
            scheduled_backup(&conn, &default_dir, &clock)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn uses_the_folder_and_generation_counts_from_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let default_dir = dir.path().join("default");
        let custom = dir.path().join("custom");
        let conn = open_in_memory().unwrap();
        bibcontainer_core::settings::set(
            &conn,
            "backup",
            &json!({
                "directory": custom.to_string_lossy(),
                "dailyKeep": 2,
                "weeklyKeep": 1,
            }),
        )
        .unwrap();
        let clock = ManualClock::new(T0);
        for _ in 0..20 {
            scheduled_backup(&conn, &default_dir, &clock).unwrap();
            clock.advance(DAY_MS);
        }
        assert!(!default_dir.exists(), "設定の保存先を使う");
        let all = backup::list_backups(&custom).unwrap();
        let count = |kind| all.iter().filter(|b| b.kind == kind).count();
        assert_eq!(count(BackupKind::Daily), 2);
        assert_eq!(count(BackupKind::Weekly), 1);
    }

    #[test]
    fn a_failure_is_an_error_value_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        // 保存先のパスが、フォルダではなくファイル
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let conn = open_in_memory().unwrap();
        bibcontainer_core::settings::set(
            &conn,
            "backup",
            &json!({ "directory": blocker.join("sub").to_string_lossy() }),
        )
        .unwrap();
        let clock = ManualClock::new(T0);
        let err = scheduled_backup(&conn, &dir.path().join("default"), &clock).unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Io);
    }

    #[test]
    fn a_relative_folder_in_the_settings_falls_back_to_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let default_dir = dir.path().join("default");
        let conn = open_in_memory().unwrap();
        bibcontainer_core::settings::set(&conn, "backup", &json!({ "directory": "relative/dir" }))
            .unwrap();
        let (resolved, keep) = configured(&conn, &default_dir).unwrap();
        assert_eq!(resolved, default_dir);
        assert_eq!((keep.daily, keep.weekly), (7, 4));
    }
}
