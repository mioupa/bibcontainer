//! バーコードの判定（FR-SCAN-01〜04）、ISBN の変換、Cコード表（付録 A）のテスト。
//! 付録 B のテスト用データと、受け入れ基準 AT-01〜04 を含む。

use bibcontainer_core::barcode::{
    CCodeTable, ScanCode, ScanCodeInvalidReason, c_code_table, classify, describe_c_code,
    ean13_check_digit, is_valid_ean8, is_valid_ean13, is_valid_isbn10, is_valid_isbn13,
    isbn10_to_13, isbn13_to_10,
};
use proptest::prelude::*;
use serde_json::json;

fn isbn13_code(raw: &str, isbn13: &str, isbn10: Option<&str>) -> ScanCode {
    ScanCode::Isbn13 {
        raw: raw.into(),
        isbn13: isbn13.into(),
        isbn10: isbn10.map(Into::into),
    }
}

fn second_tier(raw: &str, c_code: &str, list_price: Option<u32>) -> ScanCode {
    ScanCode::SecondTier {
        raw: raw.into(),
        c_code: c_code.into(),
        list_price,
    }
}

fn invalid(raw: &str, reason: ScanCodeInvalidReason) -> ScanCode {
    ScanCode::Invalid {
        raw: raw.into(),
        reason,
    }
}

fn format_error(raw: &str) -> ScanCode {
    invalid(raw, ScanCodeInvalidReason::Format)
}

fn checksum_error(raw: &str) -> ScanCode {
    invalid(raw, ScanCodeInvalidReason::Checksum)
}

/// 12 桁の本体にチェックデジットを付けて EAN-13 にする。
fn ean13(body12: &str) -> String {
    let d = ean13_check_digit(body12).expect("12 桁の数字");
    format!("{body12}{d}")
}

// ---- 付録 B と受け入れ基準 ----

#[test]
fn appendix_b_isbn13() {
    assert_eq!(
        classify("9784003101117"),
        isbn13_code("9784003101117", "9784003101117", Some("4003101111"))
    );
    assert_eq!(
        classify("9784780802047"),
        isbn13_code("9784780802047", "9784780802047", Some("4780802040"))
    );
    assert_eq!(
        classify("9784101001012"),
        isbn13_code("9784101001012", "9784101001012", Some("4101001014"))
    );
}

#[test]
fn appendix_b_isbn10_is_converted_to_isbn13() {
    assert_eq!(
        classify("4003101111"),
        isbn13_code("4003101111", "9784003101117", Some("4003101111"))
    );
}

#[test]
fn appendix_b_second_tier() {
    assert_eq!(
        classify("1920193007008"),
        second_tier("1920193007008", "0193", Some(700))
    );
    assert_eq!(
        classify("1920077013002"),
        second_tier("1920077013002", "0077", Some(1300))
    );
    assert_eq!(
        classify("1920041028001"),
        second_tier("1920041028001", "0041", Some(2800))
    );
    assert_eq!(
        classify("1923055120006"),
        second_tier("1923055120006", "3055", Some(12000))
    );
    assert_eq!(
        classify("1920193000009"),
        second_tier("1920193000009", "0193", None)
    );
    assert_eq!(classify("1920193007009"), checksum_error("1920193007009"));
}

#[test]
fn at_01_isbn13_with_isbn10() {
    let ScanCode::Isbn13 { isbn13, isbn10, .. } = classify("9784003101117") else {
        panic!("ISBN-13 と判定されるはず");
    };
    assert_eq!(isbn13, "9784003101117");
    assert_eq!(isbn10.as_deref(), Some("4003101111"));
}

