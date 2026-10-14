//! ローカル DB のバックアップと復元（FR-IO-05）。
//!
//! # ファイル名とローテーション
//!
//! バックアップは `VACUUM INTO` で作る（WAL でも一貫したスナップショットになる）。
//! アプリは、バックアップ専用に別に開いた読み取り専用の接続（`open_reader`）で実行する。WAL では
//! 読み取りと書き込みが並行できるので、バックアップ中も、アプリの接続は検索・一覧・保存を待たされない
//! （`VACUUM INTO` は 1 つの読み取りトランザクションなので、ある時点のスナップショットになり、
//! 実行中の保存は含まれない）。
//! 保存先のフォルダ（`backup_dir`）に、次の名前で置く。時刻は UTC。
//!
//! | 種類 | 名前 | 世代 |
//! |---|---|---|
//! | 日次 | `daily-YYYYMMDDTHHMMSSZ.sqlite3` | 新しい 7 つ |
//! | 週次 | `weekly-YYYYMMDDTHHMMSSZ.sqlite3` | 新しい 4 つ |
//! | 手動 | `manual-YYYYMMDDTHHMMSSZ.sqlite3` | 消さない |
//! | 移行前 | `pre-migration-v{版}-YYYYMMDDTHHMMSSZ.sqlite3` | 消さない（`migrate` が作る） |
//! | 復元前 | `pre-restore-YYYYMMDDTHHMMSSZ.sqlite3`（と `-wal`） | 消さない |
//!
//! 同じ秒に重なったときは名前の末尾に `-1`、`-2` を付ける。`.tmp` で始まる作成途中のファイルは数えない。
//!
//! `run_scheduled_backup` は、最後の日次バックアップから 24 時間以上たっていれば日次を作り、
//! 最後の週次から 7 日以上たっていれば、その日次のコピーを週次として残す。そのあとローテーションする。
//!
//! # 復元
//!
//! `restore_backup` は、バックアップを検証（開ける、`integrity_check` が ok、版が新しすぎない、
//! bibcontainer の DB である）してから DB ファイルを置き換える。呼び出し側は、DB の接続をすべて閉じてから
//! 呼ぶこと（`-wal`・`-shm` も扱う）。復元した DB には**新しい端末 ID を振り、seq のカウンタを 0 に戻す**
//! （復元前の端末が書き出した Op と `(dev, seq)` が重ならないように。`docs/decisions/0005` §2）。
//! outbox に残っていた未書き出しの Op は、古い端末 ID のまま `outbox_restored` に移す
//! （扱いはフェーズ 2 で決める）。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{NaiveDateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::BUSY_TIMEOUT_MS;
use crate::error::{Error, Result};
use crate::hlc::Clock;
use crate::ids::new_device_id;
use crate::meta;
use crate::migrate::{LATEST_VERSION, MIGRATIONS, migrate_without_backup, user_version};

/// 日次バックアップを残す世代数。
pub const DAILY_KEEP: usize = 7;
/// 週次バックアップを残す世代数。
pub const WEEKLY_KEEP: usize = 4;

/// 日次・週次バックアップを残す世代数（FR-CONF-06）。既定は日次 7、週次 4。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keep {
    pub daily: usize,
    pub weekly: usize,
}

impl Default for Keep {
    fn default() -> Self {
        Keep {
            daily: DAILY_KEEP,
            weekly: WEEKLY_KEEP,
        }
    }
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const EXTENSION: &str = "sqlite3";

/// バックアップの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BackupKind {
    Daily,
    Weekly,
    Manual,
    PreMigration,
    PreRestore,
}

impl BackupKind {
    fn prefix(self) -> &'static str {
        match self {
            BackupKind::Daily => "daily",
            BackupKind::Weekly => "weekly",
            BackupKind::Manual => "manual",
            BackupKind::PreMigration => "pre-migration",
            BackupKind::PreRestore => "pre-restore",
        }
    }

    const ALL: [BackupKind; 5] = [
        BackupKind::Daily,
        BackupKind::Weekly,
        BackupKind::Manual,
        BackupKind::PreMigration,
        BackupKind::PreRestore,
    ];
}

