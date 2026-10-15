//! 取得元の設定（`local_settings` のキー `providers`）。
//!
//! - 取得元ごとの有効・無効、項目ごとの優先順位（`MetaPriorities`）、User-Agent の連絡先。
//! - 読むときは、欠けた項目・知らない値・壊れた JSON を既定値で補う（古い版の設定でも、取得元が増えても動く）。
//! - API キーは入れない（OS のキーチェーン。`keychain.rs`）。
//!
//! 「有効」は利用者の選択で、実際に使うかどうか（`active`）とは別。認証が必須の取得元（Google Books）は、
//! キーがなければ有効にしていても使わない（`state::Active`）。キーを入れれば、そのまま使われ始める。
//! キーを使わない取得元（openBD・NDL・Open Library）は、有効なら使う。あとから足した取得元（Open Library）は、
//! 古い設定に書かれていなくても、有効で、優先順位の行の末尾に足される（`normalize`）。

use std::collections::BTreeMap;

use bibcontainer_core::settings as core_settings;
use bibcontainer_metadata::{MetaGroup, MetaPriorities, MetaSource, ProviderId};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::AppError;

/// `local_settings` のキー。
pub const SETTINGS_KEY: &str = "providers";
/// User-Agent の連絡先の既定値（0005 §8）。
pub const DEFAULT_CONTACT_URL: &str = "https://github.com/mioupa/bibcontainer";
/// 連絡先 URL の長さの上限。
const MAX_CONTACT_URL_LEN: usize = 200;

/// 取得元の設定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderSettings {
    /// 取得元ごとの有効・無効（`openbd`・`ndl`・`google`・`openlibrary`）。既定は 4 つとも有効。
    /// Google Books はキーがなければ、有効にしていても使わない。
    pub enabled: BTreeMap<ProviderId, bool>,
    /// 項目の行ごとの優先順位。読み出すときは、全部の行が（既定で補って）そろう。
    pub priorities: MetaPriorities,
    /// User-Agent に入れる連絡先 URL（`bibcontainer/<版> (+<URL>)`）。
    pub contact_url: String,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        ProviderSettings {
            enabled: ProviderId::ALL.into_iter().map(|p| (p, true)).collect(),
            priorities: MetaPriorities::default(),
            contact_url: DEFAULT_CONTACT_URL.to_owned(),
        }
    }
}

impl ProviderSettings {
    /// 保存された JSON から作る。欠けた項目と読めない値は既定値で補う。
    pub fn from_json(value: Option<&Value>) -> Self {
        let mut settings = ProviderSettings::default();
        let Some(obj) = value.and_then(Value::as_object) else {
            return settings;
        };
        if let Some(enabled) = obj.get("enabled").and_then(Value::as_object) {
            for id in ProviderId::ALL {
                if let Some(b) = enabled.get(id.as_str()).and_then(Value::as_bool) {
                    settings.enabled.insert(id, b);
                }
            }
        }
        if let Some(groups) = obj
            .get("priorities")
            .and_then(|p| p.get("groups"))
            .and_then(Value::as_object)
        {
            for group in MetaGroup::ALL {
                let Some(list) = groups.get(group_key(group)).and_then(Value::as_array) else {
                    continue;
                };
                let order: Vec<MetaSource> = list
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(source_from_str)
                    .collect();
                settings.priorities.set(group, order);
            }
        }
        if let Some(url) = obj.get("contactUrl").and_then(Value::as_str)
            && validate_contact_url(url.trim()).is_ok()
        {
            settings.contact_url = url.trim().to_owned();
        }
        settings.normalize();
        settings
    }

    /// 全部の行の優先順位を、設定の順（重複なし）＋設定にない既定の取得元、にそろえる。
    fn normalize(&mut self) {
        let completed: BTreeMap<MetaGroup, Vec<MetaSource>> = MetaGroup::ALL
            .into_iter()
            .map(|g| (g, self.priorities.order(g)))
            .collect();
        self.priorities = MetaPriorities { groups: completed };
        for id in ProviderId::ALL {
            self.enabled.entry(id).or_insert(true);
        }
    }

    /// 利用者から受け取った設定を検べて、保存する形にそろえる。
    pub fn validated(mut self) -> Result<Self, AppError> {
        self.contact_url = self.contact_url.trim().to_owned();
        validate_contact_url(&self.contact_url).map_err(AppError::Validation)?;
        self.normalize();
        Ok(self)
    }

