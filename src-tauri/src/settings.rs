//! 端末ごとの設定（`local_settings`）。型付きのセクションごとに読み書きする。同期しない。
//!
//! - 保存先はコアの `settings`（キー → JSON）。セクションのキーは `scan`・`display`・`list`・`price`・`backup`。
//!   書誌の取得元（`providers`）は書誌取得側のモジュールが持つ。
//! - どのセクションも、保存された JSON に欠けた項目があれば既定値で補って読む（アプリを更新して項目が
//!   増えても、古い設定が読める）。型が合わない項目は、その項目だけ既定値に戻す。範囲のある値は
//!   範囲に収める（手で書き換えた値や、古い版の値でも画面を壊さない）。
//! - 秘密情報（API キーなど）は入れない（OS のキーチェーンに置く。0005 §8）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::AppError;

/// 設定のセクション。`KEY` が `local_settings` のキー。
pub trait Section: Serialize + DeserializeOwned + Default {
    const KEY: &'static str;

    /// 値を範囲に収め、不正な要素を取り除く。読み込みと保存の両方で通す。
    fn normalize(&mut self) {}
}

/// 保存されている値からセクションを作る。無い・壊れている・項目が欠けているときは既定値で補う。
pub fn from_stored<S: Section>(stored: Option<Value>) -> S {
    let default = S::default();
    let mut section = match (stored, serde_json::to_value(&default)) {
        (Some(Value::Object(stored)), Ok(Value::Object(mut merged))) => {
            for (key, value) in stored {
                // この項目だけを差し替えて読めるなら採用する。型が合わない項目は既定値のままにする。
                let mut trial = merged.clone();
                trial.insert(key.clone(), value.clone());
                if serde_json::from_value::<S>(Value::Object(trial)).is_ok() {
                    merged.insert(key, value);
                }
            }
            serde_json::from_value(Value::Object(merged)).unwrap_or(default)
        }
        _ => default,
    };
    section.normalize();
    section
}

/// セクションを読む。
pub fn load<S: Section>(conn: &Connection) -> Result<S, AppError> {
    Ok(from_stored(bibcontainer_core::settings::get(conn, S::KEY)?))
}

/// セクションを範囲に収めて保存し、保存した値を返す。
pub fn save<S: Section>(conn: &Connection, mut value: S) -> Result<S, AppError> {
    value.normalize();
    let json = serde_json::to_value(&value)
        .map_err(|err| AppError::Internal(format!("設定を JSON にできません: {err}")))?;
    bibcontainer_core::settings::set(conn, S::KEY, &json)?;
    Ok(value)
}

/// 整数を範囲に収める。
fn clamp_u32(value: u32, min: u32, max: u32) -> u32 {
    value.clamp(min, max)
}

// ---- scan ----

/// バーコードリーダーが数字列の後ろに付けるキー（FR-SCAN-12）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScanTerminator {
    Enter,
    Tab,
    /// 付けない。一定時間の無入力で確定する。
    None,
}

/// 通知音 1 種類の設定。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SoundSetting {
    pub enabled: bool,
    /// 0〜1。
    pub volume: f64,
}

impl Default for SoundSetting {
    fn default() -> Self {
        SoundSetting {
            enabled: true,
            volume: 0.6,
        }
    }
}

impl SoundSetting {
    fn normalize(&mut self) {
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            SoundSetting::default().volume
        };
    }
}

/// 通知音の種類ごとの設定（FR-SCAN-11）。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SoundSettings {
    pub success: SoundSetting,
    pub error: SoundSetting,
    /// 重複・注意。
    pub attention: SoundSetting,
}

/// スキャン（FR-CONF-03）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ScanSettings {
    /// 数字列の後ろに付くキー。
    pub terminator: ScanTerminator,
    /// ペアリングのタイムアウト（秒、3〜30）。
    pub timeout_sec: u32,
    /// メイン画面でスキャンとみなすキー間隔の閾値（ミリ秒、5〜200）。
    pub key_interval_ms: u32,
    /// 末尾キーなしのとき、最後のキーから確定までの無入力時間（ミリ秒、30〜1000）。
    pub idle_commit_ms: u32,
    /// 集めた数字列の先頭から捨てる文字数（0〜4）。
    pub strip_leading: u32,
    /// 集めた数字列の末尾から捨てる文字数（0〜4）。
    pub strip_trailing: u32,
    /// 通知音の種類ごとの有効・音量。
    pub sounds: SoundSettings,
    /// すべての通知音を止める。
    pub muted: bool,
}

