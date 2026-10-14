//! バーコードの判定（FR-SCAN-01〜04）と、Cコード表（付録 A）。
//!
//! - [`classify`]: 読み取った文字列を ISBN-13・2段目・雑誌・一般 JAN・不正のどれかに判定する。
//! - [`isbn10_to_13`]・[`isbn13_to_10`]・`is_valid_*`: ISBN の変換と検証。
//! - [`describe_c_code`]・[`c_code_table`]: Cコードの各桁の名称。
//!
//! 判定の結果 [`ScanCode`] は IPC（`classify_code`）でそのままフロントへ渡す。
//! 形は `docs/decisions/0005-phase1-architecture.md` の 5 に従う。

mod c_code;
mod isbn;

use serde::Serialize;
use ts_rs::TS;

pub use c_code::{CCodeEntry, CCodeInfo, CCodeTable, c_code_table, describe_c_code};
pub use isbn::{
    ean8_check_digit, ean13_check_digit, is_valid_ean8, is_valid_ean13, is_valid_isbn10,
    is_valid_isbn13, isbn10_check_digit, isbn10_to_13, isbn13_to_10,
};

/// 判定できなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScanCodeInvalidReason {
    /// チェックデジットが合わない。
    Checksum,
    /// 桁数や文字が、判定の対象にならない形をしている。
    Format,
}

/// バーコードの判定結果（FR-SCAN-01）。
///
/// どの種類でも `raw` は、入力を整えた文字列（前後の空白・途中の空白とハイフンを除き、
/// 全角数字を半角にし、末尾の `x` を `X` にしたもの）である。入力そのものは呼び出し側が持っている。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ScanCode {
    /// ISBN-13（書籍 JAN の 1 段目）。ISBN-10 で入力されたものも、ここに変換して返す。
    Isbn13 {
        raw: String,
        /// ハイフンなしの 13 桁。
        isbn13: String,
        /// 対応する ISBN-10。`978` で始まるときだけ値が入る（`979` は `null`）。
        /// 入力が ISBN-10 だったときは、入力そのもの（整えたもの）である。
        isbn10: Option<String>,
    },
    /// 書籍 JAN の 2 段目（`192` で始まる 13 桁）。
    SecondTier {
        raw: String,
        /// 4〜7 桁目の Cコード（4 桁）。名称は Cコード表（`c_code_table`、または `describe_c_code`）で引く。
        c_code: String,
        /// 8〜12 桁目の本体価格（税抜、円）。`00000` は価格不明として `null`。
        list_price: Option<u32>,
    },
    /// 雑誌コード（`491` で始まる 13 桁）。非対応。
    Magazine { raw: String },
    /// 書籍以外の一般 JAN（上記以外の 13 桁と 8 桁）。非対応。
    OtherJan { raw: String },
    /// 受け付けない入力。
    Invalid {
        raw: String,
        reason: ScanCodeInvalidReason,
    },
}

/// 全角の ASCII（U+FF01〜U+FF5E）を半角にする。それ以外はそのまま返す。
fn to_half_width(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => {
            // 範囲内なので、差し引いた値は常に有効な ASCII
            char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c)
        }
        _ => c,
    }
}

/// ハイフンとして扱う文字。ASCII のハイフンのほか、日本語入力や貼り付けで混ざりやすいもの
/// （各種ダッシュ、マイナス記号、長音符。全角ハイフンは半角にしたあとで `-` になる）。
fn is_hyphen(c: char) -> bool {
    matches!(
        c,
        '-' | '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{30FC}' | '\u{FF70}'
    )
}

/// 入力を判定用に整える。
///
/// 前後と途中の空白を除き、全角英数字を半角にし、ハイフン類を除き、末尾の `x` を `X` にする。
/// （ISBN-10 の末尾 `X` の入力ゆれを吸収するため。ほかの位置の `x` は変えない。）
pub fn normalize(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(to_half_width)
        .filter(|&c| !is_hyphen(c))
        .collect();
    if out.ends_with('x') {
        out.pop();
        out.push('X');
    }
    out
}

fn invalid(raw: String, reason: ScanCodeInvalidReason) -> ScanCode {
    ScanCode::Invalid { raw, reason }
}

/// 整えた 13 桁の数字を判定する。
fn classify_ean13(code: String) -> ScanCode {
    if !isbn::valid_ean13(&code) {
        return invalid(code, ScanCodeInvalidReason::Checksum);
    }
    if code.starts_with("978") || code.starts_with("979") {
        // 979 は ISBN-10 に対応がないので None になる
        let isbn10 = isbn::to_isbn10(&code);
        ScanCode::Isbn13 {
            isbn13: code.clone(),
            raw: code,
            isbn10,
        }
    } else if code.starts_with("192") {
        // 4〜7 桁目が Cコード、8〜12 桁目が本体価格（00000 は不明）
        let c_code = code[3..7].to_owned();
        let list_price = code[7..12].parse::<u32>().ok().filter(|&p| p != 0);
        ScanCode::SecondTier {
            raw: code,
            c_code,
            list_price,
        }
    } else if code.starts_with("491") {
        ScanCode::Magazine { raw: code }
    } else {
        ScanCode::OtherJan { raw: code }
    }
}

