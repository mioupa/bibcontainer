//! キーワード検索（`search_metadata`・`fetch_search_hit`）の結合テスト（偽の HTTP と一時 DB）。
//! 応答は `crates/metadata/tests/fixtures/` のもの（NDL・Open Library は実際の応答、Google は合成）を、
//! `testing::online_handler` が書名の語で選んで返す。

use bibcontainer_core::library::Origin;
use bibcontainer_metadata::cover::cover_path;
use bibcontainer_metadata::http::{HttpResponse, TransportError};
use bibcontainer_metadata::{CandidateCover, ProviderId, ProviderOutcome, SearchHit};
use serde_json::json;

use super::draft::MetaStatus;
use super::fetch::FetchMetadataResult;
use super::register::NewBookDraft;
use super::search::ProviderSearchResult;
use super::testing::*;
use crate::error::{AppError, ErrorKind};

fn search(
    rig: &Rig,
    provider: ProviderId,
    title: Option<&str>,
    author: Option<&str>,
) -> Result<ProviderSearchResult, AppError> {
    rig.block(rig.state.search_metadata(
        provider,
        title.map(str::to_owned),
        author.map(str::to_owned),
    ))
}

fn found(rig: &Rig, provider: ProviderId, title: &str) -> ProviderSearchResult {
    search(rig, provider, Some(title), None).unwrap()
}

fn hit_of<'a>(result: &'a ProviderSearchResult, isbn13: &str) -> &'a SearchHit {
    result
        .hits
        .iter()
        .find(|h| h.isbn13.as_deref() == Some(isbn13))
        .unwrap_or_else(|| panic!("{isbn13} の結果がない"))
}

fn fetch_hit(rig: &Rig, hit: &SearchHit) -> Result<FetchMetadataResult, AppError> {
    rig.block(rig.state.fetch_search_hit(hit.clone()))
}

fn enable_google(rig: &Rig) {
    rig.block(rig.state.set_api_key(ProviderId::Google, "AIza-key"))
        .unwrap();
}

fn set_enabled(rig: &Rig, id: ProviderId, on: bool) {
    let mut s = rig.block(rig.state.get_provider_settings()).unwrap();
    s.enabled.insert(id, on);
    rig.block(rig.state.set_provider_settings(s)).unwrap();
}

fn provider(name: &str) -> Origin {
    Origin::Provider(name.into())
}

/// 決めた URL だけを、決めた応答にして、ほかはふだんのフィクスチャのままにする。
fn override_http(
    rig: &Rig,
    needle: &'static str,
    reply: impl Fn() -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
) {
    let base = online_handler();
    rig.http.set_handler(Box::new(move |url| {
        if url.contains(needle) {
            reply()
        } else {
            base(url)
        }
    }));
}

// ---------------------------------------------------------------------------
// search_metadata
// ---------------------------------------------------------------------------

#[test]
fn a_search_returns_the_hits_and_writes_nothing() {
    let rig = Rig::new();
    let r = search(&rig, ProviderId::Ndl, Some("スローループ"), None).unwrap();
    assert_eq!(r.provider, ProviderId::Ndl);
    assert_eq!(r.outcome, ProviderOutcome::Found);
    assert_eq!(r.total, Some(11));
    assert_eq!(r.hits.len(), 11);
    assert!(r.hits.iter().all(|h| h.provider == ProviderId::Ndl));
    // 刊行年の新しい順（11 巻が先頭）
    assert_eq!(r.hits[0].candidate.volume.as_deref(), Some("11"));

    // 通信は NDL の検索の 1 回だけ。DB にも書影にも触らない
    assert_eq!(rig.http.urls().len(), 1);
    assert_eq!(rig.http.count("ndlsearch.ndl.go.jp/api/sru"), 1);
    assert_eq!(rig.outbox_len(), 0);
    assert_eq!(rig.changed_count(), 0);
    assert!(!rig.dir.path().join("covers").exists());
}

#[test]
fn search_terms_are_normalized_before_they_are_sent() {
    let rig = Rig::new();
    search(
        &rig,
        ProviderId::Ndl,
        Some("　スローループ　"),
        Some("うちの　まいこ"),
    )
    .unwrap();
    let cql = query_param(&rig.http.urls()[0], "query");
    assert_eq!(
        cql,
        "title = \"スローループ\" AND creator = \"うちの まいこ\" AND mediatype = \"books\" AND dpid = \"iss-ndl-opac\" AND sortBy = \"issued_date/sort.descending\""
    );
    // User-Agent は ISBN の取得と同じ（通信の層は共通）
    assert_eq!(rig.user_agents.lock().unwrap().len(), 1);
}

