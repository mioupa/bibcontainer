//! 解析・復号・推測・値の読み替えの単体テスト。DB を使うものは `tests/csv_import.rs`。

use serde_json::json;

use super::decode::decode;
use super::parse::{clean_cell, read_table};
use super::suggest::{UNMAPPED_EXPORT_HEADERS, suggest};
use super::values::*;
use super::*;
use crate::export::csv_headers;

const SJIS_SAMPLE: &str = "書名,著者\r\n①髙橋のかな漢字,山田\r\n";

fn sjis(s: &str) -> Vec<u8> {
    let (bytes, _, had_errors) = encoding_rs::SHIFT_JIS.encode(s);
    assert!(!had_errors);
    bytes.into_owned()
}

// ---- 文字コード ----

#[test]
fn decodes_utf8_with_and_without_bom() {
    let (t, e) = decode("書名\r\n".as_bytes(), None).unwrap();
    assert_eq!((t.as_str(), e), ("書名\r\n", CsvEncoding::Utf8));
    let mut with_bom = b"\xEF\xBB\xBF".to_vec();
    with_bom.extend_from_slice("書名\r\n".as_bytes());
    let (t, e) = decode(&with_bom, None).unwrap();
    assert_eq!((t.as_str(), e), ("書名\r\n", CsvEncoding::Utf8));
    // 指定しても BOM は取る
    let (t, _) = decode(&with_bom, Some(CsvEncoding::Utf8)).unwrap();
    assert_eq!(t, "書名\r\n");
}

#[test]
fn decodes_shift_jis_including_extended_characters() {
    let (t, e) = decode(&sjis(SJIS_SAMPLE), None).unwrap();
    assert_eq!(e, CsvEncoding::ShiftJis);
    assert_eq!(t, SJIS_SAMPLE);
    assert!(t.contains('①') && t.contains('髙'));
}

#[test]
fn auto_detection_errors_on_bytes_that_are_neither_utf8_nor_shift_jis() {
    // 0xFF は UTF-8 にも Shift_JIS にもない。0x81 の後の 0x20 は Shift_JIS の不正な 2 バイト目。
    let err = decode(b"a,b\r\n\xFF\x81 \r\n", None).unwrap_err();
    assert!(err.to_string().contains("文字コードを判定できませんでした"));
}

#[test]
fn forced_encoding_is_used_even_if_the_bytes_do_not_fit() {
    let (t, e) = decode(b"a,\xFF\r\n", Some(CsvEncoding::Utf8)).unwrap();
    assert_eq!(e, CsvEncoding::Utf8);
    assert!(t.contains('\u{FFFD}'));
    // UTF-8 の文章を Shift_JIS と指定すれば、化けるがエラーにはならない
    let (_, e) = decode("書名".as_bytes(), Some(CsvEncoding::ShiftJis)).unwrap();
    assert_eq!(e, CsvEncoding::ShiftJis);
    // 自動判定で UTF-8 になるものを、Shift_JIS に強制できる
    let (t, _) = decode(&sjis("かな"), Some(CsvEncoding::ShiftJis)).unwrap();
    assert_eq!(t, "かな");
}

#[test]
fn preview_reports_encoding_and_whether_it_was_detected() {
    let p = preview(&sjis(SJIS_SAMPLE), None, true).unwrap();
    assert_eq!(p.encoding, CsvEncoding::ShiftJis);
    assert!(p.auto_detected);
    let p = preview(&sjis(SJIS_SAMPLE), Some(CsvEncoding::ShiftJis), true).unwrap();
    assert!(!p.auto_detected);
    assert_eq!(p.columns[0].samples, ["①髙橋のかな漢字"]);
}

#[test]
fn rejects_input_over_the_size_limit() {
    let big = vec![b'a'; MAX_CSV_BYTES + 1];
    assert!(preview(&big, None, false).is_err());
}

// ---- 解析 ----

