//! 書影（FR-COVER）のテスト。画像はテスト内で作る（実在の表紙は置かない）。
//! 「画像なし」のダミー画像は、実際に返ってきたもの（`tests/fixtures/google_noimage_*.png`）を使う。

mod common;

use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bibcontainer_metadata::Transport;
use bibcontainer_metadata::cover::*;
use bibcontainer_metadata::error::{CoverError, FetchError};
use bibcontainer_metadata::http::{HttpClient, HttpRequest, HttpResponse, TransportError};
use bibcontainer_metadata::types::{CandidateCover, ProviderId};
use common::fixture_bytes;
use image::codecs::jpeg::JpegEncoder;
use image::{
    DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage,
};

// ---------------------------------------------------------------------------
// 画像を作る補助
// ---------------------------------------------------------------------------

fn gradient(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        Rgb([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128])
    })
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    encode(&DynamicImage::ImageRgb8(gradient(w, h)), ImageFormat::Png)
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = gradient(w, h);
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(img.as_raw(), w, h, ExtendedColorType::Rgb8)
        .unwrap();
    out
}

/// JPEG の SOS より前のマーカー（SOI・APPn・DQT・SOF…）の番号を並べる。
fn jpeg_markers(bytes: &[u8]) -> Vec<u8> {
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG の先頭");
    let mut markers = Vec::new();
    let mut i = 2;
    while i + 4 <= bytes.len() {
        assert_eq!(bytes[i], 0xFF, "マーカーの位置がずれた: {i}");
        let m = bytes[i + 1];
        markers.push(m);
        if m == 0xDA {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        i += 2 + len;
    }
    markers
}

fn app_segment(marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut seg = vec![0xFF, marker];
    seg.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    seg.extend_from_slice(payload);
    seg
}

/// 向き（Orientation）と、位置情報らしき文字列を含む EXIF・XMP を JPEG の先頭に差し込む。
fn with_metadata(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend_from_slice(&tiff);
    let mut xmp = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    xmp.extend_from_slice(b"<x:xmpmeta>secret-gps-35.68,139.76</x:xmpmeta>");
    let mut out = vec![0xFF, 0xD8];
    out.extend(app_segment(0xE1, &exif));
    out.extend(app_segment(0xE1, &xmp));
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn decode_jpeg(bytes: &[u8]) -> RgbImage {
    image::load_from_memory_with_format(bytes, ImageFormat::Jpeg)
        .unwrap()
        .to_rgb8()
}

// ---------------------------------------------------------------------------
// 変換（FR-COVER-03）
// ---------------------------------------------------------------------------

#[test]
fn large_images_are_scaled_to_800_long_edge_as_jpeg_q85() {
    let p = process_cover(&png(1600, 1000)).unwrap();
    assert_eq!((p.width, p.height), (800, 500));
    assert_eq!(&p.jpeg[..3], &[0xFF, 0xD8, 0xFF]);
    let decoded = decode_jpeg(&p.jpeg);
    assert_eq!(decoded.dimensions(), (800, 500));
    // 縦長
    let p = process_cover(&jpeg(600, 2000)).unwrap();
    assert_eq!((p.width, p.height), (240, 800));
    // ちょうど 800 は縮小しない
    let p = process_cover(&png(800, 400)).unwrap();
    assert_eq!((p.width, p.height), (800, 400));
}

#[test]
fn small_images_are_not_upscaled() {
    let p = process_cover(&png(100, 150)).unwrap();
    assert_eq!((p.width, p.height), (100, 150));
}

#[test]
fn file_name_is_the_sha256_of_the_converted_jpeg_and_conversion_is_deterministic() {
    let a = process_cover(&png(300, 400)).unwrap();
    let b = process_cover(&png(300, 400)).unwrap();
    assert_eq!(a.sha256, b.sha256, "同じ画像は同じハッシュ");
    assert_eq!(a.sha256, sha256_hex(&a.jpeg));
    assert!(is_valid_hash(&a.sha256));
    let c = process_cover(&png(300, 401)).unwrap();
    assert_ne!(a.sha256, c.sha256);
    // 元の形式が違っても、画素が同じなら同じ JPEG（PNG と、可逆の WebP）
    let img = DynamicImage::ImageRgb8(gradient(64, 64));
    let from_png = process_cover(&encode(&img, ImageFormat::Png)).unwrap();
    let from_webp = process_cover(&encode(&img, ImageFormat::WebP)).unwrap();
    assert_eq!(from_png.sha256, from_webp.sha256);
}

#[test]
fn exif_xmp_and_other_metadata_are_removed() {
    let src = with_metadata(&jpeg(200, 100), 1);
    assert!(
        src.windows(11).any(|w| w == b"secret-gps-"),
        "入力には含まれている"
    );
    assert!(jpeg_markers(&src).iter().filter(|m| **m == 0xE1).count() == 2);
    let p = process_cover(&src).unwrap();
    assert!(!p.jpeg.windows(11).any(|w| w == b"secret-gps-"));
    assert!(!p.jpeg.windows(4).any(|w| w == b"Exif"));
    assert!(!p.jpeg.windows(4).any(|w| w == b"xmpm"));
    let markers = jpeg_markers(&p.jpeg);
    assert!(
        !markers.contains(&0xE1),
        "APP1（EXIF・XMP）がない: {markers:x?}"
    );
    assert!(!markers.contains(&0xED), "APP13（IPTC）がない");
    assert!(!markers.contains(&0xFE), "コメントがない");
}

#[test]
fn exif_orientation_is_applied_before_it_is_dropped() {
    // 100x50 の画像に「90 度回転して表示」（6）が付いている → 50x100 になる
    let src = with_metadata(&jpeg(100, 50), 6);
    let p = process_cover(&src).unwrap();
    assert_eq!((p.width, p.height), (50, 100));
    // 向きなし（1）はそのまま
    let p = process_cover(&with_metadata(&jpeg(100, 50), 1)).unwrap();
    assert_eq!((p.width, p.height), (100, 50));
}

#[test]
fn transparent_png_is_flattened_onto_white() {
    let mut img = RgbaImage::from_pixel(20, 20, Rgba([255, 0, 0, 0]));
    img.put_pixel(10, 10, Rgba([0, 0, 255, 255]));
    let bytes = encode(&DynamicImage::ImageRgba8(img), ImageFormat::Png);
    let p = process_cover(&bytes).unwrap();
    let decoded = decode_jpeg(&p.jpeg);
    let corner = decoded.get_pixel(1, 1);
    assert!(
        corner.0.iter().all(|c| *c > 235),
        "透明な部分は白: {corner:?}"
    );
}

#[test]
fn gif_webp_and_palette_png_are_accepted() {
    let img = DynamicImage::ImageRgb8(gradient(40, 60));
    for format in [ImageFormat::Gif, ImageFormat::WebP, ImageFormat::Png] {
        let p = process_cover(&encode(&img, format)).unwrap_or_else(|e| panic!("{format:?}: {e}"));
        assert_eq!((p.width, p.height), (40, 60), "{format:?}");
    }
    // 実際のダミー画像（グレースケール・パレット付き PNG）も読める
    for name in ["google_noimage_128x170.png", "google_noimage_57x80.png"] {
        assert!(process_cover(&fixture_bytes(name)).is_ok(), "{name}");
    }
}

#[test]
fn broken_or_non_image_data_is_rejected() {
    assert!(matches!(process_cover(b""), Err(CoverError::Decode(_))));
    assert!(matches!(
        process_cover(b"<html><body>no</body></html>"),
        Err(CoverError::Decode(_))
    ));
    assert!(matches!(
        process_cover(
            b"<html><head><title>ERROR: The request could not be satisfied</title></head></html>"
        ),
        Err(CoverError::Decode(_))
    ));
    // JPEG の先頭だけで中身がない・でたらめ
    assert!(process_cover(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]).is_err());
    let mut junk = vec![0xFF, 0xD8, 0xFF, 0xE0];
    junk.extend((0..2000u32).map(|i| (i * 7919 % 251) as u8));
    assert!(process_cover(&junk).is_err());
}

#[test]
fn oversized_input_and_decompression_bombs_are_rejected() {
    let huge = vec![0u8; 21 * 1024 * 1024];
    assert!(matches!(process_cover(&huge), Err(CoverError::TooLarge(_))));
    // 小さなファイルでも、幅が上限（16384px）を超えるものは読まない
    let wide = encode(
        &DynamicImage::ImageLuma8(image::GrayImage::new(17_000, 2)),
        ImageFormat::Png,
    );
    assert!(wide.len() < 10_000);
    assert!(matches!(process_cover(&wide), Err(CoverError::Decode(_))));
}

// ---------------------------------------------------------------------------
// ダミー画像の判定（FR-COVER-02）
// ---------------------------------------------------------------------------

#[test]
fn known_google_placeholders_are_detected_by_hash_and_size() {
    let files = [
        ("google_noimage_128x170.png", 128, 170),
        ("google_noimage_575x750.png", 575, 750),
        ("google_noimage_57x80.png", 57, 80),
    ];
    for (name, w, h) in files {
        let bytes = fixture_bytes(name);
        assert!(is_known_placeholder(&bytes), "{name}");
        let known = KNOWN_PLACEHOLDERS
            .iter()
            .find(|k| k.sha256 == sha256_hex(&bytes))
            .unwrap_or_else(|| panic!("{name} の SHA-256 が表にない"));
        assert_eq!((known.width, known.height), (w, h), "{name}");
        assert_eq!(known.provider, ProviderId::Google);
    }
    assert_eq!(KNOWN_PLACEHOLDERS.len(), files.len());
}

#[test]
fn other_images_are_not_placeholders() {
    assert!(
        !is_known_placeholder(&png(128, 170)),
        "寸法が同じでもハッシュが違えば別の画像"
    );
    assert!(!is_known_placeholder(&jpeg(575, 750)));
    assert!(!is_known_placeholder(b"not an image"));
    // ダミー画像を再エンコードした JPEG は、元のハッシュと違うのでダミーとは判定しない
    let reencoded = process_cover(&fixture_bytes("google_noimage_128x170.png")).unwrap();
    assert!(!is_known_placeholder(&reencoded.jpeg));
}

// ---------------------------------------------------------------------------
// 保存
// ---------------------------------------------------------------------------

#[test]
fn stores_under_first_two_chars_and_skips_existing() {
    let dir = tempfile::tempdir().unwrap();
    let stored = import_cover_bytes(dir.path(), &png(300, 400)).unwrap();
    assert!(stored.created);
    let expected = dir
        .path()
        .join(&stored.hash[..2])
        .join(format!("{}.jpg", stored.hash));
    assert_eq!(stored.path, expected);
    assert_eq!(cover_path(dir.path(), &stored.hash), expected);
    let bytes = std::fs::read(&expected).unwrap();
    assert_eq!(
        sha256_hex(&bytes),
        stored.hash,
        "ファイル名は中身の SHA-256"
    );
    assert_eq!((stored.width, stored.height), (300, 400));

    // 同じ画像をもう一度: 書かない
    let modified = std::fs::metadata(&expected).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let again = import_cover_bytes(dir.path(), &png(300, 400)).unwrap();
    assert!(!again.created);
    assert_eq!(again.hash, stored.hash);
    assert_eq!(
        std::fs::metadata(&expected).unwrap().modified().unwrap(),
        modified
    );
    // 一時ファイルが残らない
    let leftovers: Vec<_> = std::fs::read_dir(expected.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(leftovers, vec![format!("{}.jpg", stored.hash)]);
}

#[tokio::test]
async fn concurrent_imports_of_the_same_image_leave_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = png(500, 500);
    let tasks: Vec<_> = (0..6)
        .map(|_| {
            tokio::spawn(import_cover_bytes_async(
                dir.path().to_path_buf(),
                bytes.clone(),
            ))
        })
        .collect();
    let mut hashes = Vec::new();
    let mut created = 0;
    for t in tasks {
        let s = t.await.unwrap().unwrap();
        created += usize::from(s.created);
        hashes.push(s.hash);
    }
    assert!(hashes.windows(2).all(|w| w[0] == w[1]));
    assert!(created >= 1);
    let files: Vec<_> = walk(dir.path());
    assert_eq!(files.len(), 1, "{files:?}");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn hash_validation_blocks_path_tricks() {
    assert!(is_valid_hash(&"a".repeat(64)));
    assert!(!is_valid_hash(&"A".repeat(64)));
    assert!(!is_valid_hash("../../etc/passwd"));
    assert!(!is_valid_hash(&"a".repeat(63)));
    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    assert!(ensure_thumbnail(dir.path(), cache.path(), "../../etc/passwd", 160).is_err());
}

// ---------------------------------------------------------------------------
// サムネイル（FR-COVER-04）
// ---------------------------------------------------------------------------

#[test]
fn thumbnails_are_made_on_demand_at_80_160_and_320() {
    assert_eq!(THUMBNAIL_SIZES, [80, 160, 320]);
    let covers = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let stored = import_cover_bytes(covers.path(), &png(1200, 1800)).unwrap();
    assert_eq!((stored.width, stored.height), (533, 800));
    for size in THUMBNAIL_SIZES {
        let path = ensure_thumbnail(covers.path(), cache.path(), &stored.hash, size).unwrap();
        assert_eq!(path, thumbnail_path(cache.path(), &stored.hash, size));
        assert_eq!(
            path,
            cache
                .path()
                .join(size.to_string())
                .join(&stored.hash[..2])
                .join(format!("{}.jpg", stored.hash))
        );
        let img = decode_jpeg(&std::fs::read(&path).unwrap());
        assert_eq!(img.height(), size, "長辺が {size}px");
        assert!(img.width() < size);
    }
    // 2 回目は作り直さない
    let path = thumbnail_path(cache.path(), &stored.hash, 160);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    ensure_thumbnail(covers.path(), cache.path(), &stored.hash, 160).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn thumbnails_are_not_enlarged() {
    let covers = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let stored = import_cover_bytes(covers.path(), &png(100, 150)).unwrap();
    let path = ensure_thumbnail(covers.path(), cache.path(), &stored.hash, 320).unwrap();
    assert_eq!(
        decode_jpeg(&std::fs::read(path).unwrap()).dimensions(),
        (100, 150)
    );
    // 書影がなければエラー
    let missing = "b".repeat(64);
    assert!(ensure_thumbnail(covers.path(), cache.path(), &missing, 160).is_err());
    // 大きさの範囲外
    assert!(ensure_thumbnail(covers.path(), cache.path(), &stored.hash, 5).is_err());
    assert!(ensure_thumbnail(covers.path(), cache.path(), &stored.hash, 5000).is_err());
}

#[test]
fn the_list_thumbnail_is_80px_and_much_smaller_than_the_larger_ones() {
    let covers = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let stored = import_cover_bytes(covers.path(), &png(1200, 1800)).unwrap();
    let sizes: Vec<(u32, u32)> = [80, 160, 320]
        .into_iter()
        .map(|edge| {
            let path = ensure_thumbnail(covers.path(), cache.path(), &stored.hash, edge).unwrap();
            decode_jpeg(&std::fs::read(path).unwrap()).dimensions()
        })
        .collect();
    // 長辺 80px。デコード済みの画像（幅 × 高さ × 4 バイト）は、160px の約 4 分の 1
    assert_eq!(sizes[0], (53, 80));
    assert_eq!(sizes[1], (107, 160));
    assert_eq!(sizes[2], (213, 320));
    let pixels = |(w, h): (u32, u32)| u64::from(w) * u64::from(h);
    assert!(pixels(sizes[0]) * 3 < pixels(sizes[1]));
    // 元が小さいときは、拡大しない
    let small = import_cover_bytes(covers.path(), &png(50, 70)).unwrap();
    let path = ensure_thumbnail(covers.path(), cache.path(), &small.hash, 80).unwrap();
    assert_eq!(
        decode_jpeg(&std::fs::read(path).unwrap()).dimensions(),
        (50, 70)
    );
}

// ---------------------------------------------------------------------------
// 取得（偽の HTTP）
// ---------------------------------------------------------------------------

type Handler = Box<dyn Fn(&str) -> Result<HttpResponse, TransportError> + Send + Sync>;

struct FakeHttp {
    handler: Handler,
    urls: Mutex<Vec<String>>,
}

impl FakeHttp {
    fn new(handler: Handler) -> Arc<Self> {
        Arc::new(FakeHttp {
            handler,
            urls: Mutex::new(Vec::new()),
        })
    }

    fn transport(self: &Arc<Self>) -> Transport {
        Transport::new(self.clone())
    }
}

#[async_trait]
impl HttpClient for FakeHttp {
    async fn get(&self, req: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.urls.lock().unwrap().push(req.url.clone());
        (self.handler)(&req.url)
    }
}

fn ok(content_type: &str, body: Vec<u8>) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status: 200,
        content_type: Some(content_type.into()),
        retry_after: None,
        body,
    })
}

