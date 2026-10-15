//! 取得した値の正規化（FR-META-06）。
//!
//! 取得元ごとの表記ゆれ（全角・ハイフン・和暦・「／著」など）をここで吸収する。
//! どの関数も「読み取れなければ `None`（または空）」を返し、推測で値を作らない。

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::types::CandidateAuthor;

// ---------------------------------------------------------------------------
// テキスト
// ---------------------------------------------------------------------------

/// NFKC にそろえる（全角英数・記号 → 半角、半角カナ → 全角カナ）。
pub fn nfkc(s: &str) -> String {
    s.nfkc().collect()
}

/// 前後の空白を除き、NFKC にそろえ、連続する空白（改行を含む）を 1 つの半角空白にする。空なら `None`。
pub fn clean_text(s: &str) -> Option<String> {
    let t = nfkc(s);
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// 内容紹介などの長い文章。NFKC は掛けず（句読点や記号が変わるため）、改行を `\n` にそろえて前後の空白を除く。
pub fn clean_description(s: &str) -> Option<String> {
    let t = s.replace("\r\n", "\n").replace('\r', "\n");
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// HTML のコメントとタグ（`<…>`）。
static HTML_MARKUP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->|<[^>]*>").expect("正規表現"));
/// タグの名前（`<`、`</` のあと）。
static HTML_TAG_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<\s*/?\s*([A-Za-z][A-Za-z0-9]*)").expect("正規表現"));
/// 文字参照（`&amp;`、`&#39;`、`&#x3042;`）。
static HTML_ENTITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,15});")
        .expect("正規表現")
});
static BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").expect("正規表現"));

/// タグが作る改行。`Line` は行を改める（連続しても 1 回）、`Paragraph` は空行を 1 つ入れる（連続しても 1 つ）、
/// `Br` は `<br>` 1 つにつき 1 回の改行（`<br><br>` は空行）。
#[derive(Clone, Copy)]
enum HtmlBreak {
    Br,
    Line,
    Paragraph,
}

fn html_break(tag: &str) -> Option<HtmlBreak> {
    let name = HTML_TAG_NAME.captures(tag)?[1].to_ascii_lowercase();
    match name.as_str() {
        "br" => Some(HtmlBreak::Br),
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" => {
            Some(HtmlBreak::Paragraph)
        }
        "li" | "ul" | "ol" | "dl" | "dt" | "dd" | "tr" | "hr" => Some(HtmlBreak::Line),
        _ => None,
    }
}

/// タグを除いた本文と、タグが作る改行を組み立てる。改行はすぐに入れず、次の文字が来たときに入れる
/// （本文の前後と、改行どうしのあいだに、余計な空行ができないように）。
struct HtmlText {
    out: String,
    /// 次の文字の前に入れる改行の数
    pending: usize,
}

impl HtmlText {
    fn text(&mut self, t: &str) {
        // 改行の直後（と本文の先頭）の空白は、HTML では意味がないので捨てる。ふつうの文字のあいだの空白は残す
        let t = if self.pending > 0 || self.out.is_empty() {
            t.trim_start_matches(|c: char| c.is_ascii_whitespace())
        } else {
            t
        };
        if t.is_empty() {
            return;
        }
        if !self.out.is_empty() {
            self.out.push_str(&"\n".repeat(self.pending.min(2)));
        }
        self.pending = 0;
        self.out.push_str(t);
    }

    fn brk(&mut self, kind: HtmlBreak) {
        // 改行の前の空白も意味がない
        let kept = self
            .out
            .trim_end_matches(|c: char| c.is_ascii_whitespace())
            .len();
        self.out.truncate(kept);
        self.pending = match kind {
            HtmlBreak::Br => self.pending + 1,
            HtmlBreak::Line => self.pending.max(1),
            HtmlBreak::Paragraph => self.pending.max(2),
        };
    }
}

/// 文字参照を 1 回だけ戻す（`&amp;lt;` は `&lt;` になり、`<` にはならない）。知らない名前はそのまま残す。
fn decode_entities(s: &str) -> String {
    HTML_ENTITY
        .replace_all(s, |caps: &regex::Captures| {
            let body = &caps[1];
            let decoded = if let Some(number) = body.strip_prefix('#') {
                match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None => number.parse::<u32>(),
                }
                .ok()
                .and_then(char::from_u32)
            } else {
                match body {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    "ndash" => Some('\u{2013}'),
                    "mdash" => Some('\u{2014}'),
                    "hellip" => Some('\u{2026}'),
                    "lsquo" => Some('\u{2018}'),
                    "rsquo" => Some('\u{2019}'),
                    "ldquo" => Some('\u{201C}'),
                    "rdquo" => Some('\u{201D}'),
                    "middot" => Some('\u{00B7}'),
                    "copy" => Some('\u{00A9}'),
                    "reg" => Some('\u{00AE}'),
                    _ => None,
                }
            };
            match decoded {
                // 0 と制御文字（改行と tab は除く）は入れない。`&nbsp;`（U+00A0）は半角の空白にする
                Some('\u{00A0}') => " ".to_string(),
                Some(c) if c == '\0' || (c.is_control() && !matches!(c, '\n' | '\t')) => {
                    String::new()
                }
                Some(c) => c.to_string(),
                None => caps[0].to_string(),
            }
        })
        .into_owned()
}

/// 内容紹介に含まれる HTML を取り除く（Google Books・Open Library）。
///
/// - 段落・見出し・`<div>` の前後は空行 1 つ、`<br>` は 1 つにつき改行 1 回（`<br><br>` は空行）、
///   `<ul>`・`<ol>`・`<li>` の前後は行を改める（項目ごとに 1 行）。改行が重なっても、空行は 1 つまで。
/// - そのほかのタグ（`<i>`・`<b>`・`<em>`・`<strong>`・`<span …>`・`<a …>` など）と、コメントは除いて、中の文字は残す。
/// - 文字参照（`&amp;`・`&quot;`・`&nbsp;`・`&#39;`・`&#x3042;` など）は戻す。タグを除いたあとに戻すので、
///   `&lt;b&gt;` は文字として `<b>` になる（タグとしては扱わない）。
/// - 行末の空白を除き、3 つ以上続く改行は 2 つにし、前後の空行と空白を除く。
pub fn strip_html(s: &str) -> String {
    let mut text = HtmlText {
        out: String::with_capacity(s.len()),
        pending: 0,
    };
    let mut last = 0;
    for m in HTML_MARKUP.find_iter(s) {
        text.text(&s[last..m.start()]);
        last = m.end();
        if let Some(kind) = html_break(m.as_str()) {
            text.brk(kind);
        }
    }
    text.text(&s[last..]);

    let t = decode_entities(&text.out);
    let t: String = t.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    BLANK_LINES.replace_all(&t, "\n\n").trim().to_string()
}

/// 書名を「書名」と「副書名」に分ける。ISBD の区切り（半角空白・コロン・半角空白）の最初の 1 か所で切る。
///
/// NDL の書名は `おにぎりレシピ101 : EVERYDAY ONIGIRI` の形、openBD の NDL 由来のデータも同じ。
/// 正規化済み（NFKC 後）の文字列を渡すこと。
pub fn split_title(title: &str) -> (String, Option<String>) {
    match title.split_once(" : ") {
        Some((main, sub)) if !main.trim().is_empty() && !sub.trim().is_empty() => {
            (main.trim().to_string(), Some(sub.trim().to_string()))
        }
        _ => (title.trim().to_string(), None),
    }
}

/// `http://` を `https://` にする（書影 URL 用）。
pub fn upgrade_to_https(url: &str) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    }
}

/// Cコード（4 桁の数字）。
pub fn normalize_ccode(s: &str) -> Option<String> {
    let t = nfkc(s);
    let t = t.trim();
    (t.len() == 4 && t.chars().all(|c| c.is_ascii_digit())).then(|| t.to_string())
}

// ---------------------------------------------------------------------------
// 日付
// ---------------------------------------------------------------------------

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn take_digits(s: &str) -> (&str, &str) {
    let end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    s.split_at(end)
}

/// 年のあとの「月」「日」を読む。読めた分だけ返す。
fn parse_month_day(rest: &str) -> (Option<u32>, Option<u32>) {
    let mut rest = rest.trim_start();
    let Some(first) = rest.chars().next() else {
        return (None, None);
    };
    if !matches!(first, '-' | '.' | '/' | '年') {
        return (None, None);
    }
    rest = &rest[first.len_utf8()..];
    let (m_digits, after_m) = take_digits(rest);
    if m_digits.is_empty() || m_digits.len() > 2 {
        return (None, None);
    }
    let month: u32 = m_digits.parse().unwrap_or(0);
    let after = after_m.trim_start();
    // 「7月8日」は月のあとに日が続く。「2010-7-8」「2010.7.8」は区切りのあとに日が続く。
    let day_part = if let Some(r) = after.strip_prefix('月') {
        Some(r.trim_start())
    } else {
        after
            .chars()
            .next()
            .filter(|c| matches!(c, '-' | '.' | '/'))
            .map(|c| &after[c.len_utf8()..])
    };
    let mut day = None;
    if let Some(d) = day_part {
        let (d_digits, _) = take_digits(d);
        if !d_digits.is_empty() && d_digits.len() <= 2 {
            day = d_digits.parse().ok();
        }
    }
    (Some(month), day)
}

fn format_date(year: i32, month: Option<u32>, day: Option<u32>) -> Option<String> {
    if !(1000..=2999).contains(&year) {
        return None;
    }
    let Some(m) = month else {
        return Some(format!("{year:04}"));
    };
    if !(1..=12).contains(&m) {
        return None;
    }
    match day {
        Some(d) if (1..=days_in_month(year, m)).contains(&d) => {
            Some(format!("{year:04}-{m:02}-{d:02}"))
        }
        _ => Some(format!("{year:04}-{m:02}")),
    }
}

/// 英語の月名（フルスペルと 3 文字の略称。`Sept` も。末尾のピリオドは付いていてもよい）。
fn english_month(token: &str) -> Option<u32> {
    let t = token.trim_end_matches('.').to_ascii_lowercase();
    Some(match t.as_str() {
        "january" | "jan" => 1,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sept" | "sep" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    })
}

/// 日（1〜2 桁。`5`、`05`、`5th` など。末尾のピリオドは付いていてもよい）。
fn english_day(token: &str) -> Option<u32> {
    let t = token.trim_end_matches('.');
    let (digits, suffix) = take_digits(t);
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    matches!(
        suffix.to_ascii_lowercase().as_str(),
        "" | "st" | "nd" | "rd" | "th"
    )
    .then(|| digits.parse().ok())
    .flatten()
}

/// 年（4 桁。`2016]`、`2016.` のように、閉じ括弧やピリオドが付いていてもよい）。
fn english_year(token: &str) -> Option<i32> {
    let t = token.trim_end_matches(['.', ';', ']', ')']);
    (t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()))
        .then(|| t.parse().ok())
        .flatten()
}

/// 英語の月名を使った日付（`July 2008`、`Mar 05, 2016`、`March 5, 2016`、`5 March 2016`）。
/// 月名がなければ（数字だけの日付など）`None` を返して、呼び出し側が別の形として読む。
/// 月名のあとに年が読めないとき（`May 5` など）も `None`（推測しない）。
fn parse_english_date(t: &str) -> Option<String> {
    let tokens: Vec<&str> = t
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();
    let at = |i: usize| tokens.get(i).copied();
    if let Some(month) = at(0).and_then(english_month) {
        // `March 5, 2016`
        if let (Some(day), Some(year)) = (at(1).and_then(english_day), at(2).and_then(english_year))
        {
            return format_date(year, Some(month), Some(day));
        }
        // `July 2008`
        return format_date(at(1).and_then(english_year)?, Some(month), None);
    }
    // `5 March 2016`
    let day = at(0).and_then(english_day)?;
    let month = at(1).and_then(english_month)?;
    let year = at(2).and_then(english_year)?;
    format_date(year, Some(month), Some(day))
}

/// 日付の前に付く記号（`[2003]` の `[`、MARC の著作権年 `c1995`・`c.1995`・`©1995`）を取り除く。
fn strip_date_marks(t: &str) -> &str {
    let mut t = t.trim_start_matches(|c: char| matches!(c, '[' | '(' | '©') || c.is_whitespace());
    if let Some(rest) = t.strip_prefix(['c', 'C']) {
        let rest = rest.trim_start_matches(['.', ' ']);
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            t = rest;
        }
    }
    t
}