#[test]
fn parses_quotes_commas_newlines_and_escaped_quotes() {
    let text = "a,b,c\r\n1,\"x,y\",\"line1\nline2\"\r\n2,\"say \"\"hi\"\"\",z\r\n";
    let t = read_table(text, true).unwrap();
    assert_eq!(t.header.unwrap().cells, ["a", "b", "c"]);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[0].cells, ["1", "x,y", "line1\nline2"]);
    assert_eq!(t.rows[1].cells[1], "say \"hi\"");
    // 行番号は、レコードが始まる物理行（ヘッダーが 1 行目。引用符の中の改行も数える）
    assert_eq!(t.rows[0].line, 2);
    assert_eq!(t.rows[1].line, 4);
}

#[test]
fn accepts_rows_with_different_cell_counts_and_skips_empty_rows() {
    let text = "a,b,c\n1\n\n , ,\n1,2,3,4\n";
    let t = read_table(text, true).unwrap();
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.width, 4);
    assert_eq!(t.rows[0].cell(2), "", "足りない欄は空");
    assert_eq!(t.rows[1].line, 5, "空行のあとも物理行を数える");
}

#[test]
fn without_header_the_first_record_is_data() {
    let t = read_table("1,2\n3,4\n", false).unwrap();
    assert!(t.header.is_none());
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[0].line, 1);
}

#[test]
fn strips_the_export_formula_apostrophe_only_before_formula_prefixes() {
    assert_eq!(clean_cell("'=SUM(A1)"), "=SUM(A1)");
    assert_eq!(clean_cell("'-5"), "-5");
    assert_eq!(clean_cell("'+1"), "+1");
    assert_eq!(clean_cell("'@x"), "@x");
    assert_eq!(clean_cell("'abc"), "'abc");
    assert_eq!(clean_cell("  ' =1  "), "' =1");
    assert_eq!(clean_cell("  x  "), "x");
}

#[test]
fn preview_samples_are_the_first_three_non_empty_cleaned_values() {
    let csv = "書名,メモ\n'=a,\nb,\nc,m1\nd,m2\ne,m3\nf,m4\n";
    let p = preview(csv.as_bytes(), None, true).unwrap();
    assert_eq!(p.row_count, 6);
    assert_eq!(p.columns[0].header.as_deref(), Some("書名"));
    assert_eq!(p.columns[0].samples, ["=a", "b", "c"]);
    assert_eq!(p.columns[1].samples, ["m1", "m2", "m3"]);
    let p = preview(csv.as_bytes(), None, false).unwrap();
    assert_eq!(p.columns[0].header, None);
    assert!(p.suggested.is_empty());
    assert_eq!(p.row_count, 7);
}

// ---- 対応づけの推測 ----

fn suggested(header: &str) -> Vec<(u32, ImportTarget)> {
    let t = read_table(header, true).unwrap();
    suggest(&t.header.unwrap())
        .into_iter()
        .map(|m| (m.column, m.target))
        .collect()
}

#[test]
fn suggests_every_header_of_the_exporter() {
    let headers = csv_headers();
    let line = headers.join(",");
    let s = suggested(&line);
    for (i, h) in headers.iter().enumerate() {
        let col = u32::try_from(i).unwrap();
        let found = s.iter().find(|(c, _)| *c == col);
        if UNMAPPED_EXPORT_HEADERS.contains(h) {
            assert!(found.is_none(), "{h}");
        } else {
            assert!(found.is_some(), "書き出しの列 {h} が推測されない");
        }
    }
    // 取り込めない 2 列を除いて、全部が別々の項目
    assert_eq!(s.len(), headers.len() - UNMAPPED_EXPORT_HEADERS.len());
    assert!(s.contains(&(0, ImportTarget::Id)));
    assert!(s.contains(&(1, ImportTarget::Isbn13)));
    assert!(s.iter().any(|x| x.1 == ImportTarget::VolumeSort));
    assert!(s.iter().any(|x| x.1 == ImportTarget::WorkId));
    assert!(s.iter().any(|x| x.1 == ImportTarget::Trashed));
    assert!(s.iter().any(|x| x.1 == ImportTarget::CreatedAt));
    assert!(s.iter().any(|x| x.1 == ImportTarget::CCode));
}

