//! 書影の取得・判定・変換・保存（FR-COVER-01〜05）。
//!
//! - 取得: 書影の URL を優先順位の順に試し、最初に使える画像を保存する（`acquire_cover`）。
//! - ダミー画像の判定: 画像がないときに XML などを返す取得元があるので、`Content-Type` が `image/*` かどうかで判定する。
//!   Google Books の「image not available」は、確認した実際の画像（SHA-256 と寸法）と一致するかで判定する。
//!   長辺 40px 未満の画像と、高さ / 幅が 0.5 未満・3.0 超の画像（本文の帯など、書影の形でないもの）も使わない
//!   （`is_usable_auto_cover`。手動の取り込みには使わない）。
//! - 変換: 長辺 800px 以下の JPEG（品質 85）にして EXIF などのメタデータを残さない（デコードして再エンコードする）。
//!   EXIF の向きは先に画像へ反映する（向きの情報だけ消すと、スマホの写真が横倒しになるため）。
//!   透過のある画像は白地に重ねる。ファイル名は変換後の JPEG の SHA-256。
//! - 保存先: `<dir>/<先頭 2 文字>/<hash>.jpg`。同じハッシュのファイルがあれば書かない。一時ファイルに書いてから改名する。
//! - サムネイル（長辺 80px・160px・320px）: 端末のキャッシュに必要になったときに作る（`ensure_thumbnail`）。
//!   80px は一覧の書影の枠（幅 28px・高さ約 39px。2 倍の画面で 56×78px）用、320px は詳細ペインとフォーム用。
//!   160px は今のところ使っていないが、受け付ける（作ったキャッシュと、これから使うかもしれない画面のため）。
//! - 手動の差し替え: バイト列（ドロップ・貼り付け）と、ユーザーが入力した URL から、同じ変換で保存する。

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, Limits, RgbImage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{CoverError, FetchError};
use crate::http::{HttpRequest, MAX_IMAGE_BYTES};
use crate::providers::default_rate_limit;
use crate::transport::{ProviderRateLimit, Transport, http_error};
use crate::types::{CandidateCover, ProviderId};

/// 保存する書影の長辺の上限（px）。
pub const MAX_LONG_EDGE: u32 = 800;
/// 保存する書影の JPEG の品質。
pub const JPEG_QUALITY: u8 = 85;
/// 一覧の書影に使うサムネイルの長辺（px）。
pub const LIST_THUMBNAIL_SIZE: u32 = 80;
/// サムネイルの長辺（px）。80 は一覧、320 は詳細ペインとフォーム（docs/decisions/0010 §12）。
pub const THUMBNAIL_SIZES: [u32; 3] = [LIST_THUMBNAIL_SIZE, 160, 320];
/// サムネイルの JPEG の品質。
pub const THUMBNAIL_QUALITY: u8 = 80;
/// 自動取得で、これより小さい画像（1px の追跡画像など）は書影として使わない。
pub const MIN_AUTO_EDGE: u32 = 40;
/// 自動取得で、書影として使う画像の「高さ / 幅」の下限。これより横長の画像は書影ではない。
///
/// 書影の縦横比（高さ / 幅）は、縦長の本が 1.2〜1.6、正方形の絵本が 1.0、横長の絵本が 0.7 ほど。
/// 図書館がスキャンした本では、Google Books の `zoom=0` が、本文の 1 行を切り出した帯（575x92 = 0.16）を
/// 200 で返すことがある。この下限（0.5）は、横長の絵本（0.7）を通し、その帯を止める。
pub const MIN_AUTO_ASPECT: f64 = 0.5;
/// 自動取得で、書影として使う画像の「高さ / 幅」の上限。これより縦長の画像は書影ではない（帯状の画像）。
/// 縦長の本（1.2〜1.6）には余裕をもたせ、細長い帯だけを止める。
pub const MAX_AUTO_ASPECT: f64 = 3.0;
/// デコードする画像の幅・高さの上限（巨大な画像でメモリを使い切らないため）。
const MAX_DECODE_EDGE: u32 = 16_384;
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// 「画像なし」のダミー画像（確認した実際の画像）。ダウンロードした元のバイト列の SHA-256 と寸法が両方合えばダミー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownPlaceholder {
    pub provider: ProviderId,
    pub sha256: &'static str,
    pub width: u32,
    pub height: u32,
}

