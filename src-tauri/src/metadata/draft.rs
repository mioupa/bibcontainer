//! 書誌の統合結果（`MetaMerged`）から、登録フォームに渡す下書き（`BookDraft`）を作る。
//!
//! - 項目名は書籍の Op の項目名（`title`、`title_kana`、`c_code`、`list_price` など。snake_case）。
//!   値は `{ value, origin }`。`origin` はコアの `Origin` の形（`{ kind: "barcode" }`、
//!   `{ kind: "provider", name: "openbd" }` など）。
//! - 2段目の Cコード・本体価格と ISBN の `origin` は `barcode`。それ以外は採用された取得元（`provider`）。
//! - シリーズは名前で持つ（`seriesName`）。保存するときに `series_id` に解決する。
//! - 取得状態（`meta_status`）も項目の 1 つとして入れてある。下書きの項目を順に `FieldChange` にすれば、
//!   そのまま書籍の作成・更新に渡せる（`DraftFields::to_changes`）。
//!
//! 取得状態の決め方は `decide_status`（0007 の申し送りと 0010 に従う）。

use bibcontainer_core::Author;
use bibcontainer_core::library::{FieldChange, Origin};
use bibcontainer_core::volume::volume_sort;
use bibcontainer_metadata::{
    CandidateAuthor, MetaFetchResult, MetaField, MetaMerged, MetaSource, ProviderId,
    ProviderOutcome,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 書誌の取得状態（`books.meta_status`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MetaStatus {
    /// 取得できた
    Ok,
    /// 取得待ち（通信できなかった。あとで自動で再取得する）
    Pending,
    /// 一部だけ取得できた（取得元の一部が失敗した、書名が取れなかった）
    Partial,
    /// どの取得元にも見つからなかった（再取得は手動）
    Failed,
}

impl MetaStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MetaStatus::Ok => "ok",
            MetaStatus::Pending => "pending",
            MetaStatus::Partial => "partial",
            MetaStatus::Failed => "failed",
        }
    }

    /// `books.meta_status` の文字列から。知らない値は `Ok`（コアの読み出しと同じ補正）。
    pub fn from_db(s: &str) -> Self {
        match s {
            "pending" => MetaStatus::Pending,
            "partial" => MetaStatus::Partial,
            "failed" => MetaStatus::Failed,
            _ => MetaStatus::Ok,
        }
    }
}

/// 値とその由来。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DraftValue<T> {
    pub value: T,
    pub origin: Origin,
}

impl<T> DraftValue<T> {
    fn new(value: T, origin: Origin) -> Self {
        DraftValue { value, origin }
    }
}

/// シリーズ名とその由来。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DraftSeries {
    pub name: String,
    pub origin: Origin,
}

/// 下書きの項目。キーは書籍の Op の項目名（snake_case）。値がない項目は含まれない。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub isbn13: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title_kana: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subtitle: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub authors: Option<DraftValue<Vec<Author>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub publisher: Option<DraftValue<String>>,
    /// `YYYY` / `YYYY-MM` / `YYYY-MM-DD`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub published_on: Option<DraftValue<String>>,
    /// 4 桁
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub c_code: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ndc: Option<DraftValue<String>>,
    /// 本体価格（税抜・円）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub list_price: Option<DraftValue<i64>>,
    /// 参考の税込価格（円）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ref_price_incl: Option<DraftValue<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pages: Option<DraftValue<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub size_text: Option<DraftValue<String>>,
    /// ISO 639-1（`ja` など）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<DraftValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub volume: Option<DraftValue<String>>,
    /// 並べ替え用の巻次の数値（`volume` から作る。読み取れなければない）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub volume_sort: Option<DraftValue<f64>>,
    /// 保存した書影のハッシュ（SHA-256。小文字の 16 進 64 桁）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cover_hash: Option<DraftValue<String>>,
    /// 取得状態。`origin` に意味はない（コアは記録を付けない項目）。
    pub meta_status: DraftValue<MetaStatus>,
}

/// 登録フォームに渡す下書き。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BookDraft {
    pub fields: DraftFields,
    /// シリーズ名。保存するとき（`create_books_from_drafts`）に、名前でシリーズを探して（なければ作って）`series_id` にする。
    pub series_name: Option<DraftSeries>,
}

/// 取得状態の項目に付ける由来（記録を付けない項目なので、値に意味はない）。
fn app_origin() -> Origin {
    Origin::Provider("bibcontainer".to_owned())
}

