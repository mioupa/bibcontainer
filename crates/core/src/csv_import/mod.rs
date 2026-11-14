//! 汎用の CSV 取り込み（Tauri に依存しない部分）。
//!
//! 入口はバイト列（ファイルの読み込みは呼び出し側）。流れは次のとおり。
//!
//! 1. `preview`: 文字コードを判定して列の見本と、ヘッダーから推測した対応づけ（`suggested`）を返す。
//! 2. `plan`: 対応づけに従って各行を読み、取り込む・飛ばす・エラーの別と、作る保管場所・シリーズを数える。
//!    DB は読むだけで書かない。
//! 3. `execute`: 1 つのトランザクションの中で同じ判定をやり直し（重複の確認を最新にする）、
//!    保管場所・シリーズ・書籍を作る。途中で失敗したら全体を取り消す。
//! 4. `write_rejected`: エラーの行だけを、理由の列を付けて CSV に書き出す（直して取り込み直すため）。
//!
//! 書き出し（`export`）の CSV をそのまま取り込める（列名の対応づけ、列挙値の日本語の表示名、
//! 数式対策の `'` の除去）。設計の経緯は `docs/decisions/` の CSV 取り込みの記録を参照。

mod decode;
mod execute;
mod parse;
mod plan;
mod rejected;
mod suggest;
mod values;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result};

pub use execute::execute;
pub use plan::plan;
pub use rejected::{write_rejected, write_rejected_to_file};

/// 取り込める CSV の大きさの上限（バイト）。呼び出し側（Tauri）も、読む前にこれで確かめる。
pub const MAX_CSV_BYTES: usize = 20 * 1024 * 1024;

/// CSV の文字コード。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CsvEncoding {
    Utf8,
    ShiftJis,
}

/// 取り込み先の項目。書影（`cover_hash`）と取得状態（`meta_status`）は取り込めない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImportTarget {
    /// 重複の判定にだけ使う（書き込まない）。
    Id,
    Isbn13,
    Title,
    TitleKana,
    Subtitle,
    Series,
    Volume,
    VolumeSort,
    WorkId,
    Authors,
    Publisher,
    PublishedOn,
    #[serde(rename = "c_code")]
    CCode,
    Ndc,
    ListPrice,
    RefPriceIncl,
    Pages,
    SizeText,
    Language,
    Description,
    Ownership,
    WishPriority,
    Location,
    Tags,
    ReadStatus,
    ReadStartedOn,
    ReadFinishedOn,
    Rating,
    Review,
    PurchasedOn,
    PurchaseStore,
    PurchasePrice,
    PurchaseCond,
    PurchaseMemo,
    Memo,
    /// 値が真の行は取り込まずに飛ばす。
    Trashed,
    CreatedAt,
}

impl ImportTarget {
    /// 全項目（画面の選択肢の並び）。
    pub const ALL: [ImportTarget; 37] = [
        ImportTarget::Id,
        ImportTarget::Isbn13,
        ImportTarget::Title,
        ImportTarget::TitleKana,
        ImportTarget::Subtitle,
        ImportTarget::Series,
        ImportTarget::Volume,
        ImportTarget::VolumeSort,
        ImportTarget::WorkId,
        ImportTarget::Authors,
        ImportTarget::Publisher,
        ImportTarget::PublishedOn,
        ImportTarget::CCode,
        ImportTarget::Ndc,
        ImportTarget::ListPrice,
        ImportTarget::RefPriceIncl,
        ImportTarget::Pages,
        ImportTarget::SizeText,
        ImportTarget::Language,
        ImportTarget::Description,
        ImportTarget::Ownership,
        ImportTarget::WishPriority,
        ImportTarget::Location,
        ImportTarget::Tags,
        ImportTarget::ReadStatus,
        ImportTarget::ReadStartedOn,
        ImportTarget::ReadFinishedOn,
        ImportTarget::Rating,
        ImportTarget::Review,
        ImportTarget::PurchasedOn,
        ImportTarget::PurchaseStore,
        ImportTarget::PurchasePrice,
        ImportTarget::PurchaseCond,
        ImportTarget::PurchaseMemo,
        ImportTarget::Memo,
        ImportTarget::Trashed,
        ImportTarget::CreatedAt,
    ];