fn cover(provider: ProviderId, url: &str) -> CandidateCover {
    CandidateCover {
        provider,
        url: url.to_string(),
    }
}

/// 画像がないときに XML のエラーを 200 で返す URL
const XML_ERROR_URL: &str = "https://cover.openbd.jp/xml-error.jpg";
/// アクセスを拒否される（403）URL
const BLOCKED_URL: &str = "https://cover.openbd.jp/blocked.jpg";
const OPENBD_URL: &str = "https://cover.openbd.jp/9784780802047.jpg";
const GOOGLE_URL0: &str =
    "https://books.google.com/books/content?id=X&printsec=frontcover&img=1&zoom=0&source=gbs_api";
const GOOGLE_URL1: &str =
    "https://books.google.com/books/content?id=X&printsec=frontcover&img=1&zoom=1&source=gbs_api";

fn covers_handler() -> Handler {
    Box::new(|url| match url {
        OPENBD_URL => ok("image/jpeg", jpeg(600, 900)),
        GOOGLE_URL0 => ok("image/png", fixture_bytes("google_noimage_575x750.png")),
        GOOGLE_URL1 => ok("image/png", fixture_bytes("google_noimage_128x170.png")),
        // 画像がないときは XML のエラー
        XML_ERROR_URL => ok(
            "text/xml;charset=UTF-8",
            b"<?xml version=\"1.0\"?><error>not found</error>".to_vec(),
        ),
        _ => Ok(HttpResponse {
            status: 404,
            content_type: Some("text/plain".into()),
            retry_after: None,
            body: b"no".to_vec(),
        }),
    })
}