    /// 取得元が、利用者の設定で有効か。
    pub fn is_enabled(&self, id: ProviderId) -> bool {
        self.enabled.get(&id).copied().unwrap_or(true)
    }
}

fn group_key(group: MetaGroup) -> &'static str {
    // `MetaGroup` の serde の名前（snake_case）。JSON の `groups` のキーと同じ
    match group {
        MetaGroup::Title => "title",
        MetaGroup::TitleKana => "title_kana",
        MetaGroup::Authors => "authors",
        MetaGroup::Publisher => "publisher",
        MetaGroup::PublishedOn => "published_on",
        MetaGroup::ListPrice => "list_price",
        MetaGroup::RefPriceIncl => "ref_price_incl",
        MetaGroup::CCode => "c_code",
        MetaGroup::Ndc => "ndc",
        MetaGroup::PagesAndSize => "pages_and_size",
        MetaGroup::Series => "series",
        MetaGroup::Language => "language",
        MetaGroup::Description => "description",
        MetaGroup::Cover => "cover",
    }
}

fn source_from_str(s: &str) -> Option<MetaSource> {
    [
        MetaSource::Barcode,
        MetaSource::Openbd,
        MetaSource::Ndl,
        MetaSource::Google,
        MetaSource::OpenLibrary,
    ]
    .into_iter()
    .find(|m| m.as_str() == s)
}

/// 連絡先 URL として使えるか。User-Agent のヘッダーに入るので、`http(s)://` で始まる ASCII の
/// 表示できる文字だけ（空白・制御文字・括弧は不可）。
pub fn validate_contact_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| "連絡先は http:// か https:// で始まる URL にしてください".to_owned())?;
    if rest.is_empty() || url.len() > MAX_CONTACT_URL_LEN {
        return Err(format!(
            "連絡先の URL が正しくありません（{MAX_CONTACT_URL_LEN} 文字以内）"
        ));
    }
    if !url
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '(' && c != ')')
    {
        return Err("連絡先の URL に使えない文字が含まれています".to_owned());
    }
    Ok(())
}

/// 設定を読む（欠けた項目は既定値で補う）。
pub fn load(conn: &Connection) -> Result<ProviderSettings, AppError> {
    let value = core_settings::get(conn, SETTINGS_KEY)?;
    Ok(ProviderSettings::from_json(value.as_ref()))
}

