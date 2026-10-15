//! ファイルの書式（0013 §2・§3・§6）。
//!
//! - `library.json`、`devices/<id>.json`
//! - セグメント: 1 行 1 Op の JSON Lines。最終行は終端レコード `{"end":true,"count":N,"sha256":"…"}`。
//!   `sha256` は、終端レコードより前の全行のバイト列（各行の `\n` を含む）の SHA-256（小文字の 16 進）。
//! - スナップショット: 同じ規則で終端レコードをつけた JSON Lines を gzip したもの。1 行目が見出し、
//!   2 行目からエンティティごとに 1 行。
//!
//! 読む側は、終端レコードがない・件数やハッシュが合わないファイルを「書き込み途中か同期途中」とみなす
//! （`Incomplete`）。読まずに、次の回に回す。

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::{Read, Write};

use bibcontainer_core::hlc::Hlc;
use bibcontainer_core::op::{OP_VERSION, Op};
use bibcontainer_core::schema::Entity;
use bibcontainer_core::sync_state::EntityState;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::Result;

/// `library.json` の `format`。
pub const LIBRARY_FORMAT: &str = "bibcontainer-library";
/// このアプリが読める `library.json` の形式の版。
pub const SUPPORTED_FORMAT_VERSION: u32 = 1;
/// 新しく作るライブラリの、読める最低のアプリの版。
pub const MIN_APP_VERSION: &str = "0.1.0";
/// スナップショットの形式の版。
pub const SNAPSHOT_VERSION: u32 = 1;
/// 展開後の大きさの上限（壊れたファイル・悪意のあるファイルでメモリを使い切らないため）。
const MAX_DECOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// SHA-256 を小文字の 16 進で返す。
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `a` と `b` の版（`x.y.z`）を数字で比べる。`-` や `+` 以降は無視する。読めない部分は 0 とみなす。
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn parts(v: &str) -> [u64; 3] {
        let core = v.split(['-', '+']).next().unwrap_or("");
        let mut out = [0u64; 3];
        for (slot, part) in out.iter_mut().zip(core.split('.')) {
            *slot = part.trim().parse().unwrap_or(0);
        }
        out
    }
    parts(a).cmp(&parts(b))
}

// ---------------------------------------------------------------------------
// library.json・devices/<id>.json
// ---------------------------------------------------------------------------

/// `library.json`。作成時に 1 回だけ書く。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub format: String,
    pub format_version: u32,
    pub library_id: String,
    pub min_app_version: String,
    pub created_at: String,
    pub created_by: String,
}

impl LibraryFile {
    /// 新しいライブラリの `library.json`。
    pub fn new(library_id: String, created_at: String, created_by: String) -> Self {
        Self {
            format: LIBRARY_FORMAT.to_owned(),
            format_version: SUPPORTED_FORMAT_VERSION,
            library_id,
            min_app_version: MIN_APP_VERSION.to_owned(),
            created_at,
            created_by,
        }
    }
}

/// `devices/<device_id>.json`。その端末だけが書く。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFile {
    pub device_id: String,
    pub name: String,
    pub os: String,
    pub app_version: String,
    pub last_export_at: Option<String>,
    pub max_seq: i64,
}

// ---------------------------------------------------------------------------
// 終端レコード
// ---------------------------------------------------------------------------

/// 書き込み途中か同期途中のファイル（読まずに次の回に回す）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incomplete {
    /// 空のファイル。
    Empty,
    /// UTF-8 として読めない（途中で切れている）。
    NotUtf8,
    /// 終端レコードがない。
    NoEndRecord,
    /// 終端レコードの件数と、実際の行数が合わない。
    CountMismatch { expected: usize, actual: usize },
    /// ハッシュが合わない。
    HashMismatch,
    /// gzip として読めない（途中で切れている）。
    BadGzip(String),
}

