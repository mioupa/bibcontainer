//! ヘッダーから対応づけを推測する。

use std::collections::{HashMap, HashSet};

use super::parse::RawRow;
use super::{ColumnMapping, ImportTarget};
use crate::normalize::nfkc;

/// 書き出しの CSV のヘッダー（`export` の `COLUMNS`）と、よくある別名。
/// 比較は NFKC・前後の空白除去・大文字小文字の無視をかけたあとで行う。
/// 書影・取得状態の列は取り込めないので、ここにない（対応づけなし）。
const NAMES: &[(&str, ImportTarget)] = &[
    ("ID", ImportTarget::Id),
    ("ISBN", ImportTarget::Isbn13),
    ("書名", ImportTarget::Title),
    ("書名のヨミ", ImportTarget::TitleKana),
    ("副書名", ImportTarget::Subtitle),
    ("シリーズ", ImportTarget::Series),
    ("巻次", ImportTarget::Volume),
    ("巻次（並べ替え用）", ImportTarget::VolumeSort),
    ("作品（手動）", ImportTarget::WorkId),
    ("著者", ImportTarget::Authors),
    ("出版社", ImportTarget::Publisher),
    ("発行日", ImportTarget::PublishedOn),
    ("Cコード", ImportTarget::CCode),
    ("NDC", ImportTarget::Ndc),
    ("本体価格", ImportTarget::ListPrice),
    ("参考税込価格", ImportTarget::RefPriceIncl),
    ("ページ数", ImportTarget::Pages),
    ("大きさ", ImportTarget::SizeText),
    ("言語", ImportTarget::Language),
    ("内容紹介", ImportTarget::Description),
    ("所有状態", ImportTarget::Ownership),
    ("欲しい本の優先度", ImportTarget::WishPriority),
    ("保管場所", ImportTarget::Location),
    ("タグ", ImportTarget::Tags),
    ("読書状況", ImportTarget::ReadStatus),
    ("読み始めた日", ImportTarget::ReadStartedOn),
    ("読了日", ImportTarget::ReadFinishedOn),
    ("評価", ImportTarget::Rating),
    ("感想", ImportTarget::Review),
    ("購入日", ImportTarget::PurchasedOn),
    ("購入店", ImportTarget::PurchaseStore),
    ("購入価格", ImportTarget::PurchasePrice),
    ("新品・中古", ImportTarget::PurchaseCond),
    ("購入メモ", ImportTarget::PurchaseMemo),
    ("メモ", ImportTarget::Memo),
    ("ゴミ箱", ImportTarget::Trashed),
    ("登録日時", ImportTarget::CreatedAt),
    // 別名
    ("isbn13", ImportTarget::Isbn13),
    ("isbn-13", ImportTarget::Isbn13),
    ("isbn10", ImportTarget::Isbn13),
    ("isbn-10", ImportTarget::Isbn13),
    ("isbnコード", ImportTarget::Isbn13),
    ("タイトル", ImportTarget::Title),
    ("書籍名", ImportTarget::Title),
    ("本のタイトル", ImportTarget::Title),
    ("title", ImportTarget::Title),
    ("著者名", ImportTarget::Authors),
    ("作者", ImportTarget::Authors),
    ("author", ImportTarget::Authors),
    ("authors", ImportTarget::Authors),
    ("出版者", ImportTarget::Publisher),
    ("publisher", ImportTarget::Publisher),
    ("出版日", ImportTarget::PublishedOn),
    ("発売日", ImportTarget::PublishedOn),
    ("刊行日", ImportTarget::PublishedOn),
    ("tags", ImportTarget::Tags),
    ("rating", ImportTarget::Rating),
    ("memo", ImportTarget::Memo),
    ("note", ImportTarget::Memo),
    ("レビュー", ImportTarget::Review),
    ("review", ImportTarget::Review),
    ("価格", ImportTarget::ListPrice),
    ("定価", ImportTarget::ListPrice),
    ("ページ", ImportTarget::Pages),
    ("pages", ImportTarget::Pages),
];

/// 取り込めない（対応づけない）書き出しの列。
#[cfg(test)]
pub(super) const UNMAPPED_EXPORT_HEADERS: &[&str] = &["書影", "取得状態"];

fn key(s: &str) -> String {
    nfkc(s.trim()).trim().to_lowercase()
}

/// ヘッダー行から推測する。同じ項目になる列が複数あれば、先の列だけ。
pub(super) fn suggest(header: &RawRow) -> Vec<ColumnMapping> {
    let table: HashMap<String, ImportTarget> = NAMES
        .iter()
        .rev() // 同じ名前が重なったときは先の定義を優先
        .map(|(name, target)| (key(name), *target))
        .collect();
    let mut used = HashSet::new();
    let mut out = Vec::new();
    for (i, cell) in header.cells.iter().enumerate() {
        let Some(target) = table.get(&key(&super::parse::clean_cell(cell))) else {
            continue;
        };
        if used.insert(*target) {
            out.push(ColumnMapping {
                column: u32::try_from(i).unwrap_or(u32::MAX),
                target: *target,
            });
        }
    }
    out
}