/// 2026-10-14 に `books.google.com/books/content?id=<存在しない ID>&printsec=frontcover&img=1&zoom=<n>` で確認した。
/// どれも「image not available」と書かれたグレーの PNG。
pub const KNOWN_PLACEHOLDERS: &[KnownPlaceholder] = &[
    // zoom=1（API の thumbnail）
    KnownPlaceholder {
        provider: ProviderId::Google,
        sha256: "e3f8c414b288cbdf4e6d1e00eb6d3826157d10a5b5628b9318f726ea490eca12",
        width: 128,
        height: 170,
    },
    // zoom=0・2・3・4
    KnownPlaceholder {
        provider: ProviderId::Google,
        sha256: "3efa8c43e5b4348f303a528c81adf435f0111ea752fe9f0f6241478b60987fa6",
        width: 575,
        height: 750,
    },
    // zoom=5（API の smallThumbnail）
    KnownPlaceholder {
        provider: ProviderId::Google,
        sha256: "3abd7c366bd89e9e5e2fb472424b1245aacc948a34a751abca63381067628585",
        width: 57,
        height: 80,
    },
];

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 画像のヘッダだけを読んで、寸法を返す。
fn dimensions(bytes: &[u8]) -> Result<(u32, u32), CoverError> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoverError::Io(e.to_string()))?
        .into_dimensions()
        .map_err(|e| CoverError::Decode(e.to_string()))
}

/// 自動取得（取得元の書影の URL）で、この寸法の画像を書影として使ってよいか。
///
/// 使えないのは、長辺が `MIN_AUTO_EDGE` 未満（追跡用の小さい画像）か、高さ / 幅が `MIN_AUTO_ASPECT` 未満・
/// `MAX_AUTO_ASPECT` 超（本文の帯のような、書影の形でない画像）のとき。
/// 利用者が自分で選んだ画像（`import_cover_bytes` など）には使わない。
pub fn is_usable_auto_cover(width: u32, height: u32) -> bool {
    if width == 0 || width.max(height) < MIN_AUTO_EDGE {
        return false;
    }
    let ratio = f64::from(height) / f64::from(width);
    (MIN_AUTO_ASPECT..=MAX_AUTO_ASPECT).contains(&ratio)
}

/// 保存済みの書影（`cover_path`）の寸法を、ヘッダだけ読んで返す。ファイルがない・読めないときはエラー。
/// ファイルを読むので、非同期の処理の中では別のスレッドで呼ぶ。
pub fn stored_cover_dimensions(dir: &Path, hash: &str) -> Result<(u32, u32), CoverError> {
    if !is_valid_hash(hash) {
        return Err(CoverError::Io(
            "書影のハッシュが正しくありません".to_string(),
        ));
    }
    ImageReader::open(cover_path(dir, hash))
        .map_err(|e| CoverError::Io(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| CoverError::Io(e.to_string()))?
        .into_dimensions()
        .map_err(|e| CoverError::Decode(e.to_string()))
}

/// 既知のダミー画像（ハッシュと寸法が両方合う）か。
pub fn is_known_placeholder(bytes: &[u8]) -> bool {
    let hash = sha256_hex(bytes);
    let Some(known) = KNOWN_PLACEHOLDERS.iter().find(|k| k.sha256 == hash) else {
        return false;
    };
    matches!(dimensions(bytes), Ok((w, h)) if w == known.width && h == known.height)
}

// ---------------------------------------------------------------------------
// 変換
// ---------------------------------------------------------------------------

/// 変換後の書影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedCover {
    pub jpeg: Vec<u8>,
    /// `jpeg` の SHA-256（小文字の 16 進 64 桁）。保存するときのファイル名。
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}

fn decode(bytes: &[u8]) -> Result<DynamicImage, CoverError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(CoverError::TooLarge(format!("{} バイト", bytes.len())));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_EDGE);
    limits.max_image_height = Some(MAX_DECODE_EDGE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoverError::Io(e.to_string()))?;
    reader.limits(limits);
    if reader.format().is_none() {
        return Err(CoverError::Decode("画像の形式を判別できません".to_string()));
    }
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| CoverError::Decode(e.to_string()))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img =
        DynamicImage::from_decoder(decoder).map_err(|e| CoverError::Decode(e.to_string()))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// 透過のある画像は白地に重ねて RGB にする（JPEG に透過はない）。
