//! 巻次（`volume`。「巻1」「第2巻」「上」など）から、並べ替え用の数値（`volume_sort`）を作る。
//!
//! 書籍を作る・更新するときに、巻次が変わって `volume_sort` が明示されていなければ、ここで作った値を
//! 同じ Op に入れる（`library::book`）。取得元は巻次を文字列でしか返さないので、下書きの `volume_sort` も
//! これで作る。数値が読み取れなければ `None`（推測しない）。

use crate::normalize::nfkc;

/// 巻次の文字列から並べ替え用の数値を作る。
///
/// - 先頭の数字の並び（NFKC で全角も半角にそろえる。`2.5` のような小数も 1 つの数）: `巻1` → 1、`第12巻` → 12、`Vol.3` → 3
/// - 漢数字: `巻一` → 1、`第十二巻` → 12、`二十` → 20、`二〇` → 20
/// - 上・中・下（上下巻）: 1・2・3（前編・後編は 1・2）
/// - 読み取れない（`別巻`、`完結編` など）: `None`
pub fn volume_sort(volume: &str) -> Option<f64> {
    let text = nfkc(volume);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(n) = digits(text) {
        return Some(n);
    }
    if let Some(n) = part_word(text) {
        return Some(n);
    }
    kanji_number(text)
}

/// 最初の数字の並び（小数点を 1 つ含んでよい）。
fn digits(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(u8::is_ascii_digit)?;
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    // `2.5` のように、数字に挟まれた 1 つの小数点まで含める
    if end + 1 < bytes.len() && bytes[end] == b'.' && bytes[end + 1].is_ascii_digit() {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    let n: f64 = text[start..end].parse().ok()?;
    n.is_finite().then_some(n)
}

/// 上・中・下、前編・後編（「上巻」「下」のように、前後に「第」「巻」「編」が付いてもよい）。
fn part_word(text: &str) -> Option<f64> {
    let core = text
        .trim_matches(|c: char| matches!(c, '第' | '巻' | '編' | '(' | ')' | '（' | '）' | ' '));
    match core {
        "上" | "前" => Some(1.0),
        "中" => Some(2.0),
        "下" => Some(3.0),
        "後" => Some(2.0),
        _ => None,
    }
}

fn kanji_digit(c: char) -> Option<u32> {
    Some(match c {
        '〇' | '零' => 0,
        '一' => 1,
        '二' => 2,
        '三' => 3,
        '四' => 4,
        '五' => 5,
        '六' => 6,
        '七' => 7,
        '八' => 8,
        '九' => 9,
        _ => return None,
    })
}

fn kanji_unit(c: char) -> Option<u32> {
    Some(match c {
        '十' => 10,
        '百' => 100,
        '千' => 1000,
        _ => return None,
    })
}

/// 最初の漢数字の並び。位取り（`二十三`）と桁の並べ（`二〇二三`）の両方を読む。
fn kanji_number(text: &str) -> Option<f64> {
    let run: String = text
        .chars()
        .skip_while(|c| kanji_digit(*c).is_none() && kanji_unit(*c).is_none())
        .take_while(|c| kanji_digit(*c).is_some() || kanji_unit(*c).is_some())
        .collect();
    if run.is_empty() {
        return None;
    }
    let has_unit = run.chars().any(|c| kanji_unit(c).is_some());
    let value: u32 = if has_unit {
        let mut total = 0u32;
        let mut pending: Option<u32> = None;
        for c in run.chars() {
            if let Some(d) = kanji_digit(c) {
                pending = Some(d);
            } else if let Some(u) = kanji_unit(c) {
                total = total.checked_add(pending.take().unwrap_or(1).checked_mul(u)?)?;
            }
        }
        total.checked_add(pending.unwrap_or(0))?
    } else {
        run.chars()
            .filter_map(kanji_digit)
            .try_fold(0u32, |acc, d| acc.checked_mul(10)?.checked_add(d))?
    };
    Some(f64::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_arabic_numbers_in_any_wrapper() {
        for (text, expected) in [
            ("1", 1.0),
            ("巻1", 1.0),
            ("第2巻", 2.0),
            ("12", 12.0),
            ("０３", 3.0),
            ("Vol.3", 3.0),
            ("vol. 10", 10.0),
            ("(5)", 5.0),
            ("#7", 7.0),
            ("2.5", 2.5),
            ("第2部", 2.0),
            ("3巻", 3.0),
            ("1-2", 1.0),
            ("巻 14 特装版", 14.0),
        ] {
            assert_eq!(volume_sort(text), Some(expected), "{text}");
        }
    }

    #[test]
    fn reads_kanji_numbers() {
        for (text, expected) in [
            ("巻一", 1.0),
            ("第三巻", 3.0),
            ("十", 10.0),
            ("十二", 12.0),
            ("二十", 20.0),
            ("二十三", 23.0),
            ("百", 100.0),
            ("百五", 105.0),
            ("二〇", 20.0),
            ("〇", 0.0),
        ] {
            assert_eq!(volume_sort(text), Some(expected), "{text}");
        }
    }

    #[test]
    fn reads_upper_middle_lower() {
        for (text, expected) in [
            ("上", 1.0),
            ("上巻", 1.0),
            ("中", 2.0),
            ("中巻", 2.0),
            ("下", 3.0),
            ("下巻", 3.0),
            ("（上）", 1.0),
        ] {
            assert_eq!(volume_sort(text), Some(expected), "{text}");
        }
        assert!(volume_sort("上") < volume_sort("中"));
        assert!(volume_sort("中") < volume_sort("下"));
    }

    #[test]
    fn unreadable_volumes_have_no_number() {
        for text in ["", "  ", "別巻", "完結編", "番外編", "特装版"] {
            assert_eq!(volume_sort(text), None, "{text:?}");
        }
    }

    #[test]
    fn absurdly_long_numbers_do_not_overflow() {
        assert!(volume_sort(&"九".repeat(40)).is_none());
        assert!(volume_sort(&"9".repeat(40)).is_some_and(f64::is_finite));
    }
}