#[test]
fn at_02_second_tier_c_code_and_price() {
    let ScanCode::SecondTier {
        c_code, list_price, ..
    } = classify("1920193007008")
    else {
        panic!("2段目と判定されるはず");
    };
    assert_eq!(c_code, "0193");
    assert_eq!(list_price, Some(700));

    // 0193 = 一般・文庫・日本文学、小説・物語
    let info = describe_c_code(&c_code).expect("4 桁");
    assert_eq!(info.audience, "一般");
    assert_eq!(info.format, "文庫");
    assert_eq!(info.content_major, "文学");
    assert_eq!(info.content, "日本文学、小説・物語");
    assert!(info.content_defined);
}

#[test]
fn at_03_second_tier_checksum_mismatch_is_rejected() {
    assert_eq!(
        classify("1920193007009"),
        invalid("1920193007009", ScanCodeInvalidReason::Checksum)
    );
}

#[test]
fn at_04_second_tier_zero_price_is_unknown() {
    let ScanCode::SecondTier { list_price, .. } = classify("1920193000009") else {
        panic!("2段目と判定されるはず");
    };
    assert_eq!(list_price, None);
}

#[test]
fn appendix_b_c_code_names() {
    // 付録 B の注記: 0077 = 一般・単行本・家事、0041 = 一般・単行本・数学、3055 = 専門・単行本・電子通信
    let cases = [
        ("0077", "一般", "単行本", "家事"),
        ("0041", "一般", "単行本", "数学"),
        ("3055", "専門", "単行本", "電子通信"),
    ];
    for (code, audience, format, content) in cases {
        let info = describe_c_code(code).expect(code);
        assert_eq!(info.audience, audience, "{code}");
        assert_eq!(info.format, format, "{code}");
        assert_eq!(info.content, content, "{code}");
    }
}

// ---- 判定の分岐（FR-SCAN-01） ----

#[test]
fn isbn13_979_has_no_isbn10() {
    assert_eq!(
        classify("9791090636071"),
        isbn13_code("9791090636071", "9791090636071", None)
    );
}

#[test]
fn magazine_code_is_recognized() {
    let code = ean13("491012345678");
    assert_eq!(code, "4910123456782");
    assert_eq!(classify(&code), ScanCode::Magazine { raw: code.clone() });
}

#[test]
fn other_jan_13_digits_and_8_digits() {
    assert_eq!(
        classify("4901234567894"),
        ScanCode::OtherJan {
            raw: "4901234567894".into()
        }
    );
    assert_eq!(
        classify("96385074"),
        ScanCode::OtherJan {
            raw: "96385074".into()
        }
    );
    // 書籍の接頭辞に近い 190・193 や、雑誌に近い 490・492 は一般 JAN
    for body in [
        "190000000000",
        "193000000000",
        "490000000000",
        "492000000000",
    ] {
        let code = ean13(body);
        assert!(
            matches!(classify(&code), ScanCode::OtherJan { .. }),
            "{code}"
        );
    }
}

#[test]
fn checksum_is_verified_for_every_kind() {
    // EAN-13（ISBN-13・2段目・雑誌・一般 JAN）、EAN-8、ISBN-10
    for bad in [
        "9784003101118",
        "1920193007009",
        "4910123456783",
        "4901234567895",
        "96385075",
        "4003101112",
        "400310111X",
    ] {
        assert_eq!(
            classify(bad),
            checksum_error(bad),
            "{bad} はチェックデジット不一致"
        );
    }
}

#[test]
fn second_tier_price_boundaries() {
    let cases = [
        ("192019300000", "0193", None),
        ("192019300001", "0193", Some(1)),
        ("192019309999", "0193", Some(9999)),
        ("192019399999", "0193", Some(99999)),
        // Cコードの先頭の 0 は消さない
        ("192000000100", "0000", Some(100)),
    ];
    for (body, c_code, price) in cases {
        let code = ean13(body);
        assert_eq!(classify(&code), second_tier(&code, c_code, price), "{code}");
    }
}

// ---- 前処理と境界（FR-SCAN-01、FR-SCAN-08 の正規化） ----

#[test]
fn empty_and_blank_are_format_errors() {
    assert_eq!(classify(""), format_error(""));
    assert_eq!(classify("   "), format_error(""));
    assert_eq!(classify("\r\n"), format_error(""));
    assert_eq!(classify("-"), format_error(""));
}