    /// JSON 上の名前（`snake_case`）。書籍の項目名とほぼ同じ。
    pub fn key(self) -> &'static str {
        match self {
            ImportTarget::Id => "id",
            ImportTarget::Isbn13 => "isbn13",
            ImportTarget::Title => "title",
            ImportTarget::TitleKana => "title_kana",
            ImportTarget::Subtitle => "subtitle",
            ImportTarget::Series => "series",
            ImportTarget::Volume => "volume",
            ImportTarget::VolumeSort => "volume_sort",
            ImportTarget::WorkId => "work_id",
            ImportTarget::Authors => "authors",
            ImportTarget::Publisher => "publisher",
            ImportTarget::PublishedOn => "published_on",
            ImportTarget::CCode => "c_code",
            ImportTarget::Ndc => "ndc",
            ImportTarget::ListPrice => "list_price",
            ImportTarget::RefPriceIncl => "ref_price_incl",
            ImportTarget::Pages => "pages",
            ImportTarget::SizeText => "size_text",
            ImportTarget::Language => "language",
            ImportTarget::Description => "description",
            ImportTarget::Ownership => "ownership",
            ImportTarget::WishPriority => "wish_priority",
            ImportTarget::Location => "location",
            ImportTarget::Tags => "tags",
            ImportTarget::ReadStatus => "read_status",
            ImportTarget::ReadStartedOn => "read_started_on",
            ImportTarget::ReadFinishedOn => "read_finished_on",
            ImportTarget::Rating => "rating",
            ImportTarget::Review => "review",
            ImportTarget::PurchasedOn => "purchased_on",
            ImportTarget::PurchaseStore => "purchase_store",
            ImportTarget::PurchasePrice => "purchase_price",
            ImportTarget::PurchaseCond => "purchase_cond",
            ImportTarget::PurchaseMemo => "purchase_memo",
            ImportTarget::Memo => "memo",
            ImportTarget::Trashed => "trashed",
            ImportTarget::CreatedAt => "created_at",
        }
    }

    /// 日本語の項目名（書き出しの CSV のヘッダーと同じ）。取り込めなかった理由の文に使う。
    pub fn label_ja(self) -> &'static str {
        match self {
            ImportTarget::Id => "ID",
            ImportTarget::Isbn13 => "ISBN",
            ImportTarget::Title => "書名",
            ImportTarget::TitleKana => "書名のヨミ",
            ImportTarget::Subtitle => "副書名",
            ImportTarget::Series => "シリーズ",
            ImportTarget::Volume => "巻次",
            ImportTarget::VolumeSort => "巻次（並べ替え用）",
            ImportTarget::WorkId => "作品（手動）",
            ImportTarget::Authors => "著者",
            ImportTarget::Publisher => "出版社",
            ImportTarget::PublishedOn => "発行日",
            ImportTarget::CCode => "Cコード",
            ImportTarget::Ndc => "NDC",
            ImportTarget::ListPrice => "本体価格",
            ImportTarget::RefPriceIncl => "参考税込価格",
            ImportTarget::Pages => "ページ数",
            ImportTarget::SizeText => "大きさ",
            ImportTarget::Language => "言語",
            ImportTarget::Description => "内容紹介",
            ImportTarget::Ownership => "所有状態",
            ImportTarget::WishPriority => "欲しい本の優先度",
            ImportTarget::Location => "保管場所",
            ImportTarget::Tags => "タグ",
            ImportTarget::ReadStatus => "読書状況",
            ImportTarget::ReadStartedOn => "読み始めた日",
            ImportTarget::ReadFinishedOn => "読了日",
            ImportTarget::Rating => "評価",
            ImportTarget::Review => "感想",
            ImportTarget::PurchasedOn => "購入日",
            ImportTarget::PurchaseStore => "購入店",
            ImportTarget::PurchasePrice => "購入価格",
            ImportTarget::PurchaseCond => "新品・中古",
            ImportTarget::PurchaseMemo => "購入メモ",
            ImportTarget::Memo => "メモ",
            ImportTarget::Trashed => "ゴミ箱",
            ImportTarget::CreatedAt => "登録日時",
        }
    }
}

