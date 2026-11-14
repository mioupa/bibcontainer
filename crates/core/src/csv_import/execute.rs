//! 取り込みの実行。1 つのトランザクションの中で、見積もりをやり直してから書く。

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::json;

use super::CsvImportOptions;
use super::CsvImportResult;
use super::plan::{Class, analyze, load};
use crate::db::write_tx;
use crate::error::Result;
use crate::library::book::create_book_with_created_at_in;
use crate::library::location::create_location_in;
use crate::library::series::find_or_create_series_in;
use crate::library::{Env, FieldChange};
use crate::normalize::normalize_for_search;

/// CSV を取り込む。
///
/// 1 つのトランザクションの中で見積もり（重複の確認）をやり直し、必要な保管場所・シリーズを作り、
/// 取り込む行の本を CSV の順に作る。値は `Origin::Import` で書く。ISBN がある本は、
/// 書誌の取得待ち（`meta_status = pending`）にする（アプリの自動の再取得が、あとで書誌と書影を取得する）。
/// 途中で失敗したら、全体を取り消す。
pub fn execute(
    conn: &mut Connection,
    env: &Env<'_>,
    bytes: &[u8],
    options: &CsvImportOptions,
) -> Result<CsvImportResult> {
    let (table, pairs) = load(bytes, options)?;
    let tx = write_tx(conn)?;
    let a = analyze(&tx, &table, &pairs)?;

    // 保管場所は、親が先になる順（節は親より後に足されるので、番号の順でよい）。
    let mut location_ids: Vec<Option<String>> = a.tree.nodes.iter().map(|n| n.id.clone()).collect();
    for (i, node) in a.tree.nodes.iter().enumerate() {
        if node.id.is_some() {
            continue;
        }
        let parent = node.parent.and_then(|p| location_ids[p].clone());
        location_ids[i] = Some(create_location_in(&tx, env, &node.name, parent.as_deref())?);
    }

    let mut series_ids: HashMap<String, String> = HashMap::new();
    let mut book_ids = Vec::with_capacity(a.plan.to_import as usize);
    for (i, (row, class)) in a.parsed.iter().zip(&a.classes).enumerate() {
        if *class != Class::Import {
            continue;
        }
        let mut changes: Vec<FieldChange> = row
            .fields
            .iter()
            .map(|(field, value)| FieldChange::import(*field, value.clone()))
            .collect();
        if let Some(isbn) = &row.isbn13 {
            changes.push(FieldChange::import("isbn13", json!(isbn)));
            changes.push(FieldChange::import("meta_status", json!("pending")));
        }
        if let Some(name) = &row.series {
            let key = normalize_for_search(name);
            let id = match series_ids.get(&key) {
                Some(id) => id.clone(),
                None => {
                    let id = find_or_create_series_in(&tx, env, name)?;
                    series_ids.insert(key, id.clone());
                    id
                }
            };
            changes.push(FieldChange::import("series_id", json!(id)));
        }
        if let Some(loc) = a.location_of[i].and_then(|n| location_ids[n].as_ref()) {
            changes.push(FieldChange::import("location_id", json!(loc)));
        }
        book_ids.push(create_book_with_created_at_in(
            &tx,
            env,
            &changes,
            row.created_at.as_deref(),
        )?);
    }
    tx.commit()?;

    let plan = &a.plan;
    Ok(CsvImportResult {
        imported: plan.to_import,
        fetch_queued: plan.to_fetch,
        skipped: plan.skipped.total(),
        errors: plan.errors,
        book_ids,
        created_locations: u32::try_from(plan.new_locations.len()).unwrap_or(u32::MAX),
        created_series: u32::try_from(plan.new_series.len()).unwrap_or(u32::MAX),
    })
}