#[test]
fn suggests_aliases_ignoring_width_case_and_spaces() {
    let s = suggested(
        " ISBN-13 ,タイトル,Author,出版者,ＩＳＢＮ,発売日,TAGS,定価,ページ,読了日,購入日,note,レビュー,評価\n",
    );
    use ImportTarget as T;
    // 同じ項目になる列は先が勝つ（ＩＳＢＮ は無視される）
    assert_eq!(
        s,
        vec![
            (0, T::Isbn13),
            (1, T::Title),
            (2, T::Authors),
            (3, T::Publisher),
            (5, T::PublishedOn),
            (6, T::Tags),
            (7, T::ListPrice),
            (8, T::Pages),
            (9, T::ReadFinishedOn),
            (10, T::PurchasedOn),
            (11, T::Memo),
            (12, T::Review),
            (13, T::Rating),
        ]
    );
    assert!(suggested("foo,bar\n").is_empty());
}

#[test]
fn rejects_a_target_mapped_twice_or_a_column_out_of_range() {
    let m = |column, target| ColumnMapping { column, target };
    let opts = |mapping| CsvImportOptions {
        encoding: None,
        has_header: true,
        mapping,
    };
    assert!(
        validate_options(
            &opts(vec![m(0, ImportTarget::Title), m(1, ImportTarget::Title)]),
            3
        )
        .is_err()
    );
    assert!(validate_options(&opts(vec![m(3, ImportTarget::Title)]), 3).is_err());
    assert!(
        validate_options(
            &opts(vec![m(2, ImportTarget::Title), m(2, ImportTarget::Memo)]),
            3
        )
        .is_ok()
    );
}

#[test]
fn mapping_targets_use_snake_case_names_in_json() {
    assert_eq!(
        serde_json::to_value(ImportTarget::CCode).unwrap(),
        json!("c_code")
    );
    assert_eq!(
        serde_json::to_value(ImportTarget::RefPriceIncl).unwrap(),
        json!("ref_price_incl")
    );
    assert_eq!(
        serde_json::to_value(CsvEncoding::ShiftJis).unwrap(),
        json!("shiftJis")
    );
    for t in ImportTarget::ALL {
        assert_eq!(serde_json::to_value(t).unwrap(), json!(t.key()));
    }
    let r = IssueReason::RegisteredIsbn {
        existing_title: "x".into(),
    };
    assert_eq!(
        serde_json::to_value(r).unwrap(),
        json!({"kind":"registeredIsbn","existingTitle":"x"})
    );
}

// ---- 値の読み替え ----

fn row_of(pairs: &[(ImportTarget, &str)]) -> ParsedRow {
    let cells: Vec<String> = pairs.iter().map(|(_, v)| (*v).to_owned()).collect();
    let mapping: Vec<(usize, ImportTarget)> = pairs
        .iter()
        .enumerate()
        .map(|(i, (t, _))| (i, *t))
        .collect();
    parse_row(&super::parse::RawRow { line: 2, cells }, &mapping)
}

fn field(row: &ParsedRow, name: &str) -> Option<serde_json::Value> {
    row.fields
        .iter()
        .find(|(f, _)| *f == name)
        .map(|(_, v)| v.clone())
}