/// 列と取り込み先の対応づけ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ColumnMapping {
    /// 0 始まりの列の番号。
    pub column: u32,
    pub target: ImportTarget,
}

/// 取り込みの指定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CsvImportOptions {
    /// `None` は自動判定。
    pub encoding: Option<CsvEncoding>,
    pub has_header: bool,
    pub mapping: Vec<ColumnMapping>,
}

/// 列の見本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CsvColumn {
    pub index: u32,
    /// ヘッダーの文字（ヘッダーなし、または空なら `None`）。
    pub header: Option<String>,
    /// 空でない最初の 3 行の値（数式対策の `'` を除いたもの）。
    pub samples: Vec<String>,
}

/// 読み込んだ CSV の見え方と、推測した対応づけ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CsvPreview {
    pub encoding: CsvEncoding,
    /// 文字コードを自動判定したか（指定されていなかったか）。
    pub auto_detected: bool,
    pub columns: Vec<CsvColumn>,
    /// データの行数（空行とヘッダーを除く）。
    pub row_count: u32,
    pub suggested: Vec<ColumnMapping>,
}

/// 行の扱い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RowStatus {
    Import,
    Skip,
    Error,
}

/// 行が飛ばされた・エラーになった・警告つきで取り込まれる理由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum IssueReason {
    /// 同じ ISBN の本がすでにある（ゴミ箱・完全削除を除く。所有状態は問わない）。
    RegisteredIsbn { existing_title: String },
    /// 同じ ID の本がすでにある（完全削除を除く）。
    SameId { trashed: bool },
    /// ファイルの先に同じ ISBN（または ID）の取り込む行がある。
    DuplicateInFile { first_line: u32 },
    /// 「ゴミ箱」の列が真の行。
    TrashedRow,
    /// ISBN として読めない。`excel_number` は Excel が数値にして桁を落とした形（`9.78478E+12`）のとき真。
    InvalidIsbn { value: String, excel_number: bool },
    /// 書名も ISBN もない。
    NoTitleOrIsbn,
    /// 値を読めなかったので、その項目だけ取り込まない。
    UnreadableValue { target: ImportTarget, value: String },
}

impl IssueReason {
    /// 日本語の説明（取り込めなかった行のファイルの「取り込めなかった理由」の列に書く）。
    pub fn describe_ja(&self) -> String {
        match self {
            IssueReason::RegisteredIsbn { existing_title } => {
                format!("同じ ISBN の本がすでに登録されています（{existing_title}）")
            }
            IssueReason::SameId { trashed } => {
                if *trashed {
                    "同じ ID の本がすでにあります（ゴミ箱の中）".to_owned()
                } else {
                    "同じ ID の本がすでにあります".to_owned()
                }
            }
            IssueReason::DuplicateInFile { first_line } => {
                format!("ファイルの {first_line} 行目と同じ本です")
            }
            IssueReason::TrashedRow => "ゴミ箱の本の行です".to_owned(),
            IssueReason::InvalidIsbn {
                value,
                excel_number,
            } => {
                if *excel_number {
                    format!(
                        "ISBN を読めません（{value}。Excel で数値になり桁が落ちた可能性があります）"
                    )
                } else {
                    format!("ISBN を読めません（{value}）")
                }
            }
            IssueReason::NoTitleOrIsbn => "書名も ISBN もありません".to_owned(),
            IssueReason::UnreadableValue { target, value } => {
                format!("{}を読めません（{value}）", target.label_ja())
            }
        }
    }
}