/// 統合結果の取得元 → 値の由来。2段目は `barcode`、ほかは取得元の名前。
pub fn origin_of(source: MetaSource) -> Origin {
    match source {
        MetaSource::Barcode => Origin::Barcode,
        other => Origin::Provider(other.as_str().to_owned()),
    }
}

fn author_of(a: &CandidateAuthor) -> Author {
    Author {
        name: a.name.clone(),
        kana: a.kana.clone(),
        role: a.role.clone(),
    }
}

/// 保存した書影（ハッシュと、その画像を取れた取得元）。
#[derive(Debug, Clone, Copy)]
pub struct SavedCover<'a> {
    pub hash: &'a str,
    pub provider: ProviderId,
}

/// 統合結果から下書きを作る。
///
/// - `isbn13`: 問い合わせた ISBN-13（`barcode` 由来）。手動登録（ISBN なし）では `None`。
/// - `cover`: 取得して保存した書影。
pub fn build_draft(
    merged: &MetaMerged,
    isbn13: Option<&str>,
    cover: Option<SavedCover<'_>>,
    status: MetaStatus,
) -> BookDraft {
    let origin = |f: MetaField| merged.source(f).map(origin_of);
    let text = |f: MetaField| -> Option<DraftValue<String>> {
        Some(DraftValue::new(merged.text(f)?.to_owned(), origin(f)?))
    };
    let number = |f: MetaField| -> Option<DraftValue<i64>> {
        Some(DraftValue::new(merged.number(f)?, origin(f)?))
    };
    let authors = merged.authors();
    let volume = text(MetaField::Volume);
    let volume_sort = volume
        .as_ref()
        .and_then(|v| volume_sort(&v.value).map(|n| DraftValue::new(n, v.origin.clone())));
    let fields = DraftFields {
        isbn13: isbn13.map(|i| DraftValue::new(i.to_owned(), Origin::Barcode)),
        title: text(MetaField::Title),
        title_kana: text(MetaField::TitleKana),
        subtitle: text(MetaField::Subtitle),
        authors: (!authors.is_empty())
            .then(|| {
                Some(DraftValue::new(
                    authors.iter().map(author_of).collect(),
                    origin(MetaField::Authors)?,
                ))
            })
            .flatten(),
        publisher: text(MetaField::Publisher),
        published_on: text(MetaField::PublishedOn),
        c_code: text(MetaField::CCode),
        ndc: text(MetaField::Ndc),
        list_price: number(MetaField::ListPrice),
        ref_price_incl: number(MetaField::RefPriceIncl),
        pages: number(MetaField::Pages),
        size_text: text(MetaField::SizeText),
        language: text(MetaField::Language),
        description: text(MetaField::Description),
        volume,
        volume_sort,
        cover_hash: cover.map(|c| {
            DraftValue::new(
                c.hash.to_owned(),
                Origin::Provider(MetaSource::from(c.provider).as_str().to_owned()),
            )
        }),
        meta_status: DraftValue::new(status, app_origin()),
    };
    let series_name = text(MetaField::Series).map(|s| DraftSeries {
        name: s.value,
        origin: s.origin,
    });
    BookDraft {
        fields,
        series_name,
    }
}

impl DraftFields {
    /// 値のある項目を `FieldChange` にする（`create_book_in` や `update_book_in` に渡せる）。
    pub fn to_changes(&self) -> Vec<FieldChange> {
        fn push<T: Serialize>(out: &mut Vec<FieldChange>, field: &str, v: &Option<DraftValue<T>>) {
            if let Some(v) = v
                && let Ok(value) = serde_json::to_value(&v.value)
            {
                out.push(FieldChange::new(field, value, v.origin.clone()));
            }
        }
        let mut out = Vec::new();
        push(&mut out, "isbn13", &self.isbn13);
        push(&mut out, "title", &self.title);
        push(&mut out, "title_kana", &self.title_kana);
        push(&mut out, "subtitle", &self.subtitle);
        push(&mut out, "authors", &self.authors);
        push(&mut out, "publisher", &self.publisher);
        push(&mut out, "published_on", &self.published_on);
        push(&mut out, "c_code", &self.c_code);
        push(&mut out, "ndc", &self.ndc);
        push(&mut out, "list_price", &self.list_price);
        push(&mut out, "ref_price_incl", &self.ref_price_incl);
        push(&mut out, "pages", &self.pages);
        push(&mut out, "size_text", &self.size_text);
        push(&mut out, "language", &self.language);
        push(&mut out, "description", &self.description);
        push(&mut out, "volume", &self.volume);
        push(&mut out, "volume_sort", &self.volume_sort);
        push(&mut out, "cover_hash", &self.cover_hash);
        out.push(FieldChange::new(
            "meta_status",
            self.meta_status.value.as_str(),
            self.meta_status.origin.clone(),
        ));
        out
    }
}