impl std::fmt::Display for Incomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Incomplete::Empty => write!(f, "空のファイル"),
            Incomplete::NotUtf8 => write!(f, "UTF-8 として読めない"),
            Incomplete::NoEndRecord => write!(f, "終端レコードがない"),
            Incomplete::CountMismatch { expected, actual } => {
                write!(
                    f,
                    "件数が合わない（終端レコード {expected}、実際 {actual}）"
                )
            }
            Incomplete::HashMismatch => write!(f, "ハッシュが合わない"),
            Incomplete::BadGzip(e) => write!(f, "gzip として読めない: {e}"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct EndRecord {
    end: bool,
    count: usize,
    sha256: String,
}

/// 行（各行は `\n` で終わる）を並べたバイト列に、終端レコードをつける。`count` は行数。
fn seal(mut body: Vec<u8>, count: usize) -> Result<Vec<u8>> {
    let end = EndRecord {
        end: true,
        count,
        sha256: sha256_hex(&body),
    };
    body.extend_from_slice(serde_json::to_string(&end)?.as_bytes());
    body.push(b'\n');
    Ok(body)
}

/// 終端レコードを確かめて、本文の行を返す。
fn unseal(bytes: &[u8]) -> std::result::Result<Vec<&str>, Incomplete> {
    if bytes.is_empty() {
        return Err(Incomplete::Empty);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Incomplete::NotUtf8)?;
    let trimmed = text.trim_end_matches(['\n', '\r']);
    // 最後の行（終端レコードのはず）と、それより前の本文。
    let (body, last) = match trimmed.rfind('\n') {
        Some(i) => (&text[..=i], &trimmed[i + 1..]),
        None => ("", trimmed),
    };
    let end: EndRecord = serde_json::from_str(last).map_err(|_| Incomplete::NoEndRecord)?;
    if !end.end {
        return Err(Incomplete::NoEndRecord);
    }
    let lines: Vec<&str> = body.split_terminator('\n').collect();
    if lines.len() != end.count {
        return Err(Incomplete::CountMismatch {
            expected: end.count,
            actual: lines.len(),
        });
    }
    if sha256_hex(body.as_bytes()) != end.sha256 {
        return Err(Incomplete::HashMismatch);
    }
    Ok(lines)
}

// ---------------------------------------------------------------------------
// セグメント
// ---------------------------------------------------------------------------

/// Op の並びをセグメントのバイト列にする（各行 + 終端レコード）。
pub fn encode_segment(ops: &[Op]) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    for op in ops {
        body.extend_from_slice(op.to_json_line()?.as_bytes());
        body.push(b'\n');
    }
    seal(body, ops.len())
}

/// 読んだセグメントの中身。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SegmentContent {
    /// 適用してよい Op（ファイルの端末・seq の範囲と合うもの）。
    pub ops: Vec<Op>,
    /// このアプリが知らない `ent`・`v` の Op の数（適用せず、範囲も記録しない）。
    pub unknown: usize,
    /// 壊れている・端末や seq の範囲が合わないなどで捨てた Op の数。
    pub invalid: usize,
}

/// セグメントを読む。終端レコード・件数・ハッシュが合わなければ `Incomplete`。
///
/// 各 Op は、`dev` がフォルダの端末 ID と一致し、`seq` がファイル名の範囲に入り、HLC が読めることを
/// 確かめる。合わないものはその Op だけ捨てて警告する（`invalid`）。
pub fn decode_segment(
    bytes: &[u8],
    dev: &str,
    from: i64,
    to: i64,
) -> std::result::Result<SegmentContent, Incomplete> {
    let lines = unseal(bytes)?;
    let mut out = SegmentContent::default();
    for (i, line) in lines.iter().enumerate() {
        match classify_op_line(line) {
            OpLine::Unknown => out.unknown += 1,
            OpLine::Invalid(why) => {
                tracing::warn!(dev, line = i + 1, why, "壊れた Op を捨てた");
                out.invalid += 1;
            }
            OpLine::Op(op) => {
                if op.dev != dev {
                    tracing::warn!(dev, op_dev = %op.dev, seq = op.seq, "端末 ID がフォルダと違う Op を捨てた");
                    out.invalid += 1;
                } else if op.seq < from || op.seq > to {
                    tracing::warn!(
                        dev,
                        seq = op.seq,
                        from,
                        to,
                        "seq がファイル名の範囲にない Op を捨てた"
                    );
                    out.invalid += 1;
                } else {
                    out.ops.push(op);
                }
            }
        }
    }
    Ok(out)
}

enum OpLine {
    Op(Op),
    /// 知らない `ent` または新しい `v`。
    Unknown,
    Invalid(&'static str),
}

fn classify_op_line(line: &str) -> OpLine {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return OpLine::Invalid("JSON として読めない");
    };
    let Some(obj) = value.as_object() else {
        return OpLine::Invalid("オブジェクトでない");
    };
    let Some(v) = obj.get("v").and_then(Value::as_u64) else {
        return OpLine::Invalid("v がない");
    };
    let Some(ent) = obj.get("ent").and_then(Value::as_str) else {
        return OpLine::Invalid("ent がない");
    };
    if v > u64::from(OP_VERSION) || !Entity::ALL.iter().any(|e| e.as_str() == ent) {
        return OpLine::Unknown;
    }
    let Ok(op) = serde_json::from_value::<Op>(value) else {
        return OpLine::Invalid("Op の形が合わない");
    };
    if op.id.is_empty() {
        return OpLine::Invalid("id が空");
    }
    if Hlc::parse(&op.hlc).is_err() {
        return OpLine::Invalid("HLC が読めない");
    }
    OpLine::Op(op)
}

