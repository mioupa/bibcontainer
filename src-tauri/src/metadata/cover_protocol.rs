//! `cover` URI スキーム（0005 §7）。WebView に書影を渡す。
//!
//! - URL: `cover://localhost/<hash>?size=80`（macOS）、`http://cover.localhost/<hash>?size=80`（Windows）。
//! - `<hash>`: 小文字の 16 進 64 桁だけを受け付ける（パスの組み立てに使うので、`..`・`/`・`%`・大文字などは弾く。
//!   パストラバーサル対策）。
//! - `size`: なし（原寸）、`80`（一覧）、`160`、`320`（グリッド・詳細ペイン・フォーム）。サムネイルはキャッシュに作る。
//!   それ以外は 400。ほかのキー（`v` など）は読み捨てる。`v` は、同期で書影があとから届いたとき、画面が URL を変えて
//!   読み込み直すための版（0016）。
//! - 応答: 200（`Content-Type: image/jpeg`。サムネイル（80・160・320）はすべて、WebKit のメモリキャッシュに
//!   残さない `no-store`。原寸だけは、内容が変わらないので長いキャッシュ期間を付ける。理由は
//!   `CACHE_CONTROL_THUMBNAIL`）、
//!   400（ハッシュ・大きさが正しくない）、404（書影がない）、405（GET・HEAD 以外）、500・503（読めない・まだ使えない）。
//!
//! フェーズ 2 で同期フォルダの書影からも読めるようにするときは、`locate_original` に探す場所を足す。

use std::path::PathBuf;
use std::sync::Arc;

use bibcontainer_metadata::cover::{THUMBNAIL_SIZES, cover_path, ensure_thumbnail, is_valid_hash};
use tauri::http::{self, HeaderValue, Method, Response, StatusCode};
use tauri::{Manager, Runtime};

use super::state::{CoverDirs, MetadataState};

/// スキームの名前。
pub const SCHEME: &str = "cover";
/// 書影の原寸は内容（ハッシュ）が決まれば変わらないので、1 年・変更なしとする。
const CACHE_CONTROL_OK: &str = "public, max-age=31536000, immutable";
/// サムネイル（長辺 80・160・320px）は、WebKit のメモリキャッシュに残さない。
///
/// 一覧もグリッドも仮想化していて、画面から出た行・カードの `<img>` は外される。それでも WebKit は、
/// 読み込んだ画像を使われなくなったあともメモリキャッシュに持ち続ける。一覧の 80px でも 1 枚あたり
/// 約 11KB（書影のある 2,823 冊を表示すると、WebContent のフットプリントが約 30MB 増える）で、
/// グリッドは 320px のサムネイルを何千冊ぶんも見せるので、残させるとメモリが際限なく増える。
/// 書影は端末内のファイルで、読み込みは 1ms 未満なので、どの大きさもキャッシュさせずに、
/// 行・カードが出るたびに読み直す。詳細ペインの 320 も、本を選び直すたびに読み直すことになるが、無視できる
/// （docs/decisions/0024、0010 §14）。原寸は何千枚も並べないので、内容が変わらないことを生かして長くキャッシュさせる。
const CACHE_CONTROL_THUMBNAIL: &str = "no-store";

/// `size` に対する、200 の応答の `Cache-Control`。サムネイルは `no-store`、原寸（`size` なし）だけ長くキャッシュさせる。
fn cache_control_for(size: Option<u32>) -> &'static str {
    if size.is_some() {
        CACHE_CONTROL_THUMBNAIL
    } else {
        CACHE_CONTROL_OK
    }
}

/// 要求の URL が正しくないとき。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadRequest {
    /// ハッシュが 64 桁の小文字の 16 進数でない（パスに余計なものがある場合を含む）
    Hash,
    /// `size` が 80・160・320 以外
    Size,
}

/// URL のパスとクエリから、ハッシュと大きさ（なしは原寸）を取り出す。
pub fn parse_request(path: &str, query: Option<&str>) -> Result<(String, Option<u32>), BadRequest> {
    let hash = path.strip_prefix('/').unwrap_or(path);
    if !is_valid_hash(hash) {
        return Err(BadRequest::Hash);
    }
    let mut size = None;
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty())
    {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "size" {
            let n: u32 = value.parse().map_err(|_| BadRequest::Size)?;
            if !THUMBNAIL_SIZES.contains(&n) {
                return Err(BadRequest::Size);
            }
            size = Some(n);
        }
    }
    Ok((hash.to_owned(), size))
}

