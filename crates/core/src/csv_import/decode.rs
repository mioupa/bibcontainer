//! 文字コードの判定と復号。

use super::CsvEncoding;
use crate::error::{Error, Result};

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// 復号して、使った文字コードを返す。
///
/// 指定がなければ自動判定: UTF-8 の BOM があれば UTF-8、なければ厳密に UTF-8 として読めるときだけ UTF-8、
/// そうでなければ Shift_JIS（WHATWG の Shift_JIS は Windows-31J で、①や髙も読める）。
/// どちらでも読めなければエラー。指定があれば、読めない文字を置き換え文字にしてでも、その文字コードで読む。
pub(super) fn decode(bytes: &[u8], forced: Option<CsvEncoding>) -> Result<(String, CsvEncoding)> {
    match forced {
        Some(CsvEncoding::Utf8) => {
            let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
            Ok((
                String::from_utf8_lossy(body).into_owned(),
                CsvEncoding::Utf8,
            ))
        }
        Some(CsvEncoding::ShiftJis) => {
            let (text, _) = encoding_rs::SHIFT_JIS.decode_without_bom_handling(bytes);
            Ok((text.into_owned(), CsvEncoding::ShiftJis))
        }
        None => {
            if let Some(body) = bytes.strip_prefix(BOM) {
                return Ok((
                    String::from_utf8_lossy(body).into_owned(),
                    CsvEncoding::Utf8,
                ));
            }
            if let Ok(text) = std::str::from_utf8(bytes) {
                return Ok((text.to_owned(), CsvEncoding::Utf8));
            }
            match encoding_rs::SHIFT_JIS.decode_without_bom_handling_and_without_replacement(bytes)
            {
                Some(text) => Ok((text.into_owned(), CsvEncoding::ShiftJis)),
                None => Err(Error::validation(
                    "encoding",
                    "文字コードを判定できませんでした（UTF-8 と Shift_JIS に対応しています）",
                )),
            }
        }
    }
}
