//! ISBN の最小限の変換。
//!
//! バーコードの判定（`classify_code`）はコアの役目で、ここは取得元の応答に含まれる ISBN の表記
//! （ハイフン付き・全角・ISBN-10）を ISBN-13 にそろえて照合するためだけに使う。

use unicode_normalization::UnicodeNormalization;

/// ISBN-13 のチェックデジットが合っているか（13 桁の数字であること）。
pub fn is_valid_isbn13(s: &str) -> bool {
    let digits: Vec<u32> = match s.chars().map(|c| c.to_digit(10)).collect() {
        Some(d) => d,
        None => return false,
    };
    if digits.len() != 13 {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { *d } else { d * 3 })
        .sum();
    sum.is_multiple_of(10)
}

/// ISBN-10（10 桁、末尾は `X` もあり）を ISBN-13 に直す。チェックデジットが合わなければ `None`。
pub fn isbn10_to_isbn13(isbn10: &str) -> Option<String> {
    let chars: Vec<char> = isbn10.chars().collect();
    if chars.len() != 10 {
        return None;
    }
    let mut sum = 0u32;
    for (i, c) in chars.iter().enumerate() {
        let v = if i == 9 && (*c == 'X' || *c == 'x') {
            10
        } else {
            c.to_digit(10)?
        };
        sum += v * (10 - i as u32);
    }
    if !sum.is_multiple_of(11) {
        return None;
    }
    let body = format!("978{}", chars[..9].iter().collect::<String>());
    let check = isbn13_check_digit(&body)?;
    Some(format!("{body}{check}"))
}

/// `978` で始まる ISBN-13 を ISBN-10 に直す。`979` や不正な入力は `None`。
pub fn isbn13_to_isbn10(isbn13: &str) -> Option<String> {
    if !is_valid_isbn13(isbn13) || !isbn13.starts_with("978") {
        return None;
    }
    let body = &isbn13[3..12];
    let sum: u32 = body
        .chars()
        .enumerate()
        .map(|(i, c)| c.to_digit(10).unwrap_or(0) * (10 - i as u32))
        .sum();
    let check = (11 - sum % 11) % 11;
    let check = if check == 10 {
        'X'
    } else {
        char::from_digit(check, 10)?
    };
    Some(format!("{body}{check}"))
}

fn isbn13_check_digit(first12: &str) -> Option<u32> {
    let mut sum = 0u32;
    for (i, c) in first12.chars().enumerate() {
        let d = c.to_digit(10)?;
        sum += if i % 2 == 0 { d } else { d * 3 };
    }
    Some((10 - sum % 10) % 10)
}

/// 表記ゆれ（全角、ハイフン、空白、ISBN-10）を吸収して ISBN-13 にそろえる。
/// ISBN として成り立たなければ `None`。
pub fn to_isbn13(input: &str) -> Option<String> {
    let cleaned: String = input
        .nfkc()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '\u{2010}' && *c != '\u{2212}')
        .collect();
    match cleaned.len() {
        13 if is_valid_isbn13(&cleaned) => Some(cleaned),
        10 => isbn10_to_isbn13(&cleaned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum() {
        assert!(is_valid_isbn13("9784003101117"));
        assert!(is_valid_isbn13("9784780802047"));
        assert!(!is_valid_isbn13("9784003101118"));
        assert!(!is_valid_isbn13("978400310111"));
        assert!(!is_valid_isbn13("97840031011ab"));
    }

    #[test]
    fn isbn10_roundtrip() {
        assert_eq!(
            isbn10_to_isbn13("4003101111").as_deref(),
            Some("9784003101117")
        );
        assert_eq!(
            isbn10_to_isbn13("4780802040").as_deref(),
            Some("9784780802047")
        );
        assert_eq!(
            isbn10_to_isbn13("408873551X").as_deref(),
            Some("9784088735511")
        );
        assert_eq!(
            isbn13_to_isbn10("9784003101117").as_deref(),
            Some("4003101111")
        );
        assert_eq!(
            isbn13_to_isbn10("9784088735511").as_deref(),
            Some("408873551X")
        );
        assert_eq!(isbn13_to_isbn10("9791234567896"), None);
        assert_eq!(isbn10_to_isbn13("4003101112"), None);
    }

    #[test]
    fn notation_variants() {
        assert_eq!(
            to_isbn13("978-4-7808-0204-7").as_deref(),
            Some("9784780802047")
        );
        assert_eq!(
            to_isbn13("４-００-３１０１１１-１").as_deref(),
            Some("9784003101117")
        );
        assert_eq!(
            to_isbn13("９７８４００３１０１１１７").as_deref(),
            Some("9784003101117")
        );
        assert_eq!(to_isbn13("4-938853-04-3").as_deref(), Some("9784938853044"));
        assert_eq!(to_isbn13("4-000000-00-0"), None, "チェックデジット不一致");
        assert_eq!(to_isbn13("abc"), None);
        assert_eq!(to_isbn13(""), None);
    }
}
