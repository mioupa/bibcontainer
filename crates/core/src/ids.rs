//! ID の生成と検証。

use uuid::Uuid;

/// 新しいエンティティ ID（UUIDv7、ハイフン付き小文字）。
pub fn new_entity_id() -> String {
    Uuid::now_v7().hyphenated().to_string()
}

/// `s` が ID の書式（ハイフン付き小文字の UUID）かどうか。版は問わない。
pub fn is_valid_entity_id(s: &str) -> bool {
    Uuid::try_parse(s).is_ok_and(|u| u.hyphenated().to_string() == s)
}

/// 新しい端末 ID（`d-` + 小文字 16 進 12 文字）。
pub fn new_device_id() -> String {
    // v4 UUID の先頭 6 バイトはすべて乱数（版を示すビットは 7 バイト目）。
    let bytes = Uuid::new_v4().into_bytes();
    let mut id = String::from("d-");
    for b in &bytes[..6] {
        id.push_str(&format!("{b:02x}"));
    }
    id
}

/// `s` が端末 ID として使える文字列かどうか。
///
/// HLC 文字列の区切りに使う `:` を含まない、空でない ASCII の可視文字列であればよい。
/// 生成は `new_device_id` の書式だけだが、読む側は将来の書式の変更に備えて緩くしてある。
pub fn is_valid_device_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic() && b != b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_is_hyphenated_lowercase_uuid_v7() {
        let id = new_entity_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id, id.to_lowercase());
        assert_eq!(Uuid::try_parse(&id).unwrap().get_version_num(), 7);
        assert!(is_valid_entity_id(&id));
    }

    #[test]
    fn entity_ids_are_unique_and_sortable_by_creation() {
        let ids: Vec<String> = (0..2000).map(|_| new_entity_id()).collect();
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "UUIDv7 は同じミリ秒内でも作成順に並ぶ"
        );
    }

    #[test]
    fn entity_id_validation_rejects_other_forms() {
        assert!(!is_valid_entity_id(""));
        assert!(!is_valid_entity_id("book-1"));
        assert!(!is_valid_entity_id(
            "0192F3A0-6C1E-7CC3-9B1A-2F4E8D1C0A55" // 大文字
        ));
        assert!(!is_valid_entity_id("0192f3a06c1e7cc39b1a2f4e8d1c0a55")); // ハイフンなし
    }

    #[test]
    fn device_id_format() {
        let id = new_device_id();
        assert_eq!(id.len(), 14);
        assert!(id.starts_with("d-"));
        assert!(
            id[2..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(is_valid_device_id(&id));
        assert_ne!(id, new_device_id());
    }

    #[test]
    fn device_id_validation() {
        assert!(is_valid_device_id("d-7f3a2c"));
        assert!(!is_valid_device_id(""));
        assert!(!is_valid_device_id("d:1"));
        assert!(!is_valid_device_id("d 1"));
        assert!(!is_valid_device_id("d-日本"));
    }
}
