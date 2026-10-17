//! キーワード検索（書名・著者。FR-META-12。0025）。
//!
//! - `search_metadata`: 取得元 1 つに検索して、結果の一覧を返す（DB にも書影にも触らない）。通信の失敗・該当なしは
//!   エラーにせず、`outcome` で返す（ISBN の取得の `ProviderReport` と同じ形）。条件が正しくない・その取得元が使えない
//!   ときだけ `validation` のエラー。
//! - `fetch_search_hit`: 結果から選んだ 1 件を、登録フォームの下書きにする（`FetchMetadataResult`。DB には書かない）。
//!   ISBN があれば `fetch_metadata` と同じ流れで取り直し（キャッシュ・有効な取得元・優先順位は同じ）、
//!   選んだ取得元が取れなかったときだけ、検索結果の値を足す（`fetch::add_candidate`）。ISBN がなければ、検索結果の
//!   1 件だけを、和書の優先順位で統合して下書きにする（書影は、その URL から取れれば保存する）。

use std::time::Instant;

use bibcontainer_metadata::isbn::is_valid_isbn13;
use bibcontainer_metadata::{
    Candidate, PriorityProfile, ProviderId, ProviderOutcome, SearchError, SearchHit, SearchQuery,
    merge,
};
use serde::Serialize;
use ts_rs::TS;

use super::draft::{MetaStatus, build_draft};
use super::fetch::FetchMetadataResult;
use super::state::MetadataState;
use crate::error::AppError;

/// `search_metadata` の結果。取得元 1 つ分。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderSearchResult {
    pub provider: ProviderId,
    /// 1 件以上あれば `found`、0 件なら `notFound`。通信・HTTP・解析の失敗は、それぞれのエラーの種類（`hits` は空）。
    pub outcome: ProviderOutcome,
    /// 該当の総件数（取得元が教えてくれるとき。`hits` より多いことがある）
    pub total: Option<u64>,
    pub hits: Vec<SearchHit>,
    /// 問い合わせにかかった時間（ミリ秒）。レート制限の待ちを含む。
    pub elapsed_ms: u64,
}

impl MetadataState {
    /// 取得元 1 つに、書名・著者で検索する。条件は整えてから（NFKC・空白）問い合わせる。
    ///
    /// エラー（`validation`）: 書名も著者もない・長すぎる、取得元が使えない（無効、API キーが未設定）、
    /// 取得元がキーワード検索に対応していない（openBD）。
    pub async fn search_metadata(
        &self,
        provider: ProviderId,
        title: Option<String>,
        author: Option<String>,
    ) -> Result<ProviderSearchResult, AppError> {
        let query = SearchQuery { title, author }
            .validated()
            .map_err(search_error_to_app)?;
        let active = self.active().await?;
        let info = active
            .info(provider)
            .ok_or_else(|| AppError::Validation("この取得元は登録されていません".to_owned()))?;
        if !active.active_providers.contains(&provider) {
            return Err(AppError::Validation(format!(
                "{} は使えません（設定で無効になっているか、API キーが設定されていません）",
                info.display_name
            )));
        }
        if !info.supports_search {
            return Err(AppError::Validation(format!(
                "{} はキーワード検索に対応していません",
                info.display_name
            )));
        }

        let started = Instant::now();
        let searched = active.service.search(provider, &query).await;
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let result = match searched {
            Ok(found) => ProviderSearchResult {
                provider,
                outcome: if found.hits.is_empty() {
                    ProviderOutcome::NotFound
                } else {
                    ProviderOutcome::Found
                },
                total: found.total,
                hits: found.hits,
                elapsed_ms,
            },
            // 通信・HTTP・解析の失敗は、エラーにせず状態で返す（画面が取得元ごとに表示する）
            Err(SearchError::Fetch(e)) => {
                tracing::warn!(provider = provider.as_str(), error = %e, "キーワード検索に失敗しました");
                ProviderSearchResult {
                    provider,
                    outcome: ProviderOutcome::from_fetch_error(e),
                    total: None,
                    hits: Vec::new(),
                    elapsed_ms,
                }
            }
            Err(other) => return Err(search_error_to_app(other)),
        };
        tracing::info!(
            provider = provider.as_str(),
            hits = result.hits.len(),
            total = ?result.total,
            elapsed_ms,
            "キーワード検索をしました"
        );
        Ok(result)
    }

    /// 検索結果の 1 件から、登録フォームの下書きを作る（DB には書かない）。
    ///
    /// - ISBN あり: `fetch_metadata` と同じ流れで取得する（`fetch_isbn13`）。選んだ取得元が「取れた」にならなかった
    ///   ときだけ、検索結果の値を、その取得元の候補として足して統合し直す。取得状態は、ISBN での取得の結果で決める
    ///   （オフラインなら `pending`。値は検索結果のものが入っている）。
    /// - ISBN なし: 検索結果の 1 件だけを、和書の優先順位（設定）で統合する。書影は、候補の URL から取れれば保存し、
    ///   取れなければなし（エラーにしない）。取得状態は `ok` を入れておく（ISBN のない本の下書きでは、画面が外す）。
    pub async fn fetch_search_hit(&self, hit: SearchHit) -> Result<FetchMetadataResult, AppError> {
        let (isbn13, candidate) = checked_hit(hit)?;
        if let Some(isbn13) = isbn13 {
            return self.fetch_isbn13(&isbn13, None, Some(candidate)).await;
        }

        let active = self.active().await?;
        let profile = PriorityProfile::Japanese;
        let priorities = active
            .settings
            .priorities
            .for_profile(profile)
            .normalized(profile);
        let merged = merge("", &[candidate], None, &priorities);
        let cover = self.fetch_cover(&active, &merged).await;
        let draft = build_draft(&merged, None, cover.saved(), MetaStatus::Ok);
        tracing::info!(
            cover = cover.hash.is_some(),
            "ISBN のない検索結果から下書きを作りました"
        );
        Ok(FetchMetadataResult {
            merged,
            draft,
            reports: Vec::new(),
            offline: false,
            cover_hash: cover.hash,
        })
    }
}

