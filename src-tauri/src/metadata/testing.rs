//! テスト用の道具: 偽の HTTP（`crates/metadata` のフィクスチャを返す）と、一時 DB を使った `MetadataState`。

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bibcontainer_core::library::{Env, FieldChange, UpdateOptions, update_book};
use bibcontainer_core::{BookDetail, SystemClock, get_book_detail};
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::{RetryPolicy, Transport};

use super::keychain::MemoryKeyStore;
use super::state::{CoverDirs, MetadataState, Notifier, TransportFactory};
use crate::database::{BACKUP_DIR_NAME, DB_FILE_NAME, Database};

pub const ONIGIRI: &str = "9784780802047";
pub const KOKORO: &str = "9784003101117";
pub const YUKIGUNI: &str = "9784101001012";
pub const ONE_PIECE: &str = "9784088725093";

fn fixture_path(name: &str) -> String {
    format!(
        "{}/../crates/metadata/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

pub fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// テスト用の書影（グラデーションの JPEG。ダミー画像の判定には当たらない）。
pub fn cover_jpeg() -> Vec<u8> {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
    let img = RgbImage::from_fn(300, 450, |x, y| Rgb([(x % 251) as u8, (y % 241) as u8, 90]));
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(img.as_raw(), 300, 450, ExtendedColorType::Rgb8)
        .unwrap();
    out
}

pub fn response(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status,
        content_type: Some(content_type.to_owned()),
        retry_after: None,
        body,
    })
}

/// 通知された `library-changed`（本の ID の一覧、others）の記録。
pub type ChangedLog = Arc<Mutex<Vec<(Vec<String>, bool)>>>;

pub type Handler = Box<dyn Fn(&str) -> Result<HttpResponse, TransportError> + Send + Sync>;

/// フィクスチャから、取得元ごとの応答を返す。書影（`cover.openbd.jp`、`covers.openlibrary.org`）は JPEG、
/// 利用者が入力する画像 URL（`img.example`）も JPEG。Google は API キーがなければ 429（キーなしでは使わないはず）。
/// Open Library は、フィクスチャにある洋書（`9780132350884` など）だけが収録されていて、ほかは「該当なし」。
pub fn online_handler() -> Handler {
    let jpeg = cover_jpeg();
    Box::new(move |url: &str| {
        if let Some(list) = url.split("api.openbd.jp/v1/get?isbn=").nth(1) {
            let elements: Vec<String> = list
                .split(',')
                .map(|isbn| {
                    std::fs::read_to_string(fixture_path(&format!("openbd_{isbn}.json")))
                        .unwrap_or_else(|_| "null\n".to_owned())
                })
                .collect();
            return response(
                200,
                "application/json; charset=UTF-8",
                format!("[{}]", elements.join(",")).into_bytes(),
            );
        }
        if let Some(q) = url.split("query=isbn%3D").nth(1) {
            let isbn = q.split('&').next().unwrap_or_default();
            let body = std::fs::read_to_string(fixture_path(&format!("ndl_{isbn}.xml")))
                .unwrap_or_else(|_| fixture("ndl_notfound.xml"));
            return response(200, "text/xml;charset=UTF-8", body.into_bytes());
        }
        // Google の Dynamic Links（検索が該当なしのときの回避策）。この偽物では、どの ISBN も知らない
        if url.contains("books?bibkeys=ISBN:") {
            return response(
                200,
                "application/javascript; charset=ISO-8859-1",
                b"cb({});".to_vec(),
            );
        }
        if let Some(q) = url.split("volumes?q=isbn:").nth(1) {
            if !url.contains("key=") {
                return response(
                    429,
                    "application/json",
                    fixture("google_quota_exceeded.json").into_bytes(),
                );
            }
            let isbn = q.split('&').next().unwrap_or_default();
            let file = match isbn {
                KOKORO => "google_synthetic_9784003101117.json",
                ONIGIRI => "google_synthetic_9784780802047.json",
                _ => "google_synthetic_notfound.json",
            };
            return response(
                200,
                "application/json; charset=UTF-8",
                fixture(file).into_bytes(),
            );
        }
        // Open Library: 検索（フィクスチャがなければ「該当なし」）と、版の JSON（なければ 404）
        if let Some(q) = url.split("openlibrary.org/search.json?isbn=").nth(1) {
            let isbn = q.split('&').next().unwrap_or_default();
            let body =
                std::fs::read_to_string(fixture_path(&format!("openlibrary_search_{isbn}.json")))
                    .unwrap_or_else(|_| fixture(&format!("openlibrary_search_{KOKORO}.json")));
            return response(200, "application/json", body.into_bytes());
        }
        if let Some(olid) = url
            .split("openlibrary.org/books/")
            .nth(1)
            .and_then(|r| r.strip_suffix(".json"))
        {
            return match std::fs::read_to_string(fixture_path(&format!(
                "openlibrary_edition_{olid}.json"
            ))) {
                Ok(body) => response(200, "application/json", body.into_bytes()),
                Err(_) => response(404, "application/json", b"{}".to_vec()),
            };
        }
        if url.starts_with("https://covers.openlibrary.org/") {
            return response(200, "image/jpeg", jpeg.clone());
        }
        if url.starts_with("https://cover.openbd.jp/") || url == "https://img.example/cover.jpg" {
            return response(200, "image/jpeg", jpeg.clone());
        }
        if url == "https://img.example/page.html" {
            return response(200, "text/html", b"<html>not an image</html>".to_vec());
        }
        response(404, "text/plain", b"not found".to_vec())
    })
}