/// 応答の中身（Tauri の型に直す前）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverReply {
    pub status: u16,
    pub body: Vec<u8>,
    /// 200 のときの `Cache-Control`（エラーは常に `no-store`）。
    pub cache_control: &'static str,
}

impl CoverReply {
    fn error(status: u16) -> Self {
        CoverReply {
            status,
            body: Vec::new(),
            cache_control: "no-store",
        }
    }

    fn into_http(self, head_only: bool) -> Response<Vec<u8>> {
        let ok = self.status == 200;
        let mut builder = Response::builder()
            .status(StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
            .header(http::header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
        builder = if ok {
            builder
                .header(http::header::CONTENT_TYPE, "image/jpeg")
                .header(http::header::CONTENT_LENGTH, self.body.len())
                .header(http::header::CACHE_CONTROL, self.cache_control)
        } else {
            builder
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header(http::header::CACHE_CONTROL, "no-store")
        };
        let body = if head_only || !ok {
            Vec::new()
        } else {
            self.body
        };
        builder.body(body).unwrap_or_else(|_| {
            let mut r = Response::new(Vec::new());
            *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            r.headers_mut().insert(
                http::header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            );
            r
        })
    }
}

/// 書影の本体のファイル。いまはローカルの `covers/` だけ。
fn locate_original(dirs: &CoverDirs, hash: &str) -> Option<PathBuf> {
    let path = cover_path(&dirs.covers, hash);
    path.is_file().then_some(path)
}

/// 要求に答える（ファイルを読み、必要ならサムネイルを作る。ブロックするので別のスレッドで呼ぶ）。
pub fn respond(dirs: &CoverDirs, path: &str, query: Option<&str>) -> CoverReply {
    let (hash, size) = match parse_request(path, query) {
        Ok(parsed) => parsed,
        Err(BadRequest::Hash | BadRequest::Size) => return CoverReply::error(400),
    };
    let Some(original) = locate_original(dirs, &hash) else {
        return CoverReply::error(404);
    };
    let file = match size {
        None => original,
        Some(edge) => match ensure_thumbnail(&dirs.covers, &dirs.thumbs, &hash, edge) {
            Ok(thumb) => thumb,
            Err(err) => {
                tracing::warn!(hash, size = edge, error = %err, "サムネイルを作れませんでした");
                return CoverReply::error(500);
            }
        },
    };
    match std::fs::read(&file) {
        Ok(body) => CoverReply {
            status: 200,
            body,
            cache_control: cache_control_for(size),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => CoverReply::error(404),
        Err(err) => {
            tracing::warn!(hash, error = %err, "書影を読めませんでした");
            CoverReply::error(500)
        }
    }
}

/// 要求のメソッドから、本体を返すか（GET）・ヘッダーだけか（HEAD）・受け付けないか。
fn head_only(method: &Method) -> Option<bool> {
    if method == Method::GET {
        Some(false)
    } else if method == Method::HEAD {
        Some(true)
    } else {
        None
    }
}

/// `cover` スキームを登録する。`tauri::Builder` の組み立てで、`.setup` より前に呼ぶ。
///
/// ```ignore
/// register_cover_protocol(tauri::Builder::default()).setup(...)
/// ```
///
/// 処理には `Arc<MetadataState>`（`metadata::setup` で managed state に入れる）の書影の置き場所を使う。
/// まだ入っていなければ 503 を返す。
pub fn register_cover_protocol<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
        let app = ctx.app_handle().clone();
        let Some(head) = head_only(request.method()) else {
            responder.respond(CoverReply::error(405).into_http(false));
            return;
        };
        let path = request.uri().path().to_owned();
        let query = request.uri().query().map(str::to_owned);
        tauri::async_runtime::spawn(async move {
            let dirs = app
                .try_state::<Arc<MetadataState>>()
                .map(|state| state.dirs().clone());
            let reply = match dirs {
                None => CoverReply::error(503),
                Some(dirs) => tauri::async_runtime::spawn_blocking(move || {
                    respond(&dirs, &path, query.as_deref())
                })
                .await
                .unwrap_or_else(|_| CoverReply::error(500)),
            };
            responder.respond(reply.into_http(head));
        });
    })
}

#[cfg(test)]
mod tests {
    use bibcontainer_metadata::cover::import_cover_bytes;

    use super::*;

    fn hash64() -> String {
        "0123456789abcdef".repeat(4)
    }

