//! 同期の端末ごとの設定（`local_settings` の `sync` セクション。0016）。
//!
//! - `folder`: 同期フォルダ（`bibcontainer-library`）の絶対パス。同期フォルダの場所は端末ごとに違う（Google Drive の
//!   パスは PC によって違う）ので、DB の中身（ライブラリ ID）とは別に、端末の設定として持つ（0013 §7）。
//! - `deviceName`: 端末名。保存していなければホスト名（FR-CONF-01）。1〜64 文字。

use gethostname::gethostname;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::settings::Section;

/// 端末名の最大の長さ（文字数）。画面の `DEVICE_NAME_MAX_LENGTH`（`DeviceNameGroup.tsx`）と同じ。
pub const DEVICE_NAME_MAX_CHARS: usize = 64;

/// 同期の設定。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncSettings {
    /// 同期フォルダ（`bibcontainer-library`）の絶対パス。未設定なら `None`。
    pub folder: Option<String>,
    /// 端末名。保存していなければ `None`（ホスト名を使う）。
    pub device_name: Option<String>,
}

impl Section for SyncSettings {
    const KEY: &'static str = "sync";

    fn normalize(&mut self) {
        self.folder = self
            .folder
            .take()
            .map(|f| f.trim().to_owned())
            .filter(|f| !f.is_empty());
        self.device_name = self
            .device_name
            .take()
            .map(|n| {
                n.trim()
                    .chars()
                    .take(DEVICE_NAME_MAX_CHARS)
                    .collect::<String>()
            })
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty());
    }
}

impl SyncSettings {
    /// 画面に出す端末名（保存していなければ `default`）。
    pub fn effective_device_name(&self, default: &str) -> String {
        self.device_name
            .clone()
            .unwrap_or_else(|| default.to_owned())
    }
}

/// 端末名の既定値（ホスト名）。取れなければ「この端末」。
pub fn default_device_name() -> String {
    let host = gethostname().to_string_lossy().trim().to_owned();
    if host.is_empty() {
        "この端末".to_owned()
    } else {
        host.chars().take(DEVICE_NAME_MAX_CHARS).collect()
    }
}

/// 端末名を検査して、前後の空白を除いた値を返す。空・長すぎるものは `validation`。
pub fn validate_device_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation(
            "入力が不正です（name）: 端末名を入力してください".to_owned(),
        ));
    }
    if name.chars().count() > DEVICE_NAME_MAX_CHARS {
        return Err(AppError::Validation(format!(
            "入力が不正です（name）: 端末名は {DEVICE_NAME_MAX_CHARS} 文字までです"
        )));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::error::ErrorKind;
    use crate::settings;

    #[test]
    fn nothing_stored_reads_as_unset() {
        let conn = bibcontainer_core::open_in_memory().unwrap();
        let loaded: SyncSettings = settings::load(&conn).unwrap();
        assert_eq!(loaded, SyncSettings::default());
        assert_eq!(loaded.folder, None);
        assert_eq!(loaded.effective_device_name("MacBook"), "MacBook");
    }

    #[test]
    fn the_folder_and_name_round_trip_and_are_trimmed() {
        let conn = bibcontainer_core::open_in_memory().unwrap();
        let saved = settings::save(
            &conn,
            SyncSettings {
                folder: Some("  /Users/me/Drive/bibcontainer-library  ".into()),
                device_name: Some("  書斎の Mac ".into()),
            },
        )
        .unwrap();
        assert_eq!(
            saved.folder.as_deref(),
            Some("/Users/me/Drive/bibcontainer-library")
        );
        assert_eq!(saved.device_name.as_deref(), Some("書斎の Mac"));
        let loaded: SyncSettings = settings::load(&conn).unwrap();
        assert_eq!(loaded, saved);
        assert_eq!(
            bibcontainer_core::settings::get(&conn, "sync").unwrap(),
            Some(json!({
                "folder": "/Users/me/Drive/bibcontainer-library",
                "deviceName": "書斎の Mac"
            }))
        );
    }

    #[test]
    fn empty_values_read_as_unset_and_a_wrong_type_falls_back_alone() {
        let conn = bibcontainer_core::open_in_memory().unwrap();
        bibcontainer_core::settings::set(
            &conn,
            "sync",
            &json!({ "folder": "  ", "deviceName": "" }),
        )
        .unwrap();
        let loaded: SyncSettings = settings::load(&conn).unwrap();
        assert_eq!(loaded, SyncSettings::default());

        bibcontainer_core::settings::set(
            &conn,
            "sync",
            &json!({ "folder": 5, "deviceName": "家の PC", "unknown": true }),
        )
        .unwrap();
        let loaded: SyncSettings = settings::load(&conn).unwrap();
        assert_eq!(loaded.folder, None);
        assert_eq!(loaded.device_name.as_deref(), Some("家の PC"));
    }

    #[test]
    fn device_names_are_validated_by_length_after_trimming() {
        assert_eq!(validate_device_name("  Mac  ").unwrap(), "Mac");
        assert_eq!(
            validate_device_name("あ".repeat(64).as_str())
                .unwrap()
                .chars()
                .count(),
            64
        );
        for bad in ["", "   ", "\t\n"] {
            assert_eq!(
                validate_device_name(bad).unwrap_err().kind(),
                ErrorKind::Validation,
                "{bad:?}"
            );
        }
        let err = validate_device_name(&"あ".repeat(65)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Validation);
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn the_default_device_name_is_not_empty_and_fits() {
        let name = default_device_name();
        assert!(!name.trim().is_empty());
        assert!(name.chars().count() <= DEVICE_NAME_MAX_CHARS);
    }

    #[test]
    fn the_section_key_does_not_collide_with_the_others() {
        use crate::settings::{
            BackupSettings, DisplaySettings, ListSettings, PriceSettings, ScanSettings,
        };
        let keys = [
            ScanSettings::KEY,
            DisplaySettings::KEY,
            ListSettings::KEY,
            PriceSettings::KEY,
            BackupSettings::KEY,
            SyncSettings::KEY,
            "providers",
        ];
        let unique: std::collections::BTreeSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len());
    }
}
