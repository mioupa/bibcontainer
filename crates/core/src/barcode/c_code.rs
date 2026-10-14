//! Cコード（日本図書コードの分類記号）の名称表（要件書の付録 A、FR-SCAN-03）。
//!
//! Cコードは 4 桁で、1 桁目が販売対象、2 桁目が発行形態、3〜4 桁目が内容
//! （3 桁目が大分類、3〜4 桁目で中分類）。
//!
//! 名称は、日本図書コード管理センター「分類記号・各コードの内容解説・注意事項」（2025.04.01 版）の
//! 表に合わせている（括弧は全角にそろえ、表の中で改行されている語の区切りは読点か中黒にした）。
//! 要件書の付録 A とは表記が違うものがある（`女性`、`辞典・事典`、`電磁的記録媒体など`、
//! `伝記・系譜`、`海事・兵器`、`各種工業`、`生活` など）。要件書の修正はユーザーへの提案とする。

use serde::Serialize;
use ts_rs::TS;

/// 1 桁目: 販売対象。
const AUDIENCE: [&str; 10] = [
    "一般",
    "教養",
    "実用",
    "専門",
    "検定教科書・消費税非課税品",
    "女性",
    "学参I（小中）",
    "学参II（高校）",
    "児童",
    "雑誌扱いコミックスおよびムック",
];

/// 2 桁目: 発行形態。
const FORMAT: [&str; 10] = [
    "単行本",
    "文庫",
    "新書",
    "全集・双書",
    "ムック・その他",
    "辞典・事典",
    "図鑑",
    "絵本",
    "電磁的記録媒体など",
    "コミックス",
];

/// 3 桁目: 内容の大分類。
const CONTENT_MAJOR: [&str; 10] = [
    "総記",
    "哲学・宗教・心理",
    "歴史・地理",
    "社会科学",
    "自然科学",
    "工学・工業",
    "産業",
    "芸術・生活",
    "語学",
    "文学",
];

/// 3〜4 桁目: 内容の中分類。表にある組み合わせだけを持つ（コードの昇順）。
const CONTENT: [(&str, &str); 68] = [
    ("00", "総記"),
    ("01", "百科事典"),
    ("02", "年鑑・雑誌"),
    ("04", "情報科学"),
    ("10", "哲学"),
    ("11", "心理（学）"),
    ("12", "倫理（学）"),
    ("14", "宗教"),
    ("15", "仏教"),
    ("16", "キリスト教"),
    ("20", "歴史総記"),
    ("21", "日本歴史"),
    ("22", "外国歴史"),
    ("23", "伝記・系譜"),
    ("25", "地理"),
    ("26", "旅行"),
    ("30", "社会科学総記"),
    ("31", "政治（国防・軍事含む）"),
    ("32", "法律"),
    ("33", "経済・財政・統計"),
    ("34", "経営"),
    ("36", "社会"),
    ("37", "教育"),
    ("39", "民族・風習"),
    ("40", "自然科学総記"),
    ("41", "数学"),
    ("42", "物理学"),
    ("43", "化学"),
    ("44", "天文・地学"),
    ("45", "生物学"),
    ("47", "医学・歯学・薬学"),
    ("50", "工学・工業総記"),
    ("51", "土木"),
    ("52", "建築"),
    ("53", "機械"),
    ("54", "電気"),
    ("55", "電子通信"),
    ("56", "海事・兵器"),
    ("57", "採鉱・冶金"),
    ("58", "各種工業"),
    ("60", "産業総記"),
    ("61", "農林業"),
    ("62", "水産業"),
    ("63", "商業"),
    ("65", "交通・通信"),
    ("70", "芸術総記"),
    ("71", "絵画・彫刻"),
    ("72", "写真・工芸"),
    ("73", "音楽・舞踊"),
    ("74", "演劇・映画"),
    ("75", "体育・スポーツ"),
    ("76", "諸芸・娯楽"),
    ("77", "家事"),
    ("78", "生活"),
    ("79", "コミックス・劇画"),
    ("80", "語学総記"),
    ("81", "日本語"),
    ("82", "英米語"),
    ("84", "ドイツ語"),
    ("85", "フランス語"),
    ("87", "各国語"),
    ("90", "文学総記"),
    ("91", "日本文学総記"),
    ("92", "日本文学、詩歌"),
    ("93", "日本文学、小説・物語"),
    ("95", "日本文学、評論・随筆・その他"),
    ("97", "外国文学、小説"),
    ("98", "外国文学、その他"),
];

/// 表の 1 行（コードと名称）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CCodeEntry {
    /// コード。販売対象・発行形態・大分類は 1 桁、中分類は 2 桁の数字。
    pub code: String,
    pub name: String,
}