    #[test]
    fn accepts_only_a_lowercase_hex_hash_of_64_digits() {
        assert_eq!(
            parse_request(&format!("/{}", hash64()), None),
            Ok((hash64(), None))
        );
        let bad = [
            "/".to_owned(),
            String::new(),
            "/abc".to_owned(),
            format!("/{}0", hash64()),
            format!("/{}", &hash64()[..63]),
            format!("/{}", hash64().to_uppercase()),
            format!("/{}/", hash64()),
            format!("/{}.jpg", hash64()),
            format!("//{}", hash64()),
            format!("/x/{}", hash64()),
            // パストラバーサルの試み
            format!("/../{}", hash64()),
            "/..%2f..%2fetc%2fpasswd".to_owned(),
            format!("/%2e%2e/{}", &hash64()[..58]),
            format!("/{}", "../".repeat(21) + "a"),
            format!("/{}", "g".repeat(64)),
            format!("/{}", "é".repeat(32)),
            format!("/{}\0", &hash64()[..63]),
            format!("/{}\\", &hash64()[..63]),
        ];
        for path in bad {
            assert_eq!(
                parse_request(&path, None),
                Err(BadRequest::Hash),
                "{path:?}"
            );
        }
    }

    #[test]
    fn size_is_none_80_160_or_320() {
        let path = format!("/{}", hash64());
        assert_eq!(parse_request(&path, Some("size=80")).unwrap().1, Some(80));
        assert_eq!(parse_request(&path, Some("size=160")).unwrap().1, Some(160));
        assert_eq!(parse_request(&path, Some("size=320")).unwrap().1, Some(320));
        assert_eq!(parse_request(&path, Some("")).unwrap().1, None);
        assert_eq!(parse_request(&path, Some("foo=1")).unwrap().1, None);
        assert_eq!(
            parse_request(&path, Some("foo=1&size=320&bar")).unwrap().1,
            Some(320)
        );
        // 画面が、書影が届いたあとの読み込み直しに付ける版（`v`）は、読み捨てる（URL を変えて、ブラウザに取り直させるだけ）
        assert_eq!(
            parse_request(&path, Some("size=80&v=3")).unwrap(),
            (hash64(), Some(80))
        );
        assert_eq!(parse_request(&path, Some("v=1")).unwrap().1, None);
        for q in [
            "size=0", "size=79", "size=100", "size=161", "size=800", "size=", "size=abc", "size=-1",
        ] {
            assert_eq!(parse_request(&path, Some(q)), Err(BadRequest::Size), "{q}");
        }
    }