// ---------------------------------------------------------------------------
// 取得状態（meta_status）
// ---------------------------------------------------------------------------

/// 取得の結果から、書籍の取得状態を決める（0007 の申し送り。AT-11）。
///
/// 書誌で通信する取得元（`ProviderReport::network`）の結果だけで決める。
///
/// | 状況 | 状態 |
/// |---|---|
/// | 取得元を 1 つも使わなかった（全部無効・キーなし） | `pending`（何も試していない。取得元を有効にすれば取り直す） |
/// | 通信する取得元がすべて通信エラー（`offline`） | `pending` |
/// | 1 つも取れず、すべて「該当なし」 | `failed`（再取得は手動） |
/// | 1 つも取れず、エラー（HTTP・解析）がある | `pending`（あとで取り直す） |
/// | 取れたが、一部の取得元がエラー、または書名が取れていない | `partial` |
/// | 取れて、エラーなし | `ok` |
///
/// そのうえで、書影の取得で全 URL が通信エラー（`cover_offline`）なら、`ok`・`partial` も `pending` にする
/// （書影をあとで取り直すため）。
pub fn decide_status(result: &MetaFetchResult, cover_offline: bool) -> MetaStatus {
    let network: Vec<_> = result.reports.iter().filter(|r| r.network).collect();
    if network.is_empty() || result.offline {
        return MetaStatus::Pending;
    }
    let found = network
        .iter()
        .filter(|r| r.outcome == ProviderOutcome::Found)
        .count();
    let has_error = network.iter().any(|r| {
        matches!(
            r.outcome,
            ProviderOutcome::NetworkError { .. }
                | ProviderOutcome::HttpError { .. }
                | ProviderOutcome::ParseError { .. }
        )
    });
    if found == 0 {
        return if has_error {
            MetaStatus::Pending
        } else {
            MetaStatus::Failed
        };
    }
    let title_missing = result.merged.text(MetaField::Title).is_none();
    if cover_offline {
        MetaStatus::Pending
    } else if has_error || title_missing {
        MetaStatus::Partial
    } else {
        MetaStatus::Ok
    }
}

/// 再取得で、書籍の今の状態（`current`）と今回の判断（`decided`）から、書く状態を決める。
///
/// すでに書誌がある本（`ok`・`partial`）は、今回たまたま取れなかった（`failed`・`pending`）だけでは
/// 状態を悪くしない。
pub fn status_after_refetch(current: MetaStatus, decided: MetaStatus) -> MetaStatus {
    match (current, decided) {
        (MetaStatus::Ok | MetaStatus::Partial, MetaStatus::Failed | MetaStatus::Pending) => current,
        _ => decided,
    }
}

#[cfg(test)]
mod tests {
    use bibcontainer_metadata::providers::{GoogleBooks, Ndl, OpenBd};
    use bibcontainer_metadata::{
        Candidate, MetaPriorities, MetaSecondTier, PriorityProfile, Provider, ProviderReport,
        RawResponse, merge,
    };

    use super::*;

    const ONIGIRI: &str = "9784780802047";
    const KOKORO: &str = "9784003101117";
    const ONE_PIECE: &str = "9784088725093";

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../crates/metadata/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn openbd(isbn: &str) -> Candidate {
        let raw = RawResponse {
            provider: ProviderId::Openbd,
            isbn13: isbn.into(),
            body: fixture(&format!("openbd_{isbn}.json")),
        };
        OpenBd::new().parse(&raw).unwrap().unwrap()
    }

    fn ndl(isbn: &str) -> Candidate {
        let raw = RawResponse {
            provider: ProviderId::Ndl,
            isbn13: isbn.into(),
            body: fixture(&format!("ndl_{isbn}.xml")),
        };
        Ndl::new().parse(&raw).unwrap().unwrap()
    }

    fn google(isbn: &str) -> Candidate {
        let raw = RawResponse {
            provider: ProviderId::Google,
            isbn13: isbn.into(),
            body: fixture(&format!("google_synthetic_{isbn}.json")),
        };
        GoogleBooks::new(None).parse(&raw).unwrap().unwrap()
    }

