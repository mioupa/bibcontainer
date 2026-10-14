//! 生の応答のキャッシュ（`provider_cache` テーブル。FR-META-10）。
//!
//! `bibcontainer_metadata::RawCache` を、コアの DB（`Database` 経由）で実装する。端末ローカルで、同期しない。
//!
//! | `RawCache` の `CacheEntry` | `provider_cache` の列 |
//! |---|---|
//! | 取得元（`ProviderId::as_str()`） | `provider` |
//! | ISBN-13 | `isbn13` |
//! | `fetched_at`（UNIX 秒） | `fetched_at`（RFC 3339、UTC、秒まで。例 `2026-10-14T03:04:05Z`） |
//! | `found`（真: 書誌が見つかった） | `status`（`ok`、偽なら `not_found`） |
//! | `body`（生の応答） | `payload` |
//!
//! 有効期限の判断は `MetadataService` が行う（既定 30 日、該当なしは 1 日）。ここは読み書きだけをする。

use std::sync::Arc;

use async_trait::async_trait;
use bibcontainer_metadata::{CacheEntry, CacheError, ProviderId, RawCache};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{OptionalExtension, params};

use crate::database::Database;

pub const STATUS_OK: &str = "ok";
pub const STATUS_NOT_FOUND: &str = "not_found";

pub struct DbRawCache {
    db: Arc<Database>,
}

impl DbRawCache {
    pub fn new(db: Arc<Database>) -> Self {
        DbRawCache { db }
    }
}