    fn sample_png() -> Vec<u8> {
        std::fs::read(format!(
            "{}/../crates/metadata/tests/fixtures/google_noimage_575x750.png",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn dirs_with_cover() -> (tempfile::TempDir, CoverDirs, String) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = CoverDirs {
            covers: tmp.path().join("covers"),
            thumbs: tmp.path().join("thumbs"),
        };
        let stored = import_cover_bytes(&dirs.covers, &sample_png()).unwrap();
        (tmp, dirs, stored.hash)
    }

    fn dimensions(jpeg: &[u8]) -> (u32, u32) {
        image::ImageReader::new(std::io::Cursor::new(jpeg))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap()
    }

    #[test]
    fn serves_the_original_and_the_thumbnails_as_jpeg() {
        let (_tmp, dirs, hash) = dirs_with_cover();
        let path = format!("/{hash}");

        let original = respond(&dirs, &path, None);
        assert_eq!(original.status, 200);
        assert_eq!(&original.body[..2], &[0xFF, 0xD8], "JPEG の先頭");
        assert_eq!(
            dimensions(&original.body),
            (575, 750),
            "800px 以下なので縮小されない"
        );
        assert_eq!(
            original.body,
            std::fs::read(cover_path(&dirs.covers, &hash)).unwrap()
        );

        for edge in [80u32, 160, 320] {
            let reply = respond(&dirs, &path, Some(&format!("size={edge}")));
            assert_eq!(reply.status, 200, "{edge}");
            let (w, h) = dimensions(&reply.body);
            assert_eq!(w.max(h), edge, "長辺が {edge}");
            // サムネイルはキャッシュフォルダにできている
            assert!(
                dirs.thumbs
                    .join(edge.to_string())
                    .join(&hash[..2])
                    .join(format!("{hash}.jpg"))
                    .is_file()
            );
        }
        // 2 回目はキャッシュから（同じ内容）
        let again = respond(&dirs, &path, Some("size=160"));
        assert_eq!(again, respond(&dirs, &path, Some("size=160")));
    }

    #[test]
    fn thumbnails_are_not_kept_in_the_webview_memory_cache_but_the_original_is() {
        let (_tmp, dirs, hash) = dirs_with_cover();
        let path = format!("/{hash}");
        // サムネイル（一覧の 80、160、グリッド・詳細ペイン・フォームの 320）: WebKit のメモリキャッシュに残さない
        for edge in [80u32, 160, 320] {
            let thumb = respond(&dirs, &path, Some(&format!("size={edge}")));
            assert_eq!(thumb.status, 200, "{edge}");
            assert_eq!(thumb.cache_control, CACHE_CONTROL_THUMBNAIL, "{edge}");
            assert_eq!(thumb.cache_control, "no-store", "{edge}");
            let (w, h) = dimensions(&thumb.body);
            assert_eq!(w.max(h), edge, "{edge}");
            assert_eq!(
                thumb.clone().into_http(false).headers()["cache-control"],
                "no-store",
                "{edge}"
            );
            // HEAD でも同じヘッダー
            assert_eq!(
                thumb.into_http(true).headers()["cache-control"],
                "no-store",
                "{edge}"
            );
            // 2 回目（サムネイルがキャッシュフォルダにあるとき）も同じ
            assert_eq!(
                respond(&dirs, &path, Some(&format!("size={edge}"))).cache_control,
                "no-store",
                "{edge}"
            );
        }
        // 原寸: 内容は変わらないので長くキャッシュさせる
        let original = respond(&dirs, &path, None);
        assert_eq!(original.status, 200);
        assert_eq!(original.cache_control, CACHE_CONTROL_OK);
        assert_eq!(
            original.into_http(false).headers()["cache-control"],
            CACHE_CONTROL_OK
        );
    }

    #[test]
    fn missing_cover_is_404_and_bad_requests_are_400_without_touching_the_disk() {
        let (_tmp, dirs, _hash) = dirs_with_cover();
        let missing = format!("/{}", "f".repeat(64));
        assert_eq!(respond(&dirs, &missing, None).status, 404);
        assert_eq!(respond(&dirs, &missing, Some("size=160")).status, 404);
        assert!(!dirs.thumbs.exists(), "ないものからサムネイルは作らない");

        assert_eq!(respond(&dirs, "/../../etc/passwd", None).status, 400);
        assert_eq!(respond(&dirs, &missing, Some("size=999")).status, 400);
        assert_eq!(respond(&dirs, "/", None).status, 400);
    }

    #[test]
    fn a_corrupt_cover_file_is_a_500_for_thumbnails() {
        let (_tmp, dirs, hash) = dirs_with_cover();
        std::fs::write(cover_path(&dirs.covers, &hash), b"not an image").unwrap();
        assert_eq!(
            respond(&dirs, &format!("/{hash}"), Some("size=160")).status,
            500
        );
        // 原寸はファイルをそのまま返す（壊れているかは見ない）
        assert_eq!(respond(&dirs, &format!("/{hash}"), None).status, 200);
    }

    #[test]
    fn http_response_headers() {
        let reply = CoverReply {
            status: 200,
            body: vec![0xFF, 0xD8, 1, 2, 3],
            cache_control: CACHE_CONTROL_OK,
        };
        let r = reply.clone().into_http(false);
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["content-type"], "image/jpeg");
        assert_eq!(r.headers()["cache-control"], CACHE_CONTROL_OK);
        assert_eq!(r.headers()["x-content-type-options"], "nosniff");
        assert_eq!(r.headers()["content-length"], "5");
        assert_eq!(r.body().len(), 5);

        // HEAD はヘッダーだけ
        let r = reply.into_http(true);
        assert_eq!(r.headers()["content-length"], "5");
        assert!(r.body().is_empty());

        // エラーはキャッシュさせない
        for status in [400, 404, 405, 500, 503] {
            let r = CoverReply::error(status).into_http(false);
            assert_eq!(r.status().as_u16(), status);
            assert_eq!(r.headers()["cache-control"], "no-store");
            assert!(r.body().is_empty());
        }
    }

    #[test]
    fn the_scheme_can_be_registered_on_the_app_builder() {
        // 本番の Wry で型が合うこと（アプリの組み立てで呼ぶ形）
        let _builder = register_cover_protocol::<tauri::Wry>(tauri::Builder::default());
        assert_eq!(SCHEME, "cover");
    }

    #[test]
    fn only_get_and_head_are_served() {
        assert_eq!(head_only(&Method::GET), Some(false));
        assert_eq!(head_only(&Method::HEAD), Some(true));
        for m in [Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS] {
            assert_eq!(head_only(&m), None, "{m}");
        }
    }
}