#[test]
fn wrong_lengths_are_format_errors() {
    for s in [
        "1",
        "1234567",        // 7 桁
        "400310111",      // 9 桁
        "40031011111",    // 11 桁
        "978400310111",   // 12 桁
        "97840031011170", // 14 桁
        "0123456789012345",
    ] {
        assert_eq!(classify(s), format_error(s), "{s}");
    }
}

#[test]
fn letters_make_a_format_error() {
    for s in [
        "abc",
        "978400310111a",
        "ISBN9784003101117",
        "ISBN 978-4-00-310111-7",
        "978400310111X", // 13 桁の末尾の X は認めない
        "9784003X01117",
        "08044295X7", // X は末尾だけ
        "08x442957X",
        "4003101111A",
    ] {
        assert!(
            matches!(
                classify(s),
                ScanCode::Invalid {
                    reason: ScanCodeInvalidReason::Format,
                    ..
                }
            ),
            "{s}"
        );
    }
}

#[test]
fn surrounding_whitespace_is_ignored() {
    let expected = isbn13_code("9784003101117", "9784003101117", Some("4003101111"));
    assert_eq!(classify(" 9784003101117"), expected);
    assert_eq!(classify("9784003101117\n"), expected);
    assert_eq!(classify("\t9784003101117 \r\n"), expected);
    assert_eq!(classify("\u{3000}9784003101117\u{3000}"), expected);
}

#[test]
fn fullwidth_digits_are_accepted() {
    assert_eq!(
        classify("９７８４００３１０１１１７"),
        isbn13_code("9784003101117", "9784003101117", Some("4003101111"))
    );
    assert_eq!(
        classify("１９２０１９３００７００８"),
        second_tier("1920193007008", "0193", Some(700))
    );
}

#[test]
fn hyphenated_isbn_is_accepted() {
    let expected = isbn13_code("9784003101117", "9784003101117", Some("4003101111"));
    assert_eq!(classify("978-4-00-310111-7"), expected);
    assert_eq!(classify("978 4 00 310111 7"), expected);
    assert_eq!(classify("９７８－４－００－３１０１１１－７"), expected);
    // 日本語入力がオンのとき `-` が長音符になる
    assert_eq!(classify("978ー4ー00ー310111ー7"), expected);
    assert_eq!(
        classify("4-00-310111-1"),
        isbn13_code("4003101111", "9784003101117", Some("4003101111"))
    );
}

#[test]
fn isbn10_with_trailing_x() {
    let expected = isbn13_code("080442957X", "9780804429573", Some("080442957X"));
    assert_eq!(classify("080442957X"), expected);
    assert_eq!(classify("080442957x"), expected);
    assert_eq!(classify("080442957Ｘ"), expected);
    assert_eq!(classify("080442957ｘ"), expected);
    assert_eq!(classify("0-8044-2957-X"), expected);
}

#[test]
fn isbn10_whose_check_digit_is_wrong_is_a_checksum_error() {
    // 正しいチェックデジットは 1（X ではない）。X を受け付けない
    assert_eq!(classify("400310111X"), checksum_error("400310111X"));
    // 正しいチェックデジットは X（0 ではない）
    assert_eq!(classify("0804429570"), checksum_error("0804429570"));
}

#[test]
fn classify_is_pure_and_repeatable() {
    assert_eq!(classify("9784003101117"), classify("9784003101117"));
}

// ---- ISBN の変換と検証（FR-SCAN-04） ----

#[test]
fn isbn_conversions_round_trip() {
    for (isbn10, isbn13) in [
        ("4003101111", "9784003101117"),
        ("4780802040", "9784780802047"),
        ("4101001014", "9784101001012"),
        ("080442957X", "9780804429573"),
    ] {
        assert_eq!(isbn10_to_13(isbn10).as_deref(), Some(isbn13));
        assert_eq!(isbn13_to_10(isbn13).as_deref(), Some(isbn10));
    }
}