/// バックアップのファイル 1 つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupInfo {
    /// ファイルのパス。
    pub path: String,
    pub file_name: String,
    pub kind: BackupKind,
    /// 作った時刻（ファイル名の時刻。ISO 8601 UTC）。読めなければ null。
    pub created_at: Option<String>,
    pub size_bytes: u64,
}

/// バックアップの検証結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupCheck {
    /// バックアップの DB の版（`user_version`）。
    pub schema_version: u32,
    /// 完全削除していない本の数。
    pub book_count: i64,
    /// バックアップを取った端末の ID。
    pub device_id: Option<String>,
}

/// 復元の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreReport {
    /// バックアップの DB の版。
    pub schema_version: u32,
    /// バックアップに入っていた端末 ID。
    pub old_device_id: Option<String>,
    /// 復元した DB に振った新しい端末 ID。
    pub new_device_id: String,
    /// outbox の未書き出しの Op を `outbox_restored` に移した件数。
    pub moved_unflushed_ops: u32,
    /// 置き換える前の DB を残した場所（元の DB が無かったときは null）。
    pub pre_restore_path: Option<String>,
}

fn stamp_of(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_default()
        .format("%Y%m%dT%H%M%SZ")
        .to_string()
}

/// ファイル名から時刻（`YYYYMMDDTHHMMSSZ`）を取り出す。
fn parse_stamp(file_name: &str) -> Option<NaiveDateTime> {
    let b = file_name.as_bytes();
    (0..b.len().saturating_sub(15)).find_map(|i| {
        let w = &b[i..i + 16];
        let ok = w[..8].iter().all(u8::is_ascii_digit)
            && w[8] == b'T'
            && w[9..15].iter().all(u8::is_ascii_digit)
            && w[15] == b'Z';
        if ok {
            NaiveDateTime::parse_from_str(&file_name[i..i + 16], "%Y%m%dT%H%M%SZ").ok()
        } else {
            None
        }
    })
}

fn kind_of(file_name: &str) -> Option<BackupKind> {
    if !file_name.ends_with(&format!(".{EXTENSION}")) {
        return None;
    }
    BackupKind::ALL
        .into_iter()
        .find(|k| file_name.starts_with(&format!("{}-", k.prefix())))
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))
}

/// 既存のファイルと重ならない名前を決める。
fn unique_path(dir: &Path, base: &str) -> PathBuf {
    let mut path = dir.join(format!("{base}.{EXTENSION}"));
    let mut n = 0;
    while path.exists() {
        n += 1;
        path = dir.join(format!("{base}-{n}.{EXTENSION}"));
    }
    path
}

fn info_of(path: &Path) -> Option<BackupInfo> {
    let file_name = path.file_name()?.to_str()?.to_owned();
    let kind = kind_of(&file_name)?;
    let size_bytes = fs::metadata(path).ok()?.len();
    Some(BackupInfo {
        path: path.to_str()?.to_owned(),
        created_at: parse_stamp(&file_name).map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        file_name,
        kind,
        size_bytes,
    })
}

/// `backup_dir` のバックアップの一覧（新しい順）。フォルダが無ければ空。
pub fn list_backups(backup_dir: &Path) -> Result<Vec<BackupInfo>> {
    let entries = match fs::read_dir(backup_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out: Vec<(NaiveDateTime, BackupInfo)> = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if let Some(info) = info_of(&path) {
            let t = parse_stamp(&info.file_name).unwrap_or_default();
            out.push((t, info));
        }
    }
    out.sort_by(|a, b| (&b.0, &b.1.file_name).cmp(&(&a.0, &a.1.file_name)));
    Ok(out.into_iter().map(|(_, i)| i).collect())
}

/// バックアップを取るための、読み取り専用の接続を開く。
///
/// アプリの接続（書き込みをする側）とは別の接続で、`VACUUM INTO` を実行するために使う
/// （`create_backup` などの `conn` に渡す）。WAL モードの DB は、別の接続が書き込んでいる間も
/// 読み取れる。`VACUUM INTO` は 1 つの読み取りトランザクションで、始まった時点のスナップショットになる。
/// DB を書き換えられない接続なので、誤って DB を変えることもない。
/// DB のファイルは、すでに別の接続で開いて WAL モードにしてあること（`open` が済んでいること）。
pub fn open_reader(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // 書き込みの側がロックを持っている間（チェックポイントなど）は、待つ。
    conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
    Ok(conn)
}