#[tokio::test]
async fn first_usable_cover_is_converted_and_saved() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::new(covers_handler());
    let covers = vec![
        cover(ProviderId::Openbd, OPENBD_URL),
        cover(ProviderId::Google, GOOGLE_URL0),
    ];
    let result = acquire_cover(&http.transport(), &covers, dir.path()).await;
    let saved = result.saved.expect("保存できた");
    assert_eq!(saved.provider, ProviderId::Openbd);
    assert_eq!((saved.width, saved.height), (533, 800));
    assert!(saved.created);
    assert!(!result.offline);
    assert_eq!(result.attempts.len(), 1, "使えたところで止まる");
    assert_eq!(result.attempts[0].result, MetaCoverAttemptResult::Saved);
    let file = std::fs::read(cover_path(dir.path(), &saved.hash)).unwrap();
    assert_eq!(sha256_hex(&file), saved.hash);
    assert_eq!(http.urls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn xml_error_and_google_placeholders_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::new(covers_handler());
    let covers = vec![
        cover(ProviderId::Openbd, XML_ERROR_URL),
        cover(ProviderId::Google, GOOGLE_URL0),
        cover(ProviderId::Google, GOOGLE_URL1),
    ];
    let result = acquire_cover(&http.transport(), &covers, dir.path()).await;
    assert!(result.saved.is_none());
    assert!(!result.offline);
    let kinds: Vec<_> = result.attempts.iter().map(|a| a.result.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            MetaCoverAttemptResult::NotAnImage {
                content_type: "text/xml".into()
            },
            MetaCoverAttemptResult::Placeholder,
            MetaCoverAttemptResult::Placeholder,
        ]
    );
    assert!(walk(dir.path()).is_empty(), "何も保存しない");
}