impl Default for ScanSettings {
    fn default() -> Self {
        ScanSettings {
            terminator: ScanTerminator::Enter,
            timeout_sec: 10,
            key_interval_ms: 30,
            idle_commit_ms: 100,
            strip_leading: 0,
            strip_trailing: 0,
            sounds: SoundSettings::default(),
            muted: false,
        }
    }
}

impl Section for ScanSettings {
    const KEY: &'static str = "scan";

    fn normalize(&mut self) {
        self.timeout_sec = clamp_u32(self.timeout_sec, 3, 30);
        self.key_interval_ms = clamp_u32(self.key_interval_ms, 5, 200);
        self.idle_commit_ms = clamp_u32(self.idle_commit_ms, 30, 1000);
        self.strip_leading = clamp_u32(self.strip_leading, 0, 4);
        self.strip_trailing = clamp_u32(self.strip_trailing, 0, 4);
        self.sounds.success.normalize();
        self.sounds.error.normalize();
        self.sounds.attention.normalize();
    }
}

// ---- display ----

/// テーマ（FR-CONF-04）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Theme {
    /// OS に連動する。
    System,
    Light,
    Dark,
}

/// 表示（FR-CONF-04）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct DisplaySettings {
    pub theme: Theme,
    /// フォントの名前。null は OS の日本語フォント（既定）。
    pub font_family: Option<String>,
    /// 文字サイズ（px、12〜24。画面全体の基準の大きさ）。
    pub font_size: u32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        DisplaySettings {
            theme: Theme::System,
            font_family: None,
            font_size: 16,
        }
    }
}

impl Section for DisplaySettings {
    const KEY: &'static str = "display";

    fn normalize(&mut self) {
        self.font_size = clamp_u32(self.font_size, 12, 24);
        // 前後の空白を除く。空なら OS の既定。CSS に差し込むので、制御文字や引用符・波括弧・セミコロンの
        // ような、宣言を壊す文字を含む名前は捨てる（フォント名には要らない）。
        self.font_family = self
            .font_family
            .take()
            .map(|name| name.trim().to_owned())
            .filter(|name| {
                !name.is_empty()
                    && name.chars().count() <= 100
                    && !name
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '"' | '\'' | '{' | '}' | ';' | '\\'))
            });
    }
}

// ---- list ----

/// 並べ替えの指定 1 つ（列の ID と向き）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SortSpec {
    pub id: String,
    pub desc: bool,
}

/// 一覧の表示の形（FR-LIST-07）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ListViewMode {
    /// 表（既定）。
    Table,
    /// 書影のグリッド。
    Grid,
}

/// 一覧（FR-LIST-01）。列の ID（`title`、`authors` など）の意味はフロントが決める。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ListSettings {
    /// 列の表示・非表示。載っていない列は、フロントの既定に従う。
    pub column_visibility: BTreeMap<String, bool>,
    /// 列の並び順。空なら既定の順。
    pub column_order: Vec<String>,
    /// 列の幅（px）。載っていない列は既定の幅。
    pub column_sizing: BTreeMap<String, u32>,
    /// 並べ替え（優先する順）。
    pub sorting: Vec<SortSpec>,
    pub sidebar_open: bool,
    pub detail_open: bool,
    /// 表示の形（表・グリッド）。
    pub view_mode: ListViewMode,
    /// 同じシリーズの巻を、続けて 1 つのまとまりとして出す（FR-LIST-08。グリッドのとき）。
    pub group_volumes: bool,
}

