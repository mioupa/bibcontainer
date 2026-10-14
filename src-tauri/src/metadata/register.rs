//! 下書きからの書籍の作成（`create_books_from_drafts`）。1 冊ずつの登録・連続登録の「まとめて保存」・手動登録（ISBN なし）が使う。
//!
//! 全体を 1 つのトランザクションで行う。1 冊でも失敗したら全体を取り消す（コアの `create_books` と同じ）。
//! シリーズは名前で受け取り、`find_or_create_series_in` で探して（なければ作って）`series_id` にする。

use bibcontainer_core::library::{Env, FieldChange, create_book_in, find_or_create_series_in};
use bibcontainer_core::{SystemClock, write_tx};
use serde::Deserialize;
use ts_rs::TS;

use super::draft::DraftSeries;
use super::state::MetadataState;
use crate::error::AppError;

/// 作る本 1 冊分。
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewBookDraft {
    /// 書籍の項目の値と由来（`FieldChange`）。フォームの内容と、下書き（`BookDraft.fields`）の由来を反映したもの。
    pub changes: Vec<FieldChange>,
    /// シリーズ名と由来。あれば、名前でシリーズを探して `series_id` にする（`changes` の `series_id` より優先する）。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub series_name: Option<DraftSeries>,
}

/// 本の変更に、シリーズ名から `series_id` を足す。巻次から作る `volume_sort` は、コアの `create_book_in` が
/// 足す（書籍の作成・更新に共通の規則）。
fn resolve(
    tx: &rusqlite::Transaction<'_>,
    env: &Env<'_>,
    draft: NewBookDraft,
) -> Result<(Vec<FieldChange>, bool), AppError> {
    let mut changes = draft.changes;

    let mut touched_series = false;
    if let Some(series) = draft.series_name
        && !series.name.trim().is_empty()
    {
        let id = find_or_create_series_in(tx, env, &series.name)?;
        changes.retain(|c| c.field != "series_id");
        changes.push(FieldChange::new("series_id", id, series.origin));
        touched_series = true;
    }
    Ok((changes, touched_series))
}

impl MetadataState {
    /// 本を作る。新しい本の ID を、渡した順に返す。成功したら `library-changed` を出す。
    pub async fn create_books_from_drafts(
        &self,
        drafts: Vec<NewBookDraft>,
    ) -> Result<Vec<String>, AppError> {
        if drafts.is_empty() {
            return Ok(Vec::new());
        }
        let (ids, series_touched) = self
            .db
            .run(move |conn, _| {
                let clock = SystemClock;
                let env = Env::new(&clock);
                let tx = write_tx(conn)?;
                let mut ids = Vec::with_capacity(drafts.len());
                let mut series_touched = false;
                for draft in drafts {
                    let (changes, touched) = resolve(&tx, &env, draft)?;
                    series_touched |= touched;
                    ids.push(create_book_in(&tx, &env, &changes)?);
                }
                tx.commit().map_err(bibcontainer_core::Error::from)?;
                Ok((ids, series_touched))
            })
            .await?;
        tracing::info!(count = ids.len(), "書籍を作成しました");
        (self.notifier)(ids.clone(), series_touched);
        Ok(ids)
    }
}