/// UNIX 秒 → `provider_cache.fetched_at`（RFC 3339、UTC）。
fn format_time(secs: i64) -> String {
    DateTime::<Utc>::from_timestamp(secs, 0)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `provider_cache.fetched_at` → UNIX 秒。RFC 3339 のほか、数字だけの文字列（UNIX 秒）も読む。
fn parse_time(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(secs) = text.parse::<i64>() {
        return Some(secs);
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.timestamp())
}

#[async_trait]
impl RawCache for DbRawCache {
    async fn get(
        &self,
        provider: ProviderId,
        isbn13: &str,
    ) -> Result<Option<CacheEntry>, CacheError> {
        let isbn13 = isbn13.to_owned();
        let row = self
            .db
            .run(move |conn, _| {
                Ok(conn
                    .query_row(
                        "SELECT fetched_at, status, payload FROM provider_cache
                         WHERE provider = ?1 AND isbn13 = ?2",
                        params![provider.as_str(), isbn13],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, Option<String>>(2)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(bibcontainer_core::Error::from)?)
            })
            .await
            .map_err(|e| CacheError(e.to_string()))?;
        let Some((fetched_at, status, payload)) = row else {
            return Ok(None);
        };
        // 読めない行（時刻・状態・本文が壊れている）は、無いものとして取り直させる
        let found = match status.as_str() {
            STATUS_OK => true,
            STATUS_NOT_FOUND => false,
            _ => return Ok(None),
        };
        let (Some(fetched_at), Some(body)) = (parse_time(&fetched_at), payload) else {
            return Ok(None);
        };
        Ok(Some(CacheEntry {
            body,
            fetched_at,
            found,
        }))
    }

    async fn put(
        &self,
        provider: ProviderId,
        isbn13: &str,
        entry: CacheEntry,
    ) -> Result<(), CacheError> {
        let isbn13 = isbn13.to_owned();
        self.db
            .run(move |conn, _| {
                conn.execute(
                    "INSERT INTO provider_cache(provider, isbn13, fetched_at, status, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(provider, isbn13) DO UPDATE SET
                       fetched_at = excluded.fetched_at,
                       status = excluded.status,
                       payload = excluded.payload",
                    params![
                        provider.as_str(),
                        isbn13,
                        format_time(entry.fetched_at),
                        if entry.found {
                            STATUS_OK
                        } else {
                            STATUS_NOT_FOUND
                        },
                        entry.body,
                    ],
                )
                .map_err(bibcontainer_core::Error::from)?;
                Ok(())
            })
            .await
            .map_err(|e| CacheError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use tauri::async_runtime::block_on;

    use super::*;
    use crate::database::{BACKUP_DIR_NAME, DB_FILE_NAME};

    fn open_db(dir: &std::path::Path) -> Arc<Database> {
        Arc::new(Database::open(
            &dir.join(DB_FILE_NAME),
            &dir.join(BACKUP_DIR_NAME),
        ))
    }

    #[test]
    fn time_conversion_is_rfc3339_utc_and_reads_plain_seconds_too() {
        assert_eq!(format_time(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_time(1_800_000_000), "2027-01-15T08:00:00Z");
        for secs in [0, 1, 1_800_000_000, 4_102_444_800] {
            assert_eq!(parse_time(&format_time(secs)), Some(secs));
        }
        assert_eq!(parse_time("1800000000"), Some(1_800_000_000));
        assert_eq!(parse_time("2026-10-14T12:00:00+09:00"), Some(1_791_946_800));
        assert_eq!(parse_time("昨日"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn put_then_get_roundtrips_and_uses_the_provider_cache_columns() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let cache = DbRawCache::new(db.clone());
        block_on(async {
            assert_eq!(
                cache
                    .get(ProviderId::Openbd, "9784780802047")
                    .await
                    .unwrap(),
                None
            );
            let entry = CacheEntry {
                body: "{\"summary\":{\"title\":\"おにぎり\"}}".into(),
                fetched_at: 1_800_000_000,
                found: true,
            };
            cache
                .put(ProviderId::Openbd, "9784780802047", entry.clone())
                .await
                .unwrap();
            assert_eq!(
                cache
                    .get(ProviderId::Openbd, "9784780802047")
                    .await
                    .unwrap(),
                Some(entry.clone())
            );
            // 取得元と ISBN ごとに別の行
            assert_eq!(
                cache.get(ProviderId::Ndl, "9784780802047").await.unwrap(),
                None
            );
            assert_eq!(
                cache
                    .get(ProviderId::Openbd, "9784003101117")
                    .await
                    .unwrap(),
                None
            );

            // 列の中身（0007 の対応表）
            let row = db
                .run(|conn, _| {
                    Ok(conn
                        .query_row(
                            "SELECT provider, isbn13, fetched_at, status, payload FROM provider_cache",
                            [],
                            |r| {
                                Ok((
                                    r.get::<_, String>(0)?,
                                    r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?,
                                    r.get::<_, String>(3)?,
                                    r.get::<_, Option<String>>(4)?,
                                ))
                            },
                        )
                        .map_err(bibcontainer_core::Error::from)?)
                })
                .await
                .unwrap();
            assert_eq!(row.0, "openbd");
            assert_eq!(row.1, "9784780802047");
            assert_eq!(row.2, "2027-01-15T08:00:00Z");
            assert_eq!(row.3, "ok");
            assert_eq!(row.4.as_deref(), Some(entry.body.as_str()));
        });
    }

    #[test]
    fn not_found_is_stored_with_its_own_status_and_put_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let cache = DbRawCache::new(db.clone());
        block_on(async {
            cache
                .put(
                    ProviderId::Ndl,
                    "9784123456784",
                    CacheEntry {
                        body: "<diagnostics/>".into(),
                        fetched_at: 100,
                        found: false,
                    },
                )
                .await
                .unwrap();
            let got = cache
                .get(ProviderId::Ndl, "9784123456784")
                .await
                .unwrap()
                .unwrap();
            assert!(!got.found);
            assert_eq!(got.body, "<diagnostics/>");

            // 同じ取得元・ISBN への書き込みは上書き（行は 1 つのまま）
            cache
                .put(
                    ProviderId::Ndl,
                    "9784123456784",
                    CacheEntry {
                        body: "<record/>".into(),
                        fetched_at: 200,
                        found: true,
                    },
                )
                .await
                .unwrap();
            let got = cache
                .get(ProviderId::Ndl, "9784123456784")
                .await
                .unwrap()
                .unwrap();
            assert!(got.found);
            assert_eq!(got.fetched_at, 200);
            let rows: i64 = db
                .run(|conn, _| {
                    Ok(conn
                        .query_row("SELECT count(*) FROM provider_cache", [], |r| r.get(0))
                        .map_err(bibcontainer_core::Error::from)?)
                })
                .await
                .unwrap();
            assert_eq!(rows, 1);
        });
    }

    #[test]
    fn rows_that_cannot_be_read_count_as_misses() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let cache = DbRawCache::new(db.clone());
        block_on(async {
            db.run(|conn, _| {
                for (provider, isbn, at, status, payload) in [
                    ("openbd", "9784000000001", "昨日", "ok", Some("x")),
                    (
                        "openbd",
                        "9784000000002",
                        "2026-10-14T00:00:00Z",
                        "weird",
                        Some("x"),
                    ),
                    (
                        "openbd",
                        "9784000000003",
                        "2026-10-14T00:00:00Z",
                        "ok",
                        None,
                    ),
                ] {
                    conn.execute(
                        "INSERT INTO provider_cache VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![provider, isbn, at, status, payload],
                    )
                    .map_err(bibcontainer_core::Error::from)?;
                }
                Ok(())
            })
            .await
            .unwrap();
            for isbn in ["9784000000001", "9784000000002", "9784000000003"] {
                assert_eq!(
                    cache.get(ProviderId::Openbd, isbn).await.unwrap(),
                    None,
                    "{isbn}"
                );
            }
        });
    }

    #[test]
    fn a_db_that_could_not_be_opened_gives_a_cache_error_not_a_panic() {
        let db = Arc::new(Database::failed(crate::error::AppError::DbUnavailable {
            reason: "テスト".into(),
        }));
        let cache = DbRawCache::new(db);
        block_on(async {
            assert!(
                cache
                    .get(ProviderId::Openbd, "9784780802047")
                    .await
                    .is_err()
            );
        });
    }
}