/// 日付を `YYYY`・`YYYY-MM`・`YYYY-MM-DD` のどれかにそろえる。
///
/// 扱う形: `20100701`、`201007`、`2010-07`、`2010.7`、`2010/7/1`、`2010年7月`、`1997.12-`、
/// `2002-11-5`、和暦（`昭和32-01`）、年のあとに余計な文字が続くもの（`1957 17刷-01` → `1957`）。
/// 英語の月名（Open Library）: `July 2008`、`Mar 05, 2016`、`March 5, 2016`、`5 March 2016`（月名は
/// フルスペルと略称。ピリオドは付いていてもよい）。`c1995`・`[2003]` のような記号つきの年も読む。
/// 範囲（`1959-6-1959-11`）は先頭の年月だけを使う。読み取れなければ `None`。
pub fn normalize_date(s: &str) -> Option<String> {
    let t = nfkc(s);
    let t = strip_date_marks(t.trim());
    if t.is_empty() {
        return None;
    }
    if let Some(date) = parse_english_date(t) {
        return Some(date);
    }
    // 和暦
    for (era, offset) in [
        ("明治", 1867),
        ("大正", 1911),
        ("昭和", 1925),
        ("平成", 1988),
        ("令和", 2018),
    ] {
        if let Some(rest) = t.strip_prefix(era) {
            let rest = rest.trim_start();
            let (n, after) = if let Some(r) = rest.strip_prefix('元') {
                (1, r)
            } else {
                let (digits, r) = take_digits(rest);
                if digits.is_empty() || digits.len() > 2 {
                    return None;
                }
                (digits.parse::<i32>().ok()?, r)
            };
            let (m, d) = parse_month_day(after);
            return format_date(offset + n, m, d);
        }
    }
    let (digits, rest) = take_digits(t);
    match digits.len() {
        8 => {
            let y: i32 = digits[..4].parse().ok()?;
            format_date(y, digits[4..6].parse().ok(), digits[6..8].parse().ok())
        }
        6 => {
            let y: i32 = digits[..4].parse().ok()?;
            format_date(y, digits[4..6].parse().ok(), None)
        }
        4 => {
            let y: i32 = digits.parse().ok()?;
            let (m, d) = parse_month_day(rest);
            format_date(y, m, d)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 価格
// ---------------------------------------------------------------------------

/// 価格の表記から読み取った金額。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedPrice {
    pub amount: i64,
    /// 「(税込)」と明記されている。
    pub tax_included: bool,
}

static PRICE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d[\d,]*").expect("正規表現"));

/// `1300`、`1300円`、`310円 (税込)`、`310 (税込)`、`本体1,300円` などから金額を読む。
/// 円以外の通貨や、金額の書かれていない表記（`非売品`）は `None`。
pub fn parse_price(s: &str) -> Option<ParsedPrice> {
    let t = nfkc(s);
    const FOREIGN: [&str; 9] = [
        "ドル",
        "$",
        "ユーロ",
        "€",
        "ポンド",
        "£",
        "ウォン",
        "元",
        "USD",
    ];
    if FOREIGN.iter().any(|f| t.contains(f)) {
        return None;
    }
    let m = PRICE_NUMBER.find(&t)?;
    let amount: i64 = m.as_str().replace(',', "").parse().ok()?;
    if amount <= 0 || amount >= 10_000_000 {
        return None;
    }
    let explicit_excl = t.contains("本体") || t.contains("税別") || t.contains("税抜");
    let tax_included = !explicit_excl && (t.contains("税込") || t.contains("内税"));
    Some(ParsedPrice {
        amount,
        tax_included,
    })
}

// ---------------------------------------------------------------------------
// ページ数・大きさ
// ---------------------------------------------------------------------------

/// `dcterms:extent`（例: `218p ; 15cm`）から分けた値。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extent {
    pub pages: Option<u32>,
    pub size: Option<String>,
}

static PAGES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d+)\s*(?:[pP]ages?\b|[pP]|頁|ページ)([A-Za-z]?)").expect("正規表現")
});
static SIZE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d+(?:\.\d+)?(?:\s*[xX×]\s*\d+(?:\.\d+)?)?)\s*cm").expect("正規表現")
});

/// `218p ; 15cm`、`xi, 210p ; 22cm`、`12, 356p`、`1冊 ; 21cm`、`21cm`、`xlii, 435 p. ;`、`448 pages`（Open Library）
/// などを分ける。
/// ページ数は最後の「数字 + p」、大きさは `cm` の付いた数字（`19×26cm` も可）。
pub fn parse_extent(s: &str) -> Extent {
    let t = nfkc(s);
    let pages = PAGES_RE
        .captures_iter(&t)
        .filter(|c| c.get(2).is_none_or(|m| m.as_str().is_empty()))
        .filter_map(|c| c.get(1).and_then(|m| m.as_str().parse::<u32>().ok()))
        .last()
        .filter(|p| *p > 0);
    let size = SIZE_RE.captures(&t).map(|c| {
        let inner = c[1]
            .replace(char::is_whitespace, "")
            .replace(['x', 'X'], "×");
        format!("{inner}cm")
    });
    Extent { pages, size }
}

// ---------------------------------------------------------------------------
// 言語
// ---------------------------------------------------------------------------

/// 言語コードを ISO 639-1（2 文字）にそろえる。3 文字コード（ISO 639-2）は主なものだけ変換し、
/// 知らないものは小文字にして返す。
pub fn normalize_language(code: &str) -> Option<String> {
    let t = nfkc(code).trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    let base = t.split(['-', '_']).next().unwrap_or(&t);
    let two = match base {
        "jpn" | "jap" => "ja",
        "eng" => "en",
        "fre" | "fra" => "fr",
        "ger" | "deu" => "de",
        "chi" | "zho" => "zh",
        "kor" => "ko",
        "rus" => "ru",
        "spa" => "es",
        "ita" => "it",
        "por" => "pt",
        "lat" => "la",
        "ara" => "ar",
        "dut" | "nld" => "nl",
        other => other,
    };
    Some(two.to_string())
}

// ---------------------------------------------------------------------------
// 著者
// ---------------------------------------------------------------------------

/// 役割として扱う語（NFKC 後）。
const ROLE_WORDS: &[&str] = &[
    "著",
    "作",
    "文",
    "絵",
    "画",
    "訳",
    "譯",
    "編",
    "編著",
    "編集",
    "編訳",
    "編纂",
    "監修",
    "監訳",
    "監",
    "共著",
    "共訳",
    "共編",
    "原作",
    "原著",
    "原案",
    "原画",
    "原文",
    "作画",
    "漫画",
    "構成",
    "脚本",
    "写真",
    "撮影",
    "イラスト",
    "イラストレーション",
    "挿絵",
    "装画",
    "装丁",
    "解説",
    "解題",
    "注",
    "訳注",
    "校訂",
    "校注",
    "校閲",
    "校訳",
    "翻訳",
    "補訳",
    "補筆",
    "企画",
    "制作",
    "製作",
    "監督",
    "出演",
    "述",
    "話",
    "聞き手",
    "口述",
    "著者",
    "作者",
    "編者",
    "訳者",
    "総監修",
    "責任編集",
    "選",
    "選者",
    "絵本",
    "著作",
    "共作",
];

/// 名前の直後にくっついていても役割と判断してよい語（「川端康成著」）。
const GLUED_SAFE: &[&str] = &[
    "編著", "共著", "共訳", "共編", "編訳", "監修", "監訳", "原著", "校注", "校訂", "著", "訳",
    "譯", "編",
];

/// 名前に含まれやすい 1 文字（優作・美絵など）。前の部分がカタカナ・英字だけのときだけ役割とみなす。
const GLUED_RISKY: &[&str] = &["作", "文", "絵", "画"];

fn is_open_bracket(c: char) -> bool {
    matches!(c, '[' | '(' | '〔' | '【' | '［' | '（')
}

fn is_close_bracket(c: char) -> bool {
    matches!(c, ']' | ')' | '〕' | '】' | '］' | '）')
}

fn strip_brackets(s: &str) -> &str {
    let t = s.trim();
    let mut chars = t.chars();
    match (chars.next(), chars.next_back()) {
        (Some(a), Some(b)) if is_open_bracket(a) && is_close_bracket(b) => {
            t[a.len_utf8()..t.len() - b.len_utf8()].trim()
        }
        _ => t,
    }
}

/// `英訳`・`仏訳`（1 文字の言語名 + 訳）、`中国語訳`（言語名 + 語訳）。
fn is_lang_translation(p: &str) -> bool {
    if let Some(head) = p.strip_suffix("語訳") {
        return !head.is_empty() && head.chars().count() <= 4;
    }
    let mut chars = p.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (
            Some(
                '英' | '仏'
                    | '独'
                    | '露'
                    | '伊'
                    | '西'
                    | '中'
                    | '韓'
                    | '日'
                    | '蘭'
                    | '葡'
                    | '梵'
                    | '漢'
            ),
            Some('訳'),
            None
        )
    )
}

fn is_role_word(p: &str) -> bool {
    !p.is_empty() && (ROLE_WORDS.contains(&p) || is_lang_translation(p))
}

/// 役割の表記か（`著`、`英訳`、`訳・解説`、`[編]` など）。
pub fn is_role(s: &str) -> bool {
    let t = strip_brackets(s);
    if t.is_empty() || t.chars().count() > 14 {
        return false;
    }
    t.split(['・', ',', '、', '&', '+', '･'])
        .map(str::trim)
        .all(is_role_word)
}

fn normalize_role(s: &str) -> String {
    let t = strip_brackets(s);
    t.split('・')
        .map(|p| match p.trim() {
            "譯" => "訳",
            "著者" => "著",
            "作者" => "作",
            "編者" => "編",
            "訳者" => "訳",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("・")
}

fn is_katakana_or_latin(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            matches!(c, '\u{30A0}'..='\u{30FF}' | ' ' | '.' | '\'' | '-') || c.is_ascii_alphabetic()
        })
}

fn is_cjk_name_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FFFF}'
        | '\u{3040}'..='\u{309F}' | '々' | '〆' | 'ヶ' | ' ')
}

/// ひらがな・カタカナを含むか。
pub fn has_kana(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3040}'..='\u{30FF}'))
}

/// 漢字・ひらがな・カタカナ（と `々`・`〆`）を含むか。含まなければ、英字など日本語以外の文字だけの表記。
pub fn has_cjk_or_kana(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c,
            '\u{3040}'..='\u{30FF}'
            | '\u{3005}'..='\u{3007}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FF66}'..='\u{FF9F}'
            | '\u{20000}'..='\u{2FFFF}')
    })
}