#[test]
fn isbn13_starting_with_979_has_no_isbn10() {
    assert_eq!(isbn13_to_10("9791090636071"), None);
    assert!(is_valid_isbn13("9791090636071"));
}

#[test]
fn conversions_accept_formatted_input_and_return_plain_digits() {
    assert_eq!(
        isbn10_to_13("4-00-310111-1").as_deref(),
        Some("9784003101117")
    );
    assert_eq!(
        isbn13_to_10("978-4-00-310111-7").as_deref(),
        Some("4003101111")
    );
    assert_eq!(
        isbn10_to_13("４００３１０１１１１").as_deref(),
        Some("9784003101117")
    );
}

#[test]
fn conversions_reject_invalid_input() {
    assert_eq!(isbn10_to_13("4003101112"), None);
    assert_eq!(isbn10_to_13("400310111"), None);
    assert_eq!(isbn10_to_13("9784003101117"), None);
    assert_eq!(isbn13_to_10("9784003101118"), None);
    assert_eq!(isbn13_to_10("4003101111"), None);
    assert_eq!(isbn13_to_10("1920193007008"), None);
}

#[test]
fn validators() {
    assert!(is_valid_isbn13("978-4-00-310111-7"));
    assert!(!is_valid_isbn13("9784003101118"));
    // 2段目は EAN-13 としては正しいが ISBN-13 ではない
    assert!(is_valid_ean13("1920193007008"));
    assert!(!is_valid_isbn13("1920193007008"));
    assert!(is_valid_isbn10("4-00-310111-1"));
    assert!(is_valid_isbn10("080442957x"));
    assert!(!is_valid_isbn10("4003101112"));
    assert!(is_valid_ean8("96385074"));
    assert!(!is_valid_ean8("96385075"));
}

// ---- Cコード表（付録 A、FR-SCAN-03） ----

#[test]
fn undefined_content_code_keeps_the_major_category() {
    // 03 は表にない。大分類（0 = 総記）の名称は返す
    let info = describe_c_code("0003").expect("4 桁");
    assert_eq!(info.content, "未定義（03）");
    assert!(!info.content_defined);
    assert_eq!(info.content_major, "総記");
    assert_eq!(info.audience, "一般");
    assert_eq!(info.format, "単行本");

    // 94 は表にない（9 = 文学）
    let info = describe_c_code("0194").expect("4 桁");
    assert_eq!(info.content, "未定義（94）");
    assert_eq!(info.content_major, "文学");
    assert_eq!(info.format, "文庫");
}

#[test]
fn all_digits_of_the_first_two_positions_have_names() {
    let table = c_code_table();
    assert_eq!(table.audience.len(), 10);
    assert_eq!(table.format.len(), 10);
    assert_eq!(table.content_major.len(), 10);
    for (i, group) in [&table.audience, &table.format, &table.content_major]
        .into_iter()
        .enumerate()
    {
        for (d, entry) in group.iter().enumerate() {
            assert_eq!(entry.code, d.to_string(), "グループ {i}");
            assert!(!entry.name.is_empty(), "グループ {i} の {d}");
        }
    }
}

#[test]
fn content_table_covers_appendix_a() {
    // 付録 A の「第 3・4 桁: 内容」に載っているコードを、大分類ごとに数える
    let expected_counts = [4, 6, 6, 8, 7, 9, 5, 10, 6, 7];
    let table = c_code_table();
    assert_eq!(table.content.len(), expected_counts.iter().sum::<usize>());
    for (major, &count) in expected_counts.iter().enumerate() {
        let prefix = major.to_string();
        let n = table
            .content
            .iter()
            .filter(|e| e.code.starts_with(&prefix))
            .count();
        assert_eq!(n, count, "大分類 {major}");
    }
    for entry in &table.content {
        assert_eq!(entry.code.len(), 2);
        assert!(!entry.name.is_empty());
    }
}