fn to_rgb(img: &DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        let blend = |c: u8| -> u8 { ((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8 };
        out.put_pixel(x, y, image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }
    out
}

fn fit(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= long_edge {
        return (width, height);
    }
    let scale = |v: u32| -> u32 {
        ((u64::from(v) * u64::from(long_edge) + u64::from(long) / 2) / u64::from(long)).max(1)
            as u32
    };
    (scale(width), scale(height))
}

fn encode_jpeg(rgb: &RgbImage, quality: u8) -> Result<Vec<u8>, CoverError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| CoverError::Encode(e.to_string()))?;
    Ok(out)
}

fn resized_jpeg(
    img: &DynamicImage,
    long_edge: u32,
    quality: u8,
) -> Result<(Vec<u8>, u32, u32), CoverError> {
    let rgb = to_rgb(img);
    let (w, h) = fit(rgb.width(), rgb.height(), long_edge);
    let rgb = if (w, h) != (rgb.width(), rgb.height()) {
        image::imageops::resize(&rgb, w, h, FilterType::Lanczos3)
    } else {
        rgb
    };
    let jpeg = encode_jpeg(&rgb, quality)?;
    Ok((jpeg, rgb.width(), rgb.height()))
}

/// 任意の画像（JPEG・PNG・GIF・WebP）を、保存用の JPEG に変換する。
pub fn process_cover(bytes: &[u8]) -> Result<ProcessedCover, CoverError> {
    let img = decode(bytes)?;
    let (jpeg, width, height) = resized_jpeg(&img, MAX_LONG_EDGE, JPEG_QUALITY)?;
    let sha256 = sha256_hex(&jpeg);
    Ok(ProcessedCover {
        jpeg,
        sha256,
        width,
        height,
    })
}

// ---------------------------------------------------------------------------
// 保存
// ---------------------------------------------------------------------------

/// ハッシュとして正しいか（小文字の 16 進 64 桁）。パスの組み立てに使う前の検査。
pub fn is_valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `<dir>/<先頭 2 文字>/<hash>.jpg`
pub fn cover_path(dir: &Path, hash: &str) -> PathBuf {
    dir.join(&hash[..2]).join(format!("{hash}.jpg"))
}

/// `<cache_dir>/<長辺>/<先頭 2 文字>/<hash>.jpg`
pub fn thumbnail_path(cache_dir: &Path, hash: &str, long_edge: u32) -> PathBuf {
    cache_dir
        .join(long_edge.to_string())
        .join(&hash[..2])
        .join(format!("{hash}.jpg"))
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 一時ファイルに書いてから改名する（途中で止まっても、壊れたファイルが最終的な名前で残らない）。
/// 戻り値は、今回書いたか（すでにあって書かなかったときは `false`）。
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<bool, CoverError> {
    if path.exists() {
        return Ok(false);
    }
    let dir = path
        .parent()
        .ok_or_else(|| CoverError::Io("保存先が正しくありません".to_string()))?;
    std::fs::create_dir_all(dir).map_err(|e| CoverError::Io(e.to_string()))?;
    let tmp = dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&tmp, bytes).map_err(|e| CoverError::Io(e.to_string()))?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(true),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            // 同時に同じ書影を保存した相手が先に改名したなら、それでよい
            if path.exists() {
                Ok(false)
            } else {
                Err(CoverError::Io(e.to_string()))
            }
        }
    }
}

/// 保存した書影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCover {
    pub hash: String,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// 今回ファイルを書いた（同じハッシュがすでにあれば `false`）
    pub created: bool,
}

/// 変換済みの書影を `<dir>/<先頭 2 文字>/<hash>.jpg` に保存する。同じハッシュがあれば書かない。
pub fn store_cover(dir: &Path, cover: &ProcessedCover) -> Result<StoredCover, CoverError> {
    let path = cover_path(dir, &cover.sha256);
    let created = write_atomically(&path, &cover.jpeg)?;
    Ok(StoredCover {
        hash: cover.sha256.clone(),
        path,
        width: cover.width,
        height: cover.height,
        created,
    })
}