/// 名前の直後に役割がくっついている形（`川端康成著`）を分ける。
fn split_glued_role(t: &str) -> Option<(&str, String)> {
    for role in GLUED_SAFE {
        if let Some(name) = t.strip_suffix(role).filter(|n| !n.trim().is_empty()) {
            return Some((name, normalize_role(role)));
        }
    }
    for role in GLUED_RISKY {
        if let Some(name) = t.strip_suffix(role).filter(|n| is_katakana_or_latin(n)) {
            return Some((name, normalize_role(role)));
        }
    }
    None
}

enum Tok<'a> {
    /// 名前（空のこともある。前の語が名前の続き）と役割
    NameRole(&'a str, String),
    BareRole(String),
    /// 名前のあとに付いた役割でない丸括弧（`(料理)` など）。捨てる。
    Skip,
    Plain(&'a str),
}

/// `正夫(訳・解説)` のように、名前のうしろに括弧で役割が付いている形。
fn split_bracketed_role(t: &str) -> Option<(&str, String)> {
    let last = t.chars().next_back()?;
    if !is_close_bracket(last) {
        return None;
    }
    let open = t.rfind(is_open_bracket)?;
    let inner = &t[open + t[open..].chars().next()?.len_utf8()..t.len() - last.len_utf8()];
    is_role(inner).then(|| (&t[..open], normalize_role(inner)))
}

fn classify_token(t: &str, is_first: bool) -> Tok<'_> {
    for sep in ['/', '∥'] {
        if let Some(pos) = t.rfind(sep) {
            let (l, r) = (&t[..pos], &t[pos + sep.len_utf8()..]);
            if is_role(r) {
                return Tok::NameRole(l, normalize_role(r));
            }
        }
    }
    if t.chars().next().is_some_and(is_open_bracket) && is_role(t) {
        return Tok::BareRole(normalize_role(t));
    }
    if let Some((name, role)) = split_bracketed_role(t)
        && !name.is_empty()
    {
        return Tok::NameRole(name, role);
    }
    if !is_first && is_role(t) {
        return Tok::BareRole(normalize_role(t));
    }
    if !is_first
        && t.chars().next().is_some_and(is_open_bracket)
        && t.chars().next_back().is_some_and(is_close_bracket)
    {
        return Tok::Skip;
    }
    if let Some((name, role)) = split_glued_role(t) {
        return Tok::NameRole(name, role);
    }
    Tok::Plain(t)
}

static COMMA_SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s+").expect("正規表現"));

static YEAR_PART: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[\d?]{0,4}-[\d?]{0,4}|\d{3,4})$").expect("正規表現"));

/// 「姓,名,生没年」の区切り。
fn split_name_parts(s: &str) -> Vec<String> {
    s.split([',', '‖', '/', '∥'])
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && !YEAR_PART.is_match(p))
        .collect()
}

fn strip_trailing_parenthetical(s: &str) -> &str {
    let t = s.trim_end();
    if t.ends_with(')')
        && let Some(open) = t.rfind('(')
        && !t[..open].trim().is_empty()
    {
        return t[..open].trim_end();
    }
    t
}

/// 英字の名前の末尾に付く生没年。`, 1775-1817`、`, 1952-`、`, -1817`、`, 1775-1817.`、`, b. 1950`、`, d. 1817`、`, ca. 1775-1817`、
/// 空白だけで続く `1952-` も。`b.`・`d.` などの記号はコンマのあとだけ（`Robert D. 1952-` の `D.` は名前の一部）。
static LIFE_DATES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\s*,\s*(?:(?:b|d|fl|ca)\.\s*)?|\s+)(?:\d{1,4}\??\s*[-\u{2013}\u{2014}]\s*(?:\d{1,4}\??)?|[-\u{2013}\u{2014}]\s*\d{1,4}\??|\d{3,4}\??)\.?\s*$",
    )
    .expect("正規表現")
});

/// 末尾の生没年と、そのあとに残るコンマ・空白を除く。
fn strip_life_dates(s: &str) -> &str {
    let t = s.trim();
    let t = match LIFE_DATES.find(t) {
        Some(m) => &t[..m.start()],
        None => t,
    };
    t.trim_end_matches(|c: char| c == ',' || c.is_whitespace())
}

/// 「姓, 名, 肩書き…」を並べた部分（コンマで区切った語）を「名 姓 肩書き…」の順にする。1 語ならそのまま。
fn natural_latin_order<S: AsRef<str>>(parts: &[S]) -> String {
    let joined = match parts {
        [] => String::new(),
        [only] => only.as_ref().to_string(),
        [last, given, suffix @ ..] => {
            let mut words = vec![given.as_ref(), last.as_ref()];
            words.extend(suffix.iter().map(AsRef::as_ref));
            words.join(" ")
        }
    };
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 英字の人名 1 人分を整える（NDL の `foaf:name`、openBD の `姓,名,生没年` など）。
///
/// 末尾の生没年（`, 1775-1817`、`, 1952-`、`, b. 1950` など）を捨て、`姓, 名` を `名 姓` の順にする
/// （`Martin, Robert C.` → `Robert C. Martin`、`Smith, John, Jr.` → `John Smith Jr.`）。
/// コンマのない名前（団体名や `Robert C. Martin`）はそのまま。NFKC と空白の整理は `clean_text` と同じ。
pub fn latin_person_name(raw: &str) -> Option<String> {
    let t = clean_text(raw)?;
    let t = strip_life_dates(&t);
    let parts: Vec<&str> = t
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let name = natural_latin_order(&parts);
    (!name.is_empty()).then_some(name)
}

/// 著者名 1 人分を整える。`夏目,漱石,1867-1916` → `夏目漱石`、`Tolstoy,Leo,1828-1910` → `Leo Tolstoy`（英字は `latin_person_name`）、
/// `[岩波書店]` → `岩波書店`、`山田, 玲子 (料理)` → `山田玲子`。
pub fn clean_person_name(raw: &str) -> Option<String> {
    let t = nfkc(raw);
    let t = strip_brackets(&t);
    let t = strip_trailing_parenthetical(t);
    let parts = split_name_parts(t);
    let name = match parts.len() {
        0 => return None,
        1 => parts[0].split_whitespace().collect::<Vec<_>>().join(" "),
        _ => {
            if parts.iter().all(|p| p.chars().all(is_cjk_name_char)) {
                parts
                    .iter()
                    .flat_map(|p| p.split_whitespace())
                    .collect::<Vec<_>>()
                    .concat()
            } else if t.contains(',') && parts.iter().all(|p| !has_cjk_or_kana(p)) {
                latin_person_name(t)?
            } else {
                parts.join(", ")
            }
        }
    };
    (!name.is_empty()).then_some(name)
}

/// ヨミを整える。`ナツメ, ソウセキ, 1867-1916` → `ナツメ ソウセキ`。かなを含まなければ `None`。
pub fn clean_kana(raw: &str) -> Option<String> {
    let t = nfkc(raw);
    let t = strip_trailing_parenthetical(strip_brackets(&t)).to_string();
    let parts: Vec<String> = t
        .split([',', '‖', '/', '∥'])
        .flat_map(|p| p.split_whitespace())
        .filter(|p| !p.is_empty() && !YEAR_PART.is_match(p))
        .map(str::to_string)
        .collect();
    let kana = parts.join(" ");
    (has_kana(&kana)).then_some(kana)
}

/// 照合用のキー（英数字とかなだけを小文字で残す）。
pub fn name_key(raw: &str) -> String {
    let t = nfkc(raw).to_lowercase();
    split_name_parts(&t)
        .iter()
        .flat_map(|p| p.chars())
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn push_author(out: &mut Vec<CandidateAuthor>, name: &str, role: Option<String>) {
    let Some(name) = clean_person_name(name) else {
        return;
    };
    let author = CandidateAuthor {
        name,
        kana: None,
        role,
    };
    if !out.contains(&author) {
        out.push(author);
    }
}

/// 英字の語の並びで、`語` の前後が語の境界（英数字でない文字か、文字列の端）になっている最初の出現。
/// 大文字小文字は区別しない。見つかれば元の文字列でのバイト位置（開始、終了）。
fn find_word_ci(hay: &str, needle: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    for (i, _) in hay.char_indices() {
        if hay[..i]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let Some(len) = match_len_ci(&hay[i..], &needle) else {
            continue;
        };
        if hay[i + len..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        return Some((i, i + len));
    }
    None
}

/// `hay` の先頭が `needle`（小文字にした文字の並び）と大文字小文字を無視して合うとき、その長さ（バイト）。
fn match_len_ci(hay: &str, needle: &[char]) -> Option<usize> {
    let mut consumed = 0;
    let mut k = 0;
    for ch in hay.chars() {
        if k == needle.len() {
            break;
        }
        for lower in ch.to_lowercase() {
            if needle.get(k) != Some(&lower) {
                return None;
            }
            k += 1;
        }
        consumed += ch.len_utf8();
    }
    (k == needle.len()).then_some(consumed)
}

/// 英語の責任表示の語から役割へ。`edited by` の `edited` のように、` by ` の前の語を調べる。
fn latin_role_keyword(word: &str) -> Option<&'static str> {
    Some(match word {
        "edited" | "editor" | "editors" | "ed" | "compiled" | "compiler" => "編",
        "translated" | "translator" | "translators" | "trans" => "訳",
        "illustrated" | "illustrations" | "illustrator" | "illustrators" => "絵",
        "foreword" | "preface" => "序文",
        "introduction" | "afterword" | "notes" | "commentary" => "解説",
        "adapted" => "翻案",
        "written" | "author" => "著",
        _ => return None,
    })
}

/// ` by ` より前の語句（`edited with an introduction and notes`）の役割。語がなければ（`by Jane Austen`）`著`。
/// 最初に出てくる役割の語を使い、知らない語だけなら役割なし。
fn latin_role_from_phrase(phrase: &str) -> Option<&'static str> {
    let words: Vec<String> = phrase
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .map(|w| w.trim_matches('.').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return Some("著");
    }
    words.iter().find_map(|w| latin_role_keyword(w))
}

/// 責任表示の 1 区切り（`;` で分けたもの）の役割。`by` がなければ `著`（`Jane Austen`）。
fn latin_segment_role(segment: &str) -> Option<&'static str> {
    match find_word_ci(segment, "by") {
        None => Some("著"),
        Some((start, _)) => latin_role_from_phrase(&segment[..start]),
    }
}

/// 英字の責任表示から、構造化された人名（NDL の `foaf:name`）で著者を作る。
///
/// - 名前と順序は `foaf_names`（`Martin, Robert C.` → `Robert C. Martin`。`latin_person_name`）。責任表示にだけある人は無視する。
/// - 役割は責任表示を `;` で区切った中の、その人の姓（コンマの前）が最初に出てくる区切りから決める。
///   `by` がなければ `著`、あれば ` by ` より前の最初の役割の語（`edited` → 編、`foreword` → 序文、`introduction` → 解説 など）。
///   知らない語や、どの区切りにも出てこない人は役割なし。
/// - ヨミは付けない。
///
/// 責任表示が空、または漢字・かなを含むとき（日本語の本）、人名が 1 つも読めないときは `None`（呼び出し側が従来の `parse_authors` を使う）。
pub fn parse_latin_statement_authors(
    statement: &str,
    foaf_names: &[String],
) -> Option<Vec<CandidateAuthor>> {
    let statement = nfkc(statement);
    if statement.trim().is_empty() || has_cjk_or_kana(&statement) {
        return None;
    }
    let segments: Vec<&str> = statement
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut out: Vec<CandidateAuthor> = Vec::new();
    for raw in foaf_names {
        let Some(text) = clean_text(raw) else {
            continue;
        };
        let name = if has_cjk_or_kana(&text) {
            clean_person_name(&text)
        } else {
            latin_person_name(&text)
        };
        let Some(name) = name else { continue };
        let surname = strip_life_dates(&text)
            .split(',')
            .next()
            .unwrap_or("")
            .trim();
        let role = if surname.is_empty() {
            None
        } else {
            segments
                .iter()
                .find(|seg| find_word_ci(seg, surname).is_some())
                .and_then(|seg| latin_segment_role(seg))
        };
        let author = CandidateAuthor {
            name,
            kana: None,
            role: role.map(str::to_string),
        };
        if !out.contains(&author) {
            out.push(author);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `…` と `...`、`[et al.]`、`et al.`。
static ET_AL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[\s*et\s+al\.?\s*\]|\bet\s+al\b\.?|\.{3}|\u{2026}").expect("正規表現")
});

/// 人どうしを分ける ` and `・` & `（コンマ、空白のあとだけ。`AT&T` は分けない）。
static LATIN_CONJUNCTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:\s|,)\s*(?:and|&)\s+").expect("正規表現"));

