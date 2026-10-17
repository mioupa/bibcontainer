//! 書籍・保管場所・タグ・シリーズなどの操作。
//!
//! 同期対象の変更は、必ず `record_change`（`store`）を通す。この層は、入力の検証と
//! 「由来（origin）」の扱い、読書状況や所有状態に伴う日付の自動入力などの規則を担う。
//!
//! - 公開の関数は、`&mut Connection` を取って自前のトランザクションで実行する版と、
//!   呼び出し側のトランザクションに参加する `*_in` の版（書籍の作成・更新、シリーズの検索と作成）がある。
//! - 複数件を渡す関数（`create_books`、`update_books`、`trash_books` など）は、全体を 1 つの
//!   トランザクションで行い、1 件でも失敗したら全体を取り消す。
//! - 「今日」の日付とミリ秒時計は `Env` で受け取るので、テストで差し替えられる。

pub mod book;
pub mod location;
pub mod query;
pub mod series;
pub mod tag;
pub mod validate;
pub mod works;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::Result;
use crate::hlc::{Clock, ms_to_iso8601};
use crate::values::{SOURCE_BARCODE, SOURCE_IMPORT, SOURCE_MANUAL};

pub use book::{
    create_book, create_book_in, create_books, purge_books, restore_books, trash_books,
    update_book, update_book_in, update_books,
};
pub use location::{
    LocationInfo, create_location, delete_location, list_locations, move_location, rename_location,
    reorder_locations,
};
pub use query::{
    IsbnMatch, StoreCount, find_by_isbn, pending_book_ids, purchase_store_suggestions,
};
pub use series::{
    SeriesInfo, book_ids_in_series, find_or_create_series, find_or_create_series_in, list_series,
    rename_series,
};
pub use tag::{TagInfo, list_tags, modify_tags, rename_tag, set_tag_color};
pub use works::{MergeWorksResult, WORK_NONE, merge_works};

/// 操作の環境。時計と「今日」の日付。
#[derive(Clone, Copy)]
pub struct Env<'a> {
    /// HLC の採番と `created_at` に使う時計。
    pub clock: &'a dyn Clock,
    today: Option<&'a str>,
}

impl<'a> Env<'a> {
    /// 「今日」は時計の時刻をローカルタイムゾーンの日付にしたもの。
    pub fn new(clock: &'a dyn Clock) -> Self {
        Env { clock, today: None }
    }

    /// 「今日」を固定する（テスト用）。`YYYY-MM-DD`。
    pub fn with_today(mut self, today: &'a str) -> Self {
        self.today = Some(today);
        self
    }

    /// 今日の日付（`YYYY-MM-DD`、ローカルタイムゾーン）。
    pub fn today(&self) -> String {
        if let Some(t) = self.today {
            return t.to_owned();
        }
        use chrono::TimeZone;
        let ms = self.clock.now_ms();
        chrono::Local.timestamp_millis_opt(ms).single().map_or_else(
            || String::from("1970-01-01"),
            |dt| dt.format("%Y-%m-%d").to_string(),
        )
    }

    /// 今の時刻（ISO 8601 の UTC ミリ秒。`created_at` に使う）。
    pub fn now_iso(&self) -> Result<String> {
        ms_to_iso8601(self.clock.now_ms())
    }
}

/// 値の由来。項目の変更は「値」と「由来」の組で受ける。
///
/// IPC では `{ "kind": "manual" }`、`{ "kind": "barcode" }`、`{ "kind": "import" }`、
/// `{ "kind": "provider", "name": "openbd" }` の形になる（`OriginRepr` を介して読み書きする。
/// Rust 側は `Provider(String)` のまま）。
#[derive(Debug, Clone, PartialEq, Eq, TS)]
#[ts(
    export,
    type = r#"{ "kind": "manual" } | { "kind": "barcode" } | { "kind": "import" } | { "kind": "provider", name: string }"#
)]
pub enum Origin {
    /// 利用者が入力した。値を書き、`field_sources.<項目> = "manual"` と `manual_fields.<項目> = true` を記録する。
    Manual,
    /// バーコードから読んだ（2段目の価格・Cコードなど）。手修正済みの項目は書かない。
    Barcode,
    /// ファイルの取り込み。手修正済みの項目は書かない。
    Import,
    /// 書誌の取得元（`openbd`、`ndl`、`google` など）。手修正済みの項目は書かない。
    Provider(String),
}

/// `Origin` の JSON 上の形（`kind` で種類を分ける）。
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum OriginRepr {
    Manual,
    Barcode,
    Import,
    Provider { name: String },
}

impl From<OriginRepr> for Origin {
    fn from(repr: OriginRepr) -> Self {
        match repr {
            OriginRepr::Manual => Origin::Manual,
            OriginRepr::Barcode => Origin::Barcode,
            OriginRepr::Import => Origin::Import,
            OriginRepr::Provider { name } => Origin::Provider(name),
        }
    }
}

impl From<Origin> for OriginRepr {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::Manual => OriginRepr::Manual,
            Origin::Barcode => OriginRepr::Barcode,
            Origin::Import => OriginRepr::Import,
            Origin::Provider(name) => OriginRepr::Provider { name },
        }
    }
}

impl Serialize for Origin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OriginRepr::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Origin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OriginRepr::deserialize(deserializer).map(Origin::from)
    }
}

impl Origin {
    /// `field_sources` に記録する取得元の名前。
    pub fn source_name(&self) -> &str {
        match self {
            Origin::Manual => SOURCE_MANUAL,
            Origin::Barcode => SOURCE_BARCODE,
            Origin::Import => SOURCE_IMPORT,
            Origin::Provider(name) => name,
        }
    }
}

