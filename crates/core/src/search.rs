//! 書籍の検索（FR-LIST-03）。
//!
//! 検索語は `normalize_for_search` で正規化し、空白で区切る。区切った語はすべて満たす本（AND）を返す。
//! 語ごとに、書名・ヨミ・著者・出版社・シリーズ名・タグ・メモ・ISBN のどれかに含まれていればよい。
//!
//! - 3 文字以上の語は FTS5（trigram）の部分一致。
//! - 2 文字以下の語は、同じ列に対する LIKE（trigram は 3 文字未満を扱えないため）。
//! - ISBN は、ハイフンを除いて照合する。ISBN-10 は ISBN-13 に直して照合する。

use rusqlite::{Connection, params_from_iter};

use crate::error::Result;
use crate::normalize::{is_valid_isbn10, isbn10_to_isbn13, normalize_for_search};

/// FTS の列（検索対象）。`migrations/0002` の `books_fts` と同じ並び。
const FTS_COLUMNS: [&str; 8] = [
    "title",
    "title_kana",
    "authors",
    "publisher",
    "series",
    "tags",
    "memo",
    "isbn",
];

/// 検索語に一致する書籍の ID を、ID 順（登録順）で返す。ゴミ箱の本も含む。完全削除した本は含まない。
///
/// 検索語が空（空白だけ）なら、検索対象の全冊を返す。
pub fn search_books(conn: &Connection, query: &str) -> Result<Vec<String>> {
    let terms: Vec<String> = normalize_for_search(query)
        .split_whitespace()
        .map(interpret_term)
        .collect();

    let (long, short): (Vec<&String>, Vec<&String>) =
        terms.iter().partition(|t| t.chars().count() >= 3);

    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    if !long.is_empty() {
        // 語を二重引用符で囲んだフレーズにする（演算子や記号は文字として扱われる）。
        let expr = long
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        params.push(expr);
        conditions.push("books_fts MATCH ?1".to_owned());
    }
    for term in short {
        params.push(format!("%{}%", escape_like(term)));
        let n = params.len();
        let any_column = FTS_COLUMNS
            .iter()
            .map(|c| format!("books_fts.{c} LIKE ?{n} ESCAPE '\\'"))
            .collect::<Vec<_>>()
            .join(" OR ");
        conditions.push(format!("({any_column})"));
    }

    let sql = if conditions.is_empty() {
        "SELECT book_id FROM book_idx ORDER BY book_id".to_owned()
    } else {
        format!(
            "SELECT i.book_id FROM books_fts JOIN book_idx i ON i.idx = books_fts.rowid
             WHERE {} ORDER BY i.book_id",
            conditions.join(" AND ")
        )
    };
    let mut stmt = conn.prepare(&sql)?;
    let ids = stmt
        .query_map(params_from_iter(params.iter()), |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// 検索語を解釈する。ISBN の形（ハイフン入りを含む 10 桁・13 桁）なら、ハイフンを除いた
/// ISBN-13 にする。それ以外はそのまま。
fn interpret_term(term: &str) -> String {
    let looks_like_isbn = term.len() >= 10
        && term
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'-' || b == b'x');
    if looks_like_isbn {
        let digits: String = term.chars().filter(|c| *c != '-').collect();
        if digits.len() == 13 && digits.bytes().all(|b| b.is_ascii_digit()) {
            return digits;
        }
        if is_valid_isbn10(&digits)
            && let Some(isbn13) = isbn10_to_isbn13(&digits)
        {
            return isbn13;
        }
    }
    term.to_owned()
}

/// LIKE のパターンに入れる文字列の、`%` `_` `\` をエスケープする（ESCAPE '\' と対で使う）。
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbn_terms_are_interpreted() {
        assert_eq!(interpret_term("978-4-00-310111-7"), "9784003101117");
        assert_eq!(interpret_term("9784003101117"), "9784003101117");
        assert_eq!(interpret_term("4003101111"), "9784003101117");
        assert_eq!(interpret_term("4-00-310111-1"), "9784003101117");
        // ISBN-10 として正しくない 10 桁はそのまま
        assert_eq!(interpret_term("4003101112"), "4003101112");
        // ISBN の形でない語は触らない
        assert_eq!(interpret_term("2001-2010"), "2001-2010");
        assert_eq!(interpret_term("猫"), "猫");
    }

    #[test]
    fn like_escaping() {
        assert_eq!(escape_like("50%_off\\"), "50\\%\\_off\\\\");
    }
}