/// バックアップを 1 つ作る（`VACUUM INTO`）。種類に応じた名前で `backup_dir` に置く。
///
/// 作成途中のファイルは `.` で始まる一時ファイルで、完成してから名前を付け替える。
/// `conn` は、アプリの書き込みの接続ではなく `open_reader` で開いた読み取り専用の接続を渡すと、
/// 作成中も書き込みを止めない。
pub fn create_backup(
    conn: &Connection,
    backup_dir: &Path,
    kind: BackupKind,
    clock: &dyn Clock,
) -> Result<BackupInfo> {
    fs::create_dir_all(backup_dir)?;
    let base = format!("{}-{}", kind.prefix(), stamp_of(clock.now_ms()));
    let tmp = backup_dir.join(format!(".{base}.tmp"));
    let _ = fs::remove_file(&tmp);
    let result = conn
        .execute("VACUUM INTO ?1", [path_str(&tmp)?])
        .map_err(Error::from)
        .and_then(|_| {
            let path = unique_path(backup_dir, &base);
            fs::rename(&tmp, &path)?;
            Ok(path)
        });
    let path = match result {
        Ok(p) => p,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    tracing::info!(path = %path.display(), kind = kind.prefix(), "バックアップを作った");
    info_of(&path).ok_or_else(|| Error::InvalidPath(path))
}

/// 手動バックアップを作る。ローテーションの対象外。
pub fn manual_backup(
    conn: &Connection,
    backup_dir: &Path,
    clock: &dyn Clock,
) -> Result<BackupInfo> {
    create_backup(conn, backup_dir, BackupKind::Manual, clock)
}

/// その種類の最新のバックアップの時刻（ミリ秒）。
fn newest_ms(backup_dir: &Path, kind: BackupKind) -> Result<Option<i64>> {
    Ok(list_backups(backup_dir)?
        .iter()
        .filter(|b| b.kind == kind)
        .filter_map(|b| parse_stamp(&b.file_name))
        .map(|t| t.and_utc().timestamp_millis())
        .max())
}

/// 日次バックアップを取る時期か（最後の日次から 24 時間以上たったか、日次がまだ無い）。
pub fn daily_backup_due(backup_dir: &Path, clock: &dyn Clock) -> Result<bool> {
    Ok(
        newest_ms(backup_dir, BackupKind::Daily)?
            .is_none_or(|last| clock.now_ms() - last >= DAY_MS),
    )
}

/// 日次バックアップの自動実行。時期でなければ何もせず `None`。
///
/// 日次を作り、最後の週次から 7 日以上たっていれば（または週次がまだ無ければ）そのコピーを週次として残し、
/// ローテーションする（日次 7 世代、週次 4 世代）。作った日次の情報を返す。
/// ローテーションに失敗してもバックアップ自体は成功として扱う（警告をログに出す）。
pub fn run_scheduled_backup(
    conn: &Connection,
    backup_dir: &Path,
    clock: &dyn Clock,
) -> Result<Option<BackupInfo>> {
    run_scheduled_backup_keeping(conn, backup_dir, clock, Keep::default())
}

/// `run_scheduled_backup` の、残す世代数を指定できる版。
pub fn run_scheduled_backup_keeping(
    conn: &Connection,
    backup_dir: &Path,
    clock: &dyn Clock,
    keep: Keep,
) -> Result<Option<BackupInfo>> {
    if !daily_backup_due(backup_dir, clock)? {
        return Ok(None);
    }
    let daily = create_backup(conn, backup_dir, BackupKind::Daily, clock)?;
    let weekly_due = newest_ms(backup_dir, BackupKind::Weekly)?
        .is_none_or(|last| clock.now_ms() - last >= 7 * DAY_MS);
    if weekly_due && let Err(e) = copy_as_weekly(&daily, backup_dir, clock) {
        tracing::warn!(error = %e, "週次バックアップを作れなかった");
    }
    if let Err(e) = rotate_backups_keeping(backup_dir, keep) {
        tracing::warn!(error = %e, "古いバックアップを消せなかった");
    }
    Ok(Some(daily))
}

fn copy_as_weekly(daily: &BackupInfo, backup_dir: &Path, clock: &dyn Clock) -> Result<()> {
    let base = format!(
        "{}-{}",
        BackupKind::Weekly.prefix(),
        stamp_of(clock.now_ms())
    );
    let tmp = backup_dir.join(format!(".{base}.tmp"));
    fs::copy(&daily.path, &tmp)?;
    let path = unique_path(backup_dir, &base);
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// 日次を新しい 7 つ、週次を新しい 4 つだけ残して、古いものを消す。消したファイルを返す。
/// 手動・移行前・復元前のバックアップは消さない。
pub fn rotate_backups(backup_dir: &Path) -> Result<Vec<PathBuf>> {
    rotate_backups_keeping(backup_dir, Keep::default())
}

/// `rotate_backups` の、残す世代数を指定できる版。
pub fn rotate_backups_keeping(backup_dir: &Path, keep: Keep) -> Result<Vec<PathBuf>> {
    let all = list_backups(backup_dir)?; // 新しい順
    let mut removed = Vec::new();
    for (kind, keep) in [
        (BackupKind::Daily, keep.daily),
        (BackupKind::Weekly, keep.weekly),
    ] {
        for old in all.iter().filter(|b| b.kind == kind).skip(keep) {
            let path = PathBuf::from(&old.path);
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }
    Ok(removed)
}

/// 作業用のコピーを DB として開いて調べる。コピーの上でだけ書き込む（元のファイルは触らない）。
fn inspect(conn: &Connection) -> Result<BackupCheck> {
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| Error::InvalidBackup(format!("検査できません: {e}")))?;
    if result != "ok" {
        return Err(Error::InvalidBackup(format!(
            "整合性の検査に失敗しました: {result}"
        )));
    }
    let schema_version = user_version(conn)?;
    if schema_version == 0 {
        return Err(Error::InvalidBackup(
            "bibcontainer のデータベースではありません（版がありません）".to_owned(),
        ));
    }
    if schema_version > LATEST_VERSION {
        return Err(Error::SchemaTooNew {
            found: schema_version,
            supported: LATEST_VERSION,
        });
    }
    for table in ["books", "local_meta", "outbox", "field_clocks"] {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::InvalidBackup(format!(
                "bibcontainer のデータベースではありません（{table} がありません）"
            )));
        }
    }
    Ok(BackupCheck {
        schema_version,
        book_count: conn.query_row("SELECT count(*) FROM books WHERE purged = 0", [], |r| {
            r.get(0)
        })?,
        device_id: meta::get(conn, meta::KEY_DEVICE_ID)?,
    })
}

