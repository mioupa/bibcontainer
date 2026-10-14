//! 書き出し（CSV・JSON）と、ISBN 一覧の解析。
//!
//! ファイルの場所は、フロントがダイアログ（`@tauri-apps/plugin-dialog`）で選んだ絶対パス。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bibcontainer_core::SystemClock;
use bibcontainer_core::barcode::{self, ScanCode};
use bibcontainer_core::export;
use tauri::State;

use crate::database::Database;
use crate::error::AppError;

/// ISBN 一覧として読むファイルの大きさの上限（バイト）。1 行 14 バイトとして 60 万行ほど。
const MAX_ISBN_LIST_BYTES: u64 = 8 * 1024 * 1024;

/// 書き出し先として使えるパスか調べる。絶対パスで、フォルダではなく、親のフォルダがあること。
fn checked_output_path(path: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(AppError::Validation(
            "入力が不正です（path）: 保存先は絶対パスで指定してください".to_owned(),
        ));
    }
    if path.is_dir() {
        return Err(AppError::Validation(
            "入力が不正です（path）: 保存先がフォルダです。ファイル名を指定してください".to_owned(),
        ));
    }
    match path.parent() {
        Some(parent) if parent.is_dir() => Ok(path),
        _ => Err(AppError::Validation(
            "入力が不正です（path）: 保存先のフォルダがありません".to_owned(),
        )),
    }
}

/// CSV に書き出す（UTF-8・BOM 付き）。`ids` を渡すと、その本だけを渡した順に書く（表示中の絞り込み
/// 結果の書き出し）。渡さないと完全削除していない全冊。書いた本の数を返す。
#[tauri::command]
pub async fn export_csv(
    db: State<'_, Arc<Database>>,
    path: String,
    ids: Option<Vec<String>>,
) -> Result<usize, AppError> {
    let path = checked_output_path(&path)?;
    db.run(move |conn, _| Ok(export::export_csv_to_file(conn, &path, ids.as_deref())?))
        .await
}

/// 全データを JSON に書き出す（ID・項目ごとの時計つき）。
#[tauri::command]
pub async fn export_json(db: State<'_, Arc<Database>>, path: String) -> Result<(), AppError> {
    let path = checked_output_path(&path)?;
    db.run(move |conn, _| Ok(export::export_json_to_file(conn, &SystemClock, &path)?))
        .await
}

/// ISBN の一覧（テキストファイル）を読み、各行を `classify_code` と同じ規則で判定して返す（FR-IO-03）。
///
/// 1 行に 1 つ。空行と前後の空白は無視する。2段目のコードが混ざっていてもよい（そのまま `secondTier`
/// として返す。ISBN との組み合わせは、フロントが連続登録と同じ流れで決める）。
#[tauri::command]
pub async fn parse_isbn_list(path: String) -> Result<Vec<ScanCode>, AppError> {
    let path = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || read_isbn_list(&path))
        .await
        .map_err(|err| AppError::Internal(format!("バックグラウンド処理に失敗しました: {err}")))?
}

fn read_isbn_list(path: &Path) -> Result<Vec<ScanCode>, AppError> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_ISBN_LIST_BYTES {
        return Err(AppError::Validation(format!(
            "入力が不正です（path）: ファイルが大きすぎます（{} MB まで）",
            MAX_ISBN_LIST_BYTES / 1024 / 1024
        )));
    }
    let bytes = std::fs::read(path)?;
    // UTF-8 として読む（不正なバイトは置き換える。数字以外は判定で弾かれる）。
    Ok(parse_isbn_lines(&String::from_utf8_lossy(&bytes)))
}