#[tokio::test]
async fn falls_through_http_errors_to_the_next_cover() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::new(Box::new(|url| {
        if url == BLOCKED_URL {
            // CloudFront の 403 の形
            return Ok(HttpResponse {
                status: 403,
                content_type: Some("text/html".into()),
                retry_after: None,
                body: b"<H1>403 ERROR</H1><H2>The request could not be satisfied.</H2>".to_vec(),
            });
        }
        covers_handler()(url)
    }));
    let covers = vec![
        cover(ProviderId::Openbd, BLOCKED_URL),
        cover(ProviderId::Openbd, OPENBD_URL),
    ];
    let result = acquire_cover(&http.transport(), &covers, dir.path()).await;
    assert!(matches!(
        &result.attempts[0].result,
        MetaCoverAttemptResult::HttpError { status: 403, message } if message.contains("403 ERROR")
    ));
    assert_eq!(result.attempts[1].result, MetaCoverAttemptResult::Saved);
    assert_eq!(result.saved.unwrap().provider, ProviderId::Openbd);
}

#[tokio::test(start_paused = true)]
async fn all_network_errors_mean_offline() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::new(Box::new(|_| Err(TransportError::Timeout)));
    let covers = vec![
        cover(ProviderId::Openbd, OPENBD_URL),
        cover(ProviderId::Google, GOOGLE_URL0),
    ];
    let result = acquire_cover(&http.transport(), &covers, dir.path()).await;
    assert!(result.saved.is_none());
    assert!(result.offline);
    assert!(
        result
            .attempts
            .iter()
            .all(|a| matches!(a.result, MetaCoverAttemptResult::NetworkError { .. }))
    );
    // 候補が 1 つもなければオフラインとは言わない
    let result = acquire_cover(&http.transport(), &[], dir.path()).await;
    assert!(!result.offline);
    assert!(result.saved.is_none());
}