/// バイト列（ファイルのドロップ・クリップボード）から保存する。
pub fn import_cover_bytes(dir: &Path, bytes: &[u8]) -> Result<StoredCover, CoverError> {
    store_cover(dir, &process_cover(bytes)?)
}

/// `import_cover_bytes` を、画像処理でスレッドを止めないよう別のスレッドで行う。
pub async fn import_cover_bytes_async(
    dir: PathBuf,
    bytes: Vec<u8>,
) -> Result<StoredCover, CoverError> {
    tokio::task::spawn_blocking(move || import_cover_bytes(&dir, &bytes))
        .await
        .map_err(|e| CoverError::Io(format!("画像の処理が中断されました: {e}")))?
}

/// ユーザーが入力した URL（http・https のみ）から保存する。
pub async fn import_cover_url(
    transport: &Transport,
    dir: &Path,
    url: &str,
) -> Result<StoredCover, CoverError> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|e| CoverError::InvalidUrl(e.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(CoverError::InvalidUrl(
            "http または https の URL を入力してください".to_string(),
        ));
    }
    let limit = ProviderRateLimit::unlimited("user_url");
    let resp = transport
        .get(
            &limit,
            HttpRequest::get(url).with_max_bytes(MAX_IMAGE_BYTES),
        )
        .await?;
    if !resp.is_success() {
        return Err(http_error(&resp).into());
    }
    if let Some(mime) = resp.mime()
        && (mime.starts_with("text/") || mime.contains("xml") || mime.contains("json"))
    {
        return Err(CoverError::Decode(format!(
            "画像ではありません（Content-Type: {mime}）"
        )));
    }
    import_cover_bytes_async(dir.to_path_buf(), resp.body).await
}

// ---------------------------------------------------------------------------
// サムネイル
// ---------------------------------------------------------------------------

/// 保存済みの書影（または任意の画像）から、長辺 `long_edge` px の JPEG を作る。元より大きくはしない。
pub fn make_thumbnail(source: &[u8], long_edge: u32) -> Result<Vec<u8>, CoverError> {
    let img = decode(source)?;
    Ok(resized_jpeg(&img, long_edge.max(1), THUMBNAIL_QUALITY)?.0)
}

/// サムネイルのファイルを返す。なければ、書影から作ってキャッシュに保存する。
/// `long_edge` は 80・160・320 のどれかを使う（`THUMBNAIL_SIZES`）。
pub fn ensure_thumbnail(
    cover_dir: &Path,
    cache_dir: &Path,
    hash: &str,
    long_edge: u32,
) -> Result<PathBuf, CoverError> {
    if !is_valid_hash(hash) {
        return Err(CoverError::Io(
            "書影のハッシュが正しくありません".to_string(),
        ));
    }
    if !(16..=MAX_LONG_EDGE).contains(&long_edge) {
        return Err(CoverError::Io(format!(
            "サムネイルの大きさが正しくありません: {long_edge}"
        )));
    }
    let target = thumbnail_path(cache_dir, hash, long_edge);
    if target.exists() {
        return Ok(target);
    }
    let source =
        std::fs::read(cover_path(cover_dir, hash)).map_err(|e| CoverError::Io(e.to_string()))?;
    let jpeg = make_thumbnail(&source, long_edge)?;
    write_atomically(&target, &jpeg)?;
    Ok(target)
}

// ---------------------------------------------------------------------------
// 取得
// ---------------------------------------------------------------------------

/// 書影の URL を 1 つダウンロードした結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverDownload {
    /// 使えそうな画像（元のバイト列）
    Image(Vec<u8>),
    /// 画像ではない応答（画像がないときに XML や HTML のエラーを返す取得元がある）
    NotAnImage { content_type: String },
    /// 「画像なし」のダミー画像、小さすぎる画像、または書影の形でない（細長い帯状の）画像
    /// （`is_usable_auto_cover`）
    Placeholder,
}

