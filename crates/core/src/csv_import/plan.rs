//! 取り込みの見積もり。各行を読み、DB の状態と突き合わせて取り込む・飛ばす・エラーを決める。

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use super::parse::{Table, mapping_pairs, read_table};
use super::values::{ParsedRow, parse_row};
use super::{
    CsvImportOptions, CsvImportPlan, ImportTarget, IssueReason, RowIssue, RowStatus, check_size,
    decode, validate_options,
};
use crate::error::Result;
use crate::library::location::{PATH_SEPARATOR, list_locations};
use crate::normalize::normalize_for_search;

/// 取り込み先の保管場所の木。既存の（削除していない）保管場所と、これから作るものを持つ。
#[derive(Debug, Default)]
pub(super) struct LocTree {
    pub nodes: Vec<LocNode>,
    /// (親, 正規化した名前) → 保管場所。同じ名前が並ぶときは先のもの。
    children: HashMap<(Option<usize>, String), usize>,
    /// 既存の保管場所の、正規化した名前 → 保管場所（木のどこにあっても）。
    by_name: HashMap<String, Vec<usize>>,
}

#[derive(Debug)]
pub(super) struct LocNode {
    pub name: String,
    pub parent: Option<usize>,
    /// 既存なら ID。これから作るものは `None`。
    pub id: Option<String>,
}

impl LocTree {
    fn load(conn: &Connection) -> Result<Self> {
        let list = list_locations(conn)?;
        let index: HashMap<&str, usize> = list
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.as_str(), i))
            .collect();
        let mut tree = LocTree::default();
        for (i, loc) in list.iter().enumerate() {
            let parent = loc.parent_id.as_deref().and_then(|p| index.get(p)).copied();
            let key = normalize_for_search(loc.name.trim());
            tree.nodes.push(LocNode {
                name: loc.name.clone(),
                parent,
                id: Some(loc.id.clone()),
            });
            tree.children.entry((parent, key.clone())).or_insert(i);
            tree.by_name.entry(key).or_default().push(i);
        }
        Ok(tree)
    }

    /// パス（根から）に当たる保管場所を返す。無い部分は作る予定の節として足す。
    ///
    /// 1. 根からたどって全部合えばそれ。
    /// 2. 区切りのない名前で、根に合うものがなく、同じ名前の既存の保管場所がちょうど 1 つならそれ。
    /// 3. そうでなければ、合った所まで（最も深い一致）の下に、残りを作る。
    pub fn resolve(&mut self, segments: &[String]) -> usize {
        let norms: Vec<String> = segments.iter().map(|s| normalize_for_search(s)).collect();
        let mut cur: Option<usize> = None;
        let mut matched = 0;
        for n in &norms {
            match self.children.get(&(cur, n.clone())) {
                Some(&i) => {
                    cur = Some(i);
                    matched += 1;
                }
                None => break,
            }
        }
        if matched == segments.len()
            && let Some(i) = cur
        {
            return i;
        }
        if segments.len() == 1
            && matched == 0
            && let Some(v) = self.by_name.get(&norms[0])
            && v.len() == 1
        {
            return v[0];
        }
        for (name, norm) in segments.iter().zip(norms).skip(matched) {
            let idx = self.nodes.len();
            self.nodes.push(LocNode {
                name: name.clone(),
                parent: cur,
                id: None,
            });
            self.children.insert((cur, norm), idx);
            cur = Some(idx);
        }
        cur.unwrap_or_default()
    }

    /// これから作る保管場所のパス。
    fn new_paths(&self) -> Vec<String> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.id.is_none())
            .map(|(i, _)| self.path(i))
            .collect()
    }

    fn path(&self, mut i: usize) -> String {
        let mut names = vec![self.nodes[i].name.as_str()];
        while let Some(p) = self.nodes[i].parent {
            names.push(self.nodes[p].name.as_str());
            i = p;
        }
        names.reverse();
        names.join(PATH_SEPARATOR)
    }
}

/// 行の判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Class {
    Import,
    Skip(IssueReason),
    Error,
}

/// 見積もりの結果（実行にも使う）。
pub(super) struct Analysis {
    pub parsed: Vec<ParsedRow>,
    pub classes: Vec<Class>,
    /// 各行の保管場所（`tree` の節の番号）。取り込む行だけ。
    pub location_of: Vec<Option<usize>>,
    pub tree: LocTree,
    pub plan: CsvImportPlan,
}