    fn provider(name: &str) -> Origin {
        Origin::Provider(name.into())
    }

    #[test]
    fn at10_draft_has_title_publisher_c_code_and_origins_from_the_provider() {
        let merged = merge(
            ONIGIRI,
            &[openbd(ONIGIRI), ndl(ONIGIRI), google(ONIGIRI)],
            None,
            &MetaPriorities::default(),
        );
        let cover = SavedCover {
            hash: &"a".repeat(64),
            provider: ProviderId::Openbd,
        };
        let draft = build_draft(&merged, Some(ONIGIRI), Some(cover), MetaStatus::Ok);
        let f = &draft.fields;
        assert_eq!(f.title.as_ref().unwrap().value, "おにぎりレシピ101");
        assert_eq!(f.title.as_ref().unwrap().origin, provider("openbd"));
        assert_eq!(f.publisher.as_ref().unwrap().value, "ポット出版");
        assert_eq!(f.publisher.as_ref().unwrap().origin, provider("openbd"));
        assert_eq!(f.c_code.as_ref().unwrap().value, "0077");
        assert_eq!(f.c_code.as_ref().unwrap().origin, provider("openbd"));
        // 書名のヨミと NDC は NDL
        assert_eq!(f.title_kana.as_ref().unwrap().origin, provider("ndl"));
        assert_eq!(f.ndc.as_ref().unwrap().origin, provider("ndl"));
        // ページ数は NDL が先
        assert_eq!(f.pages.as_ref().unwrap().origin, provider("ndl"));
        // ISBN は barcode
        let isbn = f.isbn13.as_ref().unwrap();
        assert_eq!(isbn.value, ONIGIRI);
        assert_eq!(isbn.origin, Origin::Barcode);
        // 書影は保存した取得元
        let c = f.cover_hash.as_ref().unwrap();
        assert_eq!(c.value, "a".repeat(64));
        assert_eq!(c.origin, provider("openbd"));
        // 著者は役割付き
        let authors = &f.authors.as_ref().unwrap().value;
        assert!(!authors.is_empty());
        assert!(authors.iter().all(|a| !a.name.is_empty()));
        assert_eq!(f.meta_status.value, MetaStatus::Ok);
    }

    #[test]
    fn second_tier_values_have_the_barcode_origin() {
        let tier = MetaSecondTier {
            c_code: Some("0193".into()),
            list_price: Some(700),
        };
        let merged = merge(
            KOKORO,
            &[openbd(KOKORO), ndl(KOKORO)],
            Some(&tier),
            &MetaPriorities::default(),
        );
        let draft = build_draft(&merged, Some(KOKORO), None, MetaStatus::Ok);
        let c = draft.fields.c_code.as_ref().unwrap();
        assert_eq!((c.value.as_str(), &c.origin), ("0193", &Origin::Barcode));
        let p = draft.fields.list_price.as_ref().unwrap();
        assert_eq!((p.value, &p.origin), (700, &Origin::Barcode));
        // 2段目以外の項目は取得元
        assert_eq!(
            draft.fields.title.as_ref().unwrap().origin,
            provider("openbd")
        );
        assert!(draft.fields.cover_hash.is_none());
    }

    #[test]
    fn offline_draft_keeps_only_isbn_and_second_tier() {
        let tier = MetaSecondTier {
            c_code: Some("0193".into()),
            list_price: None,
        };
        let merged = merge(KOKORO, &[], Some(&tier), &MetaPriorities::default());
        let draft = build_draft(&merged, Some(KOKORO), None, MetaStatus::Pending);
        let changes = draft.fields.to_changes();
        let names: Vec<&str> = changes.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(names, vec!["isbn13", "c_code", "meta_status"]);
        assert_eq!(changes[0].origin, Origin::Barcode);
        assert_eq!(changes[1].origin, Origin::Barcode);
        assert_eq!(changes[2].value, serde_json::json!("pending"));
        assert!(draft.series_name.is_none());
    }