/// Cコード表の全体。フロントが一度だけ取得して、各桁の名称を引くのに使う。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CCodeTable {
    /// 1 桁目: 販売対象（0〜9）。
    pub audience: Vec<CCodeEntry>,
    /// 2 桁目: 発行形態（0〜9）。
    pub format: Vec<CCodeEntry>,
    /// 3 桁目: 内容の大分類（0〜9）。
    pub content_major: Vec<CCodeEntry>,
    /// 3〜4 桁目: 内容の中分類（表にある 2 桁のコードだけ）。
    pub content: Vec<CCodeEntry>,
}

/// Cコード 1 件の各桁の名称。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CCodeInfo {
    /// 4 桁のCコード。
    pub code: String,
    /// 1 桁目の名称（販売対象）。
    pub audience: String,
    /// 2 桁目の名称（発行形態）。
    pub format: String,
    /// 3 桁目の名称（内容の大分類）。表にない中分類でも返す。
    pub content_major: String,
    /// 3〜4 桁目の名称（内容の中分類）。表にないときは「未定義（xx）」（xx は 3〜4 桁目）。
    pub content: String,
    /// 3〜4 桁目が表にあるか。`false` のときは「未定義（xx）」なので、内容は大分類（`contentMajor`）だけを使う。
    pub content_defined: bool,
}

/// 1 桁の数字のコードから、10 要素の名称表を引く。
fn digit(b: u8) -> usize {
    usize::from(b - b'0')
}

fn entries_by_digit(names: &[&str; 10]) -> Vec<CCodeEntry> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| CCodeEntry {
            code: i.to_string(),
            name: (*name).to_owned(),
        })
        .collect()
}

/// Cコード表の全体を返す。
pub fn c_code_table() -> CCodeTable {
    CCodeTable {
        audience: entries_by_digit(&AUDIENCE),
        format: entries_by_digit(&FORMAT),
        content_major: entries_by_digit(&CONTENT_MAJOR),
        content: CONTENT
            .iter()
            .map(|(code, name)| CCodeEntry {
                code: (*code).to_owned(),
                name: (*name).to_owned(),
            })
            .collect(),
    }
}

/// 4 桁のCコードの各桁の名称を返す（FR-SCAN-03）。
///
/// 3〜4 桁目が表にないときは、`content` を「未定義（xx）」にし、`content_major`（3 桁目の名称）は返す。
/// `code` が 4 桁の ASCII 数字でなければ `None`（桁の名称を決められないため）。
pub fn describe_c_code(code: &str) -> Option<CCodeInfo> {
    let b = code.as_bytes();
    if b.len() != 4 || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let content_code = &code[2..4];
    let defined = CONTENT.iter().find(|(c, _)| *c == content_code);
    Some(CCodeInfo {
        code: code.to_owned(),
        audience: AUDIENCE[digit(b[0])].to_owned(),
        format: FORMAT[digit(b[1])].to_owned(),
        content_major: CONTENT_MAJOR[digit(b[2])].to_owned(),
        content: match defined {
            Some((_, name)) => (*name).to_owned(),
            None => format!("未定義（{content_code}）"),
        },
        content_defined: defined.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_table_is_sorted_unique_and_well_formed() {
        assert_eq!(CONTENT.len(), 68);
        for pair in CONTENT.windows(2) {
            assert!(pair[0].0 < pair[1].0, "昇順で重複なし: {pair:?}");
        }
        for (code, name) in CONTENT {
            assert_eq!(code.len(), 2);
            assert!(code.bytes().all(|b| b.is_ascii_digit()), "{code}");
            assert!(!name.is_empty(), "{code}");
        }
    }

    #[test]
    fn names_are_non_empty() {
        for name in AUDIENCE.iter().chain(&FORMAT).chain(&CONTENT_MAJOR) {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn every_four_digit_code_can_be_described() {
        for n in 0..10_000u32 {
            let code = format!("{n:04}");
            let info = describe_c_code(&code).unwrap_or_else(|| panic!("{code}"));
            assert_eq!(info.code, code);
            assert!(!info.audience.is_empty());
            assert!(!info.format.is_empty());
            assert!(!info.content_major.is_empty());
            assert_eq!(
                info.content_defined,
                CONTENT.iter().any(|(c, _)| *c == &code[2..4]),
                "{code}"
            );
            if info.content_defined {
                assert!(!info.content.starts_with("未定義"), "{code}");
            } else {
                assert_eq!(info.content, format!("未定義（{}）", &code[2..4]));
            }
        }
    }

    #[test]
    fn describe_rejects_non_four_digit_input() {
        for s in [
            "",
            "019",
            "01930",
            "01a3",
            "０１９３",
            "0あ",
            "01あ",
            " 0193",
            "C0193",
        ] {
            assert_eq!(describe_c_code(s), None, "{s:?}");
        }
    }
}
