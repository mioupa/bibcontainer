//! 書籍の項目のうち、取りうる値が決まっているもの。

/// `ownership`: 所有・欲しい本・手放した。
pub const OWNERSHIPS: &[&str] = &["owned", "wanted", "released"];
/// `read_status`: 未読・読書中・読了・中断。
pub const READ_STATUSES: &[&str] = &["unread", "reading", "read", "paused"];
/// `wish_priority`: 高・中・低（欲しい本の優先度。null 可）。
pub const WISH_PRIORITIES: &[&str] = &["high", "mid", "low"];
/// `purchase_cond`: 新品・中古（null 可）。
pub const PURCHASE_CONDS: &[&str] = &["new", "used"];
/// `meta_status`: 書誌の取得状態。
pub const META_STATUSES: &[&str] = &["ok", "pending", "partial", "failed"];

/// 取得元・手修正の記録（`field_sources.*`、`manual_fields.*`）を付ける項目。
/// 外部から取得する書誌の項目で、再取得で上書きしてよいかの判断（FR-META-07）の対象になる。
/// 読書記録・購入情報・所有状態・保管場所・タグ・メモなど、利用者が持つ項目は含まない。
pub const TRACKED_FIELDS: &[&str] = &[
    "isbn13",
    "title",
    "title_kana",
    "subtitle",
    "series_id",
    "volume",
    "volume_sort",
    "authors",
    "publisher",
    "published_on",
    "c_code",
    "ndc",
    "list_price",
    "ref_price_incl",
    "pages",
    "size_text",
    "language",
    "description",
    "cover_hash",
];

/// 取得元の名前（`field_sources.*` の値）のうち、アプリが決めているもの。
pub const SOURCE_MANUAL: &str = "manual";
/// 2段目のバーコードなど、バーコードから読んだ値。
pub const SOURCE_BARCODE: &str = "barcode";
/// ファイルからの取り込み。
pub const SOURCE_IMPORT: &str = "import";

/// `value` が `allowed` のどれかなら `value`、そうでなければ `default`（外部から届いた不正な値を
/// 画面に渡さないための読み出し側の補正）。
pub fn pick_known(value: String, allowed: &[&str], default: &str) -> String {
    if allowed.contains(&value.as_str()) {
        value
    } else {
        default.to_owned()
    }
}