    #[test]
    fn series_and_volume_come_from_the_provider_and_volume_sort_is_derived() {
        let merged = merge(
            ONE_PIECE,
            &[openbd(ONE_PIECE), ndl(ONE_PIECE)],
            None,
            &MetaPriorities::default(),
        );
        let draft = build_draft(&merged, Some(ONE_PIECE), None, MetaStatus::Ok);
        let series = draft.series_name.expect("シリーズ名がある");
        assert!(!series.name.is_empty());
        assert!(matches!(series.origin, Origin::Provider(_)));
        let volume = draft.fields.volume.as_ref().expect("巻次がある");
        let sort = draft.fields.volume_sort.as_ref().expect("巻次の数値がある");
        assert_eq!(sort.origin, volume.origin, "巻次と同じ由来");
        assert_eq!(sort.value, 1.0, "{}", volume.value);
        // 書名から巻次は除かれている
        assert_eq!(draft.fields.title.as_ref().unwrap().value, "One piece");
    }

    #[test]
    fn to_changes_covers_every_filled_field_with_op_names() {
        let merged = merge(
            ONIGIRI,
            &[openbd(ONIGIRI), ndl(ONIGIRI)],
            None,
            &MetaPriorities::default(),
        );
        let draft = build_draft(&merged, Some(ONIGIRI), None, MetaStatus::Partial);
        let changes = draft.fields.to_changes();
        let get = |n: &str| changes.iter().find(|c| c.field == n);
        assert_eq!(
            get("title").unwrap().value,
            serde_json::json!("おにぎりレシピ101")
        );
        assert!(get("authors").unwrap().value.is_array());
        assert_eq!(
            get("meta_status").unwrap().value,
            serde_json::json!("partial")
        );
        assert!(get("series_id").is_none(), "シリーズは名前で持つ");
        // どの変更も、コアが知っている書籍の項目名
        for c in &changes {
            assert!(
                bibcontainer_core::Entity::Book.field(&c.field).is_some(),
                "{}",
                c.field
            );
        }
    }

    #[test]
    fn draft_json_shape_for_the_frontend() {
        let merged = merge(KOKORO, &[openbd(KOKORO)], None, &MetaPriorities::default());
        let draft = build_draft(&merged, Some(KOKORO), None, MetaStatus::Ok);
        let json = serde_json::to_value(&draft).unwrap();
        assert_eq!(json["fields"]["isbn13"]["value"], KOKORO);
        assert_eq!(
            json["fields"]["isbn13"]["origin"],
            serde_json::json!({ "kind": "barcode" })
        );
        assert_eq!(
            json["fields"]["title"]["origin"],
            serde_json::json!({ "kind": "provider", "name": "openbd" })
        );
        assert_eq!(json["fields"]["meta_status"]["value"], "ok");
        // 値がない項目はキーごと出ない
        assert!(json["fields"].get("cover_hash").is_none());
        // シリーズは名前と由来で持つ（`series_id` ではない）
        assert_eq!(
            json["seriesName"],
            serde_json::json!({ "name": "岩波文庫", "origin": { "kind": "provider", "name": "openbd" } })
        );
        assert!(json["fields"].get("series_id").is_none());
    }

    // ----- 取得状態 -----

    fn report(provider: ProviderId, outcome: ProviderOutcome) -> ProviderReport {
        ProviderReport {
            provider,
            outcome,
            from_cache: false,
            network: true,
        }
    }

    fn net_err() -> ProviderOutcome {
        ProviderOutcome::NetworkError {
            message: "offline".into(),
        }
    }

    fn http_err() -> ProviderOutcome {
        ProviderOutcome::HttpError {
            status: 500,
            message: "x".into(),
        }
    }

    /// 取得元ごとの結果から、`MetaFetchResult` を作る（`Found` には、その取得元の実際の応答の候補を使う）。
    fn result(outcomes: Vec<(ProviderId, ProviderOutcome)>, with_title: bool) -> MetaFetchResult {
        let mut candidates = Vec::new();
        for (p, o) in &outcomes {
            if *o == ProviderOutcome::Found {
                let mut c = Candidate::empty(*p, ONIGIRI);
                c.title = with_title.then(|| "題".to_owned());
                candidates.push(c);
            }
        }
        let network_reports: Vec<ProviderReport> =
            outcomes.into_iter().map(|(p, o)| report(p, o)).collect();
        let offline = !network_reports.is_empty()
            && network_reports
                .iter()
                .all(|r| matches!(r.outcome, ProviderOutcome::NetworkError { .. }));
        MetaFetchResult {
            isbn13: ONIGIRI.into(),
            merged: merge(ONIGIRI, &candidates, None, &MetaPriorities::default()),
            profile: PriorityProfile::Japanese,
            candidates,
            reports: network_reports,
            offline,
        }
    }

    use ProviderId::{Google, Ndl as NdlId, Openbd};
    use ProviderOutcome::{Found, NotFound};