/// 書影の URL からダウンロードして、ダミー画像・小さすぎる画像・書影の形でない画像かどうかを判定する。
/// HTTP のステータスが 2xx 以外なら `FetchError::Http`（403・404 など）。
pub async fn download_cover(
    transport: &Transport,
    cover: &CandidateCover,
) -> Result<CoverDownload, FetchError> {
    let limit = default_rate_limit(cover.provider);
    let resp = transport
        .get(
            &limit,
            HttpRequest::get(cover.url.clone()).with_max_bytes(MAX_IMAGE_BYTES),
        )
        .await?;
    if !resp.is_success() {
        return Err(http_error(&resp));
    }
    let mime = resp.mime().unwrap_or_default();
    if !mime.starts_with("image/") {
        return Ok(CoverDownload::NotAnImage { content_type: mime });
    }
    if is_known_placeholder(&resp.body) {
        return Ok(CoverDownload::Placeholder);
    }
    match dimensions(&resp.body) {
        Ok((w, h)) if is_usable_auto_cover(w, h) => Ok(CoverDownload::Image(resp.body)),
        Ok(_) => Ok(CoverDownload::Placeholder),
        Err(_) => Ok(CoverDownload::NotAnImage {
            content_type: format!("{mime}（画像として読めません）"),
        }),
    }
}

/// 書影を 1 つ試した結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum MetaCoverAttemptResult {
    Saved,
    Placeholder,
    NotAnImage { content_type: String },
    NetworkError { message: String },
    HttpError { status: u16, message: String },
    DecodeError { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaCoverAttempt {
    pub provider: ProviderId,
    /// ログと同じく、API キーなどを伏せた URL
    pub url: String,
    pub result: MetaCoverAttemptResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaSavedCover {
    /// 保存した書影の SHA-256（`books.cover_hash`）
    pub hash: String,
    pub provider: ProviderId,
    pub url: String,
    pub width: u32,
    pub height: u32,
    /// 今回ファイルを書いた（同じ画像がすでにあれば `false`）
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaCoverResult {
    pub saved: Option<MetaSavedCover>,
    pub attempts: Vec<MetaCoverAttempt>,
    /// 試した URL がすべて通信エラーだった（オフライン。あとで取り直す）
    pub offline: bool,
}

/// 書影の URL を順に試し、最初に使えた画像を変換して `dir` に保存する（FR-COVER-01〜03）。
/// ダミー画像・小さすぎる画像・書影の形でない画像（`is_usable_auto_cover`。結果は `placeholder`）・
/// 画像でない応答・HTTP エラーなら次の URL へ進む。
pub async fn acquire_cover(
    transport: &Transport,
    covers: &[CandidateCover],
    dir: &Path,
) -> MetaCoverResult {
    let mut attempts: Vec<MetaCoverAttempt> = Vec::new();
    for cover in covers {
        let shown = crate::http::mask_url(&cover.url);
        let result = match download_cover(transport, cover).await {
            Err(FetchError::Network(message)) => MetaCoverAttemptResult::NetworkError { message },
            Err(FetchError::Http { status, message }) => {
                MetaCoverAttemptResult::HttpError { status, message }
            }
            Err(FetchError::Parse(message)) => MetaCoverAttemptResult::DecodeError { message },
            Ok(CoverDownload::Placeholder) => MetaCoverAttemptResult::Placeholder,
            Ok(CoverDownload::NotAnImage { content_type }) => {
                MetaCoverAttemptResult::NotAnImage { content_type }
            }
            Ok(CoverDownload::Image(bytes)) => {
                match import_cover_bytes_async(dir.to_path_buf(), bytes).await {
                    Ok(stored) => {
                        attempts.push(MetaCoverAttempt {
                            provider: cover.provider,
                            url: shown.clone(),
                            result: MetaCoverAttemptResult::Saved,
                        });
                        return MetaCoverResult {
                            saved: Some(MetaSavedCover {
                                hash: stored.hash,
                                provider: cover.provider,
                                url: shown,
                                width: stored.width,
                                height: stored.height,
                                created: stored.created,
                            }),
                            attempts,
                            offline: false,
                        };
                    }
                    Err(e) => MetaCoverAttemptResult::DecodeError {
                        message: e.to_string(),
                    },
                }
            }
        };
        tracing::debug!(url = %shown, ?result, "書影を使えませんでした");
        attempts.push(MetaCoverAttempt {
            provider: cover.provider,
            url: shown,
            result,
        });
    }
    let offline = !attempts.is_empty()
        && attempts
            .iter()
            .all(|a| matches!(a.result, MetaCoverAttemptResult::NetworkError { .. }));
    MetaCoverResult {
        saved: None,
        attempts,
        offline,
    }
}