#[tokio::test]
async fn tiny_images_and_fake_image_content_types_are_not_covers() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::new(Box::new(|url| match url {
        "https://a.example/1px.gif" => ok(
            "image/gif",
            encode(&DynamicImage::ImageRgb8(gradient(1, 1)), ImageFormat::Gif),
        ),
        "https://a.example/liar.jpg" => ok("image/jpeg", b"<html>not an image</html>".to_vec()),
        "https://a.example/ok.jpg" => ok("image/jpeg; charset=binary", jpeg(200, 300)),
        _ => ok("text/plain", Vec::new()),
    }));
    // 1px の画像は書影として使わない。Content-Type だけ image/jpeg で中身が HTML のものも使わない
    let covers = [
        cover(ProviderId::Openbd, "https://a.example/1px.gif"),
        cover(ProviderId::Openbd, "https://a.example/liar.jpg"),
    ];
    let result = acquire_cover(&http.transport(), &covers, dir.path()).await;
    assert_eq!(
        result.attempts[0].result,
        MetaCoverAttemptResult::Placeholder
    );
    assert!(matches!(
        result.attempts[1].result,
        MetaCoverAttemptResult::NotAnImage { .. }
    ));
    assert!(result.saved.is_none());
    // Content-Type のパラメータ（; charset=binary）は無視して画像と認める。URL の秘密は結果から伏せる
    let covers = vec![cover(ProviderId::Openbd, "https://a.example/ok.jpg")];
    let result = acquire_cover(&http.transport(), &covers, dir.path()).await;
    assert!(result.saved.is_some());
    let masked = FakeHttp::new(Box::new(|_| ok("text/plain", Vec::new())));
    let result = acquire_cover(
        &masked.transport(),
        &[cover(
            ProviderId::Openbd,
            "https://a.example/x.jpg?key=SECRET123",
        )],
        dir.path(),
    )
    .await;
    assert_eq!(result.attempts[0].url, "https://a.example/x.jpg?key=***");
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("SECRET123")
    );
}