/// 行ごとに判定する。BOM と、前後の空白・空行は無視する。改行は LF・CRLF・CR のどれでもよい。
pub fn parse_isbn_lines(text: &str) -> Vec<ScanCode> {
    text.trim_start_matches('\u{feff}')
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(barcode::classify)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISBN: &str = "9784003101117";
    const ISBN_HYPHEN: &str = "978-4-00-310111-7";
    // 2段目: 192 + Cコード 0193 + 価格 00700 + チェックデジット
    const SECOND: &str = "1920193007008";

    fn kinds(codes: &[ScanCode]) -> Vec<&'static str> {
        codes
            .iter()
            .map(|c| match c {
                ScanCode::Isbn13 { .. } => "isbn13",
                ScanCode::SecondTier { .. } => "secondTier",
                ScanCode::Magazine { .. } => "magazine",
                ScanCode::OtherJan { .. } => "otherJan",
                ScanCode::Invalid { .. } => "invalid",
            })
            .collect()
    }

    #[test]
    fn one_code_per_line_blank_lines_and_spaces_are_ignored() {
        let text = format!("\n  {ISBN}  \n\n{ISBN_HYPHEN}\r\n\t\r\n9784003101117\n");
        let codes = parse_isbn_lines(&text);
        assert_eq!(kinds(&codes), ["isbn13", "isbn13", "isbn13"]);
        for code in &codes {
            assert!(matches!(code, ScanCode::Isbn13 { isbn13, .. } if isbn13 == ISBN));
        }
    }

    #[test]
    fn second_tier_codes_may_be_mixed_in_and_come_back_in_order() {
        let text = format!("{ISBN}\n{SECOND}\n{ISBN}\n");
        let codes = parse_isbn_lines(&text);
        assert_eq!(kinds(&codes), ["isbn13", "secondTier", "isbn13"]);
        assert!(matches!(
            &codes[1],
            ScanCode::SecondTier { c_code, list_price: Some(700), .. } if c_code == "0193"
        ));
    }

    #[test]
    fn isbn10_is_converted_and_bad_lines_are_returned_as_invalid() {
        let text = "4003101111\n9784003101118\nabc\n4910000000000\n";
        let codes = parse_isbn_lines(text);
        assert!(matches!(&codes[0], ScanCode::Isbn13 { isbn13, .. } if isbn13 == ISBN));
        assert!(
            matches!(&codes[1], ScanCode::Invalid { .. }),
            "チェックデジット違い"
        );
        assert!(matches!(&codes[2], ScanCode::Invalid { .. }));
        assert_eq!(codes.len(), 4);
    }

    #[test]
    fn bom_and_old_mac_line_endings_are_handled() {
        let text = format!("\u{feff}{ISBN}\r{ISBN}\r");
        assert_eq!(kinds(&parse_isbn_lines(&text)), ["isbn13", "isbn13"]);
        assert!(parse_isbn_lines("").is_empty());
        assert!(parse_isbn_lines("\n \r\n\t").is_empty());
    }

    #[test]
    fn reads_a_file_and_rejects_huge_or_missing_ones() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("isbn.txt");
        std::fs::write(&file, format!("{ISBN}\n{SECOND}\n")).unwrap();
        assert_eq!(
            kinds(&read_isbn_list(&file).unwrap()),
            ["isbn13", "secondTier"]
        );

        let missing = dir.path().join("none.txt");
        assert_eq!(
            read_isbn_list(&missing).unwrap_err().kind(),
            crate::error::ErrorKind::Io
        );

        let big = dir.path().join("big.txt");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(MAX_ISBN_LIST_BYTES + 1).unwrap();
        assert_eq!(
            read_isbn_list(&big).unwrap_err().kind(),
            crate::error::ErrorKind::Validation
        );
    }

    #[test]
    fn output_paths_must_be_absolute_files_in_an_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("out.csv");
        assert_eq!(checked_output_path(ok.to_str().unwrap()).unwrap(), ok);
        for bad in [
            "out.csv".to_owned(),
            dir.path().to_string_lossy().into_owned(),
            dir.path()
                .join("no-such")
                .join("out.csv")
                .to_string_lossy()
                .into_owned(),
            String::new(),
        ] {
            assert_eq!(
                checked_output_path(&bad).unwrap_err().kind(),
                crate::error::ErrorKind::Validation,
                "{bad}"
            );
        }
    }
}