    #[test]
    fn every_provider_down_means_pending() {
        let r = result(
            vec![(Openbd, net_err()), (NdlId, net_err()), (Google, net_err())],
            false,
        );
        assert!(r.offline);
        assert_eq!(decide_status(&r, false), MetaStatus::Pending);
    }

    #[test]
    fn partial_when_only_some_providers_failed_or_there_is_no_title() {
        let r = result(
            vec![(Openbd, Found), (NdlId, net_err()), (Google, NotFound)],
            true,
        );
        assert_eq!(decide_status(&r, false), MetaStatus::Partial);
        let r = result(vec![(Openbd, Found), (NdlId, http_err())], true);
        assert_eq!(decide_status(&r, false), MetaStatus::Partial);
        // 取れた取得元に書名がない
        let r = result(vec![(Openbd, Found), (NdlId, NotFound)], false);
        assert_eq!(decide_status(&r, false), MetaStatus::Partial);
    }

    #[test]
    fn ok_when_found_without_errors_even_if_other_providers_have_no_record() {
        let r = result(
            vec![(Openbd, Found), (NdlId, NotFound), (Google, NotFound)],
            true,
        );
        assert_eq!(decide_status(&r, false), MetaStatus::Ok);
        let r = result(vec![(Openbd, NotFound), (NdlId, Found)], true);
        assert_eq!(decide_status(&r, false), MetaStatus::Ok);
    }

    #[test]
    fn failed_when_every_provider_says_not_found() {
        let r = result(
            vec![(Openbd, NotFound), (NdlId, NotFound), (Google, NotFound)],
            false,
        );
        assert!(!r.offline);
        assert_eq!(decide_status(&r, false), MetaStatus::Failed);
    }

    #[test]
    fn nothing_found_with_a_server_error_is_retried_later() {
        let r = result(vec![(Openbd, NotFound), (NdlId, http_err())], false);
        assert_eq!(decide_status(&r, false), MetaStatus::Pending);
        let r = result(vec![(Openbd, NotFound), (NdlId, net_err())], false);
        assert!(!r.offline, "一部だけ通信エラーならオフラインとは言わない");
        assert_eq!(decide_status(&r, false), MetaStatus::Pending);
    }

    #[test]
    fn no_provider_used_means_pending_and_cover_offline_downgrades_to_pending() {
        let r = result(vec![], false);
        assert_eq!(decide_status(&r, false), MetaStatus::Pending);

        let r = result(vec![(Openbd, Found), (NdlId, Found)], true);
        assert_eq!(decide_status(&r, false), MetaStatus::Ok);
        assert_eq!(decide_status(&r, true), MetaStatus::Pending);
    }

    #[test]
    fn providers_that_do_not_use_the_network_do_not_count() {
        let mut r = result(vec![(Openbd, NotFound)], false);
        // 通信しない取得元が成功扱いでも、書誌の判断には入れない
        r.reports.push(ProviderReport {
            provider: NdlId,
            outcome: Found,
            from_cache: false,
            network: false,
        });
        assert_eq!(decide_status(&r, false), MetaStatus::Failed);
    }

    #[test]
    fn a_book_that_already_has_data_keeps_its_status_when_the_refetch_finds_nothing() {
        use MetaStatus::{Failed, Ok as Good, Partial, Pending};
        assert_eq!(status_after_refetch(Good, Failed), Good);
        assert_eq!(status_after_refetch(Partial, Pending), Partial);
        assert_eq!(status_after_refetch(Pending, Failed), Failed);
        assert_eq!(status_after_refetch(Pending, Good), Good);
        assert_eq!(status_after_refetch(Failed, Partial), Partial);
        assert_eq!(status_after_refetch(Good, Partial), Partial);
        assert_eq!(status_after_refetch(Pending, Pending), Pending);
    }

    #[test]
    fn status_strings_match_the_db_values() {
        for s in [
            MetaStatus::Ok,
            MetaStatus::Pending,
            MetaStatus::Partial,
            MetaStatus::Failed,
        ] {
            assert_eq!(MetaStatus::from_db(s.as_str()), s);
            assert_eq!(
                serde_json::to_value(s).unwrap(),
                serde_json::json!(s.as_str())
            );
        }
        assert_eq!(MetaStatus::from_db("???"), MetaStatus::Ok);
        for s in bibcontainer_core::values::META_STATUSES {
            assert_eq!(MetaStatus::from_db(s).as_str(), *s);
        }
    }
}