// ---------------------------------------------------------------------------
// スナップショット
// ---------------------------------------------------------------------------

/// スナップショットの見出し（1 行目）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeader {
    pub v: u32,
    pub kind: String,
    pub library_id: String,
    /// 書いた端末。
    pub dev: String,
    /// スナップショットの HLC（ファイル名の元）。
    pub hlc: String,
    /// 端末ごとの「この時点で、Op を seq 1 から何番まで切れ目なく反映したか」。
    pub covers: BTreeMap<String, i64>,
}

#[derive(Serialize)]
struct EntityLine<'a> {
    ent: &'static str,
    id: &'a str,
    fields: &'a BTreeMap<String, (Value, String)>,
}

fn gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::with_capacity(data.len() / 4), Compression::default());
    enc.write_all(data)
        .map_err(|e| crate::error::SyncError::Internal(format!("gzip の書き込みに失敗: {e}")))?;
    enc.finish()
        .map_err(|e| crate::error::SyncError::Internal(format!("gzip の終了に失敗: {e}")))
}

fn gunzip(bytes: &[u8]) -> std::result::Result<Vec<u8>, Incomplete> {
    if bytes.is_empty() {
        return Err(Incomplete::Empty);
    }
    let mut out = Vec::new();
    GzDecoder::new(bytes)
        .take(MAX_DECOMPRESSED_BYTES)
        .read_to_end(&mut out)
        .map_err(|e| Incomplete::BadGzip(e.to_string()))?;
    Ok(out)
}

/// スナップショットをバイト列（gzip した JSON Lines）にする。
pub fn encode_snapshot(header: &SnapshotHeader, entities: &[EntityState]) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(entities.len() * 600 + 512);
    body.extend_from_slice(serde_json::to_string(header)?.as_bytes());
    body.push(b'\n');
    for e in entities {
        let line = EntityLine {
            ent: e.ent.as_str(),
            id: &e.id,
            fields: &e.fields,
        };
        serde_json::to_writer(&mut body, &line)?;
        body.push(b'\n');
    }
    let sealed = seal(body, entities.len() + 1)?;
    gzip(&sealed)
}

/// 読んだスナップショットの中身。
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSnapshot {
    pub header: SnapshotHeader,
    /// 適用できるエンティティ。
    pub entities: Vec<EntityState>,
    /// このアプリが知らない `ent` のエンティティの数（適用しない）。
    pub unknown: usize,
    /// 壊れていて捨てた行・項目の数。
    pub invalid: usize,
}

/// スナップショットを読んだ結果。
#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotRead {
    /// 読めた。
    Snapshot(DecodedSnapshot),
    /// 完成していて壊れていないが、このアプリでは使えない（新しい形式、種類が違う、端末が合わない）。
    Unusable(String),
}

/// スナップショットを読む。gzip・終端レコード・件数・ハッシュが合わなければ `Incomplete`。
/// `dev` は、ファイルのあるフォルダの端末 ID（見出しの `dev` と合うことを確かめる）。
pub fn decode_snapshot(bytes: &[u8], dev: &str) -> std::result::Result<SnapshotRead, Incomplete> {
    let raw = gunzip(bytes)?;
    let lines = unseal(&raw)?;
    let Some(first) = lines.first() else {
        return Ok(SnapshotRead::Unusable("見出しがない".to_owned()));
    };
    let header: SnapshotHeader = match serde_json::from_str(first) {
        Ok(h) => h,
        Err(e) => return Ok(SnapshotRead::Unusable(format!("見出しを読めない: {e}"))),
    };
    if header.v > SNAPSHOT_VERSION || header.kind != "snapshot" {
        return Ok(SnapshotRead::Unusable(format!(
            "対応していない形式（v{}、{}）",
            header.v, header.kind
        )));
    }
    if header.dev != dev {
        return Ok(SnapshotRead::Unusable(format!(
            "見出しの端末 ID（{}）がフォルダ（{dev}）と違う",
            header.dev
        )));
    }
    let mut out = DecodedSnapshot {
        header,
        entities: Vec::with_capacity(lines.len().saturating_sub(1)),
        unknown: 0,
        invalid: 0,
    };
    for line in &lines[1..] {
        match parse_entity_line(line) {
            EntityLineResult::Entity { state, dropped } => {
                out.invalid += dropped;
                out.entities.push(state);
            }
            EntityLineResult::Unknown => out.unknown += 1,
            EntityLineResult::Invalid => out.invalid += 1,
        }
    }
    Ok(SnapshotRead::Snapshot(out))
}