/// `1952-`、`1821-1881`、`?-1900`、`1952?` など、年・生没年だけの文字列か。
fn is_date_tail(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '?' | '.' | '\u{2013}'))
        && s.chars().any(|c| c.is_ascii_digit() || c == '?')
}

/// 空白で区切られた「姓,名,生没年」の並び（NDL 由来の openBD）で、`tok` が前の語 `prev` と同じ人の続きか。
///
/// 英字の名前が空白を含むと（`Martin,Robert C.,1952-`）、空白で切れて `Martin,Robert` と `C.,1952-` になる。
/// 前の語が英字で、この語も英字で、コンマがない（`C.`）か、最初のコンマのあとが年（`C.,1952-`）なら続き。
/// 新しい人の語は `姓,名` の形（最初のコンマのあとが年でない）か、日本語。
fn continues_latin_person(prev: &str, tok: &str) -> bool {
    if has_cjk_or_kana(prev) || has_cjk_or_kana(tok) {
        return false;
    }
    match tok.split_once(',') {
        None => true,
        Some((_, tail)) => is_date_tail(tail),
    }
}

/// 空白で区切られた語を、`continues_latin_person` で人ごとにまとめる。
fn group_person_tokens<'a>(tokens: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    let mut prev: Option<&str> = None;
    for tok in tokens {
        match (groups.last_mut(), prev) {
            (Some(group), Some(p)) if continues_latin_person(p, tok) => {
                group.push(' ');
                group.push_str(tok);
            }
            _ => groups.push(tok.to_string()),
        }
        prev = Some(tok);
    }
    groups
}

/// 漢字・かなを含まない区切り（英字の責任表示、または英字だけの `姓,名,生没年`）。空白では人を分けない。
///
/// `...`・`[et al.]` を除き、先頭の `<語> by ` は役割の語（`foreword by`、`edited by`）にして外す。
/// 人は ` and `・` & ` だけで分け、1 人の中身は 1 つの名前にする（`Robert C. Martin`）。
/// ただし最初の語が `姓,名` の形なら、openBD の `Dostoyevsky,Fyodor,1821-1881 Tolstoy,Leo,1828-1910` と同じに、
/// 空白で区切られた語を `continues_latin_person` で人ごとにまとめる。
/// `by` がなければ役割なし（NDL の構造化された人名を使えるときの `著` の既定は `parse_latin_statement_authors` だけ）。
fn parse_latin_segment(segment: &str, out: &mut Vec<CandidateAuthor>) {
    let cleaned = ET_AL.replace_all(segment, " ");
    let s = cleaned.trim();
    let (role, rest) = match find_word_ci(s, "by") {
        Some((start, end)) => (latin_role_from_phrase(&s[..start]), &s[end..]),
        None => (None, s),
    };
    for chunk in LATIN_CONJUNCTION.split(rest) {
        let chunk = chunk.trim_matches(|c: char| c == ',' || c.is_whitespace());
        if chunk.is_empty() {
            continue;
        }
        let comma_first = chunk
            .split_whitespace()
            .next()
            .is_some_and(|first| first.contains(','));
        let persons = if comma_first {
            group_person_tokens(chunk.split_whitespace())
        } else {
            vec![chunk.to_string()]
        };
        for person in persons {
            push_author(out, &person, role.map(str::to_string));
        }
    }
}

fn parse_author_segment(segment: &str, out: &mut Vec<CandidateAuthor>) {
    if !has_cjk_or_kana(segment) {
        parse_latin_segment(segment, out);
        return;
    }
    let tokens: Vec<&str> = segment.split_whitespace().collect();
    if tokens.is_empty() {
        return;
    }
    let classified: Vec<Tok<'_>> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| classify_token(t, i == 0))
        .collect();
    if classified
        .iter()
        .any(|t| matches!(t, Tok::NameRole(..) | Tok::BareRole(_)))
    {
        // 役割つき: 名前の語を貯めておき、役割の語が来たら 1 人分にする
        let mut acc: Vec<&str> = Vec::new();
        for tok in classified {
            match tok {
                Tok::Skip => {}
                Tok::Plain(t) => acc.push(t),
                Tok::NameRole(name, role) => {
                    if !name.trim().is_empty() {
                        acc.push(name.trim());
                    }
                    push_author(out, &acc.join(" "), Some(role));
                    acc.clear();
                }
                Tok::BareRole(role) => {
                    if !acc.is_empty() {
                        push_author(out, &acc.join(" "), Some(role));
                        acc.clear();
                    }
                }
            }
        }
        if !acc.is_empty() {
            push_author(out, &acc.join(" "), None);
        }
    } else {
        let names: Vec<&str> = classified
            .iter()
            .filter_map(|t| {
                if let Tok::Plain(t) = t {
                    Some(*t)
                } else {
                    None
                }
            })
            .collect();
        if names.len() > 1
            && names.iter().all(|t| {
                t.chars()
                    .all(|c| c.is_ascii_alphabetic() || matches!(c, '.' | '\'' | '-'))
            })
        {
            // 英字だけの複数語は 1 人の名前（John Smith）
            push_author(out, &names.join(" "), None);
        } else {
            // 役割がなければ、空白で区切られた語はそれぞれ別の著者（NDL 由来の openBD は「姓,名,生没年 姓,名,生没年」の形）。
            // ただし英字の名前が空白を含むときの続きの語（`C.,1952-`）は前の人にまとめる。
            for person in group_person_tokens(names) {
                push_author(out, &person, None);
            }
        }
    }
}

/// 著者の表記を役割つきの一覧にする。
///
/// 扱う形（NFKC 後）:
/// - `山田玲子／著 水野菜生／英訳`（openBD の版元データ）
/// - `山田玲子 著` `川端康成著` `尾田栄一郎∥著` `米川, 正夫（訳・解説）` `[岩波書店] [編]`（NDL）
/// - `ドストエーフスキイ作 ; 米川正夫譯`（`;` と `、` で人を区切る）
/// - `夏目,漱石,1867-1916`、`Tolstoy,Leo,1828-1910 中村,融,1911-1990`（NDL 由来の openBD。役割なし。英字の名前は `Leo Tolstoy` の順）
/// - 漢字・かなを含まない区切りは、空白で人を分けない（`Robert C. Martin ... [et al.]`、`foreword by James O. Coplien`、
///   `edited by A and B`。`parse_latin_segment`）。NDL は構造化された人名があればそちらを使う（`parse_latin_statement_authors`）。
///
/// 役割は `著`・`訳`・`編`・`監修`・`画`・`作`・`文`・`絵`・`英訳` など。同じ人（名前と役割が同じ）は 1 つにまとめる。
pub fn parse_authors(raw: &str) -> Vec<CandidateAuthor> {
    // 「姓, 名」の空白は名前の途中なので、区切りの空白と区別できるように詰める
    let t = COMMA_SPACE.replace_all(&nfkc(raw), ",").into_owned();
    let mut out = Vec::new();
    for segment in t.split([';', '、', '\n']) {
        parse_author_segment(segment.trim(), &mut out);
    }
    out
}

/// 名前の照合キー（`name_key`）が合う著者にヨミを付ける。`pairs` は（原文の名前, 原文のヨミ）。
///
/// 原文の名前は `山田, 玲子, 料理`（姓, 名, 肩書き）のように、著者の名前のあとに別の語が続くことがある。
/// 先頭から何個の部分で著者の名前になるかを数え、ヨミも同じ個数の部分だけ使う（`ヤマダ, レイコ, リョウリ` → `ヤマダ レイコ`）。
/// 英字の名前は `姓, 名` を `名 姓` の順にしたもの（`parse_authors` の結果）とも照合する。
pub fn attach_kana(authors: &mut [CandidateAuthor], pairs: &[(String, String)]) {
    for author in authors.iter_mut() {
        if author.kana.is_some() {
            continue;
        }
        let key = name_key(&author.name);
        if key.is_empty() {
            continue;
        }
        for (name, kana) in pairs {
            let parts = split_name_parts(&nfkc(name).to_lowercase());
            let mut acc = String::new();
            let mut count = None;
            for (i, p) in parts.iter().enumerate() {
                acc.extend(p.chars().filter(|c| c.is_alphanumeric()));
                let natural_order_matches = i > 0
                    && parts[..=i].iter().all(|p| !has_cjk_or_kana(p))
                    && name_key(&natural_latin_order(&parts[..=i])) == key;
                if acc == key || natural_order_matches {
                    count = Some(i + 1);
                    break;
                }
            }
            let Some(count) = count else { continue };
            let used: Vec<String> = split_name_parts(&nfkc(kana))
                .into_iter()
                .take(count)
                .collect();
            if let Some(k) = clean_kana(&used.join(" ")) {
                author.kana = Some(k);
                break;
            }
        }
    }
}

/// 英字の人名の照合キー（NFKC、小文字、ASCII の英数字だけ）。`Robert C. Martin`・`RobertC.Martin`・`ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ`
/// はどれも `robertcmartin`。空白・記号の有無・全角半角のゆれを除いて、同じ人の綴りを突き合わせる。
pub fn latin_name_key(s: &str) -> String {
    nfkc(s)
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// 構造化された人名（NDL の `foaf:name`、openBD の ONIX の `PersonName`）のうち、英字のものを `名 姓` の形にして並べる。
/// 漢字・かなを含む名前、空の名前は除く。`restore_latin_name_spacing` の「参照」に使う。
pub fn latin_reference_names<I, S>(raw: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    raw.into_iter()
        .filter_map(|r| {
            let text = clean_text(r.as_ref())?;
            if has_cjk_or_kana(&text) {
                return None;
            }
            latin_person_name(&text)
        })
        .collect()
}

/// 綴りの比較用（空白・記号を除き、大文字小文字と英数字はそのまま）。
fn spelling_form(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// `key` と照合キーが合う参照のうち、使うもの。
///
/// 合うものがなければ `None`。複数あっても、空白・記号が違うだけ（`RobertC.Martin` と `Robert C. Martin`）なら
/// 空白が最も多いもの（同数なら先に出たもの）。綴りが違うもの（大文字小文字・英字以外の文字が違う）が混ざるなら、
/// どれとも決められないので `None`。
fn pick_reference<'a>(key: &str, references: &'a [String]) -> Option<&'a str> {
    let mut matching = references.iter().filter(|r| latin_name_key(r) == key);
    let first = matching.next()?;
    let form = spelling_form(first);
    let mut best = first;
    for r in matching {
        if spelling_form(r) != form {
            return None;
        }
        let spaces = |s: &str| s.chars().filter(|c| c.is_whitespace()).count();
        if spaces(r) > spaces(best) {
            best = r;
        }
    }
    Some(best.as_str())
}