pub struct FakeHttp {
    handler: Mutex<Handler>,
    urls: Mutex<Vec<String>>,
}

impl FakeHttp {
    pub fn new() -> Arc<Self> {
        Arc::new(FakeHttp {
            handler: Mutex::new(online_handler()),
            urls: Mutex::new(Vec::new()),
        })
    }

    pub fn set_handler(&self, handler: Handler) {
        *self.handler.lock().unwrap() = handler;
    }

    pub fn set_offline(&self) {
        self.set_handler(Box::new(|_| Err(TransportError::Connect("offline".into()))));
    }

    pub fn set_online(&self) {
        self.set_handler(online_handler());
    }

    pub fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }

    /// URL に `needle` を含む要求の数。
    pub fn count(&self, needle: &str) -> usize {
        self.urls().iter().filter(|u| u.contains(needle)).count()
    }

    pub fn clear(&self) {
        self.urls.lock().unwrap().clear();
    }
}

#[async_trait]
impl HttpClient for FakeHttp {
    async fn get(&self, req: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.urls.lock().unwrap().push(req.url.clone());
        (self.handler.lock().unwrap())(&req.url)
    }
}

/// 一時フォルダの DB と偽の HTTP でつないだ `MetadataState`。
pub struct Rig {
    pub state: Arc<MetadataState>,
    pub db: Arc<Database>,
    pub http: Arc<FakeHttp>,
    pub keys: Arc<MemoryKeyStore>,
    /// `library-changed` として通知された（本の ID の一覧、others）
    pub changed: ChangedLog,
    /// 通信の層を作ったときの User-Agent
    pub user_agents: Arc<Mutex<Vec<String>>>,
    pub dir: tempfile::TempDir,
}

impl Rig {
    pub fn new() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(
            &dir.path().join(DB_FILE_NAME),
            &dir.path().join(BACKUP_DIR_NAME),
        ));
        assert!(db.failure().is_none());
        let http = FakeHttp::new();
        let keys = Arc::new(MemoryKeyStore::default());
        let changed: ChangedLog = Arc::default();
        let user_agents: Arc<Mutex<Vec<String>>> = Arc::default();

        let notifier: Notifier = {
            let changed = Arc::clone(&changed);
            Arc::new(move |books, others| changed.lock().unwrap().push((books, others)))
        };
        let factory: TransportFactory = {
            let http = Arc::clone(&http);
            let user_agents = Arc::clone(&user_agents);
            Arc::new(move |ua: &str| {
                user_agents.lock().unwrap().push(ua.to_owned());
                // リトライなし（テストを速くする）
                Ok(Arc::new(Transport::new(http.clone()).with_retry(
                    RetryPolicy {
                        max_retries: 0,
                        base_delay: Duration::ZERO,
                        max_delay: Duration::ZERO,
                    },
                )))
            })
        };
        let state = MetadataState::new(
            Arc::clone(&db),
            CoverDirs {
                covers: dir.path().join("covers"),
                thumbs: dir.path().join("thumbs"),
            },
            keys.clone(),
            "0.0.0-test".to_owned(),
            factory,
            notifier,
        );
        Rig {
            state,
            db,
            http,
            keys,
            changed,
            user_agents,
            dir,
        }
    }

    pub fn block<F: Future>(&self, f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    pub fn book(&self, id: &str) -> BookDetail {
        let id = id.to_owned();
        self.block(self.db.run(move |conn, _| Ok(get_book_detail(conn, &id)?)))
            .unwrap()
    }

    /// 書籍に変更を書く（コアの `update_book`）。
    pub fn update(&self, id: &str, changes: Vec<FieldChange>, opts: UpdateOptions) {
        let id = id.to_owned();
        self.block(self.db.run(move |conn, _| {
            let clock = SystemClock;
            let env = Env::new(&clock);
            update_book(conn, &env, &id, &changes, opts)?;
            Ok(())
        }))
        .unwrap();
    }

    /// outbox の Op の数（書き込みがあったかを見る）。
    pub fn outbox_len(&self) -> i64 {
        self.block(self.db.run(|conn, _| {
            Ok(conn
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))
                .map_err(bibcontainer_core::Error::from)?)
        }))
        .unwrap()
    }

    pub fn changed_count(&self) -> usize {
        self.changed.lock().unwrap().len()
    }

    pub fn series_names(&self) -> Vec<String> {
        self.block(self.db.run(|conn, _| {
            Ok(bibcontainer_core::library::list_series(conn)?
                .into_iter()
                .map(|s| s.name)
                .collect())
        }))
        .unwrap()
    }
}
