//! 変更ログの 1 行（Op）。

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::schema::{Entity, Fields};

/// Op の形式バージョン。
pub const OP_VERSION: u32 = 1;

/// 変更ログの 1 件。JSON のキー順は `v, dev, seq, hlc, ent, id, set`（宣言順）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    /// 形式のバージョン。
    pub v: u32,
    /// 端末 ID。
    pub dev: String,
    /// 端末内の通し番号（1 から）。
    pub seq: i64,
    /// ハイブリッド論理時計（文字列形式）。
    pub hlc: String,
    /// 対象のエンティティ。
    pub ent: Entity,
    /// 対象の ID。
    pub id: String,
    /// 変更した項目と新しい値。
    pub set: Fields,
}

impl Op {
    /// 1 行の JSON（改行を含まない）にする。outbox の `op_json` と変更ログの 1 行はこの形式。
    pub fn to_json_line(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// 1 行の JSON から読む。未知のキーは無視する。
    pub fn from_json_line(line: &str) -> Result<Op> {
        Ok(serde_json::from_str(line)?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn example() -> Op {
        Op {
            v: 1,
            dev: "d-7f3a2c".into(),
            seq: 251,
            hlc: "001760412345678:00000:d-7f3a2c".into(),
            ent: Entity::Book,
            id: "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55".into(),
            set: [
                ("title".to_owned(), json!("坊っちゃん")),
                ("rating".to_owned(), json!(4)),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn json_line_has_the_documented_key_order() {
        let line = example().to_json_line().unwrap();
        // set の中身は辞書順
        assert_eq!(
            line,
            r#"{"v":1,"dev":"d-7f3a2c","seq":251,"hlc":"001760412345678:00000:d-7f3a2c","ent":"book","id":"0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55","set":{"rating":4,"title":"坊っちゃん"}}"#
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn json_line_roundtrip() {
        let op = example();
        assert_eq!(Op::from_json_line(&op.to_json_line().unwrap()).unwrap(), op);
    }

    #[test]
    fn spec_example_line_parses() {
        let line = r#"{"v":1,"dev":"d-7f3a2c","seq":252,"hlc":"001760412345901:00000:d-7f3a2c","ent":"book","id":"0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0a55","set":{"deleted":true}}"#;
        let op = Op::from_json_line(line).unwrap();
        assert_eq!(op.seq, 252);
        assert_eq!(op.set["deleted"], json!(true));
    }

    #[test]
    fn newlines_in_values_stay_on_one_line() {
        let mut op = example();
        op.set.insert("memo".into(), json!("1行目\n2行目"));
        assert!(!op.to_json_line().unwrap().contains('\n'));
    }

    #[test]
    fn unknown_envelope_keys_are_ignored_and_unknown_entities_fail() {
        let line =
            r#"{"v":2,"dev":"d-a","seq":1,"hlc":"x","ent":"book","id":"i","set":{},"future":1}"#;
        assert!(Op::from_json_line(line).is_ok());
        let line = r#"{"v":1,"dev":"d-a","seq":1,"hlc":"x","ent":"widget","id":"i","set":{}}"#;
        assert!(Op::from_json_line(line).is_err());
    }
}
