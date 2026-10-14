//! チェックデジットの計算と、ISBN-10 / ISBN-13 の相互変換（FR-SCAN-02、FR-SCAN-04）。
//!
//! 公開関数は、ハイフン・空白・全角数字を含む入力を [`normalize`] で整えてから処理する。
//! 返す文字列は常にハイフンなしである（表示は既定でハイフンなし）。
//! ハイフンを入れる位置は ISBN の範囲表（RangeMessage）が必要なため、ここでは扱わない。

use super::normalize;

/// 空でなく、すべて ASCII 数字であるか。
pub(super) fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// EAN（GTIN）系のチェックデジット。本体（チェックデジットを除いた数字列）の右端から
/// 重み 3, 1, 3, 1… を掛けた合計を 10 の倍数に切り上げるための数を返す。
/// EAN-13 は左から重み 1, 3, 1, 3…（12 桁）、EAN-8 は左から 3, 1, 3, 1…（7 桁）で、
/// どちらも「右端が 3」で同じ式になる。
fn gs1_check_digit(body: &str) -> Option<u8> {
    if !is_digits(body) {
        return None;
    }
    let sum: u32 = body
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    // 0..=9 なので u8 に収まる
    Some(((10 - sum % 10) % 10) as u8)
}

/// EAN-13 のチェックデジット（0〜9）。`first12` は左 12 桁の数字。形が違えば `None`。
pub fn ean13_check_digit(first12: &str) -> Option<u8> {
    if first12.len() != 12 {
        return None;
    }
    gs1_check_digit(first12)
}

/// EAN-8 のチェックデジット（0〜9）。`first7` は左 7 桁の数字。形が違えば `None`。
pub fn ean8_check_digit(first7: &str) -> Option<u8> {
    if first7.len() != 7 {
        return None;
    }
    gs1_check_digit(first7)
}

/// ISBN-10 のチェックデジット（`'0'`〜`'9'` または `'X'`）。`first9` は左 9 桁の数字。
/// 左から重み 10〜2 を掛けた合計 `s` に対し `(11 − s mod 11) mod 11` で、10 のときは `X`。
pub fn isbn10_check_digit(first9: &str) -> Option<char> {
    if first9.len() != 9 || !is_digits(first9) {
        return None;
    }
    let sum: u32 = first9
        .bytes()
        .zip((2..=10).rev())
        .map(|(b, w)| u32::from(b - b'0') * w)
        .sum();
    match (11 - sum % 11) % 11 {
        10 => Some('X'),
        d => char::from_digit(d, 10),
    }
}

/// 整形済みの 13 桁が EAN-13 として正しいか。
pub(super) fn valid_ean13(s: &str) -> bool {
    s.len() == 13
        && is_digits(s)
        && ean13_check_digit(&s[..12]).is_some_and(|d| s.as_bytes()[12] == b'0' + d)
}

/// 整形済みの 8 桁が EAN-8 として正しいか。
pub(super) fn valid_ean8(s: &str) -> bool {
    s.len() == 8
        && is_digits(s)
        && ean8_check_digit(&s[..7]).is_some_and(|d| s.as_bytes()[7] == b'0' + d)
}

/// 整形済みの 10 桁（末尾 X 可）が ISBN-10 として正しいか。
pub(super) fn valid_isbn10(s: &str) -> bool {
    // 先に ASCII を確かめる（バイト位置で切るので、非 ASCII が混じると境界で panic するため）
    s.len() == 10
        && s.is_ascii()
        && is_digits(&s[..9])
        && isbn10_check_digit(&s[..9]).is_some_and(|d| s.as_bytes()[9] == d as u8)
}

/// 整形済みの 13 桁が ISBN-13（`978` / `979` で始まる正しい EAN-13）か。
pub(super) fn valid_isbn13(s: &str) -> bool {
    valid_ean13(s) && (s.starts_with("978") || s.starts_with("979"))
}

/// EAN-13 として正しいか（チェックデジットを検証する）。
pub fn is_valid_ean13(s: &str) -> bool {
    valid_ean13(&normalize(s))
}

/// EAN-8 として正しいか（チェックデジットを検証する）。
pub fn is_valid_ean8(s: &str) -> bool {
    valid_ean8(&normalize(s))
}

/// ISBN-10 として正しいか（末尾の `X` を含め、チェックデジットを検証する）。
pub fn is_valid_isbn10(s: &str) -> bool {
    valid_isbn10(&normalize(s))
}

/// ISBN-13 として正しいか（`978` / `979` で始まり、チェックデジットが合う）。
pub fn is_valid_isbn13(s: &str) -> bool {
    valid_isbn13(&normalize(s))
}

/// 整形済みの ISBN-10 を ISBN-13 に変換する。ISBN-10 として正しくなければ `None`。
pub(super) fn to_isbn13(isbn10: &str) -> Option<String> {
    if !valid_isbn10(isbn10) {
        return None;
    }
    let mut out = String::with_capacity(13);
    out.push_str("978");
    out.push_str(&isbn10[..9]);
    out.push(char::from(b'0' + ean13_check_digit(&out)?));
    Some(out)
}