enum EntityLineResult {
    Entity { state: EntityState, dropped: usize },
    Unknown,
    Invalid,
}

fn parse_entity_line(line: &str) -> EntityLineResult {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return EntityLineResult::Invalid;
    };
    let Some(ent_name) = value.get("ent").and_then(Value::as_str) else {
        return EntityLineResult::Invalid;
    };
    let Some(ent) = Entity::ALL.iter().copied().find(|e| e.as_str() == ent_name) else {
        return EntityLineResult::Unknown;
    };
    let Some(id) = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return EntityLineResult::Invalid;
    };
    let Some(fields) = value.get("fields").and_then(Value::as_object) else {
        return EntityLineResult::Invalid;
    };
    let mut state = EntityState {
        ent,
        id: id.to_owned(),
        fields: BTreeMap::new(),
    };
    let mut dropped = 0;
    for (name, pair) in fields {
        let parsed = pair
            .as_array()
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].clone(), a[1].as_str()?.to_owned())))
            .filter(|(_, hlc)| Hlc::parse(hlc).is_ok());
        match parsed {
            Some(entry) => {
                state.fields.insert(name.clone(), entry);
            }
            None => {
                tracing::warn!(ent = ent_name, id, field = %name, "壊れた項目を捨てた");
                dropped += 1;
            }
        }
    }
    EntityLineResult::Entity { state, dropped }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const DEV: &str = "d-aaaaaaaaaaaa";

    fn op(seq: i64) -> Op {
        Op {
            v: 1,
            dev: DEV.to_owned(),
            seq,
            hlc: format!("{:015}:00000:{DEV}", 1_000 + seq),
            ent: Entity::Book,
            id: "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned(),
            set: [("title".to_owned(), json!(format!("本{seq}")))]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn segment_roundtrip_and_layout() {
        let ops = vec![op(1), op(2), op(3)];
        let bytes = encode_segment(&ops).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        let end: Value = serde_json::from_str(lines[3]).unwrap();
        assert_eq!(end["end"], json!(true));
        assert_eq!(end["count"], json!(3));
        // ハッシュは、終端レコードより前の全行（改行を含む）の SHA-256
        let body: String = lines[..3].iter().map(|l| format!("{l}\n")).collect();
        assert_eq!(end["sha256"], json!(sha256_hex(body.as_bytes())));

        let content = decode_segment(&bytes, DEV, 1, 3).unwrap();
        assert_eq!(content.ops, ops);
        assert_eq!((content.unknown, content.invalid), (0, 0));
    }

    #[test]
    fn truncated_and_corrupted_segments_are_incomplete() {
        let bytes = encode_segment(&[op(1), op(2)]).unwrap();
        // どこで切れても、完成扱いにならない
        // 最後の改行だけが欠けた場合は、中身がそろっているので完成扱いでよい。
        for cut in 0..bytes.len() - 1 {
            assert!(
                decode_segment(&bytes[..cut], DEV, 1, 2).is_err(),
                "{cut} バイトで切れたファイルを完成扱いにした"
            );
        }
        // 終端レコードの直前で切れている
        let text = std::str::from_utf8(&bytes).unwrap();
        let no_end = text.rsplit_once("{\"end\"").unwrap().0;
        assert_eq!(
            decode_segment(no_end.as_bytes(), DEV, 1, 2),
            Err(Incomplete::NoEndRecord)
        );
        // 件数が合わない
        let wrong_count = text.replace("\"count\":2", "\"count\":3");
        assert!(matches!(
            decode_segment(wrong_count.as_bytes(), DEV, 1, 2),
            Err(Incomplete::CountMismatch { .. })
        ));
        // ハッシュが合わない
        let wrong_body = text.replacen("本1", "本9", 1);
        assert_eq!(
            decode_segment(wrong_body.as_bytes(), DEV, 1, 2),
            Err(Incomplete::HashMismatch)
        );
        assert_eq!(decode_segment(b"", DEV, 1, 2), Err(Incomplete::Empty));
    }

    #[test]
    fn empty_segment_is_complete() {
        let bytes = encode_segment(&[]).unwrap();
        assert_eq!(decode_segment(&bytes, DEV, 1, 1).unwrap().ops, vec![]);
    }

    #[test]
    fn invalid_unknown_ops_are_classified() {
        let good = op(1).to_json_line().unwrap();
        let other_dev = Op {
            dev: "d-bbbbbbbbbbbb".into(),
            ..op(2)
        }
        .to_json_line()
        .unwrap();
        let out_of_range = op(9).to_json_line().unwrap();
        let bad_hlc = good.replace("000000000001001:00000", "x");
        let future_ent = r#"{"v":1,"dev":"d-aaaaaaaaaaaa","seq":3,"hlc":"000000000001003:00000:d-aaaaaaaaaaaa","ent":"widget","id":"i","set":{}}"#;
        let future_v = r#"{"v":2,"dev":"d-aaaaaaaaaaaa","seq":4,"hlc":"000000000001004:00000:d-aaaaaaaaaaaa","ent":"book","id":"i","set":{"title":{"new":1}}}"#;
        let body: String = [
            good.as_str(),
            &other_dev,
            &out_of_range,
            &bad_hlc,
            future_ent,
            future_v,
            "not json",
        ]
        .iter()
        .map(|l| format!("{l}\n"))
        .collect();
        let bytes = seal(body.into_bytes(), 7).unwrap();
        let c = decode_segment(&bytes, DEV, 1, 5).unwrap();
        assert_eq!(c.ops.len(), 1);
        assert_eq!(c.unknown, 2);
        assert_eq!(c.invalid, 4);
    }

    fn state() -> Vec<EntityState> {
        let h = |ms: i64| format!("{ms:015}:00000:{DEV}");
        vec![
            EntityState {
                ent: Entity::Book,
                id: "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".to_owned(),
                fields: [
                    ("title".to_owned(), (json!("坊っちゃん"), h(10))),
                    ("rating".to_owned(), (Value::Null, h(11))),
                    ("field_sources.title".to_owned(), (json!("openbd"), h(10))),
                    ("zz_unknown".to_owned(), (json!({"a": [1, 2.5]}), h(12))),
                ]
                .into_iter()
                .collect(),
            },
            EntityState {
                ent: Entity::Tag,
                id: "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0b01".to_owned(),
                fields: BTreeMap::new(),
            },
        ]
    }

    fn header() -> SnapshotHeader {
        SnapshotHeader {
            v: SNAPSHOT_VERSION,
            kind: "snapshot".to_owned(),
            library_id: "lib".to_owned(),
            dev: DEV.to_owned(),
            hlc: format!("{:015}:00000:{DEV}", 99),
            covers: [(DEV.to_owned(), 5), ("d-bbbbbbbbbbbb".to_owned(), 7)]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn snapshot_roundtrip() {
        let bytes = encode_snapshot(&header(), &state()).unwrap();
        assert_eq!(&bytes[..2], &[0x1f, 0x8b], "gzip");
        let SnapshotRead::Snapshot(s) = decode_snapshot(&bytes, DEV).unwrap() else {
            panic!("読めるはず");
        };
        assert_eq!(s.header, header());
        assert_eq!(s.entities, state());
        assert_eq!((s.unknown, s.invalid), (0, 0));
    }

    #[test]
    fn snapshot_truncation_is_incomplete_and_foreign_dev_is_unusable() {
        let bytes = encode_snapshot(&header(), &state()).unwrap();
        for cut in [0, 1, 10, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode_snapshot(&bytes[..cut], DEV).is_err(), "{cut}");
        }
        assert!(matches!(
            decode_snapshot(&bytes, "d-cccccccccccc").unwrap(),
            SnapshotRead::Unusable(_)
        ));
    }

    #[test]
    fn snapshot_unknown_entities_and_bad_fields() {
        let mut body = Vec::new();
        body.extend_from_slice(serde_json::to_string(&header()).unwrap().as_bytes());
        body.push(b'\n');
        let h = format!("{:015}:00000:{DEV}", 10);
        for line in [
            json!({"ent":"book","id":"b1","fields":{"title":["x", h], "memo":["y","bad-hlc"], "rating":[1]}}).to_string(),
            json!({"ent":"widget","id":"w1","fields":{}}).to_string(),
            "garbage".to_owned(),
        ] {
            body.extend_from_slice(line.as_bytes());
            body.push(b'\n');
        }
        let sealed = seal(body, 4).unwrap();
        let bytes = gzip(&sealed).unwrap();
        let SnapshotRead::Snapshot(s) = decode_snapshot(&bytes, DEV).unwrap() else {
            panic!()
        };
        assert_eq!(s.entities.len(), 1);
        assert_eq!(s.entities[0].fields.len(), 1);
        assert_eq!(s.unknown, 1);
        assert_eq!(s.invalid, 3); // 壊れた 2 項目 + 壊れた 1 行
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.10.0", "0.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1.0.1"), Ordering::Less);
        assert_eq!(compare_versions("0.2.0-beta", "0.2.0"), Ordering::Equal);
    }
}