/// バイト列を読み込み、対応づけを確かめる。
pub(super) fn load(
    bytes: &[u8],
    options: &CsvImportOptions,
) -> Result<(Table, Vec<(usize, ImportTarget)>)> {
    check_size(bytes)?;
    let (text, _) = decode::decode(bytes, options.encoding)?;
    let table = read_table(&text, options.has_header)?;
    validate_options(options, table.width)?;
    Ok((table, mapping_pairs(&options.mapping)))
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 各行を判定する。DB は読むだけ。
pub(super) fn analyze(
    conn: &Connection,
    table: &Table,
    pairs: &[(usize, ImportTarget)],
) -> Result<Analysis> {
    let mapped = |t: ImportTarget| pairs.iter().any(|(_, x)| *x == t);

    // 既存の本。重複の確認は、1 回の問い合わせで引き当てる。
    let mut isbns: HashMap<String, String> = HashMap::new();
    if mapped(ImportTarget::Isbn13) {
        let mut stmt = conn.prepare(
            "SELECT isbn13, title FROM books WHERE deleted = 0 AND purged = 0 AND isbn13 IS NOT NULL",
        )?;
        for r in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (isbn, title) = r?;
            isbns.entry(isbn).or_insert(title);
        }
    }
    let mut ids: HashMap<String, bool> = HashMap::new();
    if mapped(ImportTarget::Id) {
        let mut stmt = conn.prepare("SELECT id, deleted FROM books WHERE purged = 0")?;
        for r in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))? {
            let (id, deleted) = r?;
            ids.insert(id, deleted);
        }
    }
    let mut existing_series: HashSet<String> = HashSet::new();
    if mapped(ImportTarget::Series) {
        let mut stmt = conn.prepare("SELECT name FROM series WHERE deleted = 0")?;
        for name in stmt.query_map([], |r| r.get::<_, String>(0))? {
            existing_series.insert(normalize_for_search(name?.trim()));
        }
    }

    let mut tree = LocTree::load(conn)?;
    let parsed: Vec<ParsedRow> = table.rows.iter().map(|r| parse_row(r, pairs)).collect();
    let mut classes = Vec::with_capacity(parsed.len());
    let mut location_of = vec![None; parsed.len()];
    let mut plan = CsvImportPlan {
        total_rows: count(parsed.len()),
        ..CsvImportPlan::default()
    };
    let mut seen_isbn: HashMap<String, u32> = HashMap::new();
    let mut seen_id: HashMap<String, u32> = HashMap::new();
    let mut seen_series: HashSet<String> = HashSet::new();

    for (i, row) in parsed.iter().enumerate() {
        let class = classify(row, &isbns, &ids, &seen_isbn, &seen_id);
        let (status, reasons) = match &class {
            Class::Error => (RowStatus::Error, row.error.iter().cloned().collect()),
            Class::Skip(reason) => (RowStatus::Skip, vec![reason.clone()]),
            Class::Import => (RowStatus::Import, row.warnings.clone()),
        };
        match &class {
            Class::Error => plan.errors += 1,
            Class::Skip(reason) => match reason {
                IssueReason::RegisteredIsbn { .. } => plan.skipped.registered_isbn += 1,
                IssueReason::SameId { .. } => plan.skipped.same_id += 1,
                IssueReason::DuplicateInFile { .. } => plan.skipped.duplicate_in_file += 1,
                _ => plan.skipped.trashed_row += 1,
            },
            Class::Import => {
                plan.to_import += 1;
                if let Some(isbn) = &row.isbn13 {
                    plan.to_fetch += 1;
                    seen_isbn.entry(isbn.clone()).or_insert(row.line);
                }
                if let Some(id) = &row.id {
                    seen_id.entry(id.clone()).or_insert(row.line);
                }
                if !row.warnings.is_empty() {
                    plan.warned_rows += 1;
                }
                if let Some(path) = &row.location {
                    location_of[i] = Some(tree.resolve(path));
                }
                if let Some(name) = &row.series {
                    let key = normalize_for_search(name);
                    if !existing_series.contains(&key) && seen_series.insert(key) {
                        plan.new_series.push(name.clone());
                    }
                }
            }
        }
        if class != Class::Import || !row.warnings.is_empty() {
            plan.issues.push(RowIssue {
                line: row.line,
                status,
                isbn13: row.isbn13.clone(),
                title: row.title.clone(),
                reasons,
            });
        }
        classes.push(class);
    }
    plan.new_locations = tree.new_paths();
    Ok(Analysis {
        parsed,
        classes,
        location_of,
        tree,
        plan,
    })
}

/// 行の扱いを決める。順序は、エラー（読めない ISBN・書名も ISBN もない）、ゴミ箱の行、同じ ID、
/// 登録済みの ISBN、ファイルの中の重複、の順。
fn classify(
    row: &ParsedRow,
    isbns: &HashMap<String, String>,
    ids: &HashMap<String, bool>,
    seen_isbn: &HashMap<String, u32>,
    seen_id: &HashMap<String, u32>,
) -> Class {
    if row.error.is_some() {
        return Class::Error;
    }
    if row.trashed {
        return Class::Skip(IssueReason::TrashedRow);
    }
    if let Some(trashed) = row.id.as_ref().and_then(|id| ids.get(id)) {
        return Class::Skip(IssueReason::SameId { trashed: *trashed });
    }
    if let Some(title) = row.isbn13.as_ref().and_then(|i| isbns.get(i)) {
        return Class::Skip(IssueReason::RegisteredIsbn {
            existing_title: title.clone(),
        });
    }
    let first = row
        .isbn13
        .as_ref()
        .and_then(|i| seen_isbn.get(i))
        .or_else(|| row.id.as_ref().and_then(|i| seen_id.get(i)));
    if let Some(first_line) = first {
        return Class::Skip(IssueReason::DuplicateInFile {
            first_line: *first_line,
        });
    }
    Class::Import
}

/// 取り込みの見積もりを作る。DB は読むだけで書かない。
pub fn plan(conn: &Connection, bytes: &[u8], options: &CsvImportOptions) -> Result<CsvImportPlan> {
    let (table, pairs) = load(bytes, options)?;
    Ok(analyze(conn, &table, &pairs)?.plan)
}
