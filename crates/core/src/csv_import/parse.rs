//! CSV の解析（RFC 4180）。ヘッダーの扱いと空行の読み飛ばしは、ここで行う。

use super::ImportTarget;
use crate::error::{Error, Result};
use crate::export::FORMULA_PREFIXES;

/// 1 レコード（元の欄の文字そのまま）。
#[derive(Debug, Clone)]
pub(super) struct RawRow {
    /// そのレコードが始まる物理行（1 始まり）。
    pub line: u32,
    pub cells: Vec<String>,
}

impl RawRow {
    /// 整えた欄の値。足りない欄は空。
    pub fn cell(&self, i: usize) -> String {
        self.cells.get(i).map(|c| clean_cell(c)).unwrap_or_default()
    }

    /// 元の欄の値（整える前）。足りない欄は空。
    pub fn raw(&self, i: usize) -> &str {
        self.cells.get(i).map_or("", String::as_str)
    }
}

/// 読み込んだ表。
#[derive(Debug)]
pub(super) struct Table {
    pub header: Option<RawRow>,
    pub rows: Vec<RawRow>,
    /// 最も広い行の欄の数（ヘッダーを含む）。
    pub width: usize,
}

/// 欄の整え。書き出しが付けた数式対策の `'` を外し（`'` の次が数式の先頭になりうる文字のときだけ）、前後の空白を除く。
pub(super) fn clean_cell(cell: &str) -> String {
    let mut c = cell;
    if let Some(rest) = c.strip_prefix('\'')
        && rest.starts_with(FORMULA_PREFIXES)
    {
        c = rest;
    }
    c.trim().to_owned()
}

/// 解析する。欄の数がそろっていなくてもよい。すべての欄が空（空白だけ）のレコードは読み飛ばす。
pub(super) fn read_table(text: &str, has_header: bool) -> Result<Table> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        // 行番号は csv の数え方に頼らず、バイト位置から自分で数える（CRLF のあとの行や、空行を読み飛ばした
        // あとの行がずれるため）。LF を区切りにして、行末の CR は自分で外す。
        .terminator(csv::Terminator::Any(b'\n'))
        .from_reader(text.as_bytes());
    let mut header = None;
    let mut rows = Vec::new();
    let mut width = 0;
    let mut record = csv::StringRecord::new();
    let bytes = text.as_bytes();
    // 行番号の数え方: 直前のレコードの位置から、改行を数え足していく（位置は単調に増える）。
    let (mut scan, mut line_no) = (0_usize, 1_u32);
    loop {
        match reader.read_record(&mut record) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                let line = e.position().map_or(0, csv::Position::line);
                return Err(Error::validation(
                    "file",
                    format!("CSV を読めませんでした（{line} 行目付近）"),
                ));
            }
        }
        if record.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let row = RawRow {
            line: {
                // csv は、空行を読み飛ばす前の位置を返すことがある。レコードの本体が始まる所まで進める。
                let mut at = record
                    .position()
                    .map_or(scan, |p| usize::try_from(p.byte()).unwrap_or(scan))
                    .max(scan);
                while at < bytes.len() {
                    if bytes[at] == b'\n' {
                        at += 1;
                    } else if bytes[at..].starts_with(b"\r\n") {
                        at += 2;
                    } else {
                        break;
                    }
                }
                let newlines = bytes[scan..at].iter().filter(|b| **b == b'\n').count();
                line_no += u32::try_from(newlines).unwrap_or(0);
                scan = at;
                line_no
            },
            cells: record.iter().map(str::to_owned).collect(),
        };
        let mut row = row;
        if let Some(last) = row.cells.last_mut()
            && last.ends_with('\r')
        {
            last.pop();
        }
        width = width.max(row.cells.len());
        if has_header && header.is_none() {
            header = Some(row);
        } else {
            rows.push(row);
        }
    }
    Ok(Table {
        header,
        rows,
        width,
    })
}

/// 対応づけを (列, 項目) の組にする。
pub(super) fn mapping_pairs(mapping: &[super::ColumnMapping]) -> Vec<(usize, ImportTarget)> {
    mapping
        .iter()
        .map(|m| (usize::try_from(m.column).unwrap_or(usize::MAX), m.target))
        .collect()
}