#[test]
fn table_and_describe_agree_for_all_codes() {
    let table = c_code_table();
    let name_of = |group: &[bibcontainer_core::barcode::CCodeEntry], code: &str| {
        group
            .iter()
            .find(|e| e.code == code)
            .map(|e| e.name.clone())
    };
    for n in 0..10_000u32 {
        let code = format!("{n:04}");
        let info = describe_c_code(&code).expect(&code);
        assert_eq!(Some(info.audience), name_of(&table.audience, &code[0..1]));
        assert_eq!(Some(info.format), name_of(&table.format, &code[1..2]));
        assert_eq!(
            Some(info.content_major),
            name_of(&table.content_major, &code[2..3])
        );
        match name_of(&table.content, &code[2..4]) {
            Some(name) => {
                assert!(info.content_defined, "{code}");
                assert_eq!(info.content, name);
            }
            None => {
                assert!(!info.content_defined, "{code}");
                assert_eq!(info.content, format!("未定義（{}）", &code[2..4]));
            }
        }
    }
}

#[test]
fn describe_rejects_malformed_codes() {
    for s in ["", "0", "019", "01930", "01a3", "０１９３", "C019"] {
        assert_eq!(describe_c_code(s), None, "{s:?}");
    }
}

// ---- IPC の JSON の形（0005 の 5） ----

#[test]
fn scan_code_json_shape() {
    let to_json = |s: &str| serde_json::to_value(classify(s)).expect("直列化");
    assert_eq!(
        to_json("9784003101117"),
        json!({"kind": "isbn13", "raw": "9784003101117", "isbn13": "9784003101117", "isbn10": "4003101111"})
    );
    assert_eq!(
        to_json("9791090636071"),
        json!({"kind": "isbn13", "raw": "9791090636071", "isbn13": "9791090636071", "isbn10": null})
    );
    assert_eq!(
        to_json("1920193007008"),
        json!({"kind": "secondTier", "raw": "1920193007008", "cCode": "0193", "listPrice": 700})
    );
    assert_eq!(
        to_json("1920193000009"),
        json!({"kind": "secondTier", "raw": "1920193000009", "cCode": "0193", "listPrice": null})
    );
    assert_eq!(
        to_json("4910123456782"),
        json!({"kind": "magazine", "raw": "4910123456782"})
    );
    assert_eq!(
        to_json("4901234567894"),
        json!({"kind": "otherJan", "raw": "4901234567894"})
    );
    assert_eq!(
        to_json("1920193007009"),
        json!({"kind": "invalid", "raw": "1920193007009", "reason": "checksum"})
    );
    assert_eq!(
        to_json("abc"),
        json!({"kind": "invalid", "raw": "abc", "reason": "format"})
    );
}

#[test]
fn c_code_json_shape() {
    let info = serde_json::to_value(describe_c_code("0003").expect("4 桁")).expect("直列化");
    assert_eq!(
        info,
        json!({
            "code": "0003",
            "audience": "一般",
            "format": "単行本",
            "contentMajor": "総記",
            "content": "未定義（03）",
            "contentDefined": false,
        })
    );
    let table: CCodeTable = c_code_table();
    let value = serde_json::to_value(&table).expect("直列化");
    let object = value.as_object().expect("オブジェクト");
    let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["audience", "content", "contentMajor", "format"]);
    assert_eq!(value["audience"][0], json!({"code": "0", "name": "一般"}));
}

// ---- 性質テスト ----

fn digits(n: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(0u8..10, n)
        .prop_map(|v| v.into_iter().map(|d| char::from(b'0' + d)).collect())
}

/// 数字を全角にし、途中にハイフンと空白を入れる（判定結果は変わらないはず）。
fn decorate(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push_str(if i % 2 == 0 { "－" } else { " " });
        }
        out.push(match c {
            '0'..='9' => char::from_u32(u32::from(c) + 0xFEE0).expect("全角数字"),
            other => other,
        });
    }
    out
}