/// 問題のある行（飛ばす・エラー・警告つきで取り込む）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RowIssue {
    /// ファイルの物理行（そのレコードが始まる行。ヘッダーが 1 行目）。
    pub line: u32,
    pub status: RowStatus,
    pub isbn13: Option<String>,
    pub title: Option<String>,
    pub reasons: Vec<IssueReason>,
}

/// 飛ばす行の内訳。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkipCounts {
    pub registered_isbn: u32,
    pub same_id: u32,
    pub duplicate_in_file: u32,
    pub trashed_row: u32,
}

impl SkipCounts {
    pub fn total(&self) -> u32 {
        self.registered_isbn + self.same_id + self.duplicate_in_file + self.trashed_row
    }
}

/// 取り込みの見積もり。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CsvImportPlan {
    pub total_rows: u32,
    pub to_import: u32,
    /// 取り込む行のうち ISBN がある行（あとで書誌・書影を取得する）。
    pub to_fetch: u32,
    pub skipped: SkipCounts,
    pub errors: u32,
    /// 警告つきで取り込む行の数。
    pub warned_rows: u32,
    pub issues: Vec<RowIssue>,
    /// 作る保管場所（`自宅 > 書斎` のパス。親が先）。
    pub new_locations: Vec<String>,
    /// 作るシリーズ。
    pub new_series: Vec<String>,
}

/// 取り込みの結果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CsvImportResult {
    pub imported: u32,
    /// ISBN があり、書誌の取得待ちにした本の数。
    pub fetch_queued: u32,
    pub skipped: u32,
    pub errors: u32,
    /// 作った本の ID（CSV の行の順）。
    pub book_ids: Vec<String>,
    pub created_locations: u32,
    pub created_series: u32,
}

/// 入力が大きすぎないか。
fn check_size(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_CSV_BYTES {
        return Err(Error::validation(
            "file",
            format!(
                "ファイルが大きすぎます（{} MB まで）",
                MAX_CSV_BYTES / (1024 * 1024)
            ),
        ));
    }
    Ok(())
}

/// 対応づけを確かめる（同じ項目は 1 回まで、列の番号は範囲内）。
fn validate_options(options: &CsvImportOptions, width: usize) -> Result<()> {
    let mut seen = HashSet::new();
    for m in &options.mapping {
        if !seen.insert(m.target) {
            return Err(Error::validation(
                "mapping",
                format!(
                    "「{}」が複数の列に対応づけられています",
                    m.target.label_ja()
                ),
            ));
        }
        if usize::try_from(m.column).map_or(true, |c| c >= width) {
            return Err(Error::validation(
                "mapping",
                format!("列 {} はありません（{width} 列）", m.column),
            ));
        }
    }
    Ok(())
}

/// 文字コードを判定して列の見本と、対応づけの推測を返す。
pub fn preview(
    bytes: &[u8],
    encoding: Option<CsvEncoding>,
    has_header: bool,
) -> Result<CsvPreview> {
    check_size(bytes)?;
    let (text, used) = decode::decode(bytes, encoding)?;
    let table = parse::read_table(&text, has_header)?;
    let columns = (0..table.width)
        .map(|i| CsvColumn {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            header: table
                .header
                .as_ref()
                .map(|h| h.cell(i))
                .filter(|s| !s.is_empty()),
            samples: table
                .rows
                .iter()
                .map(|r| r.cell(i))
                .filter(|s| !s.is_empty())
                .take(3)
                .collect(),
        })
        .collect();
    let suggested = match &table.header {
        Some(h) if has_header => suggest::suggest(h),
        _ => Vec::new(),
    };
    Ok(CsvPreview {
        encoding: used,
        auto_detected: encoding.is_none(),
        columns,
        row_count: u32::try_from(table.rows.len()).unwrap_or(u32::MAX),
        suggested,
    })
}

#[cfg(test)]
mod tests;