/// 作業用のコピーを置くファイル。使い終わったら（`-wal`・`-shm` も）消す。
struct Scratch(PathBuf);

impl Scratch {
    fn new(path: PathBuf) -> Result<Self> {
        let scratch = Scratch(path);
        scratch.remove();
        Ok(scratch)
    }

    fn sidecar(&self, suffix: &str) -> PathBuf {
        let mut name = self.0.clone().into_os_string();
        name.push(suffix);
        PathBuf::from(name)
    }

    fn remove(&self) {
        for p in [
            self.0.clone(),
            self.sidecar("-wal"),
            self.sidecar("-shm"),
            self.sidecar("-journal"),
        ] {
            let _ = fs::remove_file(p);
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.remove();
    }
}

fn open_scratch(backup: &Path, scratch: &Path) -> Result<Connection> {
    if !backup.is_file() {
        return Err(Error::InvalidBackup(format!(
            "ファイルが見つかりません: {}",
            backup.display()
        )));
    }
    fs::copy(backup, scratch)?;
    Connection::open_with_flags(
        scratch,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| Error::InvalidBackup(format!("開けません: {e}")))
}

/// バックアップを検証する（コピーを作って開き、`integrity_check` と版を調べる）。元のファイルは変えない。
pub fn verify_backup(backup: &Path) -> Result<BackupCheck> {
    let scratch = Scratch::new(std::env::temp_dir().join(format!(
        "bibcontainer-verify-{}-{}.sqlite3",
        std::process::id(),
        crate::ids::new_entity_id()
    )))?;
    let conn = open_scratch(backup, &scratch.0)?;
    let check = inspect(&conn)?;
    drop(conn);
    Ok(check)
}

/// バックアップから DB を復元する。
///
/// 1. バックアップを `db_path` の隣の作業用ファイルにコピーして検証する（`verify_backup` と同じ検査）。
///    古い版ならマイグレーションで最新の版にそろえる（バックアップは取り直さない）。
/// 2. 作業用ファイルに新しい端末 ID を振り、seq のカウンタ（`op_seq_last`）を 0 に戻す。
///    outbox は空にして、未書き出しの Op を古い端末 ID のまま `outbox_restored` に移す
///    （outbox は seq が主キーなので、seq を振り直すと重なるため）。
/// 3. 今の DB（`-wal` も）を `safety_dir/pre-restore-<時刻>.sqlite3` にコピーして残す。
/// 4. 今の DB の `-wal`・`-shm` を消してから、作業用ファイルを `db_path` に置き換える。
///
/// 呼び出し側は、DB の接続をすべて閉じてから呼ぶこと。置き換えたあとは `open` で開き直す
/// （検索用の派生データは、バックアップが古い版のときだけ `open` が作り直す）。
/// 検証に失敗したときは、今の DB には何もしない。
pub fn restore_backup(
    backup: &Path,
    db_path: &Path,
    safety_dir: &Path,
    clock: &dyn Clock,
) -> Result<RestoreReport> {
    if let (Ok(a), Ok(b)) = (fs::canonicalize(backup), fs::canonicalize(db_path))
        && a == b
    {
        return Err(Error::InvalidBackup(
            "復元元が今の DB と同じファイルです".to_owned(),
        ));
    }
    let parent = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let scratch_name = format!(
        "{}.restore-tmp",
        db_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::InvalidPath(db_path.to_path_buf()))?
    );
    let scratch = Scratch::new(parent.join(scratch_name))?;

