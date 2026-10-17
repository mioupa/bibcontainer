//! ログ・バックアップのフォルダを OS のファイラで開く。

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::backups::{configured, default_backup_dir};
use crate::database::Database;
use crate::error::AppError;

/// フォルダがなければ作ってから、OS で開く。
fn open_dir<R: Runtime>(app: &AppHandle<R>, dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|err| AppError::Io(format!("フォルダを開けませんでした: {err}")))
}

/// ログのフォルダを開く。
#[tauri::command]
pub async fn open_logs_dir<R: Runtime>(app: AppHandle<R>) -> Result<(), AppError> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|err| AppError::Io(format!("ログのフォルダを決められません: {err}")))?;
    open_dir(&app, &dir)
}

/// `open_link` で開いてよいホスト（完全一致）。取得元のクレジット（`ProviderInfo.credit`。NDLサーチ・openBD・
/// Google Books・Open Library）と、検索結果のページ（NDLサーチ・Google Books・Open Library）、連絡先の URL の既定
/// （GitHub）。サブドメインや、ホスト名を含む別のドメインは通さない。
pub const LINK_HOSTS: [&str; 5] = [
    "books.google.com",
    "ndlsearch.ndl.go.jp",
    "openlibrary.org",
    "openbd.jp",
    "github.com",
];

/// `open_link` に渡された URL を確かめて、開く文字列（URL として整えたもの）を返す。
///
/// 通すのは、`https`、ユーザー名・パスワードなし、ポートの指定なし、ホストが `LINK_HOSTS` のどれか、の URL だけ。
/// フロントが組み立てた URL（検索結果の `link`）も、利用者が目にする URL も、ここを通してからブラウザへ渡す。
pub fn check_link(raw: &str) -> Result<String, AppError> {
    let invalid =
        |reason: &str| AppError::Validation(format!("このリンクは開けません（{reason}）"));
    let url = url::Url::parse(raw.trim()).map_err(|_| invalid("URL として読めません"))?;
    if url.scheme() != "https" {
        return Err(invalid("https のリンクだけ開けます"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("ユーザー名やパスワードを含みます"));
    }
    if url.port().is_some() {
        return Err(invalid("ポートが指定されています"));
    }
    match url.host_str() {
        Some(host) if LINK_HOSTS.contains(&host) => Ok(url.to_string()),
        _ => Err(invalid("開けるサイトではありません")),
    }
}

/// 取得元のページ・クレジットのリンクを、既定のブラウザで開く。開けるのは `check_link` を通った URL だけ。
/// Rust から開くので、プラグインの権限（capabilities）の設定は要らない（権限が見るのは、JS からの呼び出し）。
#[tauri::command]
pub async fn open_link<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), AppError> {
    let url = check_link(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| AppError::Io(format!("ブラウザを開けませんでした: {err}")))
}

/// バックアップの保存先のフォルダを開く（設定の保存先。未設定なら既定の場所）。
#[tauri::command]
pub async fn open_backup_dir<R: Runtime>(
    app: AppHandle<R>,
    db: State<'_, Arc<Database>>,
) -> Result<(), AppError> {
    let default_dir = default_backup_dir(&app)?;
    let dir = {
        let for_settings = default_dir.clone();
        db.run(move |conn, _| Ok(configured(conn, &for_settings)?.0))
            .await
            .unwrap_or(default_dir)
    };
    open_dir(&app, &dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_to_the_listed_sites_over_https_are_allowed() {
        for (raw, opened) in [
            // 検索結果のページ
            (
                "https://ndlsearch.ndl.go.jp/books/R100000002-I034807010",
                "https://ndlsearch.ndl.go.jp/books/R100000002-I034807010",
            ),
            (
                "https://books.google.com/books?id=hjEFCAAAQBAJ",
                "https://books.google.com/books?id=hjEFCAAAQBAJ",
            ),
            (
                "https://openlibrary.org/books/OL26222911M",
                "https://openlibrary.org/books/OL26222911M",
            ),
            // 「このアプリについて」のクレジットの URL（`ProviderInfo.credit`）と、連絡先の既定
            (
                "https://ndlsearch.ndl.go.jp/",
                "https://ndlsearch.ndl.go.jp/",
            ),
            ("https://openbd.jp/", "https://openbd.jp/"),
            ("https://books.google.com/", "https://books.google.com/"),
            ("https://openlibrary.org/", "https://openlibrary.org/"),
            (
                "https://github.com/mioupa/bibcontainer",
                "https://github.com/mioupa/bibcontainer",
            ),
            // URL として整えたものを開く（ホストの大文字小文字、既定のポート、前後の空白、`#` の部分）
            (
                "  HTTPS://OpenLibrary.ORG:443/works/OL1W#x ",
                "https://openlibrary.org/works/OL1W#x",
            ),
            // `\` は、ブラウザと同じく `/` として読む（ホストは github.com のまま。`@` 以降は経路）
            (
                "https://github.com\\@evil.example/",
                "https://github.com/@evil.example/",
            ),
        ] {
            assert_eq!(check_link(raw).as_deref().ok(), Some(opened), "{raw}");
        }
    }

    #[test]
    fn every_url_in_the_credits_is_allowed() {
        // 取得元のクレジットの文言（`ProviderInfo.credit`）に含まれる URL は、すべて開ける
        for info in bibcontainer_metadata::default_providers(None)
            .iter()
            .map(|p| p.info())
        {
            let url = info
                .credit
                .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
                .find(|w| w.starts_with("https://"))
                .unwrap_or_else(|| panic!("{} のクレジットに URL がない", info.credit));
            assert!(check_link(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn other_links_are_rejected_as_validation_errors() {
        for raw in [
            // https でない
            "http://books.google.com/books?id=x",
            "ftp://github.com/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "mailto:a@openlibrary.org",
            "//openlibrary.org/works/OL1W",
            "openlibrary.org/works/OL1W",
            // ほかのホスト
            "https://example.com/",
            "https://www.google.com/books",
            "https://google.com/",
            "https://www.github.com/",
            "https://api.github.com/",
            "https://evil.books.google.com/",
            "https://books.google.com.evil.example/",
            "https://notgithub.com/",
            "https://github.com.evil.example/mioupa/bibcontainer",
            "https://ndlsearch.ndl.go.jp.evil.example/",
            "https://evil.example/https://ndlsearch.ndl.go.jp/",
            "https://evil.example/?u=https://github.com/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://2130706433/",
            // ユーザー名・パスワード（ホストの見かけを偽る）
            "https://github.com@evil.example/",
            "https://github.com:pw@evil.example/",
            "https://user@github.com/",
            "https://user:pw@openlibrary.org/",
            "https://evil.example\\@github.com/",
            // ポートの指定
            "https://github.com:8443/",
            "https://openlibrary.org:444/",
            // 空・読めない
            "",
            "   ",
            "https://",
            "https:///x",
            "not a url",
            "https://git hub.com/",
        ] {
            match check_link(raw) {
                Err(AppError::Validation(message)) => {
                    assert!(message.contains("開けません"), "{raw}: {message}")
                }
                other => panic!("{raw:?}: {other:?}"),
            }
        }
    }
}
