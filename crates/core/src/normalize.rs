//! 検索・並べ替え用の正規化と、ISBN の小さな計算。
//!
//! バーコード判定（`barcode` モジュール）には依存しない。ここの ISBN の関数は、検索語の解釈と
//! 保存前の検証に使う最小限のもの。

use unicode_normalization::UnicodeNormalization;

/// 検索用に文字列を正規化する。書名・著者などを索引に入れるときと、検索語に使う。
///
/// 1. NFKC（全角英数→半角、半角カナ→全角カナ、合成文字の統一など）
/// 2. 英字を小文字にする
/// 3. カタカナをひらがなにする（`ァ`〜`ヶ`、`ヽ`・`ヾ`。長音符 `ー` はそのまま）
///
/// 前後の空白は取らない（呼び出し側が必要なら `trim` する）。
pub fn normalize_for_search(s: &str) -> String {
    s.nfkc()
        .flat_map(char::to_lowercase)
        .map(katakana_to_hiragana)
        .collect()
}

/// NFKC だけをかける（検索以外の入力の正規化用。全角数字を半角にするなど）。
pub fn nfkc(s: &str) -> String {
    s.nfkc().collect()
}

fn katakana_to_hiragana(c: char) -> char {
    match c {
        // ァ(30A1)〜ヶ(30F6) と ヽ(30FD)・ヾ(30FE) は、0x60 引くと対応するひらがな
        '\u{30A1}'..='\u{30F6}' | '\u{30FD}' | '\u{30FE}' => {
            char::from_u32(u32::from(c) - 0x60).unwrap_or(c)
        }
        _ => c,
    }
}

/// ISBN-13 として正しいか（13 桁の半角数字で、チェックデジットが合う）。
pub fn is_valid_isbn13(s: &str) -> bool {
    s.len() == 13
        && s.bytes().all(|b| b.is_ascii_digit())
        && isbn13_check_digit(&s[..12]) == Some(s.as_bytes()[12] - b'0')
}

/// 先頭 12 桁から ISBN-13（EAN-13）のチェックデジットを計算する。12 桁の数字でなければ `None`。
pub fn isbn13_check_digit(first12: &str) -> Option<u8> {
    if first12.len() != 12 || !first12.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = first12
        .bytes()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    u8::try_from((10 - sum % 10) % 10).ok()
}

/// ISBN-10 として正しいか（9 桁の数字＋数字か `X`/`x` のチェックデジットが合う）。
pub fn is_valid_isbn10(s: &str) -> bool {
    s.is_ascii()
        && s.len() == 10
        && isbn10_check_char(&s[..9])
            .is_some_and(|c| s.as_bytes()[9].to_ascii_uppercase() == c as u8)
}

/// 先頭 9 桁から ISBN-10 のチェック文字（`0`〜`9` か `X`）を計算する。9 桁の数字でなければ `None`。
fn isbn10_check_char(first9: &str) -> Option<char> {
    if first9.len() != 9 || !first9.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = first9
        .bytes()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * (10 - u32::try_from(i).unwrap_or(0)))
        .sum();
    match (11 - sum % 11) % 11 {
        10 => Some('X'),
        n => char::from_digit(n, 10),
    }
}

/// 正しい ISBN-10 を ISBN-13（`978` 始まり）に直す。正しくなければ `None`。
pub fn isbn10_to_isbn13(isbn10: &str) -> Option<String> {
    if !is_valid_isbn10(isbn10) {
        return None;
    }
    let first12 = format!("978{}", &isbn10[..9]);
    let check = isbn13_check_digit(&first12)?;
    Some(format!("{first12}{check}"))
}

/// `978` 始まりの正しい ISBN-13 を ISBN-10 に直す。直せなければ `None`。
pub fn isbn13_to_isbn10(isbn13: &str) -> Option<String> {
    if !is_valid_isbn13(isbn13) || !isbn13.starts_with("978") {
        return None;
    }
    let first9 = &isbn13[3..12];
    let check = isbn10_check_char(first9)?;
    Some(format!("{first9}{check}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_follows_the_spec() {
        // 全角英数・半角カナ・カタカナ・大文字をそろえる
        assert_eq!(normalize_for_search("ＡＢＣ１２３"), "abc123");
        assert_eq!(normalize_for_search("ﾊﾟｿｺﾝ"), "ぱそこん");
        assert_eq!(normalize_for_search("ノルウェイの森"), "のるうぇいの森");
        assert_eq!(normalize_for_search("Rust Book"), "rust book");
        // 長音符、漢字、ひらがなはそのまま
        assert_eq!(normalize_for_search("コンピューター"), "こんぴゅーたー");
        assert_eq!(normalize_for_search("吾輩は猫である"), "吾輩は猫である");
        // ヴ・ヶ・繰り返し記号
        assert_eq!(normalize_for_search("ヴァイオリン"), "ゔぁいおりん");
        assert_eq!(normalize_for_search("三ヶ月"), "三ゖ月");
        assert_eq!(normalize_for_search("ヽヾ"), "ゝゞ");
        // 全角スペースは半角スペースになる
        assert_eq!(normalize_for_search("a\u{3000}b"), "a b");
        // 冪等
        let once = normalize_for_search("ﾊﾟｿｺﾝ ＡＢＣ ガ");
        assert_eq!(normalize_for_search(&once), once);
    }

    #[test]
    fn isbn_helpers() {
        assert!(is_valid_isbn13("9784003101117"));
        assert!(!is_valid_isbn13("9784003101118"));
        assert!(!is_valid_isbn13("978400310111"));
        assert!(!is_valid_isbn13("97840031011１7"));
        assert_eq!(isbn13_check_digit("978400310111"), Some(7));
        assert_eq!(isbn13_check_digit("97840031011"), None);

        assert!(is_valid_isbn10("4003101111"));
        assert!(!is_valid_isbn10("4003101112"));
        assert!(is_valid_isbn10("080442957X"));
        assert!(is_valid_isbn10("080442957x"));
        assert!(!is_valid_isbn10("400310111"));

        assert_eq!(
            isbn10_to_isbn13("4003101111").as_deref(),
            Some("9784003101117")
        );
        assert_eq!(isbn10_to_isbn13("4003101112"), None);
        assert_eq!(
            isbn13_to_isbn10("9784003101117").as_deref(),
            Some("4003101111")
        );
        assert_eq!(
            isbn13_to_isbn10("9791234567896"),
            None,
            "979 始まりは ISBN-10 にならない"
        );
        assert_eq!(
            isbn10_to_isbn13("080442957X").as_deref(),
            Some("9780804429573")
        );
    }
}