/// 設定を書く。保存した（検証・補完済みの）設定を返す。
pub fn save(conn: &Connection, settings: ProviderSettings) -> Result<ProviderSettings, AppError> {
    let settings = settings.validated()?;
    let value = serde_json::to_value(&settings)
        .map_err(|e| AppError::Internal(format!("設定を JSON にできません: {e}")))?;
    core_settings::set(conn, SETTINGS_KEY, &value)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn defaults_enable_the_four_providers_and_use_the_documented_contact() {
        let s = ProviderSettings::default();
        assert_eq!(s.enabled.len(), 4);
        assert!(s.enabled[&ProviderId::OpenLibrary]);
        assert!(s.enabled.values().all(|b| *b));
        assert_eq!(s.contact_url, "https://github.com/mioupa/bibcontainer");
        assert_eq!(s.priorities, MetaPriorities::default());
        assert_eq!(ProviderSettings::from_json(None), s);
    }

    #[test]
    fn missing_or_broken_parts_are_filled_with_defaults() {
        let s = ProviderSettings::from_json(Some(&json!({
            "enabled": { "openbd": false, "ndl": "yes", "ndl_thumbnail": true, "zzz": false },
            "priorities": { "groups": {
                "title": ["ndl", "nope", "ndl", "openbd"],
                "cover": "google",
                "ndc": []
            } },
            "contactUrl": "not a url"
        })));
        assert!(!s.enabled[&ProviderId::Openbd]);
        assert!(s.enabled[&ProviderId::Ndl], "bool でない値は既定");
        assert!(s.enabled[&ProviderId::Google], "欠けた取得元は既定");
        assert!(s.enabled[&ProviderId::OpenLibrary], "欠けた取得元は有効");
        assert_eq!(s.enabled.len(), 4, "知らない取得元は捨てる");
        // title: 設定の順（重複・未知は除く）に、設定にない既定の取得元（google、openlibrary）を足す
        assert_eq!(
            s.priorities.groups[&MetaGroup::Title],
            vec![
                MetaSource::Ndl,
                MetaSource::Openbd,
                MetaSource::Google,
                MetaSource::OpenLibrary
            ]
        );
        // 配列でない値は既定のまま
        assert_eq!(
            s.priorities.groups[&MetaGroup::Cover],
            vec![
                MetaSource::Openbd,
                MetaSource::Google,
                MetaSource::OpenLibrary
            ]
        );
        // 空の配列は、既定の順で補われる
        assert_eq!(s.priorities.groups[&MetaGroup::Ndc], vec![MetaSource::Ndl]);
        // 全部の行がそろう
        assert_eq!(s.priorities.groups.len(), MetaGroup::ALL.len());
        assert_eq!(s.contact_url, DEFAULT_CONTACT_URL);
    }

    #[test]
    fn json_roundtrip_keeps_the_shape_the_ui_sees() {
        let mut s = ProviderSettings::default();
        s.enabled.insert(ProviderId::Google, false);
        s.contact_url = "https://example.org/me".into();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["enabled"]["google"], json!(false));
        assert_eq!(v["enabled"]["openbd"], json!(true));
        assert_eq!(v["contactUrl"], json!("https://example.org/me"));
        assert_eq!(v["enabled"]["openlibrary"], json!(true));
        assert_eq!(
            v["priorities"]["groups"]["title"],
            json!(["openbd", "ndl", "google", "openlibrary"])
        );
        assert_eq!(v["priorities"]["groups"]["pages_and_size"][0], json!("ndl"));
        assert_eq!(ProviderSettings::from_json(Some(&v)), s);
    }

    /// Open Library を足す前に保存された設定（3 つの取得元）。読むと、Open Library が有効で、関係する行の
    /// 末尾に足される。利用者が並べ替えた順は保たれる。保存し直しても同じ。
    #[test]
    fn a_setting_saved_before_open_library_gets_it_enabled_and_appended_last() {
        let old = json!({
            "enabled": { "openbd": true, "ndl": false, "google": true },
            "priorities": { "groups": {
                "title": ["google", "ndl", "openbd"],
                "authors": ["openbd", "ndl", "google"],
                "publisher": ["openbd", "ndl", "google"],
                "published_on": ["ndl", "openbd", "google"],
                "pages_and_size": ["openbd", "ndl", "google"],
                "language": ["openbd", "ndl", "google"],
                "description": ["google", "openbd"],
                "cover": ["google", "openbd"],
                "list_price": ["barcode", "openbd", "ndl"],
                "c_code": ["barcode", "openbd"],
                "ndc": ["ndl"],
                "title_kana": ["ndl"],
                "ref_price_incl": ["openbd", "ndl"],
                "series": ["openbd", "ndl"]
            } },
            "contactUrl": "https://example.org/me"
        });
        let s = ProviderSettings::from_json(Some(&old));
        assert!(s.enabled[&ProviderId::OpenLibrary], "既定で有効");
        assert!(!s.enabled[&ProviderId::Ndl], "利用者の選択は保つ");
        let order = |g: MetaGroup| s.priorities.groups[&g].clone();
        use MetaSource::{Barcode, Google, Ndl, OpenLibrary, Openbd};
        // Open Library が入る行は、末尾に足される（並べ替えた順は保つ）
        assert_eq!(
            order(MetaGroup::Title),
            vec![Google, Ndl, Openbd, OpenLibrary]
        );
        assert_eq!(
            order(MetaGroup::Authors),
            vec![Openbd, Ndl, Google, OpenLibrary]
        );
        assert_eq!(
            order(MetaGroup::Publisher),
            vec![Openbd, Ndl, Google, OpenLibrary]
        );
        assert_eq!(
            order(MetaGroup::PublishedOn),
            vec![Ndl, Openbd, Google, OpenLibrary]
        );
        assert_eq!(
            order(MetaGroup::PagesAndSize),
            vec![Openbd, Ndl, Google, OpenLibrary]
        );
        assert_eq!(
            order(MetaGroup::Language),
            vec![Openbd, Ndl, Google, OpenLibrary]
        );
        assert_eq!(
            order(MetaGroup::Description),
            vec![Google, Openbd, OpenLibrary]
        );
        assert_eq!(order(MetaGroup::Cover), vec![Google, Openbd, OpenLibrary]);
        // Open Library が取れない項目の行には入らない
        assert_eq!(order(MetaGroup::ListPrice), vec![Barcode, Openbd, Ndl]);
        assert_eq!(order(MetaGroup::CCode), vec![Barcode, Openbd]);
        assert_eq!(order(MetaGroup::Ndc), vec![Ndl]);
        assert_eq!(order(MetaGroup::TitleKana), vec![Ndl]);
        assert_eq!(order(MetaGroup::RefPriceIncl), vec![Openbd, Ndl]);
        assert_eq!(order(MetaGroup::Series), vec![Openbd, Ndl]);
        assert_eq!(s.priorities.groups.len(), MetaGroup::ALL.len());
        assert_eq!(s.contact_url, "https://example.org/me");

        // DB に古い設定が入っていても同じ。保存し直すと、Open Library も保存される（読み直しても同じ）
        let conn = bibcontainer_core::open_in_memory().unwrap();
        core_settings::set(&conn, SETTINGS_KEY, &old).unwrap();
        let loaded = load(&conn).unwrap();
        assert_eq!(loaded, s);
        let saved = save(&conn, loaded).unwrap();
        let stored = core_settings::get(&conn, SETTINGS_KEY).unwrap().unwrap();
        assert_eq!(stored["enabled"]["openlibrary"], json!(true));
        assert_eq!(
            stored["priorities"]["groups"]["cover"],
            json!(["google", "openbd", "openlibrary"])
        );
        assert_eq!(load(&conn).unwrap(), saved);

        // 利用者が Open Library を無効にしたり、先頭に動かしたりした設定は、そのまま読める
        let mut s2 = saved.clone();
        s2.enabled.insert(ProviderId::OpenLibrary, false);
        s2.priorities
            .set(MetaGroup::Cover, vec![OpenLibrary, Google, Openbd]);
        let v = serde_json::to_value(&s2).unwrap();
        assert_eq!(v["enabled"]["openlibrary"], json!(false));
        assert_eq!(
            v["priorities"]["groups"]["cover"],
            json!(["openlibrary", "google", "openbd"])
        );
        assert_eq!(ProviderSettings::from_json(Some(&v)), s2);
    }

    #[test]
    fn every_group_key_matches_the_serde_name() {
        for g in MetaGroup::ALL {
            assert_eq!(
                serde_json::to_value(g).unwrap(),
                json!(group_key(g)),
                "{g:?}"
            );
        }
    }

    #[test]
    fn contact_url_is_checked_because_it_goes_into_a_header() {
        for ok in [
            "https://github.com/mioupa/bibcontainer",
            "http://a.example/x?y=1",
        ] {
            assert!(validate_contact_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "github.com/x",
            "ftp://x.example",
            "https://",
            "https://a b.example",
            "https://a.example/\n",
            "https://a.example/(x)",
            "https://例.jp/",
            &format!("https://a.example/{}", "x".repeat(300)),
        ] {
            assert!(validate_contact_url(bad).is_err(), "{bad:?}");
        }
        let s = ProviderSettings {
            contact_url: "  https://a.example/me  ".into(),
            ..ProviderSettings::default()
        };
        assert_eq!(s.validated().unwrap().contact_url, "https://a.example/me");
        let s = ProviderSettings {
            contact_url: "mailto:me@example.org".into(),
            ..ProviderSettings::default()
        };
        assert!(matches!(s.validated(), Err(AppError::Validation(_))));
    }

    #[test]
    fn saved_settings_come_back_from_the_db_and_a_corrupt_value_means_defaults() {
        let conn = bibcontainer_core::open_in_memory().unwrap();
        assert_eq!(load(&conn).unwrap(), ProviderSettings::default());

        let mut s = ProviderSettings::default();
        s.enabled.insert(ProviderId::Ndl, false);
        s.priorities.set(MetaGroup::Title, vec![MetaSource::Google]);
        let saved = save(&conn, s).unwrap();
        assert_eq!(
            saved.priorities.groups[&MetaGroup::Title],
            vec![
                MetaSource::Google,
                MetaSource::Openbd,
                MetaSource::Ndl,
                MetaSource::OpenLibrary
            ]
        );
        assert_eq!(load(&conn).unwrap(), saved);

        // 型が違う値が入っていても読める
        core_settings::set(&conn, SETTINGS_KEY, &json!("壊れている")).unwrap();
        assert_eq!(load(&conn).unwrap(), ProviderSettings::default());
        // 壊れた JSON 文字列は core が None として返す
        conn.execute(
            "UPDATE local_settings SET value_json = '{' WHERE key = ?1",
            [SETTINGS_KEY],
        )
        .unwrap();
        assert_eq!(load(&conn).unwrap(), ProviderSettings::default());
    }
}