/// 小文字のすぐあとの `C.`（`RobertC.`）。
static INITIAL_AFTER_LOWER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z])([A-Z]\.)").expect("正規表現"));

/// 語頭（文字列の先頭、空白、ピリオドのあと）の頭文字 `C.` のすぐあとに、大文字で始まる語が続く形（`C.Martin`、`J.R.R.Tolkien`）。
static INITIAL_BEFORE_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[\s.])([A-Z]\.)([A-Z][a-z])").expect("正規表現"));

/// 英字の人名で、頭文字のまわりの欠けた空白だけを補う。
///
/// 補うのは次の 2 つだけ（`RobertC.Martin` → `Robert C. Martin`、`J.R.R.Tolkien` → `J.R.R. Tolkien`）。
/// - 小文字のすぐあとに、大文字 1 字とピリオドの頭文字が続く形（`t` + `C.`）。
/// - 語頭の頭文字（大文字 1 字とピリオド）のすぐあとに、大文字で始まる語が続く形（`C.` + `Martin`）。
///
/// `McDonald`・`DeMarco`・`MacArthur`・`St.John`・`O'Brien` や、すでに空白のある名前は変えない。最後に空白を 1 つにそろえる。
pub fn space_latin_initials(name: &str) -> String {
    let t = INITIAL_AFTER_LOWER.replace_all(name, "$1 $2");
    let t = INITIAL_BEFORE_WORD.replace_all(&t, "$1$2 $3");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 英字の著者名に欠けた空白を直す（日本語訳の本では、元のデータが `RobertC.Martin` のように名前の空白を落としている）。
///
/// 日本語の名前（漢字・かなを含む）には触らない。`role` と `kana` はそのまま。
/// 1. 構造化された名前（`references`。`latin_reference_names` の結果）と照合キー（`latin_name_key`）が合えば、その綴りに置き換える。
///    合う参照が複数あるときの選び方は `pick_reference`。
/// 2. そのあと（置き換えたかどうかによらず）、`space_latin_initials` で頭文字のまわりだけを直す。参照の綴りにも空白の
///    欠けたもの（`Robert C.Martin`）があるため。整った名前には何もしない。
///
/// 置き換えで同じ著者（名前・ヨミ・役割が同じ）が重なったら、先に出たものだけを残す。
pub fn restore_latin_name_spacing(authors: &mut Vec<CandidateAuthor>, references: &[String]) {
    for author in authors.iter_mut() {
        if has_cjk_or_kana(&author.name) {
            continue;
        }
        let key = latin_name_key(&author.name);
        let reference = if key.is_empty() {
            None
        } else {
            pick_reference(&key, references)
        };
        // 構造化された名前で置き換えたあとも、イニシャルの補いをかける（整った名前には何もしない）
        let name = reference.unwrap_or(&author.name);
        author.name = space_latin_initials(name);
    }
    let mut unique: Vec<CandidateAuthor> = Vec::with_capacity(authors.len());
    for author in authors.drain(..) {
        if !unique.contains(&author) {
            unique.push(author);
        }
    }
    *authors = unique;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(name: &str, role: Option<&str>) -> CandidateAuthor {
        CandidateAuthor {
            name: name.into(),
            kana: None,
            role: role.map(Into::into),
        }
    }

    #[test]
    fn text_cleanup() {
        assert_eq!(
            clean_text("  Ｏｎｅ　Ｐｉｅｃｅ \n 巻１ ").as_deref(),
            Some("One Piece 巻1")
        );
        assert_eq!(clean_text("ﾎﾟｯﾄ出版").as_deref(), Some("ポット出版"));
        assert_eq!(clean_text("   ").as_deref(), None);
        assert_eq!(
            clean_description("\r\n  一行目\r\n二行目 \n").as_deref(),
            Some("一行目\n二行目")
        );
        assert_eq!(
            split_title("おにぎりレシピ101 : EVERYDAY ONIGIRI"),
            ("おにぎりレシピ101".into(), Some("EVERYDAY ONIGIRI".into()))
        );
        assert_eq!(
            split_title("人はなんで生きるか : 他四篇 : トルストイ民話集"),
            (
                "人はなんで生きるか".into(),
                Some("他四篇 : トルストイ民話集".into())
            )
        );
        assert_eq!(split_title("Re:ゼロ"), ("Re:ゼロ".into(), None));
        assert_eq!(
            upgrade_to_https("http://books.google.com/x?a=1"),
            "https://books.google.com/x?a=1"
        );
        assert_eq!(upgrade_to_https("https://a/b"), "https://a/b");
        assert_eq!(normalize_ccode("００７７").as_deref(), Some("0077"));
        assert_eq!(normalize_ccode("077"), None);
    }

    #[test]
    fn html_is_stripped() {
        assert_eq!(
            strip_html("先生と私。<br><br><b>改版</b>。&quot;K&quot;と&amp;遺書"),
            "先生と私。\n\n改版。\"K\"と&遺書"
        );
        // 段落は空行 1 つ。前後の空行は付かない
        assert_eq!(strip_html("a<p>b</p><p>c</p>"), "a\n\nb\n\nc");
        assert_eq!(strip_html("<p>a</p>"), "a");
        // `<br>` は 1 つにつき改行 1 回。3 つ以上でも空行は 1 つ
        assert_eq!(strip_html("a<br>b<br/>c<BR />d"), "a\nb\nc\nd");
        assert_eq!(strip_html("a<br><br><br><br>b"), "a\n\nb");
        assert_eq!(strip_html("<br>a<br>"), "a");
        // 改行が重なっても、空行は 1 つまで
        assert_eq!(strip_html("a<br></p><p><br>b"), "a\n\nb");
    }

    #[test]
    fn html_lists_become_one_line_per_item() {
        assert_eq!(
            strip_html("<p>Contents:</p><ul><li>One</li><li>Two<br>(more)</li></ul><p>End</p>"),
            "Contents:\n\nOne\nTwo\n(more)\n\nEnd"
        );
        // 整形用の改行・空白が間にあっても、空行は増えない
        assert_eq!(
            strip_html("Intro\n<ol>\n  <li> A </li>\n  <li>B</li>\n</ol>\nOutro"),
            "Intro\nA\nB\nOutro"
        );
        assert_eq!(strip_html("x<ul><li>a</li></ul>y"), "x\na\ny");
    }

    #[test]
    fn inline_tags_are_dropped_and_their_text_is_kept() {
        assert_eq!(
            strip_html(
                r#"<b>Bold</b> and <i>italic</i>, <em>em</em> <strong>strong</strong> <span class="x">span</span> <a href="https://example.org/?a=1&amp;b=2">link</a>."#
            ),
            "Bold and italic, em strong span link."
        );
        // コメントも除く
        assert_eq!(strip_html("a<!-- note <br> -->b"), "ab");
        // 本文の中の空白は残す
        assert_eq!(strip_html("a <b>b</b> c"), "a b c");
    }

    #[test]
    fn html_entities_are_decoded_once() {
        assert_eq!(
            strip_html("&lt;b&gt; &amp;amp; &quot;q&quot; &#39;s&#39; &apos;t&apos;"),
            "<b> &amp; \"q\" 's' 't'"
        );
        // 10 進・16 進の数値参照
        assert_eq!(
            strip_html("&#12354;&#x3042;&#X3042; &#65;&#x41;"),
            "あああ AA"
        );
        // &nbsp; は半角の空白
        assert_eq!(strip_html("a&nbsp;b&#160;c"), "a b c");
        // よく使う名前
        assert_eq!(
            strip_html("a&ndash;b&mdash;c&hellip;&rsquo;&ldquo;x&rdquo;"),
            "a\u{2013}b\u{2014}c\u{2026}\u{2019}\u{201C}x\u{201D}"
        );
        // 知らない名前、閉じていないもの、範囲外の数値は、そのまま残す
        assert_eq!(
            strip_html("&unknown; &amp &#xD800; &#99999999;"),
            "&unknown; &amp &#xD800; &#99999999;"
        );
        // タグを除いたあとに戻すので、`&lt;br&gt;` はタグではなく文字
        assert_eq!(strip_html("a&lt;br&gt;b"), "a<br>b");
    }

    #[test]
    fn html_output_has_no_leading_trailing_or_repeated_blank_lines() {
        assert_eq!(strip_html("  \n\n<p>a</p>\n\n\n\n<p>b</p>\n  "), "a\n\nb");
        assert_eq!(strip_html("a  <br>  b  "), "a\nb");
        assert_eq!(strip_html("<p></p><br>"), "");
        assert_eq!(strip_html(""), "");
        // タグのない文章は、改行の形（`\r\n`・3 つ以上の改行）だけを整える
        assert_eq!(strip_html("a\r\n\r\n\r\n\r\nb"), "a\n\nb");
    }

    #[test]
    fn dates() {
        for (input, want) in [
            ("20100701", Some("2010-07-01")),
            ("201007", Some("2010-07")),
            ("198905", Some("1989-05")),
            ("2010-07", Some("2010-07")),
            ("2010-7-1", Some("2010-07-01")),
            ("2010.7", Some("2010-07")),
            ("2014.4", Some("2014-04")),
            ("2010/7/1", Some("2010-07-01")),
            ("2010年7月", Some("2010-07")),
            ("2010年7月8日", Some("2010-07-08")),
            ("２０１０年７月", Some("2010-07")),
            ("1997.12-", Some("1997-12")),
            ("１９９７．１２", Some("1997-12")),
            ("1987", Some("1987")),
            ("2002-11-5", Some("2002-11-05")),
            ("1959-6-1959-11", Some("1959-06")),
            ("1957 17刷-01", Some("1957")),
            ("昭和32-01", Some("1957-01")),
            ("昭和32年1月", Some("1957-01")),
            ("平成元年5月1日", Some("1989-05-01")),
            ("令和3年", Some("2021")),
            ("2014-02-30", Some("2014-02")),
            ("2014-13", None),
            ("0000-00-00 00:00:00", None),
            ("", None),
            ("発売予定", None),
            ("12345", None),
            ("3999", None),
        ] {
            assert_eq!(normalize_date(input).as_deref(), want, "入力: {input:?}");
        }
    }

    /// Open Library の英語の日付（自由な書式）。
    #[test]
    fn english_dates() {
        for (input, want) in [
            ("2003", Some("2003")),
            ("July 2008", Some("2008-07")),
            ("Mar 05, 2016", Some("2016-03-05")),
            ("March 5, 2016", Some("2016-03-05")),
            ("5 March 2016", Some("2016-03-05")),
            ("05 Mar 2016", Some("2016-03-05")),
            ("2016-03-05", Some("2016-03-05")),
            ("c1995", Some("1995")),
            ("[2003]", Some("2003")),
            // 月名: 略称、ピリオド、大文字小文字、Sept、日の序数
            ("Jan. 2001", Some("2001-01")),
            ("SEPT. 12, 1999", Some("1999-09-12")),
            ("sep 1999", Some("1999-09")),
            ("December 25th, 2020", Some("2020-12-25")),
            ("1 Feb. 2012", Some("2012-02-01")),
            ("May 2010", Some("2010-05")),
            // 記号つき・閉じ括弧つき
            ("c1995.", Some("1995")),
            ("c.1995", Some("1995")),
            ("[c1995]", Some("1995")),
            ("©1995", Some("1995")),
            ("[July 2008]", Some("2008-07")),
            ("(2003)", Some("2003")),
            // 前後の空白・全角
            ("  March 5, 2016 ", Some("2016-03-05")),
            ("Ｍａｒｃｈ　５，　２０１６", Some("2016-03-05")),
            // 範囲・余計な文字は、先頭の日付だけ
            ("March 2016 - April 2017", Some("2016-03")),
            ("March 5, 2016 (reprint)", Some("2016-03-05")),
            // 日が存在しなければ、年月まで
            ("February 30, 2014", Some("2014-02")),
            // 読めなければ空（推測しない）
            ("May 5", None),
            ("July", None),
            ("March of the Penguins", None),
            ("circa 1995", None),
            ("Spring 2008", None),
            ("March 12345", None),
            ("Smarch 5, 2016", None),
            ("5 March", None),
            ("13 Jan 99", None),
        ] {
            assert_eq!(normalize_date(input).as_deref(), want, "入力: {input:?}");
        }
    }

    #[test]
    fn prices() {
        let p = |s| parse_price(s);
        assert_eq!(
            p("1300"),
            Some(ParsedPrice {
                amount: 1300,
                tax_included: false
            })
        );
        assert_eq!(
            p("1300円"),
            Some(ParsedPrice {
                amount: 1300,
                tax_included: false
            })
        );
        assert_eq!(
            p("310 (税込)"),
            Some(ParsedPrice {
                amount: 310,
                tax_included: true
            })
        );
        assert_eq!(
            p("310円 (税込)"),
            Some(ParsedPrice {
                amount: 310,
                tax_included: true
            })
        );
        assert_eq!(
            p("３９０円"),
            Some(ParsedPrice {
                amount: 390,
                tax_included: false
            })
        );
        assert_eq!(
            p("本体1,300円"),
            Some(ParsedPrice {
                amount: 1300,
                tax_included: false
            })
        );
        assert_eq!(
            p("本体1,300円 (税込1,430円)"),
            Some(ParsedPrice {
                amount: 1300,
                tax_included: false
            })
        );
        assert_eq!(p("12 ドル"), None);
        assert_eq!(p("非売品"), None);
        assert_eq!(p("0円"), None);
    }

    #[test]
    fn extents() {
        let e = |s| parse_extent(s);
        assert_eq!(
            e("218p ; 15cm"),
            Extent {
                pages: Some(218),
                size: Some("15cm".into())
            }
        );
        assert_eq!(
            e("123p ; 21cm"),
            Extent {
                pages: Some(123),
                size: Some("21cm".into())
            }
        );
        assert_eq!(
            e("xi, 210p ; 22cm"),
            Extent {
                pages: Some(210),
                size: Some("22cm".into())
            }
        );
        assert_eq!(
            e("12, 356p ; 19 cm"),
            Extent {
                pages: Some(356),
                size: Some("19cm".into())
            }
        );
        assert_eq!(
            e("２０７ｐ ; １８ｃｍ"),
            Extent {
                pages: Some(207),
                size: Some("18cm".into())
            }
        );
        assert_eq!(
            e("21cm"),
            Extent {
                pages: None,
                size: Some("21cm".into())
            }
        );
        assert_eq!(
            e("3冊 ; 19cm"),
            Extent {
                pages: None,
                size: Some("19cm".into())
            }
        );
        assert_eq!(
            e("300 p. ; 15 cm"),
            Extent {
                pages: Some(300),
                size: Some("15cm".into())
            }
        );
        assert_eq!(
            e("19×26cm"),
            Extent {
                pages: None,
                size: Some("19×26cm".into())
            }
        );
        assert_eq!(
            e("120ページ"),
            Extent {
                pages: Some(120),
                size: None
            }
        );
        // Open Library の `pagination`
        assert_eq!(
            e("xlii, 435 p. ;"),
            Extent {
                pages: Some(435),
                size: None
            }
        );
        assert_eq!(
            e("448 pages"),
            Extent {
                pages: Some(448),
                size: None
            }
        );
        assert_eq!(e("12 pt"), Extent::default());
        assert_eq!(e(""), Extent::default());
    }

    #[test]
    fn languages() {
        assert_eq!(normalize_language("jpn").as_deref(), Some("ja"));
        assert_eq!(normalize_language("ja-JP").as_deref(), Some("ja"));
        assert_eq!(normalize_language("ENG").as_deref(), Some("en"));
        // Open Library の言語コード（MARC の言語コード）
        for (code, want) in [
            ("eng", "en"),
            ("fre", "fr"),
            ("fra", "fr"),
            ("ger", "de"),
            ("deu", "de"),
            ("spa", "es"),
            ("ita", "it"),
            ("jpn", "ja"),
            ("chi", "zh"),
            ("zho", "zh"),
            ("kor", "ko"),
        ] {
            assert_eq!(normalize_language(code).as_deref(), Some(want), "{code}");
        }
        assert_eq!(normalize_language("xyz").as_deref(), Some("xyz"));
        assert_eq!(normalize_language(" ").as_deref(), None);
    }

    #[test]
    fn authors_openbd_publisher_style() {
        assert_eq!(
            parse_authors("山田玲子／著 水野菜生／英訳"),
            vec![a("山田玲子", Some("著")), a("水野菜生", Some("英訳"))]
        );
        assert_eq!(
            parse_authors("山田　玲子／著"),
            vec![a("山田 玲子", Some("著"))]
        );
        assert_eq!(
            parse_authors("ジェーン・オースティン／著 中野康司／訳"),
            vec![
                a("ジェーン・オースティン", Some("著")),
                a("中野康司", Some("訳"))
            ]
        );
        assert_eq!(
            parse_authors("John Smith／著"),
            vec![a("John Smith", Some("著"))]
        );
        assert_eq!(
            parse_authors("山田玲子／著、水野菜生／監修"),
            vec![a("山田玲子", Some("著")), a("水野菜生", Some("監修"))]
        );
        assert_eq!(
            parse_authors("尾田栄一郎／作画"),
            vec![a("尾田栄一郎", Some("作画"))]
        );
        assert_eq!(
            parse_authors("山田太郎／文 鈴木花子／絵"),
            vec![a("山田太郎", Some("文")), a("鈴木花子", Some("絵"))]
        );
    }

    #[test]
    fn authors_ndl_style() {
        assert_eq!(
            parse_authors("山田玲子 著"),
            vec![a("山田玲子", Some("著"))]
        );
        assert_eq!(parse_authors("川端康成著"), vec![a("川端康成", Some("著"))]);
        assert_eq!(
            parse_authors("尾田栄一郎∥著"),
            vec![a("尾田栄一郎", Some("著"))]
        );
        assert_eq!(
            parse_authors("夏目漱石 作"),
            vec![a("夏目漱石", Some("作"))]
        );
        assert_eq!(
            parse_authors("ドストエフスキー　著"),
            vec![a("ドストエフスキー", Some("著"))]
        );
        assert_eq!(
            parse_authors("米川, 正夫（訳・解説）"),
            vec![a("米川正夫", Some("訳・解説"))]
        );
        assert_eq!(
            parse_authors("[岩波書店] [編]"),
            vec![a("岩波書店", Some("編"))]
        );
        assert_eq!(
            parse_authors("ドストエーフスキイ作 ; 米川正夫譯"),
            vec![
                a("ドストエーフスキイ", Some("作")),
                a("米川正夫", Some("訳"))
            ]
        );
        assert_eq!(
            parse_authors("ドストエフスキー 著 ; 米川正夫 訳"),
            vec![a("ドストエフスキー", Some("著")), a("米川正夫", Some("訳"))]
        );
        assert_eq!(parse_authors("永田寛定"), vec![a("永田寛定", None)]);
        // 名前の末尾が役割の 1 文字と同じでも、漢字の名前は切らない
        assert_eq!(parse_authors("山田優作"), vec![a("山田優作", None)]);
        assert_eq!(
            parse_authors("山田優作 著"),
            vec![a("山田優作", Some("著"))]
        );
    }

    #[test]
    fn authors_ndl_derived_openbd_comma_style() {
        assert_eq!(
            parse_authors("夏目,漱石,1867-1916"),
            vec![a("夏目漱石", None)]
        );
        assert_eq!(
            parse_authors("尾田,栄一郎,1975-"),
            vec![a("尾田栄一郎", None)]
        );
        assert_eq!(
            parse_authors("Dostoyevsky,Fyodor,1821-1881 江川,卓,1927-2001"),
            vec![a("Fyodor Dostoyevsky", None), a("江川卓", None)]
        );
        assert_eq!(
            parse_authors(
                "Ｄｏｓｔｏｅｖｓｋｉｉ，ＦｙｏｄｏｒＭｉｋｈａｉｌｏｖｉｃｈ 米川／正夫"
            ),
            vec![
                a("FyodorMikhailovich Dostoevskii", None),
                a("米川正夫", None)
            ]
        );
        assert_eq!(
            parse_authors(
                "レフ・ニコラエヴィチ・トルストイ 中村白葉 レフ・ニコラエヴィチ・トルストイ 中村白葉"
            ),
            vec![
                a("レフ・ニコラエヴィチ・トルストイ", None),
                a("中村白葉", None)
            ]
        );
        assert_eq!(parse_authors("谷‖耕平"), vec![a("谷耕平", None)]);
        assert_eq!(
            parse_authors("山田, 玲子 (料理)"),
            vec![a("山田玲子", None)]
        );
        assert_eq!(parse_authors(""), vec![]);
    }

    #[test]
    fn kana() {
        assert_eq!(
            clean_kana("ナツメ, ソウセキ, 1867-1916").as_deref(),
            Some("ナツメ ソウセキ")
        );
        assert_eq!(
            clean_kana("ヤマダ レイコ").as_deref(),
            Some("ヤマダ レイコ")
        );
        assert_eq!(
            clean_kana("ヨネカワ，マサオ").as_deref(),
            Some("ヨネカワ マサオ")
        );
        assert_eq!(clean_kana("Tolstoy, Leo"), None);
        assert_eq!(clean_kana(""), None);
        let mut authors = vec![
            a("夏目漱石", None),
            a("Dostoyevsky, Fyodor", None),
            a("山田玲子", Some("著")),
        ];
        attach_kana(
            &mut authors,
            &[
                (
                    "夏目, 漱石, 1867-1916".into(),
                    "ナツメ, ソウセキ, 1867-1916".into(),
                ),
                ("Dostoyevsky, Fyodor, 1821-1881".into(), "".into()),
                ("山田　玲子".into(), "ヤマダ レイコ".into()),
            ],
        );
        assert_eq!(authors[0].kana.as_deref(), Some("ナツメ ソウセキ"));
        assert_eq!(authors[1].kana, None);
        assert_eq!(authors[2].kana.as_deref(), Some("ヤマダ レイコ"));
    }

    // -----------------------------------------------------------------------
    // 英字の著者
    // -----------------------------------------------------------------------

    #[test]
    fn latin_person_name_drops_dates_and_puts_the_given_name_first() {
        for (raw, want) in [
            ("Martin, Robert C.", "Robert C. Martin"),
            ("Austen, Jane, 1775-1817", "Jane Austen"),
            ("Austen, Jane, 1775-1817.", "Jane Austen"),
            ("Martin, Robert C., 1952-", "Robert C. Martin"),
            ("Someone, Jane, -1817", "Jane Someone"),
            ("Someone, Jane, 1817", "Jane Someone"),
            ("Someone, Jane, 1775?-1817?", "Jane Someone"),
            ("Someone, Jane, b. 1950", "Jane Someone"),
            ("Someone, Jane, d. 1817", "Jane Someone"),
            ("Someone, Jane, ca. 1775-1817", "Jane Someone"),
            ("Someone, Jane, 1775 - 1817", "Jane Someone"),
            // 空白だけで続く生没年
            ("Someone, Jane 1775-1817", "Jane Someone"),
            // 姓, 名, 肩書き（Jr. など）は 名 姓 肩書き
            ("Smith, John, Jr.", "John Smith Jr."),
            ("Smith, John, Jr., 1950-", "John Smith Jr."),
            // 名前のイニシャル（D.）は生没年の記号ではない
            ("Martin, Robert D. 1952-", "Robert D. Martin"),
            ("Martin, Robert D.", "Robert D. Martin"),
            // コンマのない名前（団体名、すでに 名 姓 の順）はそのまま
            ("Penguin Books", "Penguin Books"),
            ("Robert C. Martin", "Robert C. Martin"),
            ("Homer", "Homer"),
            // 全角・余分な空白は clean_text と同じに整える
            ("Ｍａｒｔｉｎ，  Ｒｏｂｅｒｔ  Ｃ．", "Robert C. Martin"),
            ("  Coplien,James O.  ", "James O. Coplien"),
        ] {
            assert_eq!(latin_person_name(raw).as_deref(), Some(want), "{raw}");
        }
        assert_eq!(latin_person_name("  "), None);
        assert_eq!(latin_person_name(""), None);
    }

    #[test]
    fn clean_person_name_uses_the_latin_form_for_latin_names_only() {
        assert_eq!(
            clean_person_name("Tolstoy,Leo,1828-1910").as_deref(),
            Some("Leo Tolstoy")
        );
        assert_eq!(
            clean_person_name("Martin, Robert C.").as_deref(),
            Some("Robert C. Martin")
        );
        assert_eq!(
            clean_person_name("Martin,Robert C.,1952-").as_deref(),
            Some("Robert C. Martin")
        );
        // 日本語の名前は従来どおり（詰める）
        assert_eq!(
            clean_person_name("夏目, 漱石, 1867-1916").as_deref(),
            Some("夏目漱石")
        );
        // コンマのない英字（`/` や `‖` の区切りだけ）は並べ替えない
        assert_eq!(clean_person_name("AC/DC").as_deref(), Some("AC, DC"));
    }

    #[test]
    fn latin_role_keywords() {
        for (phrase, want) in [
            ("edited", Some("編")),
            ("Edited", Some("編")),
            ("editor", Some("編")),
            ("ed.", Some("編")),
            ("compiled", Some("編")),
            ("translated", Some("訳")),
            ("translator", Some("訳")),
            ("trans.", Some("訳")),
            ("illustrated", Some("絵")),
            ("illustrations", Some("絵")),
            ("illustrator", Some("絵")),
            ("foreword", Some("序文")),
            ("preface", Some("序文")),
            ("introduction", Some("解説")),
            ("afterword", Some("解説")),
            ("notes", Some("解説")),
            ("commentary", Some("解説")),
            ("adapted", Some("翻案")),
            ("written", Some("著")),
            // 最初に出てくる語を使う
            ("edited with an introduction and notes", Some("編")),
            (
                "with the original Penguin Classics introduction",
                Some("解説"),
            ),
            ("translated from the French", Some("訳")),
            ("with a foreword", Some("序文")),
            ("illustrated and edited", Some("絵")),
            // ` by ` の前に語がなければ（`by Jane Austen`）著
            ("", Some("著")),
            ("  ", Some("著")),
            // 知らない語は役割なし
            ("revised", None),
            ("photographs", None),
        ] {
            assert_eq!(latin_role_from_phrase(phrase), want, "{phrase:?}");
        }
        assert_eq!(latin_segment_role("Jane Austen"), Some("著"));
        assert_eq!(latin_segment_role("by Jane Austen"), Some("著"));
        assert_eq!(latin_segment_role("written by Jane Austen"), Some("著"));
        assert_eq!(latin_segment_role("foreword by X"), Some("序文"));
        assert_eq!(latin_segment_role("revised by X"), None);
        // `by` は語として探す（Ruby の by ではない）
        assert_eq!(latin_segment_role("Ruby Bybee"), Some("著"));
    }

    #[test]
    fn find_word_ci_respects_word_boundaries_and_case() {
        assert_eq!(find_word_ci("Robert C. Martin", "martin"), Some((10, 16)));
        assert_eq!(find_word_ci("Martinez", "Martin"), None);
        assert_eq!(find_word_ci("Dr. Martin-Smith", "Martin"), Some((4, 10)));
        assert_eq!(find_word_ci("edited by A", "BY"), Some((7, 9)));
        assert!(find_word_ci("Gabriel García Márquez", "garcía márquez").is_some());
        assert_eq!(find_word_ci("abc", ""), None);
    }

    #[test]
    fn latin_segments_are_not_split_on_whitespace() {
        // 1 人。`...`・`[et al.]` は除く
        assert_eq!(
            parse_authors("Robert C. Martin ... [et al.]"),
            vec![a("Robert C. Martin", None)]
        );
        assert_eq!(
            parse_authors("Robert C. Martin … et al."),
            vec![a("Robert C. Martin", None)]
        );
        assert_eq!(
            parse_authors("Robert C. Martin, et al."),
            vec![a("Robert C. Martin", None)]
        );
        assert_eq!(parse_authors("Gary S. Was"), vec![a("Gary S. Was", None)]);
        assert_eq!(parse_authors("Jane Austen"), vec![a("Jane Austen", None)]);
        // `by` の前の語が役割。` and `・` & ` で人を分ける
        assert_eq!(
            parse_authors("edited by Alice Smith and Bob Jones"),
            vec![a("Alice Smith", Some("編")), a("Bob Jones", Some("編"))]
        );
        assert_eq!(
            parse_authors("translated by Alice Smith & Bob Jones"),
            vec![a("Alice Smith", Some("訳")), a("Bob Jones", Some("訳"))]
        );
        assert_eq!(
            parse_authors("foreword by James O. Coplien"),
            vec![a("James O. Coplien", Some("序文"))]
        );
        assert_eq!(
            parse_authors("with an introduction by Tony Tanner"),
            vec![a("Tony Tanner", Some("解説"))]
        );
        assert_eq!(
            parse_authors("by Jane Austen"),
            vec![a("Jane Austen", Some("著"))]
        );
        // 知らない語は役割なし（名前に混ぜない）
        assert_eq!(
            parse_authors("revised by Alice Smith"),
            vec![a("Alice Smith", None)]
        );
        // `;` で区切った責任表示
        assert_eq!(
            parse_authors("Robert C. Martin ... [et al.] ; foreword by James O. Coplien"),
            vec![
                a("Robert C. Martin", None),
                a("James O. Coplien", Some("序文"))
            ]
        );
        assert_eq!(
            parse_authors("Jane Austen ; edited with an introduction and notes by Vivien Jones"),
            vec![a("Jane Austen", None), a("Vivien Jones", Some("編"))]
        );
        // 団体名・括弧
        assert_eq!(
            parse_authors("[Penguin Books]"),
            vec![a("Penguin Books", None)]
        );
        assert_eq!(parse_authors("AT&T Labs"), vec![a("AT&T Labs", None)]);
        // `姓, 名` はコンマのあとの空白を詰めて、名 姓 の順にする
        assert_eq!(
            parse_authors("Smith, John and Doe, Jane"),
            vec![a("John Smith", None), a("Jane Doe", None)]
        );
        // 英字の `姓,名,生没年` が空白で並ぶ（NDL 由来の openBD）。英字だけでも人ごとに分ける
        assert_eq!(
            parse_authors("Dostoyevsky,Fyodor,1821-1881 Tolstoy,Leo,1828-1910"),
            vec![a("Fyodor Dostoyevsky", None), a("Leo Tolstoy", None)]
        );
        assert_eq!(parse_authors("..."), vec![]);
        assert_eq!(parse_authors("[et al.]"), vec![]);
    }

    #[test]
    fn openbd_whitespace_split_rejoins_latin_names_with_spaces() {
        // 英字の名前が空白を含むと、空白で切れた語が続きになる（`C.,1952-`）
        assert_eq!(
            parse_authors("Martin,Robert C.,1952- 角,征典"),
            vec![a("Robert C. Martin", None), a("角征典", None)]
        );
        assert_eq!(
            parse_authors("Martin,Robert C. 角,征典"),
            vec![a("Robert C. Martin", None), a("角征典", None)]
        );
        assert_eq!(
            parse_authors("Dostoyevsky,Fyodor,1821-1881 江川,卓,1927-2001"),
            vec![a("Fyodor Dostoyevsky", None), a("江川卓", None)]
        );
        // 日本語の人が先でも、英字の人が続く
        assert_eq!(
            parse_authors("江川,卓,1927-2001 Dostoyevsky,Fyodor,1821-1881"),
            vec![a("江川卓", None), a("Fyodor Dostoyevsky", None)]
        );
        // 英字 2 人（どちらも `姓,名`）は続きにしない
        assert_eq!(
            parse_authors("Martin,Robert C.,1952- Fowler,Martin,1963- 角,征典"),
            vec![
                a("Robert C. Martin", None),
                a("Martin Fowler", None),
                a("角征典", None)
            ]
        );
        // 日本語の語は、前が英字でも続きにしない
        assert_eq!(
            parse_authors("Homer ホメロス"),
            vec![a("Homer", None), a("ホメロス", None)]
        );
    }

    #[test]
    fn continuation_rule() {
        for (prev, tok, want) in [
            ("Martin,Robert", "C.,1952-", true),
            ("Martin,Robert", "C.", true),
            ("Martin,Robert", "1952-", true),
            ("Martin,Robert", "Charles,1952-1960", true),
            ("Martin,Robert", "Fowler,Martin,1963-", false),
            ("Martin,Robert", "Fowler,Martin", false),
            ("Martin,Robert", "Jr.,Sr.", false),
            ("Martin,Robert", "角,征典", false),
            ("Martin,Robert", "ホメロス", false),
            ("角,征典", "C.", false),
        ] {
            assert_eq!(continues_latin_person(prev, tok), want, "{prev} / {tok}");
        }
    }

    #[test]
    fn japanese_translation_statements_keep_the_role_word_logic() {
        // 漢字・かなを含む責任表示は従来どおり。英字の名前と役割 `著` は 1 人のまま
        assert_eq!(
            parse_authors("Robert C. Martin 著 ; 花井志生 訳"),
            vec![a("Robert C. Martin", Some("著")), a("花井志生", Some("訳"))]
        );
        assert_eq!(
            parse_authors("Robert C.Martin著 ; 花井志生訳"),
            vec![a("Robert C.Martin", Some("著")), a("花井志生", Some("訳"))]
        );
        assert_eq!(
            parse_authors("ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ 著 ; 花井志生 訳"),
            vec![a("RobertC.Martin", Some("著")), a("花井志生", Some("訳"))]
        );
        assert_eq!(
            parse_authors("ドストエーフスキイ作 ; 米川正夫譯"),
            vec![
                a("ドストエーフスキイ", Some("作")),
                a("米川正夫", Some("訳"))
            ]
        );
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn latin_statement_authors_from_structured_names() {
        // Clean Code
        assert_eq!(
            parse_latin_statement_authors(
                "Robert C. Martin ... [et al.] ; foreword by James O. Coplien",
                &names(&["Martin, Robert C.", "Coplien, James O."])
            ),
            Some(vec![
                a("Robert C. Martin", Some("著")),
                a("James O. Coplien", Some("序文"))
            ])
        );
        // Pride and Prejudice（2003 年版）
        assert_eq!(
            parse_latin_statement_authors(
                "Jane Austen ; edited with an introduction and notes by Vivien Jones ; with the original Penguin Classics introduction by Tony Tanner",
                &names(&["Austen, Jane", "Jones, Vivien", "Tanner, Tony"])
            ),
            Some(vec![
                a("Jane Austen", Some("著")),
                a("Vivien Jones", Some("編")),
                a("Tony Tanner", Some("解説"))
            ])
        );
        // 同じ区切りの人は同じ役割（`edited by A and B`）。foaf の順を保つ。生没年は捨てる
        assert_eq!(
            parse_latin_statement_authors(
                "edited by Alice Smith and Bob Jones",
                &names(&["Jones, Bob, 1950-", "Smith, Alice, 1960-"])
            ),
            Some(vec![
                a("Bob Jones", Some("編")),
                a("Alice Smith", Some("編"))
            ])
        );
        // 責任表示にだけある人は無視する。責任表示のどこにもいない人は役割なし。知らない語も役割なし
        assert_eq!(
            parse_latin_statement_authors(
                "Jane Austen ; revised by Someone Else ; photographs by Pat Lee",
                &names(&["Austen, Jane", "Else, Someone", "Lee, Pat", "Nobody, Ann"])
            ),
            Some(vec![
                a("Jane Austen", Some("著")),
                a("Someone Else", None),
                a("Pat Lee", None),
                a("Ann Nobody", None)
            ])
        );
        // 姓は大文字小文字を区別せず、語の境界で探す（Austenite の中の Austen には当たらない）
        assert_eq!(
            parse_latin_statement_authors(
                "an Austenite ; translated by JANE AUSTEN",
                &names(&["Austen, Jane"])
            ),
            Some(vec![a("Jane Austen", Some("訳"))])
        );
        // 団体名（コンマなし）はそのまま
        assert_eq!(
            parse_latin_statement_authors(
                "Penguin Books ; illustrated by Pat Lee",
                &names(&["Penguin Books", "Lee, Pat"])
            ),
            Some(vec![
                a("Penguin Books", Some("著")),
                a("Pat Lee", Some("絵"))
            ])
        );
        // 姓, 名, Jr.
        assert_eq!(
            parse_latin_statement_authors("John Smith, Jr.", &names(&["Smith, John, Jr."])),
            Some(vec![a("John Smith Jr.", Some("著"))])
        );
        // 同じ人が 2 回あっても 1 人
        assert_eq!(
            parse_latin_statement_authors(
                "Jane Austen",
                &names(&["Austen, Jane", "Austen, Jane, 1775-1817"])
            ),
            Some(vec![a("Jane Austen", Some("著"))])
        );
    }

    #[test]
    fn latin_statement_authors_do_not_apply_to_japanese_or_empty_input() {
        let foaf = names(&["Martin, Robert C.", "花井, 志生"]);
        // 責任表示に漢字・かなを含む（日本語の本）
        assert_eq!(
            parse_latin_statement_authors("Robert C. Martin 著 ; 花井志生 訳", &foaf),
            None
        );
        assert_eq!(
            parse_latin_statement_authors("ドストエーフスキイ作 ; 米川正夫譯", &foaf),
            None
        );
        // 責任表示が空、構造化された名前がない
        assert_eq!(parse_latin_statement_authors("", &foaf), None);
        assert_eq!(parse_latin_statement_authors("   ", &foaf), None);
        assert_eq!(parse_latin_statement_authors("Jane Austen", &[]), None);
        assert_eq!(
            parse_latin_statement_authors("Jane Austen", &names(&["", "  "])),
            None
        );
    }

    #[test]
    fn kana_attaches_to_latin_names_in_natural_order() {
        let mut authors = vec![
            a("FyodorMikhailovich Dostoevskii", None),
            a("Leo Tolstoy", None),
        ];
        attach_kana(
            &mut authors,
            &[
                (
                    "Dostoevskii,Fyodor Mikhailovich".into(),
                    "ドストエフスキー，フョードル・ミハイロヴィチ".into(),
                ),
                ("Tolstoy, Leo, 1828-1910".into(), "".into()),
            ],
        );
        assert_eq!(
            authors[0].kana.as_deref(),
            Some("ドストエフスキー フョードル・ミハイロヴィチ")
        );
        assert_eq!(authors[1].kana, None);
    }
    #[test]
    fn latin_name_key_keeps_only_ascii_letters_and_digits() {
        for raw in [
            "Robert C. Martin",
            "RobertC.Martin",
            "ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ",
            "ROBERT C MARTIN",
            "Robert-C. Martin",
        ] {
            assert_eq!(latin_name_key(raw), "robertcmartin", "{raw}");
        }
        assert_eq!(latin_name_key("O'Brien, Tim 1946-"), "obrientim1946");
        assert_eq!(latin_name_key("花井志生"), "");
        assert_eq!(latin_name_key("Müller"), "mller");
    }

    #[test]
    fn latin_reference_names_are_latin_only_and_in_natural_order() {
        assert_eq!(
            latin_reference_names([
                "Martin, Robert C.",
                "花井, 志生",
                "ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ",
                "Dostoyevsky, Fyodor, 1821-1881",
                "  ",
                "KADOKAWA ",
            ]),
            vec![
                "Robert C. Martin",
                "RobertC.Martin",
                "Fyodor Dostoyevsky",
                "KADOKAWA"
            ]
        );
    }

    fn refs(list: &[&str]) -> Vec<String> {
        latin_reference_names(list)
    }

    fn restored(name: &str, role: Option<&str>, references: &[&str]) -> CandidateAuthor {
        let mut authors = vec![CandidateAuthor {
            name: name.into(),
            kana: Some("ヨミ".into()),
            role: role.map(Into::into),
        }];
        restore_latin_name_spacing(&mut authors, &refs(references));
        assert_eq!(authors.len(), 1);
        authors.remove(0)
    }

    #[test]
    fn restore_substitutes_the_structured_name_when_the_keys_match() {
        let got = restored("RobertC. Martin", Some("著"), &["Martin, Robert C."]);
        assert_eq!(got.name, "Robert C. Martin");
        assert_eq!(got.role.as_deref(), Some("著"), "役割はそのまま");
        assert_eq!(got.kana.as_deref(), Some("ヨミ"), "ヨミはそのまま");
        // 参照に生没年があっても、`名 姓` の形で、生没年は除く
        assert_eq!(
            restored(
                "FyodorMikhailovich Dostoevskii",
                None,
                &["Dostoevskii, Fyodor Mikhailovich, 1821-1881"]
            )
            .name,
            "Fyodor Mikhailovich Dostoevskii"
        );
        // 参照が複数あっても、照合キーの合うものだけが対象
        assert_eq!(
            restored(
                "RobertC.Martin",
                None,
                &["Coplien, James O.", "Martin, Robert C."]
            )
            .name,
            "Robert C. Martin"
        );
    }

    #[test]
    fn restore_prefers_the_reference_with_the_most_spaces() {
        let both = ["ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ", "Martin, Robert C."];
        assert_eq!(
            restored("RobertC.Martin", None, &both).name,
            "Robert C. Martin"
        );
        let reversed = ["Martin, Robert C.", "ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ"];
        assert_eq!(
            restored("RobertC.Martin", None, &reversed).name,
            "Robert C. Martin"
        );
    }

    #[test]
    fn restore_leaves_the_name_when_references_disagree_on_spelling() {
        // 照合キーは同じ（`robertcmartin`）だが、大文字小文字の綴りが違う参照が混ざる: どちらとも決められない
        let ambiguous = ["Martin, Robert C.", "MARTIN, ROBERT C."];
        assert_eq!(
            restored("Robert C Martin", None, &ambiguous).name,
            "Robert C Martin"
        );
        // 英字以外の文字（アクセント）の違いも、綴りの違い
        let ambiguous = ["Müller, Hans", "Mïller, Hans"];
        assert_eq!(restored("Hans Mller", None, &ambiguous).name, "Hans Mller");
        // 置き換えがなかった著者には、頭文字の補いが働く
        assert_eq!(
            restored(
                "RobertC.Martin",
                None,
                &["Martin, Robert C.", "MARTIN, ROBERT C."]
            )
            .name,
            "Robert C. Martin"
        );
    }

    #[test]
    fn restore_without_a_match_falls_back_to_the_initials_only() {
        assert_eq!(
            restored("RobertC. Martin", None, &[]).name,
            "Robert C. Martin"
        );
        assert_eq!(
            restored("RobertC.Martin", None, &["Coplien, James O."]).name,
            "Robert C. Martin"
        );
        // 同じ綴りの参照しかなくても（置き換えがなくても）、頭文字の補いは働く
        assert_eq!(
            restored("RobertC.Martin", None, &["ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ"]).name,
            "Robert C. Martin"
        );
        // 空白のある名前は、参照がなくても変わらない
        assert_eq!(
            restored("Robert C. Martin", None, &[]).name,
            "Robert C. Martin"
        );
        assert_eq!(
            restored("Robert C. Martin", None, &["Martin, Robert C."]).name,
            "Robert C. Martin"
        );
    }

    #[test]
    fn restore_never_touches_japanese_names_or_empty_keys() {
        assert_eq!(
            restored("花井志生", Some("訳"), &["花井, 志生", "Hanai, Shisei"]).name,
            "花井志生"
        );
        assert_eq!(
            restored("Ｃ．マーティン", None, &["Martin, C."]).name,
            "Ｃ．マーティン"
        );
        // 英字（ASCII）を含まない名前は照合しない
        assert_eq!(
            restored("Достоевский", None, &["Достоевский, Фёдор"]).name,
            "Достоевский"
        );
    }

    #[test]
    fn restore_merges_authors_that_become_identical() {
        let mut authors = vec![
            CandidateAuthor {
                name: "RobertC.Martin".into(),
                kana: None,
                role: Some("著".into()),
            },
            CandidateAuthor {
                name: "RobertC. Martin".into(),
                kana: None,
                role: Some("著".into()),
            },
            CandidateAuthor {
                name: "RobertC. Martin".into(),
                kana: None,
                role: Some("訳".into()),
            },
        ];
        restore_latin_name_spacing(&mut authors, &refs(&["Martin, Robert C."]));
        assert_eq!(
            authors,
            vec![
                a("Robert C. Martin", Some("著")),
                a("Robert C. Martin", Some("訳"))
            ]
        );
    }

    #[test]
    fn space_latin_initials_fixes_only_the_two_initial_patterns() {
        for (raw, want) in [
            ("RobertC.", "Robert C."),
            ("RobertC. Martin", "Robert C. Martin"),
            ("RobertC.Martin", "Robert C. Martin"),
            ("C.Martin", "C. Martin"),
            ("J.R.R.Tolkien", "J.R.R. Tolkien"),
            ("J.K.Rowling", "J.K. Rowling"),
            ("Ph.D.Smith", "Ph.D. Smith"),
            ("Robert D.Smith", "Robert D. Smith"),
            ("JohnR.Smith  ", "John R. Smith"),
            // 変えない
            ("McDonald", "McDonald"),
            ("DeMarco", "DeMarco"),
            ("Tom DeMarco", "Tom DeMarco"),
            ("MacArthur", "MacArthur"),
            ("St.John", "St.John"),
            ("O'Brien", "O'Brien"),
            ("J.R.R. Tolkien", "J.R.R. Tolkien"),
            ("Robert C. Martin", "Robert C. Martin"),
            ("Martin, Robert C.", "Martin, Robert C."),
            ("KADOKAWA", "KADOKAWA"),
            ("花井志生", "花井志生"),
        ] {
            assert_eq!(space_latin_initials(raw), want, "{raw}");
        }
        // 空白は 1 つにそろえる
        assert_eq!(
            space_latin_initials("Robert   C.  Martin"),
            "Robert C. Martin"
        );
    }
}