/// 書籍の 1 項目の変更。IPC では `{ "field": "title", "value": …, "origin": { "kind": "manual" } }`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FieldChange {
    /// 項目名（Op 上の名前。`title`、`authors`、`list_price` など）。
    pub field: String,
    /// 新しい値（JSON）。消すときは `null`。
    #[ts(type = "unknown")]
    pub value: Value,
    pub origin: Origin,
}

impl FieldChange {
    pub fn new(field: impl Into<String>, value: impl Into<Value>, origin: Origin) -> Self {
        FieldChange {
            field: field.into(),
            value: value.into(),
            origin,
        }
    }

    /// 利用者の入力。
    pub fn manual(field: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::new(field, value, Origin::Manual)
    }

    /// バーコードから読んだ値。
    pub fn barcode(field: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::new(field, value, Origin::Barcode)
    }

    /// ファイルの取り込み。
    pub fn import(field: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::new(field, value, Origin::Import)
    }

    /// 書誌の取得元（`provider` は `openbd` など）の値。
    pub fn provider(
        field: impl Into<String>,
        value: impl Into<Value>,
        provider: impl Into<String>,
    ) -> Self {
        Self::new(field, value, Origin::Provider(provider.into()))
    }
}

/// 更新の指定。IPC では `{ "overrideManual": false }`（省略した項目は既定値）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UpdateOptions {
    /// `true` なら、`Provider`・`Barcode`・`Import` の変更が手修正済みの項目も上書きし、
    /// その項目の手修正の記録（`manual_fields.<項目>`）を外す。
    pub override_manual: bool,
    /// `true` なら、`Provider` の変更の値が空（null・空文字・空の配列）のとき、その項目の今の値が**同じ取得元**の
    /// ものであれば、値と取得元の記録（`field_sources.<項目>`）を消す。再取得で、取得元がもう値を返さなくなった項目に
    /// 古い値を残さないため（0010 §9）。手修正済みの項目、ほかの取得元・バーコード・取り込みの値は消さない
    /// （`override_manual` でも消さない。消す値ではなく、置き換える値がないため）。巻次（`volume`）を消したときは、
    /// 並べ替え用の数値（`volume_sort`）も消す。
    ///
    /// Rust の中だけの指定で、IPC には出さない（画面からは指定できない）。
    #[serde(skip)]
    #[ts(skip)]
    pub clear_stale: bool,
}

/// 1 冊の更新の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeReport {
    /// 対象の書籍 ID。
    pub id: String,
    /// 書き換えた項目名（取得元・手修正の記録だけが変わった項目も含む。規則で自動入力した項目も含む）。
    pub written: Vec<String>,
    /// 手修正済みのために書かなかった項目名（`Provider` などの変更で `override_manual` でないとき）。
    pub skipped_manual: Vec<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn origin_has_the_ipc_json_shape() {
        let cases = [
            (Origin::Manual, json!({ "kind": "manual" })),
            (Origin::Barcode, json!({ "kind": "barcode" })),
            (Origin::Import, json!({ "kind": "import" })),
            (
                Origin::Provider("openbd".into()),
                json!({ "kind": "provider", "name": "openbd" }),
            ),
        ];
        for (origin, expected) in cases {
            assert_eq!(serde_json::to_value(&origin).unwrap(), expected);
            assert_eq!(serde_json::from_value::<Origin>(expected).unwrap(), origin);
        }
        assert!(serde_json::from_value::<Origin>(json!({ "kind": "other" })).is_err());
        assert!(serde_json::from_value::<Origin>(json!({ "kind": "provider" })).is_err());
    }

    #[test]
    fn field_change_round_trips_any_json_value() {
        let change: FieldChange = serde_json::from_value(json!({
            "field": "authors",
            "value": [{ "name": "著者", "kana": null, "role": null }],
            "origin": { "kind": "provider", "name": "ndl" },
        }))
        .unwrap();
        assert_eq!(change.field, "authors");
        assert_eq!(change.origin, Origin::Provider("ndl".into()));
        let back = serde_json::to_value(&change).unwrap();
        assert_eq!(back["origin"]["name"], "ndl");
        assert_eq!(back["value"][0]["name"], "著者");

        // 消す指示は、値を null と明示する。値を省いたものは受け付けない。
        let clear: FieldChange = serde_json::from_value(
            json!({ "field": "memo", "value": null, "origin": { "kind": "manual" } }),
        )
        .unwrap();
        assert_eq!(clear.value, Value::Null);
        assert!(
            serde_json::from_value::<FieldChange>(
                json!({ "field": "memo", "origin": { "kind": "manual" } })
            )
            .is_err()
        );
    }

    #[test]
    fn update_options_reads_camel_case_and_defaults() {
        let opts: UpdateOptions =
            serde_json::from_value(json!({ "overrideManual": true })).unwrap();
        assert!(opts.override_manual);
        assert!(!opts.clear_stale);
        let opts: UpdateOptions = serde_json::from_value(json!({})).unwrap();
        assert!(!opts.override_manual);
        // `clear_stale` は IPC に出さない（送られても読まず、書き出しにも出ない）
        let opts: UpdateOptions =
            serde_json::from_value(json!({ "clearStale": true, "overrideManual": true })).unwrap();
        assert!(!opts.clear_stale);
        let json = serde_json::to_value(UpdateOptions {
            clear_stale: true,
            ..UpdateOptions::default()
        })
        .unwrap();
        assert_eq!(json, json!({ "overrideManual": false }));
    }
}