impl Default for ListSettings {
    fn default() -> Self {
        ListSettings {
            column_visibility: BTreeMap::new(),
            column_order: Vec::new(),
            column_sizing: BTreeMap::new(),
            sorting: vec![SortSpec {
                id: "title".to_owned(),
                desc: false,
            }],
            sidebar_open: true,
            detail_open: true,
            view_mode: ListViewMode::Table,
            group_volumes: false,
        }
    }
}

/// 列の ID として受け付ける長さ。
const MAX_COLUMN_ID_CHARS: usize = 64;
/// 並べ替えに使える列の数。
const MAX_SORT_SPECS: usize = 8;

fn valid_column_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= MAX_COLUMN_ID_CHARS
}

impl Section for ListSettings {
    const KEY: &'static str = "list";

    fn normalize(&mut self) {
        self.column_visibility.retain(|id, _| valid_column_id(id));
        self.column_sizing.retain(|id, _| valid_column_id(id));
        for width in self.column_sizing.values_mut() {
            *width = clamp_u32(*width, 16, 2000);
        }
        let mut seen = BTreeSet::new();
        self.column_order
            .retain(|id| valid_column_id(id) && seen.insert(id.clone()));
        let mut seen = BTreeSet::new();
        self.sorting
            .retain(|spec| valid_column_id(&spec.id) && seen.insert(spec.id.clone()));
        self.sorting.truncate(MAX_SORT_SPECS);
    }
}

// ---- price ----

/// 価格表示（FR-CONF-05）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct PriceSettings {
    /// 本体価格（税抜）に、税込価格を併記する。
    pub show_tax_included: bool,
    /// 税率（%、0〜100）。
    pub tax_rate_percent: u32,
}

impl Default for PriceSettings {
    fn default() -> Self {
        PriceSettings {
            show_tax_included: false,
            tax_rate_percent: 10,
        }
    }
}

impl Section for PriceSettings {
    const KEY: &'static str = "price";

    fn normalize(&mut self) {
        self.tax_rate_percent = clamp_u32(self.tax_rate_percent, 0, 100);
    }
}

// ---- backup ----

/// バックアップ（FR-CONF-06）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct BackupSettings {
    /// 保存先のフォルダ（絶対パス）。保存するときに空にすると、既定の場所
    /// （`app_local_data_dir/backups`）に戻る。読んだときは、既定のときも実際のパスが入る。
    pub directory: String,
    /// 日次バックアップを残す世代数（1〜365）。
    pub daily_keep: u32,
    /// 週次バックアップを残す世代数（1〜52）。
    pub weekly_keep: u32,
}

impl Default for BackupSettings {
    fn default() -> Self {
        BackupSettings {
            directory: String::new(),
            daily_keep: bibcontainer_core::backup::DAILY_KEEP as u32,
            weekly_keep: bibcontainer_core::backup::WEEKLY_KEEP as u32,
        }
    }
}

impl Section for BackupSettings {
    const KEY: &'static str = "backup";

    fn normalize(&mut self) {
        self.directory = self.directory.trim().to_owned();
        self.daily_keep = clamp_u32(self.daily_keep, 1, 365);
        self.weekly_keep = clamp_u32(self.weekly_keep, 1, 52);
    }
}

impl BackupSettings {
    /// 保存先のフォルダ。未設定（空）か、絶対パスでない値のときは `default_dir`。
    pub fn resolve_dir(&self, default_dir: &Path) -> PathBuf {
        let dir = Path::new(&self.directory);
        if self.directory.is_empty() || !dir.is_absolute() {
            default_dir.to_owned()
        } else {
            dir.to_owned()
        }
    }

    /// 残す世代数。
    pub fn keep(&self) -> bibcontainer_core::backup::Keep {
        bibcontainer_core::backup::Keep {
            daily: self.daily_keep as usize,
            weekly: self.weekly_keep as usize,
        }
    }

    /// 画面に返す形。保存先が未設定なら、実際の既定のパスを入れる。
    pub fn for_display(mut self, default_dir: &Path) -> Self {
        self.directory = self.resolve_dir(default_dir).to_string_lossy().into_owned();
        self
    }

