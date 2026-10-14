//! ログの初期化。
//!
//! - 出力先は `app_log_dir()`（macOS は `~/Library/Logs/<identifier>`、
//!   Windows は `%LOCALAPPDATA%\<identifier>\logs`）。
//! - 日次ローテーションで、最大 14 ファイルまで残す。日付の区切りは UTC。
//! - デバッグビルドでは標準出力にも出す。
//! - 絞り込みは環境変数 `RUST_LOG` で上書きできる。

use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tracing::Subscriber;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

const LOG_FILE_PREFIX: &str = "bibcontainer";
const LOG_FILE_SUFFIX: &str = "log";
const MAX_LOG_FILES: usize = 14;

/// non-blocking writer のガード。落とすとバッファに残ったログが書き出される。
/// アプリの state として保持し、終了時に `flush` で明示的に落とす。
pub struct LogGuard(Mutex<Option<WorkerGuard>>);

impl LogGuard {
    fn new(guard: WorkerGuard) -> Self {
        Self(Mutex::new(Some(guard)))
    }

    /// バッファのログをすべて書き出す。以後のログは捨てられる。
    pub fn flush(&self) {
        if let Ok(mut guard) = self.0.lock() {
            drop(guard.take());
        }
    }
}

/// グローバルなサブスクライバを設定する。`setup` から 1 回だけ呼ぶ。
pub fn init(app: &AppHandle) -> Result<LogGuard, Box<dyn Error>> {
    let log_dir = app.path().app_log_dir()?;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_directives()));
    let (subscriber, guard) = build_subscriber(&log_dir, cfg!(debug_assertions), filter)?;
    subscriber.try_init()?;
    install_panic_hook();
    Ok(LogGuard::new(guard))
}

fn default_directives() -> &'static str {
    if cfg!(debug_assertions) {
        "info,bibcontainer_lib=debug,bibcontainer_core=debug"
    } else {
        "info"
    }
}

fn build_subscriber(
    log_dir: &Path,
    with_stdout: bool,
    filter: EnvFilter,
) -> Result<(impl Subscriber + Send + Sync, WorkerGuard), Box<dyn Error>> {
    fs::create_dir_all(log_dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let file_layer = fmt::layer().with_ansi(false).with_writer(writer);
    let stdout_layer = with_stdout.then(|| fmt::layer().with_writer(std::io::stdout));
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer);
    Ok((subscriber, guard))
}

/// パニックの内容もログに残す（リリースビルドの Windows ではコンソールが無いため）。
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "パニックが発生しました");
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_events_to_a_daily_log_file_in_the_log_dir() {
        let dir =
            std::env::temp_dir().join(format!("bibcontainer-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let (subscriber, guard) = build_subscriber(&dir, false, EnvFilter::new("info")).unwrap();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("テスト用のログ");
            tracing::debug!("debug は出ない");
        });
        drop(guard); // バッファを書き出す

        let files: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1, "ログファイルは 1 つだけのはず: {files:?}");
        let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("bibcontainer.") && name.ends_with(".log"),
            "{name}"
        );

        let content = fs::read_to_string(&files[0]).unwrap();
        assert!(content.contains("テスト用のログ"), "{content}");
        assert!(!content.contains("debug は出ない"), "{content}");

        fs::remove_dir_all(&dir).unwrap();
    }
}