#[test]
fn each_searchable_provider_can_be_asked_and_reports_not_found_for_zero_hits() {
    let rig = Rig::new();
    enable_google(&rig);
    let google = found(&rig, ProviderId::Google, "clean code");
    assert_eq!(google.outcome, ProviderOutcome::Found);
    assert_eq!(google.total, Some(1234));
    assert_eq!(google.hits.len(), 6);
    let open_library = found(&rig, ProviderId::OpenLibrary, "clean code");
    assert_eq!(open_library.outcome, ProviderOutcome::Found);
    assert_eq!(open_library.total, Some(3));
    // Google はキーを付けて問い合わせる
    assert!(rig.http.urls().iter().any(|u| u.contains("&key=AIza-key")));

    for provider in [ProviderId::Ndl, ProviderId::Google, ProviderId::OpenLibrary] {
        let r = found(&rig, provider, "zzzz");
        assert_eq!(r.outcome, ProviderOutcome::NotFound, "{provider:?}");
        assert_eq!(r.total, Some(0), "{provider:?}");
        assert!(r.hits.is_empty());
    }
}

#[test]
fn network_http_and_parse_failures_are_outcomes_not_errors() {
    let rig = Rig::new();
    rig.http.set_offline();
    let r = found(&rig, ProviderId::Ndl, "スローループ");
    assert!(
        matches!(r.outcome, ProviderOutcome::NetworkError { .. }),
        "{:?}",
        r.outcome
    );
    assert!(r.hits.is_empty());
    assert_eq!(r.total, None);
    assert_eq!(rig.changed_count(), 0);

    rig.http.set_online();
    override_http(&rig, "ndlsearch.ndl.go.jp", || {
        response(500, "text/plain", b"busy".to_vec())
    });
    let r = found(&rig, ProviderId::Ndl, "スローループ");
    assert_eq!(
        r.outcome,
        ProviderOutcome::HttpError {
            status: 500,
            message: "busy".into()
        }
    );

    // NDL のエラーの応答（`illegal mediaType value`）
    rig.http.set_online();
    let r = found(&rig, ProviderId::Ndl, "ERROR");
    match r.outcome {
        ProviderOutcome::ParseError { message } => {
            assert!(message.contains("illegal mediaType value"), "{message}")
        }
        other => panic!("{other:?}"),
    }
    assert!(r.hits.is_empty());

    // 429（キーなしの Google の割り当て 0）も、キーを入れていれば HTTP エラーの状態
    enable_google(&rig);
    override_http(&rig, "googleapis.com", || {
        response(
            429,
            "application/json",
            fixture("google_quota_exceeded.json").into_bytes(),
        )
    });
    let r = found(&rig, ProviderId::Google, "clean code");
    match r.outcome {
        ProviderOutcome::HttpError { status, message } => {
            assert_eq!(status, 429);
            assert!(message.contains("Queries per day"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn invalid_queries_and_unusable_providers_are_validation_errors_without_a_request() {
    let rig = Rig::new();
    let long = "あ".repeat(201);
    for (title, author) in [
        (None, None),
        (Some(""), Some("  ")),
        (Some("　"), Some("\u{3000}")),
        (Some(long.as_str()), None),
        (None, Some(long.as_str())),
        // 引用符とバックスラッシュだけ（NDL は、取り除くと何も残らない）
        (Some("\"\\"), None),
    ] {
        let err = search(&rig, ProviderId::Ndl, title, author).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation, "{title:?} {author:?}");
    }
    let err = search(&rig, ProviderId::Ndl, None, None).unwrap_err();
    assert_eq!(err.to_string(), "書名か著者を入れてください");

    // Google はキーがなければ使えない
    let err = search(&rig, ProviderId::Google, Some("clean code"), None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.to_string().contains("Google Books"), "{err}");
    // キーがあっても、無効にしていれば使えない。有効にすれば使える
    enable_google(&rig);
    set_enabled(&rig, ProviderId::Google, false);
    let err = search(&rig, ProviderId::Google, Some("clean code"), None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    // NDL も、無効にしていれば使えない
    set_enabled(&rig, ProviderId::Ndl, false);
    let err = search(&rig, ProviderId::Ndl, Some("スローループ"), None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    // openBD は有効でも、キーワード検索に対応していない
    let err = search(&rig, ProviderId::Openbd, Some("スローループ"), None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Validation);
    assert!(err.to_string().contains("対応していません"), "{err}");
    assert!(rig.http.urls().is_empty(), "{:?}", rig.http.urls());

    set_enabled(&rig, ProviderId::Google, true);
    let r = found(&rig, ProviderId::Google, "clean code");
    assert_eq!(r.outcome, ProviderOutcome::Found);
}

#[test]
fn the_result_has_the_shape_the_frontend_reads() {
    let rig = Rig::new();
    let r = found(&rig, ProviderId::Ndl, "スローループ");
    let v = serde_json::to_value(&r).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["elapsedMs", "hits", "outcome", "provider", "total"]);
    assert_eq!(v["provider"], "ndl");
    assert_eq!(v["outcome"], json!({ "kind": "found" }));
    assert_eq!(v["total"], 11);
    assert!(v["elapsedMs"].is_u64());
    let hit = &v["hits"][0];
    assert_eq!(hit["provider"], "ndl");
    assert_eq!(hit["isbn13"], "9784832297333");
    assert_eq!(hit["candidate"]["isbn13"], "9784832297333");
    assert_eq!(hit["candidate"]["publishedOn"], "2026-07");
    assert_eq!(
        hit["link"],
        "https://ndlsearch.ndl.go.jp/books/R100000002-I034807010"
    );
    // 結果の 1 件は、そのまま `fetch_search_hit` の引数に送り返せる
    let back: SearchHit = serde_json::from_value(hit.clone()).unwrap();
    assert_eq!(back, r.hits[0]);
    // 失敗の状態
    rig.http.set_offline();
    let v = serde_json::to_value(found(&rig, ProviderId::Ndl, "スローループ")).unwrap();
    assert_eq!(v["outcome"]["kind"], "networkError");
    assert_eq!(v["total"], serde_json::Value::Null);
}

// ---------------------------------------------------------------------------
// fetch_search_hit: ISBN あり
// ---------------------------------------------------------------------------

#[test]
fn a_hit_with_an_isbn_is_fetched_like_fetch_metadata_when_its_provider_has_the_book() {
    let rig = Rig::new();
    let results = found(&rig, ProviderId::Ndl, "スローループ");
    let hit = hit_of(&results, SLOW_LOOP);
    rig.http.clear();

    let fetched = fetch_hit(&rig, hit).unwrap();
    let urls = rig.http.urls();
    // ISBN での取得（openBD・NDL・Open Library の検索）。キーワード検索は繰り返さない
    assert!(urls.iter().any(|u| u.contains("api.openbd.jp")), "{urls:?}");
    assert_eq!(rig.http.count(&format!("query=isbn%3D{SLOW_LOOP}")), 1);
    assert_eq!(rig.http.count("sortBy"), 0, "{urls:?}");

    // 選んだ取得元（NDL）が ISBN での取得で取れているので、検索結果の値は足さず、ふだんの取得と同じ結果になる
    let plain = rig
        .block(rig.state.fetch_metadata(SLOW_LOOP, None))
        .unwrap();
    assert_eq!(fetched.merged, plain.merged);
    assert_eq!(fetched.draft, plain.draft);
    assert_eq!(
        fetched.draft.fields.isbn13.as_ref().unwrap().value,
        SLOW_LOOP
    );
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Ok);
    assert!(!fetched.offline);
    assert_eq!(fetched.reports.len(), 3);
    // DB には書かない
    assert_eq!(rig.outbox_len(), 0);
}

#[test]
fn a_hit_is_merged_in_when_its_provider_did_not_find_the_isbn() {
    let rig = Rig::new();
    enable_google(&rig);
    let results = found(&rig, ProviderId::Google, "clean code");
    let hit = hit_of(&results, CLEAN_CODE);
    assert_eq!(
        hit.candidate.publisher.as_deref(),
        Some("Pearson Education")
    );

    // ISBN での取得では、Google は該当なし（NDL・Open Library は収録がある）。洋書は Google が先の優先順位
    let plain = rig
        .block(rig.state.fetch_metadata(CLEAN_CODE, None))
        .unwrap();
    let google_report = |r: &FetchMetadataResult| {
        r.reports
            .iter()
            .find(|r| r.provider == ProviderId::Google)
            .unwrap()
            .outcome
            .clone()
    };
    assert_eq!(google_report(&plain), ProviderOutcome::NotFound);
    assert_ne!(
        plain.draft.fields.publisher.as_ref().unwrap().origin,
        provider("google")
    );

    let fetched = fetch_hit(&rig, hit).unwrap();
    // 検索結果の値（Google）が、優先順位に従って採られる。その取得元の報告は「取れた」になる
    let publisher = fetched.draft.fields.publisher.as_ref().unwrap();
    assert_eq!(publisher.value, "Pearson Education");
    assert_eq!(publisher.origin, provider("google"));
    assert_eq!(
        fetched.draft.fields.title.as_ref().unwrap().origin,
        provider("google")
    );
    assert_eq!(google_report(&fetched), ProviderOutcome::Found);
    assert_eq!(
        fetched.draft.fields.isbn13.as_ref().unwrap().value,
        CLEAN_CODE
    );
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Ok);
    // ページ数も、検索結果（Google の 464）が先。検索結果にない値（大きさは NDL にだけある）は、ほかの取得元のまま
    let pages = fetched.draft.fields.pages.as_ref().unwrap();
    assert_eq!(
        (pages.value, pages.origin.clone()),
        (464, provider("google"))
    );
    let size = fetched.draft.fields.size_text.as_ref().unwrap();
    assert_eq!(size.origin, provider("ndl"));
    assert_eq!(fetched.draft.fields.isbn13, plain.draft.fields.isbn13);
    // 選ばれなかった値は、候補として残る（編集フォームで選び直せる）
    let publisher_choice = fetched
        .merged
        .fields
        .iter()
        .find(|f| f.field == bibcontainer_metadata::MetaField::Publisher)
        .unwrap();
    assert!(!publisher_choice.others.is_empty());
}

#[test]
fn a_hit_of_a_provider_that_was_not_asked_is_merged_in_too() {
    // Google のキーを外したあと（取得元を問い合わせないとき）も、選んだ検索結果の値は使う
    let rig = Rig::new();
    enable_google(&rig);
    let results = found(&rig, ProviderId::Google, "clean code");
    let hit = hit_of(&results, CLEAN_CODE).clone();
    rig.block(rig.state.delete_api_key(ProviderId::Google))
        .unwrap();
    rig.http.clear();

    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert_eq!(rig.http.count("googleapis.com"), 0);
    assert!(
        fetched
            .reports
            .iter()
            .all(|r| r.provider != ProviderId::Google)
    );
    let publisher = fetched.draft.fields.publisher.as_ref().unwrap();
    assert_eq!(publisher.value, "Pearson Education");
    assert_eq!(publisher.origin, provider("google"));
}

#[test]
fn a_hit_whose_isbn_no_provider_finds_is_ok_rather_than_failed() {
    // 検索で見つかった本でも、ISBN ではどの取得元にも見つからないことがある。検索結果の値があるので `failed` にしない
    let rig = Rig::new();
    let results = search(&rig, ProviderId::Ndl, Some("こころ"), Some("夏目漱石")).unwrap();
    let hit = results
        .hits
        .iter()
        .find(|h| {
            h.isbn13
                .as_deref()
                .is_some_and(|isbn| isbn != "9784003101117")
        })
        .expect("ISBN のある結果")
        .clone();
    let isbn13 = hit.isbn13.clone().unwrap();
    let plain = rig.block(rig.state.fetch_metadata(&isbn13, None)).unwrap();
    assert!(
        plain
            .reports
            .iter()
            .all(|r| r.outcome == ProviderOutcome::NotFound)
    );
    assert_eq!(plain.draft.fields.meta_status.value, MetaStatus::Failed);

    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Ok);
    let ndl = fetched
        .reports
        .iter()
        .find(|r| r.provider == ProviderId::Ndl)
        .unwrap();
    assert_eq!(ndl.outcome, ProviderOutcome::Found);
    assert_eq!(
        fetched.draft.fields.title.as_ref().unwrap().origin,
        provider("ndl")
    );
    // ほかの取得元の「該当なし」はそのまま
    assert!(
        fetched
            .reports
            .iter()
            .filter(|r| r.provider != ProviderId::Ndl)
            .all(|r| r.outcome == ProviderOutcome::NotFound)
    );
}

#[test]
fn offline_a_hit_with_an_isbn_becomes_pending_but_keeps_the_values_of_the_hit() {
    let rig = Rig::new();
    let results = found(&rig, ProviderId::Ndl, "スローループ");
    let hit = hit_of(&results, SLOW_LOOP).clone();
    assert_eq!(hit.candidate.publisher.as_deref(), Some("芳文社"));

    rig.http.set_offline();
    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert!(fetched.offline);
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Pending);
    assert!(
        fetched
            .reports
            .iter()
            .all(|r| matches!(r.outcome, ProviderOutcome::NetworkError { .. }))
    );
    // 取れなかった取得元の値のかわりに、検索結果の値が入っている
    let f = &fetched.draft.fields;
    assert_eq!(f.isbn13.as_ref().unwrap().value, SLOW_LOOP);
    assert_eq!(f.title.as_ref().unwrap().value, "スローループ");
    assert_eq!(f.title.as_ref().unwrap().origin, provider("ndl"));
    assert_eq!(f.publisher.as_ref().unwrap().value, "芳文社");
    assert_eq!(f.volume.as_ref().unwrap().value, "7");
    assert_eq!(fetched.cover_hash, None);
    // オフラインで、検索結果にない値（Cコードなど）はない
    assert!(f.c_code.is_none());
}

// ---------------------------------------------------------------------------
// fetch_search_hit: ISBN なし
// ---------------------------------------------------------------------------

/// 古い本（ISBN なし）の検索結果。`こころ` の NDL の結果の、いちばん古い 1 件（1914 年）。
fn old_book_hit(rig: &Rig) -> SearchHit {
    let results = search(rig, ProviderId::Ndl, Some("こころ"), Some("夏目漱石")).unwrap();
    let hit = results.hits.last().unwrap().clone();
    assert_eq!(hit.isbn13, None);
    assert_eq!(hit.candidate.published_on.as_deref(), Some("1914"));
    hit
}

#[test]
fn a_hit_without_an_isbn_becomes_a_draft_from_its_own_values_only() {
    let rig = Rig::new();
    let hit = old_book_hit(&rig);
    rig.http.clear();

    let fetched = fetch_hit(&rig, &hit).unwrap();
    // 通信しない（ISBN がないので取得するものがなく、書影の URL もない）。DB にも書かない
    assert!(rig.http.urls().is_empty(), "{:?}", rig.http.urls());
    assert_eq!(rig.outbox_len(), 0);
    assert!(fetched.reports.is_empty());
    assert!(!fetched.offline);
    assert_eq!(fetched.cover_hash, None);
    assert_eq!(fetched.merged.isbn13, "");

    let f = &fetched.draft.fields;
    assert!(f.isbn13.is_none(), "ISBN のない下書き");
    assert_eq!(f.title.as_ref().unwrap().value, "こゝろ");
    assert_eq!(f.title.as_ref().unwrap().origin, provider("ndl"));
    assert_eq!(f.publisher.as_ref().unwrap().value, "岩波書店");
    assert_eq!(f.published_on.as_ref().unwrap().value, "1914");
    let authors = &f.authors.as_ref().unwrap().value;
    assert_eq!(authors[0].name, "夏目漱石");
    assert_eq!(authors[0].role.as_deref(), Some("著"));
    // 取得状態は `ok` を入れておく（ISBN のない本の下書きは、画面が `meta_status` を外す）
    assert_eq!(f.meta_status.value, MetaStatus::Ok);

    // そのまま登録できる（ISBN なし）。`meta_status` は画面が外すので、外して作る
    let changes = f
        .to_changes()
        .into_iter()
        .filter(|c| c.field != "meta_status")
        .collect();
    let id = rig
        .block(rig.state.create_books_from_drafts(vec![NewBookDraft {
            changes,
            series_name: fetched.draft.series_name.clone(),
        }]))
        .unwrap()
        .remove(0);
    let book = rig.book(&id);
    assert_eq!(book.isbn13, None);
    assert_eq!(book.title, "こゝろ");
    assert_eq!(book.publisher.as_deref(), Some("岩波書店"));
    assert_eq!(book.field_sources["title"], "ndl");
}

#[test]
fn a_hit_without_an_isbn_saves_its_cover_when_it_can_and_goes_on_without_it_when_it_cannot() {
    let rig = Rig::new();
    let results = found(&rig, ProviderId::OpenLibrary, "こころ");
    let hit = results.hits[0].clone();
    assert_eq!(hit.isbn13, None);
    assert_eq!(
        hit.candidate.covers[0].url,
        "https://covers.openlibrary.org/b/id/9440872-L.jpg?default=false"
    );

    // 書影が取れる: 保存して、下書きに入る
    let fetched = fetch_hit(&rig, &hit).unwrap();
    let hash = fetched.cover_hash.clone().expect("書影を保存した");
    assert!(cover_path(&rig.state.dirs().covers, &hash).is_file());
    let cover = fetched.draft.fields.cover_hash.as_ref().unwrap();
    assert_eq!(cover.value, hash);
    assert_eq!(cover.origin, provider("openlibrary"));
    assert!(fetched.reports.is_empty());

    // 書影が取れない（404）: エラーにせず、書影なし
    override_http(&rig, "covers.openlibrary.org", || {
        response(404, "text/plain", b"not found".to_vec())
    });
    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert_eq!(fetched.cover_hash, None);
    assert!(fetched.draft.fields.cover_hash.is_none());
    assert_eq!(
        fetched.draft.fields.title.as_ref().unwrap().value,
        hit.candidate.title.clone().unwrap()
    );

    // 通信できない（オフライン）ときも、エラーにしない。`pending` にもしない（ISBN がなく、取り直せない）
    rig.http.set_offline();
    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert_eq!(fetched.cover_hash, None);
    assert!(!fetched.offline);
    assert_eq!(fetched.draft.fields.meta_status.value, MetaStatus::Ok);
}

#[test]
fn only_https_cover_urls_of_a_hit_are_downloaded() {
    let rig = Rig::new();
    let results = found(&rig, ProviderId::OpenLibrary, "こころ");
    let mut hit = results.hits[0].clone();
    let insecure = |url: &str| CandidateCover {
        provider: ProviderId::OpenLibrary,
        url: url.to_owned(),
    };
    hit.candidate
        .covers
        .insert(0, insecure("http://covers.openlibrary.org/b/id/1-L.jpg"));
    hit.candidate
        .covers
        .insert(1, insecure("file:///etc/passwd"));
    rig.http.clear();

    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert!(fetched.cover_hash.is_some(), "https の書影は取れる");
    let urls = rig.http.urls();
    assert!(
        urls.iter().all(|u| u.starts_with("https://")),
        "https でない URL には通信しない: {urls:?}"
    );
}

// ---------------------------------------------------------------------------
// fetch_search_hit: 検証
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_hit_is_a_validation_error_and_sends_nothing() {
    let rig = Rig::new();
    let results = found(&rig, ProviderId::Ndl, "スローループ");
    let good = hit_of(&results, SLOW_LOOP).clone();
    rig.http.clear();

    // 候補の取得元が、結果の取得元と食い違う
    let mut hit = good.clone();
    hit.candidate.provider = ProviderId::Google;
    assert_eq!(
        fetch_hit(&rig, &hit).unwrap_err().kind(),
        ErrorKind::Validation
    );
    // ISBN が正しくない（チェックデジット、ISBN-10、ハイフン付き、空）
    for bad in ["9784832274168", "4832274163", "978-4-8322-7416-7", ""] {
        let mut hit = good.clone();
        hit.isbn13 = Some(bad.to_owned());
        assert_eq!(
            fetch_hit(&rig, &hit).unwrap_err().kind(),
            ErrorKind::Validation,
            "{bad:?}"
        );
    }
    assert!(rig.http.urls().is_empty(), "{:?}", rig.http.urls());
    assert_eq!(rig.outbox_len(), 0);
}

#[test]
fn the_hit_json_from_the_frontend_is_accepted_with_missing_optional_fields() {
    // 画面が送る形（結果の 1 件をそのまま）。`null` の項目は省いてもよい
    let hit: SearchHit = serde_json::from_value(json!({
        "provider": "openlibrary",
        "isbn13": null,
        "link": null,
        "candidate": {
            "provider": "openlibrary",
            "isbn13": "",
            "title": "こころ",
            "authors": [{ "name": "夏目漱石", "kana": null, "role": null }],
            "covers": []
        }
    }))
    .unwrap();
    assert_eq!(hit.provider, ProviderId::OpenLibrary);
    let rig = Rig::new();
    let fetched = fetch_hit(&rig, &hit).unwrap();
    assert_eq!(fetched.draft.fields.title.as_ref().unwrap().value, "こころ");
    assert_eq!(
        fetched.draft.fields.title.as_ref().unwrap().origin,
        provider("openlibrary")
    );
    assert!(fetched.draft.fields.isbn13.is_none());
}