    /// 保存する形。保存先が既定の場所と同じなら、空（既定）にする。
    /// 絶対パスでない保存先は受け付けない。
    pub fn for_storage(mut self, default_dir: &Path) -> Result<Self, AppError> {
        self.normalize();
        if !self.directory.is_empty() {
            let dir = Path::new(&self.directory);
            if !dir.is_absolute() {
                return Err(AppError::Validation(
                    "入力が不正です（directory）: 保存先は絶対パスで指定してください".to_owned(),
                ));
            }
            if dir == default_dir {
                self.directory.clear();
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn db() -> Connection {
        bibcontainer_core::open_in_memory().unwrap()
    }

    #[test]
    fn every_section_has_the_documented_defaults() {
        let scan = ScanSettings::default();
        assert_eq!(scan.terminator, ScanTerminator::Enter);
        assert_eq!(
            (
                scan.timeout_sec,
                scan.key_interval_ms,
                scan.idle_commit_ms,
                scan.strip_leading,
                scan.strip_trailing
            ),
            (10, 30, 100, 0, 0)
        );
        assert!(!scan.muted);
        assert!(scan.sounds.success.enabled && scan.sounds.error.enabled);

        let display = DisplaySettings::default();
        assert_eq!(
            (display.theme, display.font_family, display.font_size),
            (Theme::System, None, 16)
        );

        let list = ListSettings::default();
        assert!(list.sidebar_open && list.detail_open);
        assert_eq!(list.sorting.len(), 1);
        assert_eq!(list.sorting[0].id, "title");
        assert_eq!(list.view_mode, ListViewMode::Table);
        assert!(!list.group_volumes);

        let price = PriceSettings::default();
        assert_eq!(
            (price.show_tax_included, price.tax_rate_percent),
            (false, 10)
        );

        let backup = BackupSettings::default();
        assert_eq!((backup.daily_keep, backup.weekly_keep), (7, 4));
        assert_eq!(backup.directory, "");
    }

    #[test]
    fn nothing_stored_reads_as_the_defaults() {
        let conn = db();
        assert_eq!(
            load::<ScanSettings>(&conn).unwrap(),
            ScanSettings::default()
        );
        assert_eq!(
            load::<ListSettings>(&conn).unwrap(),
            ListSettings::default()
        );
        assert_eq!(
            load::<PriceSettings>(&conn).unwrap(),
            PriceSettings::default()
        );
    }

    #[test]
    fn missing_fields_are_filled_with_defaults_at_every_level() {
        // 古い版が保存した、項目の少ない設定
        let scan: ScanSettings = from_stored(Some(json!({
            "terminator": "tab",
            "timeoutSec": 20,
            "sounds": { "error": { "volume": 0.2 } },
        })));
        assert_eq!(scan.terminator, ScanTerminator::Tab);
        assert_eq!(scan.timeout_sec, 20);
        assert_eq!(scan.key_interval_ms, 30, "欠けた項目は既定値");
        assert_eq!(scan.sounds.error.volume, 0.2);
        assert!(scan.sounds.error.enabled, "入れ子の欠けた項目も既定値");
        assert_eq!(scan.sounds.success, SoundSetting::default());

        let list: ListSettings = from_stored(Some(json!({ "sidebarOpen": false })));
        assert!(!list.sidebar_open);
        assert!(list.detail_open);
        assert_eq!(list.sorting, ListSettings::default().sorting);
        assert_eq!(
            list.view_mode,
            ListViewMode::Table,
            "古い保存には表示の形がない"
        );
        assert!(!list.group_volumes);

        let price: PriceSettings = from_stored(Some(json!({ "showTaxIncluded": true })));
        assert_eq!(
            (price.show_tax_included, price.tax_rate_percent),
            (true, 10)
        );

        let backup: BackupSettings = from_stored(Some(json!({ "dailyKeep": 3 })));
        assert_eq!((backup.daily_keep, backup.weekly_keep), (3, 4));
    }

    #[test]
    fn a_field_of_the_wrong_type_falls_back_alone() {
        let scan: ScanSettings = from_stored(Some(json!({
            "terminator": "gamepad",
            "timeoutSec": "ten",
            "keyIntervalMs": 50,
            "muted": true,
            "futureField": [1, 2, 3],
        })));
        assert_eq!(scan.terminator, ScanTerminator::Enter);
        assert_eq!(scan.timeout_sec, 10);
        assert_eq!(scan.key_interval_ms, 50, "正しい項目は生かす");
        assert!(scan.muted);

        // 一覧の表示の形: 知らない値・型が違う値は、その項目だけ既定値。正しい値は生かす
        let list: ListSettings = from_stored(Some(json!({
            "viewMode": "carousel",
            "groupVolumes": "yes",
            "sidebarOpen": false,
        })));
        assert_eq!(list.view_mode, ListViewMode::Table);
        assert!(!list.group_volumes);
        assert!(!list.sidebar_open, "正しい項目は生かす");
        let list: ListSettings = from_stored(Some(json!({ "viewMode": 3, "groupVolumes": 1 })));
        assert_eq!(list.view_mode, ListViewMode::Table);
        assert!(!list.group_volumes);
        let list: ListSettings =
            from_stored(Some(json!({ "viewMode": "grid", "groupVolumes": true })));
        assert_eq!(list.view_mode, ListViewMode::Grid);
        assert!(list.group_volumes);

        // 全体の形が違っても落ちない
        for stored in [json!(null), json!("x"), json!([1]), json!(42)] {
            assert_eq!(
                from_stored::<ScanSettings>(Some(stored)),
                ScanSettings::default()
            );
        }
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let scan: ScanSettings = from_stored(Some(json!({
            "timeoutSec": 999,
            "keyIntervalMs": 0,
            "idleCommitMs": 5000,
            "stripLeading": 9,
            "stripTrailing": 5,
            "sounds": { "success": { "volume": 7.5 }, "error": { "volume": -1 } },
        })));
        assert_eq!(scan.timeout_sec, 30);
        assert_eq!(scan.key_interval_ms, 5);
        assert_eq!(scan.idle_commit_ms, 1000);
        assert_eq!((scan.strip_leading, scan.strip_trailing), (4, 4));
        assert_eq!(scan.sounds.success.volume, 1.0);
        assert_eq!(scan.sounds.error.volume, 0.0);
        let scan: ScanSettings = from_stored(Some(json!({ "timeoutSec": 1 })));
        assert_eq!(scan.timeout_sec, 3);

        let display: DisplaySettings = from_stored(Some(json!({ "fontSize": 200 })));
        assert_eq!(display.font_size, 24);
        let display: DisplaySettings = from_stored(Some(json!({ "fontSize": 3 })));
        assert_eq!(display.font_size, 12);

        let price: PriceSettings = from_stored(Some(json!({ "taxRatePercent": 1000 })));
        assert_eq!(price.tax_rate_percent, 100);

        let backup: BackupSettings =
            from_stored(Some(json!({ "dailyKeep": 0, "weeklyKeep": 9999 })));
        assert_eq!((backup.daily_keep, backup.weekly_keep), (1, 52));
    }

    #[test]
    fn font_family_is_trimmed_and_unsafe_names_are_dropped() {
        let family = |v: Value| from_stored::<DisplaySettings>(Some(json!({ "fontFamily": v })));
        assert_eq!(
            family(json!("  Hiragino Sans ")).font_family.as_deref(),
            Some("Hiragino Sans")
        );
        assert_eq!(family(json!("   ")).font_family, None);
        assert_eq!(family(json!("a; color: red")).font_family, None);
        assert_eq!(family(json!("a\"b")).font_family, None);
        assert_eq!(family(json!("x".repeat(101))).font_family, None);
        assert_eq!(family(json!(null)).font_family, None);
    }

    #[test]
    fn list_settings_drop_bad_ids_and_clamp_widths() {
        let list: ListSettings = from_stored(Some(json!({
            "columnVisibility": { "title": true, "": false, "isbn13": false },
            "columnOrder": ["title", "authors", "title", ""],
            "columnSizing": { "title": 1, "authors": 99999, "publisher": 150 },
            "sorting": [
                { "id": "authors", "desc": true },
                { "id": "authors", "desc": false },
                { "id": "", "desc": false },
            ],
        })));
        assert_eq!(list.column_visibility.len(), 2);
        assert_eq!(list.column_order, ["title", "authors"]);
        assert_eq!(list.column_sizing["title"], 16);
        assert_eq!(list.column_sizing["authors"], 2000);
        assert_eq!(list.column_sizing["publisher"], 150);
        assert_eq!(
            list.sorting,
            [SortSpec {
                id: "authors".into(),
                desc: true
            }]
        );
        let many: Vec<Value> = (0..20)
            .map(|i| json!({ "id": format!("c{i}"), "desc": false }))
            .collect();
        let list: ListSettings = from_stored(Some(json!({ "sorting": many })));
        assert_eq!(list.sorting.len(), 8);
    }

    #[test]
    fn saving_normalizes_and_a_reload_returns_the_same_value() {
        let conn = db();
        let saved = save(
            &conn,
            ScanSettings {
                timeout_sec: 500,
                muted: true,
                ..ScanSettings::default()
            },
        )
        .unwrap();
        assert_eq!(saved.timeout_sec, 30, "保存した値を返す");
        assert_eq!(load::<ScanSettings>(&conn).unwrap(), saved);
        // 保存された JSON は camelCase
        let stored = bibcontainer_core::settings::get(&conn, "scan")
            .unwrap()
            .unwrap();
        assert_eq!(stored["timeoutSec"], 30);
        assert_eq!(stored["muted"], true);
        assert_eq!(stored["terminator"], "enter");
        assert_eq!(stored["sounds"]["success"]["enabled"], true);
        // 別のセクションには触らない
        assert_eq!(
            load::<ListSettings>(&conn).unwrap(),
            ListSettings::default()
        );
    }

    #[test]
    fn a_corrupt_stored_value_reads_as_the_defaults() {
        let conn = db();
        conn.execute(
            "INSERT INTO local_settings(key, value_json) VALUES ('scan', '{壊れた')",
            [],
        )
        .unwrap();
        assert_eq!(
            load::<ScanSettings>(&conn).unwrap(),
            ScanSettings::default()
        );
    }

    #[test]
    fn backup_directory_defaults_and_is_validated() {
        // 絶対パスの形は OS で違う（Windows はドライブ名が要る）
        fn abs(unix: &str) -> String {
            if cfg!(windows) {
                format!("C:{}", unix.replace('/', "\\"))
            } else {
                unix.to_owned()
            }
        }
        let default_str = abs("/data/app/backups");
        let default_dir = Path::new(&default_str);
        let unset = BackupSettings::default();
        assert_eq!(unset.resolve_dir(default_dir), default_dir);
        assert_eq!(
            unset.clone().for_display(default_dir).directory,
            default_str
        );

        let custom_str = abs("/Volumes/Backup/bib");
        let custom = BackupSettings {
            directory: format!("  {custom_str}  "),
            ..BackupSettings::default()
        };
        let stored = custom.for_storage(default_dir).unwrap();
        assert_eq!(stored.directory, custom_str);
        assert_eq!(stored.resolve_dir(default_dir), Path::new(&custom_str));

        // 既定の場所と同じなら「未設定」として保存する
        let same = BackupSettings {
            directory: default_str.clone(),
            ..BackupSettings::default()
        };
        assert_eq!(same.for_storage(default_dir).unwrap().directory, "");

        // 相対パスは受け付けない。読み込みでは既定の場所に戻す
        let relative = BackupSettings {
            directory: "backups".into(),
            ..BackupSettings::default()
        };
        assert!(matches!(
            relative.clone().for_storage(default_dir),
            Err(AppError::Validation(_))
        ));
        assert_eq!(relative.resolve_dir(default_dir), default_dir);
    }

    #[test]
    fn section_keys_are_distinct_and_do_not_collide_with_providers() {
        let keys = [
            ScanSettings::KEY,
            DisplaySettings::KEY,
            ListSettings::KEY,
            PriceSettings::KEY,
            BackupSettings::KEY,
        ];
        let unique: BTreeSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len());
        assert!(!keys.contains(&"providers"));
    }
}