/// 整形済みの ISBN-13 を ISBN-10 に変換する。`979` で始まるものや、正しくない ISBN-13 は `None`。
pub(super) fn to_isbn10(isbn13: &str) -> Option<String> {
    if !valid_isbn13(isbn13) || !isbn13.starts_with("978") {
        return None;
    }
    let mut out = String::with_capacity(10);
    out.push_str(&isbn13[3..12]);
    out.push(isbn10_check_digit(&out)?);
    Some(out)
}

/// ISBN-10 を ISBN-13（`978` 始まり、ハイフンなし）に変換する。
/// ISBN-10 として正しくない（桁数・文字・チェックデジット）場合は `None`。
pub fn isbn10_to_13(isbn10: &str) -> Option<String> {
    to_isbn13(&normalize(isbn10))
}

/// ISBN-13 を ISBN-10（ハイフンなし）に変換する。
/// `979` で始まるもの（ISBN-10 に対応がない）と、ISBN-13 として正しくないものは `None`。
pub fn isbn13_to_10(isbn13: &str) -> Option<String> {
    to_isbn10(&normalize(isbn13))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ean13_check_digit_matches_known_values() {
        // 要件書 付録 B
        assert_eq!(ean13_check_digit("978400310111"), Some(7));
        assert_eq!(ean13_check_digit("978478080204"), Some(7));
        assert_eq!(ean13_check_digit("978410100101"), Some(2));
        assert_eq!(ean13_check_digit("192019300700"), Some(8));
        assert_eq!(ean13_check_digit("192019300000"), Some(9));
        // 合計がちょうど 10 の倍数なら 0
        assert_eq!(ean13_check_digit("000000000000"), Some(0));
    }

    #[test]
    fn ean13_check_digit_rejects_bad_shapes() {
        assert_eq!(ean13_check_digit(""), None);
        assert_eq!(ean13_check_digit("97840031011"), None);
        assert_eq!(ean13_check_digit("9784003101170"), None);
        assert_eq!(ean13_check_digit("97840031011a"), None);
        // バイト数は 12 だが、非 ASCII を含む
        assert_eq!(ean13_check_digit("12345678あ"), None);
    }

    #[test]
    fn ean8_check_digit_matches_known_values() {
        assert_eq!(ean8_check_digit("9638507"), Some(4));
        assert_eq!(ean8_check_digit("4901234"), Some(7));
        assert_eq!(ean8_check_digit("963850"), None);
        assert_eq!(ean8_check_digit("96385a7"), None);
    }

    #[test]
    fn isbn10_check_digit_uses_x_for_ten() {
        assert_eq!(isbn10_check_digit("400310111"), Some('1'));
        assert_eq!(isbn10_check_digit("478080204"), Some('0'));
        assert_eq!(isbn10_check_digit("410100101"), Some('4'));
        assert_eq!(isbn10_check_digit("080442957"), Some('X'));
        assert_eq!(isbn10_check_digit("123456789"), Some('X'));
        assert_eq!(isbn10_check_digit("12345678"), None);
        assert_eq!(isbn10_check_digit("12345678X"), None);
    }

    #[test]
    fn strict_validators() {
        assert!(valid_ean13("9784003101117"));
        assert!(!valid_ean13("9784003101118"));
        assert!(!valid_ean13("978400310111"));
        assert!(valid_ean8("96385074"));
        assert!(!valid_ean8("96385075"));
        assert!(valid_isbn10("4003101111"));
        assert!(valid_isbn10("080442957X"));
        // 小文字の x は整える前の形なので、ここでは受け付けない
        assert!(!valid_isbn10("080442957x"));
        assert!(!valid_isbn10("08044295X7"));
        // 中途半端な位置で切れる非 ASCII（10 バイト）でも panic しない
        assert!(!valid_isbn10("1234567あ"));
        // ISBN-13 は 978 / 979 だけ
        assert!(valid_isbn13("9784003101117"));
        assert!(valid_isbn13("9791090636071"));
        assert!(!valid_isbn13("1920193007008"));
    }

    #[test]
    fn conversions_are_hyphen_free() {
        assert_eq!(
            isbn10_to_13("4-00-310111-1").as_deref(),
            Some("9784003101117")
        );
        assert_eq!(
            isbn13_to_10("978-4-00-310111-7").as_deref(),
            Some("4003101111")
        );
        assert_eq!(isbn10_to_13("080442957X").as_deref(), Some("9780804429573"));
        assert_eq!(isbn13_to_10("9780804429573").as_deref(), Some("080442957X"));
    }

    #[test]
    fn conversions_reject_invalid_input() {
        assert_eq!(isbn10_to_13("4003101112"), None);
        assert_eq!(isbn10_to_13("9784003101117"), None);
        assert_eq!(isbn10_to_13(""), None);
        assert_eq!(isbn13_to_10("9784003101118"), None);
        assert_eq!(isbn13_to_10("4003101111"), None);
        // 979 は ISBN-10 にならない
        assert_eq!(isbn13_to_10("9791090636071"), None);
        // 978 でも 979 でもない EAN-13
        assert_eq!(isbn13_to_10("1920193007008"), None);
    }
}