fn unreadable(row: &ParsedRow) -> Vec<ImportTarget> {
    row.warnings
        .iter()
        .map(|w| match w {
            IssueReason::UnreadableValue { target, .. } => *target,
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn isbn_accepts_isbn13_isbn10_hyphens_and_full_width() {
    use ImportTarget::{Isbn13, Title};
    for cell in [
        "9784065192900",
        "978-4-06-519290-0",
        "９７８４０６５１９２９００",
        "4065192900",
        "4-06-519290-0",
    ] {
        let r = row_of(&[(Isbn13, cell), (Title, "t")]);
        assert!(r.error.is_none(), "{cell}");
        assert_eq!(r.isbn13.as_deref(), Some("9784065192900"), "{cell}");
    }
    let r = row_of(&[(Isbn13, "  "), (Title, "t")]);
    assert!(r.error.is_none());
    assert_eq!(r.isbn13, None);
}

#[test]
fn bad_isbn_is_an_error_and_excel_numbers_are_flagged() {
    use ImportTarget::{Isbn13, Title};
    for (cell, excel) in [
        ("9.78478E+12", true),
        ("9.78478e+12", true),
        ("9E+12", true),
        ("9784065192901", false),
        ("abc", false),
        ("1.5E-3", false),
        ("1920123456789", false),
    ] {
        let r = row_of(&[(Isbn13, cell), (Title, "t")]);
        assert_eq!(
            r.error,
            Some(IssueReason::InvalidIsbn {
                value: cell.to_owned(),
                excel_number: excel
            }),
            "{cell}"
        );
    }
    let text = IssueReason::InvalidIsbn {
        value: "9.78478E+12".into(),
        excel_number: true,
    }
    .describe_ja();
    assert_eq!(
        text,
        "ISBN を読めません（9.78478E+12。Excel で数値になり桁が落ちた可能性があります）"
    );
    assert_eq!(
        IssueReason::NoTitleOrIsbn.describe_ja(),
        "書名も ISBN もありません"
    );
}

#[test]
fn row_without_title_and_isbn_is_an_error() {
    use ImportTarget::{Memo, Title};
    let r = row_of(&[(Title, " "), (Memo, "m")]);
    assert_eq!(r.error, Some(IssueReason::NoTitleOrIsbn));
}

#[test]
fn authors_split_on_semicolons_and_japanese_commas_not_ascii_commas() {
    let a = parse_authors("夏目 漱石（著）; 山田太郎(訳)；佐藤、鈴木（編）\n高橋");
    assert_eq!(
        serde_json::Value::Array(a),
        json!([
            {"name": "夏目 漱石", "role": "著"},
            {"name": "山田太郎", "role": "訳"},
            {"name": "佐藤"},
            {"name": "鈴木", "role": "編"},
            {"name": "高橋"},
        ])
    );
    let a = parse_authors("Smith, John");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0]["name"], "Smith, John");
    assert!(parse_authors(" ; ；").is_empty());
    // 名前が空になる括弧だけの欄は、そのまま名前にする
    assert_eq!(parse_authors("（著）")[0]["name"], "（著）");
}

#[test]
fn tags_split_on_all_separators() {
    assert_eq!(
        parse_tags("a; b；c,d，e、f\ng ;; "),
        ["a", "b", "c", "d", "e", "f", "g"]
    );
}

#[test]
fn location_is_split_on_gt_with_nfkc_and_trimmed() {
    assert_eq!(
        parse_location("自宅 > 書斎＞ 3段目 >> "),
        ["自宅", "書斎", "3段目"]
    );
    assert_eq!(parse_location(" ＞ "), Vec::<String>::new());
}

#[test]
fn enums_accept_keys_and_japanese_labels() {
    use ImportTarget::*;
    let r = row_of(&[
        (Title, "t"),
        (Ownership, "欲しい本"),
        (WishPriority, "HIGH"),
        (ReadStatus, "読書中"),
        (PurchaseCond, " 中古 "),
    ]);
    assert_eq!(field(&r, "ownership"), Some(json!("wanted")));
    assert_eq!(field(&r, "wish_priority"), Some(json!("high")));
    assert_eq!(field(&r, "read_status"), Some(json!("reading")));
    assert_eq!(field(&r, "purchase_cond"), Some(json!("used")));
    let r = row_of(&[
        (Title, "t"),
        (Ownership, "owned"),
        (ReadStatus, "Paused"),
        (PurchaseCond, "new"),
        (WishPriority, "低"),
    ]);
    assert_eq!(field(&r, "ownership"), Some(json!("owned")));
    assert_eq!(field(&r, "read_status"), Some(json!("paused")));
    assert_eq!(field(&r, "purchase_cond"), Some(json!("new")));
    assert_eq!(field(&r, "wish_priority"), Some(json!("low")));
    let r = row_of(&[(Title, "t"), (Ownership, "持ってる"), (ReadStatus, "xx")]);
    assert_eq!(unreadable(&r), [Ownership, ReadStatus]);
    assert!(
        r.fields
            .iter()
            .all(|(f, _)| *f != "ownership" && *f != "read_status"),
        "読めない項目は取り込まない"
    );
    assert!(r.error.is_none(), "行は取り込む");
}

#[test]
fn dates_accept_many_forms_and_zero_pad() {
    for (input, want) in [
        ("2026", "2026"),
        ("2026-3", "2026-03"),
        ("2026-3-5", "2026-03-05"),
        ("2026/3/5", "2026-03-05"),
        ("2026.03.05", "2026-03-05"),
        ("2026年", "2026"),
        ("2026年3月", "2026-03"),
        ("2026年3月5日", "2026-03-05"),
        ("２０２６年３月５日", "2026-03-05"),
        ("2026-03-05 12:34:56", "2026-03-05"),
        ("2026-03-05T12:34:56Z", "2026-03-05"),
    ] {
        assert_eq!(parse_date(input).as_deref(), Some(want), "{input}");
    }
    for bad in [
        "26-3-5",
        "2026-3-5-1",
        "2026年3",
        "abc",
        "2026年3月5",
        "20260305",
    ] {
        assert_eq!(parse_date(bad), None, "{bad}");
    }
    use ImportTarget::*;
    // 形は読めても実在しない日付は、取り込まずに警告
    let r = row_of(&[
        (Title, "t"),
        (PublishedOn, "2026-02-30"),
        (ReadFinishedOn, "2026/2/28"),
    ]);
    assert_eq!(unreadable(&r), [PublishedOn]);
    assert_eq!(field(&r, "read_finished_on"), Some(json!("2026-02-28")));
}

#[test]
fn integers_accept_yen_commas_and_page_suffixes() {
    use ImportTarget::*;
    let r = row_of(&[
        (Title, "t"),
        (ListPrice, "¥1,200円"),
        (RefPriceIncl, "１，３２０ 円"),
        (Pages, "320ページ"),
        (PurchasePrice, "800.0"),
    ]);
    assert_eq!(field(&r, "list_price"), Some(json!(1200)));
    assert_eq!(field(&r, "ref_price_incl"), Some(json!(1320)));
    assert_eq!(field(&r, "pages"), Some(json!(320)));
    assert_eq!(field(&r, "purchase_price"), Some(json!(800)));
    for (cell, want) in [
        ("320p", Some(320)),
        ("320 P", Some(320)),
        ("320頁", Some(320)),
        ("0", Some(0)),
        ("-1", None),
        ("12.5", None),
        ("abc", None),
        ("99999999999", None),
        ("", None),
    ] {
        let r = row_of(&[(Title, "t"), (Pages, cell)]);
        match want {
            Some(n) => assert_eq!(field(&r, "pages"), Some(json!(n)), "{cell}"),
            None => assert!(field(&r, "pages").is_none(), "{cell}"),
        }
        if !cell.is_empty() && want.is_none() {
            assert_eq!(unreadable(&r), [Pages], "{cell}");
        }
    }
}

#[test]
fn rating_accepts_1_to_5_and_treats_zero_as_empty() {
    use ImportTarget::*;
    let r = row_of(&[(Title, "t"), (Rating, "4.0")]);
    assert_eq!(field(&r, "rating"), Some(json!(4)));
    let r = row_of(&[(Title, "t"), (Rating, "0")]);
    assert!(field(&r, "rating").is_none());
    assert!(r.warnings.is_empty());
    for bad in ["6", "★★★", "3.5", "-1"] {
        let r = row_of(&[(Title, "t"), (Rating, bad)]);
        assert_eq!(unreadable(&r), [Rating], "{bad}");
    }
}

#[test]
fn volume_sort_work_id_and_id() {
    use ImportTarget::*;
    let r = row_of(&[(Title, "t"), (VolumeSort, "２.5")]);
    assert_eq!(field(&r, "volume_sort"), Some(json!(2.5)));
    let r = row_of(&[(Title, "t"), (VolumeSort, "inf")]);
    assert_eq!(unreadable(&r), [VolumeSort]);
    let id = "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55";
    let r = row_of(&[(Title, "t"), (WorkId, "まとめない"), (Id, id)]);
    assert_eq!(field(&r, "work_id"), Some(json!("none")));
    assert_eq!(r.id.as_deref(), Some(id));
    let r = row_of(&[(Title, "t"), (WorkId, "NONE")]);
    assert_eq!(field(&r, "work_id"), Some(json!("none")));
    let r = row_of(&[(Title, "t"), (WorkId, id)]);
    assert_eq!(field(&r, "work_id"), Some(json!(id)));
    let r = row_of(&[(Title, "t"), (WorkId, "abc"), (Id, "xyz")]);
    assert_eq!(unreadable(&r), [WorkId, Id]);
    assert_eq!(r.id, None);
}

#[test]
fn trashed_column_values() {
    use ImportTarget::*;
    for yes in ["はい", "TRUE", "1", "Yes", "y", "○", "✓"] {
        assert!(row_of(&[(Title, "t"), (Trashed, yes)]).trashed, "{yes}");
    }
    for no in ["いいえ", "false", "0", "no", ""] {
        let r = row_of(&[(Title, "t"), (Trashed, no)]);
        assert!(!r.trashed && r.warnings.is_empty(), "{no:?}");
    }
    let r = row_of(&[(Title, "t"), (Trashed, "たぶん")]);
    assert!(!r.trashed);
    assert_eq!(unreadable(&r), [Trashed]);
}

#[test]
fn created_at_formats() {
    use chrono::{Local, TimeZone, Utc};
    assert_eq!(
        parse_created_at("2026-11-14T03:04:05.123Z").as_deref(),
        Some("2026-11-14T03:04:05.123Z")
    );
    assert_eq!(
        parse_created_at("2026-11-14T12:04:05+09:00").as_deref(),
        Some("2026-11-14T03:04:05.000Z")
    );
    assert_eq!(
        parse_created_at("2026/11/14T03:04:05Z").as_deref(),
        Some("2026-11-14T03:04:05.000Z")
    );
    // ゾーンなしはこの PC のローカル時刻
    let local = |y, mo, d, h, mi, s| {
        Local
            .with_ymd_and_hms(y, mo, d, h, mi, s)
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string()
    };
    assert_eq!(
        parse_created_at("2026-11-14 03:04:05").unwrap(),
        local(2026, 11, 14, 3, 4, 5)
    );
    assert_eq!(
        parse_created_at("2026/11/14 03:04").unwrap(),
        local(2026, 11, 14, 3, 4, 0)
    );
    assert_eq!(
        parse_created_at("2026-11-14").unwrap(),
        local(2026, 11, 14, 0, 0, 0)
    );
    for bad in ["", "yesterday", "2026-13-01", "2026-11-14T25:00"] {
        assert_eq!(parse_created_at(bad), None, "{bad}");
    }
}

#[test]
fn plain_text_fields_are_trimmed_and_empty_ones_are_not_set() {
    use ImportTarget::*;
    let r = row_of(&[
        (Title, "  書名 "),
        (Publisher, " 講談社 "),
        (Memo, "  "),
        (CCode, "0093"),
        (Ndc, "913"),
    ]);
    assert_eq!(r.title.as_deref(), Some("書名"));
    assert_eq!(field(&r, "publisher"), Some(json!("講談社")));
    assert!(field(&r, "memo").is_none());
    assert_eq!(field(&r, "c_code"), Some(json!("0093")));
    let r = row_of(&[(Title, "t"), (CCode, "12")]);
    assert_eq!(unreadable(&r), [CCode], "canonical の検証も通す");
}

#[test]
fn crlf_and_lf_files_give_the_same_cells_and_physical_lines() {
    for text in [
        "a,b\r\n1,\"x\ny\"\r\n\r\n3,4\r\n",
        "a,b\n1,\"x\ny\"\n\n3,4\n",
        "a,b\r\n1,\"x\ny\"\r\n\r\n3,4",
    ] {
        let t = read_table(text, true).unwrap();
        assert_eq!(t.header.as_ref().unwrap().cells, ["a", "b"]);
        assert_eq!(t.rows[0].cells, ["1", "x\ny"]);
        assert_eq!(t.rows[1].cells, ["3", "4"]);
        let lines: Vec<u32> = t.rows.iter().map(|r| r.line).collect();
        assert_eq!(lines, [2, 5], "{text:?}");
    }
}