    let mut conn = open_scratch(backup, &scratch.0)?;
    let check = inspect(&conn)?;
    migrate_without_backup(&mut conn, MIGRATIONS)?;

    let new_device_id = new_device_id();
    let moved = {
        let tx = conn.transaction()?;
        let old_dev = meta::get(&tx, meta::KEY_DEVICE_ID)?;
        let moved = match &old_dev {
            Some(dev) => tx.execute(
                "INSERT OR REPLACE INTO outbox_restored(dev, seq, hlc, op_json)
                 SELECT ?1, seq, hlc, op_json FROM outbox WHERE flushed = 0",
                params![dev],
            )?,
            None => 0,
        };
        tx.execute("DELETE FROM outbox", [])?;
        meta::set(&tx, meta::KEY_DEVICE_ID, &new_device_id)?;
        meta::save_op_seq(&tx, 0)?;
        tx.commit()?;
        moved
    };
    // 1 つのファイルにまとめて閉じる（`open` が WAL に切り替える）。
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    conn.close().map_err(|(_, e)| Error::from(e))?;

    // 今の DB を残す。
    let pre_restore_path = if db_path.exists() {
        fs::create_dir_all(safety_dir)?;
        let base = format!(
            "{}-{}",
            BackupKind::PreRestore.prefix(),
            stamp_of(clock.now_ms())
        );
        let dest = unique_path(safety_dir, &base);
        fs::copy(db_path, &dest)?;
        let wal = sidecar(db_path, "-wal");
        if wal.exists() {
            fs::copy(&wal, sidecar(&dest, "-wal"))?;
        }
        Some(dest)
    } else {
        None
    };

    // 古い WAL を新しい DB に重ねないよう、置き換える前に消す。
    for suffix in ["-wal", "-shm"] {
        let p = sidecar(db_path, suffix);
        match fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    fs::rename(&scratch.0, db_path)?;
    tracing::info!(
        backup = %backup.display(),
        new_device_id = %new_device_id,
        "バックアップから復元した"
    );

    Ok(RestoreReport {
        schema_version: check.schema_version,
        old_device_id: check.device_id,
        new_device_id,
        moved_unflushed_ops: u32::try_from(moved).unwrap_or(u32::MAX),
        pre_restore_path: pre_restore_path
            .as_deref()
            .and_then(Path::to_str)
            .map(str::to_owned),
    })
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