/// 読み取った文字列を判定する（FR-SCAN-01、FR-SCAN-02）。
///
/// 前処理は [`normalize`] を参照。判定は次のとおり。
///
/// | 整えた入力 | 結果 |
/// |---|---|
/// | 13 桁、`978` / `979` 始まり | `Isbn13` |
/// | 13 桁、`192` 始まり | `SecondTier`（Cコードと本体価格に分解） |
/// | 13 桁、`491` 始まり | `Magazine` |
/// | 13 桁のそれ以外、8 桁 | `OtherJan` |
/// | 10 桁（末尾 `X` 可） | ISBN-13 に変換して `Isbn13`（`isbn10` に入力） |
/// | チェックデジット不一致 | `Invalid`（`checksum`） |
/// | 上記以外の形 | `Invalid`（`format`） |
pub fn classify(raw: &str) -> ScanCode {
    let code = normalize(raw);
    let digits = isbn::is_digits(&code);
    match code.len() {
        13 if digits => classify_ean13(code),
        8 if digits => {
            if isbn::valid_ean8(&code) {
                ScanCode::OtherJan { raw: code }
            } else {
                invalid(code, ScanCodeInvalidReason::Checksum)
            }
        }
        // 10 桁は、左 9 桁が数字で末尾が数字か X のときだけ ISBN-10 の形とみなす
        10 if code.is_ascii()
            && isbn::is_digits(&code[..9])
            && matches!(code.as_bytes()[9], b'0'..=b'9' | b'X') =>
        {
            match isbn::to_isbn13(&code) {
                Some(isbn13) => ScanCode::Isbn13 {
                    raw: code.clone(),
                    isbn13,
                    isbn10: Some(code),
                },
                None => invalid(code, ScanCodeInvalidReason::Checksum),
            }
        }
        _ => invalid(code, ScanCodeInvalidReason::Format),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_and_strips() {
        assert_eq!(normalize("  9784003101117 \r\n"), "9784003101117");
        assert_eq!(normalize("978-4-00-310111-7"), "9784003101117");
        assert_eq!(normalize("978 4 00 310111 7"), "9784003101117");
        assert_eq!(normalize("９７８４００３１０１１１７"), "9784003101117");
        // 全角ハイフン、各種ダッシュ、マイナス、長音符（日本語入力で `-` を押したとき）
        assert_eq!(normalize("４－００－３１０１１１－１"), "4003101111");
        assert_eq!(
            normalize("4\u{2010}00\u{2013}310111\u{2212}1"),
            "4003101111"
        );
        assert_eq!(normalize("4ー00ー310111ー1"), "4003101111");
        assert_eq!(normalize("\u{3000}4003101111\u{3000}"), "4003101111");
    }

    #[test]
    fn normalize_uppercases_only_the_trailing_x() {
        assert_eq!(normalize("080442957x"), "080442957X");
        assert_eq!(normalize("080442957Ｘ"), "080442957X");
        assert_eq!(normalize("080442957ｘ"), "080442957X");
        // 末尾以外の x は変えない（そのあと形が合わず弾かれる）
        assert_eq!(normalize("08x442957"), "08x442957");
        assert_eq!(normalize("x"), "X");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn normalize_leaves_other_characters() {
        assert_eq!(normalize("ISBN9784003101117"), "ISBN9784003101117");
        assert_eq!(normalize("あ１"), "あ1");
    }

    #[test]
    fn to_half_width_maps_only_fullwidth_ascii() {
        assert_eq!(to_half_width('０'), '0');
        assert_eq!(to_half_width('９'), '9');
        assert_eq!(to_half_width('Ｘ'), 'X');
        assert_eq!(to_half_width('～'), '~');
        assert_eq!(to_half_width('あ'), 'あ');
        assert_eq!(to_half_width('0'), '0');
    }

    #[test]
    fn classify_returns_the_normalized_raw() {
        assert_eq!(
            classify(" 978-4-00-310111-7 "),
            ScanCode::Isbn13 {
                raw: "9784003101117".into(),
                isbn13: "9784003101117".into(),
                isbn10: Some("4003101111".into()),
            }
        );
        assert_eq!(
            classify("ＡＢＣ"),
            ScanCode::Invalid {
                raw: "ABC".into(),
                reason: ScanCodeInvalidReason::Format,
            }
        );
    }

    #[test]
    fn classify_does_not_panic_on_multibyte_input() {
        // バイト数が 10 や 13 になる非 ASCII
        for s in [
            "1234567あ",
            "123456789あ",
            "あいい",
            "12345678901あ",
            "😀😀",
        ] {
            assert!(matches!(
                classify(s),
                ScanCode::Invalid {
                    reason: ScanCodeInvalidReason::Format,
                    ..
                }
            ));
        }
    }
}
