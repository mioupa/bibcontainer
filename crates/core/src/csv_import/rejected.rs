//! 取り込めなかった行（エラーの行）だけを、理由の列を付けた CSV に書き出す。
//!
//! 直して取り込み直せるように、元の欄の値はそのまま書く。形は書き出しと同じ（UTF-8 の BOM・CRLF・引用符・
//! 数式対策の `'`）。エラーかどうかは DB の状態に依らないので、接続は要らない。

use std::io::Write;
use std::path::Path;

use super::CsvImportOptions;
use super::plan::load;
use super::values::parse_row;
use crate::error::Result;
use crate::export::{UTF8_BOM, csv_line, write_file_atomically};

/// 最後に足す列の見出し。
pub const REASON_HEADER: &str = "取り込めなかった理由";

/// エラーの行を `out` に書く。書いた行数を返す。ヘッダーがあるときは、元のヘッダーに理由の列を足して先頭に書く。
pub fn write_rejected(
    bytes: &[u8],
    options: &CsvImportOptions,
    out: &mut impl Write,
) -> Result<u32> {
    let (table, pairs) = load(bytes, options)?;
    out.write_all(UTF8_BOM)?;
    if let Some(header) = &table.header {
        let cells = (0..table.width)
            .map(|i| header.raw(i).to_owned())
            .chain([REASON_HEADER.to_owned()]);
        out.write_all(csv_line(cells).as_bytes())?;
    }
    let mut written = 0;
    for row in &table.rows {
        let parsed = parse_row(row, &pairs);
        let Some(reason) = parsed.error else { continue };
        let cells = (0..table.width)
            .map(|i| row.raw(i).to_owned())
            .chain([reason.describe_ja()]);
        out.write_all(csv_line(cells).as_bytes())?;
        written += 1;
    }
    Ok(written)
}

/// `write_rejected` をファイルに書く。途中で失敗しても、既存のファイルは壊さない。
pub fn write_rejected_to_file(
    bytes: &[u8],
    options: &CsvImportOptions,
    path: &Path,
) -> Result<u32> {
    write_file_atomically(path, |w| write_rejected(bytes, options, w))
}