#[tokio::test]
async fn download_cover_reports_http_errors() {
    let http = FakeHttp::new(covers_handler());
    let err = download_cover(
        &http.transport(),
        &cover(ProviderId::Openbd, "https://cover.openbd.jp/none.jpg"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, FetchError::Http { status: 404, .. }));
}

// ---------------------------------------------------------------------------
// 手動の差し替え（FR-COVER-05）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn import_from_url_rejects_non_http_schemes_and_non_images() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::new(Box::new(|url| match url {
        "https://a.example/cover.png" => ok("image/png", png(900, 1200)),
        "https://a.example/page.html" => ok("text/html; charset=utf-8", b"<html></html>".to_vec()),
        _ => Ok(HttpResponse {
            status: 404,
            content_type: None,
            retry_after: None,
            body: Vec::new(),
        }),
    }));
    let t = http.transport();
    let stored = import_cover_url(&t, dir.path(), "  https://a.example/cover.png  ")
        .await
        .unwrap();
    assert_eq!((stored.width, stored.height), (600, 800));
    assert!(stored.path.exists());

    for bad in [
        "file:///etc/passwd",
        "ftp://a.example/x.png",
        "javascript:alert(1)",
        "data:image/png;base64,AAAA",
        "not a url",
        "",
    ] {
        assert!(
            matches!(
                import_cover_url(&t, dir.path(), bad).await,
                Err(CoverError::InvalidUrl(_))
            ),
            "{bad}"
        );
    }
    assert!(matches!(
        import_cover_url(&t, dir.path(), "https://a.example/page.html").await,
        Err(CoverError::Decode(_))
    ));
    assert!(matches!(
        import_cover_url(&t, dir.path(), "https://a.example/missing.png").await,
        Err(CoverError::Fetch(FetchError::Http { status: 404, .. }))
    ));
    // 入力した URL は通信の対象だが、file: などは通信しない
    assert_eq!(http.urls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn import_from_dropped_file_or_clipboard_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let stored =
        import_cover_bytes_async(dir.path().to_path_buf(), with_metadata(&jpeg(300, 200), 3))
            .await
            .unwrap();
    assert_eq!(
        (stored.width, stored.height),
        (300, 200),
        "180 度回転は大きさが変わらない"
    );
    let bytes = std::fs::read(&stored.path).unwrap();
    assert!(!jpeg_markers(&bytes).contains(&0xE1));
    assert!(matches!(
        import_cover_bytes_async(dir.path().to_path_buf(), b"plain text".to_vec()).await,
        Err(CoverError::Decode(_))
    ));
}