proptest! {
    #[test]
    fn classify_never_panics(s in ".*") {
        let _ = classify(&s);
    }

    #[test]
    fn classify_never_panics_on_digitish_input(s in "[0-9０-９xXｘＸ\\- \\u{3000}ー]{0,16}") {
        let _ = classify(&s);
    }

    #[test]
    fn valid_ean13_is_never_invalid_and_kind_follows_prefix(body in digits(12)) {
        let code = ean13(&body);
        let result = classify(&code);
        let kind_matches = match &body[..3] {
            "978" | "979" => matches!(result, ScanCode::Isbn13 { .. }),
            "192" => matches!(result, ScanCode::SecondTier { .. }),
            "491" => matches!(result, ScanCode::Magazine { .. }),
            _ => matches!(result, ScanCode::OtherJan { .. }),
        };
        prop_assert!(kind_matches, "{} -> {:?}", code, result);
    }

    #[test]
    fn single_digit_error_in_ean13_is_detected(body in digits(12), pos in 0usize..13, delta in 1u8..10) {
        let code = ean13(&body);
        let mut bytes = code.into_bytes();
        bytes[pos] = b'0' + (bytes[pos] - b'0' + delta) % 10;
        let broken = String::from_utf8(bytes).expect("ASCII");
        prop_assert_eq!(classify(&broken), checksum_error(&broken));
    }

    #[test]
    fn second_tier_decomposes_c_code_and_price(c_code in digits(4), price in digits(5)) {
        let code = ean13(&format!("192{c_code}{price}"));
        let expected_price = match price.parse::<u32>().expect("数字") {
            0 => None,
            p => Some(p),
        };
        prop_assert_eq!(classify(&code), second_tier(&code, &c_code, expected_price));
    }

    #[test]
    fn isbn10_round_trips_through_isbn13(body in digits(9)) {
        let isbn10 = {
            let check = bibcontainer_core::barcode::isbn10_check_digit(&body).expect("9 桁");
            format!("{body}{check}")
        };
        prop_assert!(is_valid_isbn10(&isbn10));
        let isbn13 = isbn10_to_13(&isbn10).expect("変換できる");
        prop_assert!(is_valid_isbn13(&isbn13));
        prop_assert!(isbn13.starts_with("978"));
        prop_assert_eq!(isbn13_to_10(&isbn13), Some(isbn10.clone()));
        prop_assert_eq!(
            classify(&isbn10),
            isbn13_code(&isbn10, &isbn13, Some(&isbn10))
        );
        // ISBN-13 で入力しても同じ本になる
        prop_assert_eq!(
            classify(&isbn13),
            isbn13_code(&isbn13, &isbn13, Some(&isbn10))
        );
    }

    #[test]
    fn single_digit_error_in_isbn10_is_detected(body in digits(9), pos in 0usize..9, delta in 1u8..10) {
        let check = bibcontainer_core::barcode::isbn10_check_digit(&body).expect("9 桁");
        let mut chars: Vec<char> = format!("{body}{check}").chars().collect();
        let d = chars[pos].to_digit(10).expect("数字");
        chars[pos] = char::from_digit((d + u32::from(delta)) % 10, 10).expect("数字");
        let broken: String = chars.into_iter().collect();
        prop_assert!(!is_valid_isbn10(&broken));
        prop_assert_eq!(classify(&broken), checksum_error(&broken));
    }

    #[test]
    fn decoration_does_not_change_the_result(body in digits(12)) {
        let code = ean13(&body);
        prop_assert_eq!(classify(&decorate(&code)), classify(&code));
    }

    #[test]
    fn decoration_does_not_change_isbn10_result(body in digits(9)) {
        let check = bibcontainer_core::barcode::isbn10_check_digit(&body).expect("9 桁");
        let isbn10 = format!("{body}{check}");
        prop_assert_eq!(classify(&decorate(&isbn10)), classify(&isbn10));
    }
}