/// 画面から届いた検索結果の 1 件を確かめて、使う形にする（ISBN-13 と、整えた候補）。
///
/// - 候補の取得元が、結果の取得元と同じであること。
/// - ISBN があれば、チェックデジットの合う ISBN-13 であること（ISBN-10 やハイフン付きは受けない。検索が直してある）。
/// - 候補の ISBN は、結果の ISBN（なければ空）にそろえる。
/// - 書影は https の URL だけを残し、取得元は結果の取得元にそろえる（レート制限とダミー画像の判定が、取得元で決まるため）。
fn checked_hit(hit: SearchHit) -> Result<(Option<String>, Candidate), AppError> {
    let SearchHit {
        provider,
        isbn13,
        mut candidate,
        ..
    } = hit;
    if candidate.provider != provider {
        return Err(AppError::Validation(
            "検索結果の取得元が食い違っています".to_owned(),
        ));
    }
    if let Some(isbn13) = &isbn13
        && !is_valid_isbn13(isbn13)
    {
        return Err(AppError::Validation(format!(
            "ISBN-13 として読めません（チェックデジットを確かめてください）: {isbn13}"
        )));
    }
    candidate.isbn13 = isbn13.clone().unwrap_or_default();
    candidate.covers.retain(|c| c.url.starts_with("https://"));
    for cover in &mut candidate.covers {
        cover.provider = provider;
    }
    Ok((isbn13, candidate))
}

fn search_error_to_app(e: SearchError) -> AppError {
    match e {
        SearchError::InvalidQuery(message) => AppError::Validation(message),
        SearchError::Unsupported => {
            AppError::Validation("この取得元はキーワード検索に対応していません".to_owned())
        }
        // 呼び出し側が、状態として返す（`search_metadata`）。ここに来るのは想定外
        SearchError::Fetch(e) => AppError::Network(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use bibcontainer_metadata::CandidateCover;

    use super::*;

    fn hit(provider: ProviderId, isbn13: Option<&str>) -> SearchHit {
        SearchHit {
            provider,
            isbn13: isbn13.map(str::to_owned),
            candidate: Candidate::empty(provider, "stale"),
            link: None,
        }
    }

    #[test]
    fn a_hit_is_checked_and_its_candidate_is_made_to_agree_with_it() {
        let mut h = hit(ProviderId::Google, Some("9784003101117"));
        h.candidate.covers = vec![
            CandidateCover {
                provider: ProviderId::Ndl,
                url: "https://books.google.com/a.jpg".into(),
            },
            CandidateCover {
                provider: ProviderId::Google,
                url: "http://books.google.com/b.jpg".into(),
            },
            CandidateCover {
                provider: ProviderId::Google,
                url: "file:///etc/passwd".into(),
            },
            CandidateCover {
                provider: ProviderId::Google,
                url: "javascript:alert(1)".into(),
            },
        ];
        let (isbn, c) = checked_hit(h).unwrap();
        assert_eq!(isbn.as_deref(), Some("9784003101117"));
        assert_eq!(
            c.isbn13, "9784003101117",
            "候補の ISBN は結果の ISBN にそろえる"
        );
        assert_eq!(c.covers.len(), 1, "https の URL だけ残す");
        assert_eq!(c.covers[0].url, "https://books.google.com/a.jpg");
        assert_eq!(
            c.covers[0].provider,
            ProviderId::Google,
            "書影の取得元も、結果の取得元に直す"
        );

        let (isbn, c) = checked_hit(hit(ProviderId::Ndl, None)).unwrap();
        assert_eq!(isbn, None);
        assert_eq!(c.isbn13, "", "ISBN がなければ空");
    }

    #[test]
    fn a_hit_that_disagrees_with_itself_or_has_a_bad_isbn_is_rejected() {
        let mut other_provider = hit(ProviderId::Ndl, None);
        other_provider.candidate.provider = ProviderId::Google;
        assert!(matches!(
            checked_hit(other_provider),
            Err(AppError::Validation(_))
        ));
        for bad in [
            "9784003101118", // チェックデジットが違う
            "4003101111",    // ISBN-10
            "978-4-00-310111-7",
            "",
            "97840031011",
            "abcdefghijklm",
        ] {
            assert!(
                matches!(
                    checked_hit(hit(ProviderId::Ndl, Some(bad))),
                    Err(AppError::Validation(_))
                ),
                "{bad:?}"
            );
        }
    }
}
