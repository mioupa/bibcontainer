//! 取得元ごとの候補の統合（FR-META-04／05）。
//!
//! 項目ごとに、優先順位（`MetaPriorities`）の順に値のある取得元を探し、最初のものを採用する。
//! 優先順位は、ISBN の区分（和書・洋書など。`PriorityProfile`）ごとに 1 組ずつ持つ（`MetaPrioritySets`）。
//! 採用しなかった取得元の値は「他の取得元の値」として残し、編集フォームで選び直せるようにする。
//!
//! 規則:
//! - 優先順位は設定（引数）で項目の行ごとに変えられる。設定に書かれていない取得元は、既定の順で末尾に足す
//!   （あとから取得元が増えても、古い設定のまま動くように）。
//! - 書名と副書名は同じ取得元から取る（別の取得元の副書名を混ぜると、書名と合わなくなるため）。
//!   採用した書名の取得元に副書名がなければ、副書名は空になり、ほかの取得元の副書名は候補に回る。
//! - 2段目のバーコードは `MetaSource::Barcode` として、Cコードと本体価格の最優先の取得元になる。
//! - 採用した書名が、採用した巻次で終わっているとき（`One piece 巻1` と巻次 `巻1`）は、書名から巻次を除く。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::isbn::to_isbn13;
use crate::types::{
    Candidate, CandidateAuthor, CandidateCover, MetaField, MetaGroup, MetaSecondTier, MetaSource,
    MetaValue,
};

/// 優先順位の組を選ぶ区分。ISBN の登録グループ記号で決める（`for_isbn`）。
///
/// - `Japanese`（和書）: ISBN-13 が `9784` で始まるもの（ISBN-10 は `4` で始まるもの）。ISBN がない本・読めない ISBN も、こちら。
/// - `Foreign`（洋書など）: それ以外のすべての ISBN（978-0・978-1・978-3・979-…）。
///
/// 洋書は、NDL の書名が図書館の書誌の書き方（`Clean code`、副題が小文字）で、内容紹介も書影もないため、
/// Google・Open Library を先にした別の既定の順を持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PriorityProfile {
    #[default]
    Japanese,
    Foreign,
}

impl PriorityProfile {
    pub const ALL: [PriorityProfile; 2] = [PriorityProfile::Japanese, PriorityProfile::Foreign];

    /// ISBN から区分を決める。ハイフン・全角・ISBN-10 は、ISBN-13 にそろえて見る。
    /// `None`・ISBN として読めない文字列は `Japanese`。
    pub fn for_isbn(isbn: Option<&str>) -> PriorityProfile {
        match isbn.and_then(to_isbn13) {
            Some(isbn13) if !isbn13.starts_with("9784") => PriorityProfile::Foreign,
            _ => PriorityProfile::Japanese,
        }
    }
}

/// 項目の行（`MetaGroup`）ごとの優先順位（1 つの区分の分）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaPriorities {
    pub groups: BTreeMap<MetaGroup, Vec<MetaSource>>,
}

/// 既定の優先順位（FR-META-04 の表。楽天・Amazon は未実装なので除く）。Open Library は、取れる項目の行の最後に置く。
/// 洋書の区分は、書名・副書名・著者・出版社・言語・発行日・内容紹介・書影で、Google > Open Library を先にし、
/// シリーズ・巻次は NDL を先にする（ほかの行は和書と同じ）。
fn default_order(profile: PriorityProfile, group: MetaGroup) -> Vec<MetaSource> {
    use MetaSource::{Barcode, Google, Ndl, OpenLibrary, Openbd};
    match (profile, group) {
        (PriorityProfile::Foreign, MetaGroup::Series) => vec![Ndl, Openbd],
        (
            PriorityProfile::Foreign,
            MetaGroup::Title
            | MetaGroup::Authors
            | MetaGroup::Publisher
            | MetaGroup::Language
            | MetaGroup::PublishedOn,
        ) => vec![Google, OpenLibrary, Ndl, Openbd],
        (PriorityProfile::Foreign, MetaGroup::Description | MetaGroup::Cover) => {
            vec![Google, OpenLibrary, Openbd]
        }
        (_, MetaGroup::Title | MetaGroup::Authors | MetaGroup::Publisher | MetaGroup::Language) => {
            vec![Openbd, Ndl, Google, OpenLibrary]
        }
        (_, MetaGroup::TitleKana) => vec![Ndl],
        (_, MetaGroup::PublishedOn) => vec![Openbd, Ndl, Google, OpenLibrary],
        (_, MetaGroup::ListPrice) => vec![Barcode, Openbd, Ndl],
        (_, MetaGroup::RefPriceIncl) => vec![Openbd, Ndl],
        (_, MetaGroup::CCode) => vec![Barcode, Openbd],
        (_, MetaGroup::Ndc) => vec![Ndl],
        (_, MetaGroup::PagesAndSize) => vec![Ndl, Openbd, Google, OpenLibrary],
        (_, MetaGroup::Series) => vec![Openbd, Ndl],
        (_, MetaGroup::Description) => vec![Openbd, Google, OpenLibrary],
        (_, MetaGroup::Cover) => vec![Openbd, Google, OpenLibrary],
    }
}

impl Default for MetaPriorities {
    /// 和書の区分の既定。
    fn default() -> Self {
        MetaPriorities::default_for(PriorityProfile::Japanese)
    }
}

impl MetaPriorities {
    /// 区分ごとの既定の順。
    pub fn default_for(profile: PriorityProfile) -> Self {
        MetaPriorities {
            groups: MetaGroup::ALL
                .into_iter()
                .map(|g| (g, default_order(profile, g)))
                .collect(),
        }
    }

    /// 実際に使う順。設定の順（重複は除く）に、設定にない既定の取得元を末尾に足したもの。
    /// 足す取得元は和書の区分の既定から選ぶ。洋書の区分は `order_in` を使う。
    pub fn order(&self, group: MetaGroup) -> Vec<MetaSource> {
        self.order_in(PriorityProfile::Japanese, group)
    }

    /// `order` と同じで、足す取得元を `profile` の既定から選ぶ。
    pub fn order_in(&self, profile: PriorityProfile, group: MetaGroup) -> Vec<MetaSource> {
        let mut order: Vec<MetaSource> = Vec::new();
        for s in self.groups.get(&group).into_iter().flatten() {
            if !order.contains(s) {
                order.push(*s);
            }
        }
        for s in default_order(profile, group) {
            if !order.contains(&s) {
                order.push(s);
            }
        }
        order
    }

    /// 全部の行を `order_in(profile, ..)` にそろえたもの（欠けた行は既定、重複は除く、足りない取得元は末尾）。
    pub fn normalized(&self, profile: PriorityProfile) -> Self {
        MetaPriorities {
            groups: MetaGroup::ALL
                .into_iter()
                .map(|g| (g, self.order_in(profile, g)))
                .collect(),
        }
    }

    /// 1 つの行の順位を差し替える。
    pub fn set(&mut self, group: MetaGroup, order: Vec<MetaSource>) {
        self.groups.insert(group, order);
    }
}

/// 区分（`PriorityProfile`）ごとの優先順位。設定・取得の依頼はこの形で持ち、統合のときに ISBN で選ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaPrioritySets {
    /// 和書（ISBN 978-4。ISBN がない本も）
    pub japanese: MetaPriorities,
    /// 洋書など（それ以外の ISBN）
    pub foreign: MetaPriorities,
}

impl Default for MetaPrioritySets {
    fn default() -> Self {
        MetaPrioritySets {
            japanese: MetaPriorities::default_for(PriorityProfile::Japanese),
            foreign: MetaPriorities::default_for(PriorityProfile::Foreign),
        }
    }
}

impl MetaPrioritySets {
    pub fn for_profile(&self, profile: PriorityProfile) -> &MetaPriorities {
        match profile {
            PriorityProfile::Japanese => &self.japanese,
            PriorityProfile::Foreign => &self.foreign,
        }
    }

    pub fn for_profile_mut(&mut self, profile: PriorityProfile) -> &mut MetaPriorities {
        match profile {
            PriorityProfile::Japanese => &mut self.japanese,
            PriorityProfile::Foreign => &mut self.foreign,
        }
    }

    /// ISBN に合う区分の優先順位（`PriorityProfile::for_isbn`）。
    pub fn for_isbn(&self, isbn: Option<&str>) -> &MetaPriorities {
        self.for_profile(PriorityProfile::for_isbn(isbn))
    }

    /// 両方の区分を、それぞれの既定でそろえたもの（`MetaPriorities::normalized`）。
    pub fn normalized(&self) -> Self {
        MetaPrioritySets {
            japanese: self.japanese.normalized(PriorityProfile::Japanese),
            foreign: self.foreign.normalized(PriorityProfile::Foreign),
        }
    }
}

/// ある取得元の値。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaPick {
    pub value: MetaValue,
    pub source: MetaSource,
}

/// 1 項目の統合結果。`chosen` が採用した値、`others` が他の取得元の値（優先順位の順、採用した値と同じものは除く）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaFieldChoice {
    pub field: MetaField,
    pub chosen: Option<MetaPick>,
    pub others: Vec<MetaPick>,
}

/// 統合結果。`fields` には `MetaField::ALL` の全項目が（値がなくても）この順で入る。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetaMerged {
    pub isbn13: String,
    pub fields: Vec<MetaFieldChoice>,
}

impl MetaMerged {
    pub fn choice(&self, field: MetaField) -> Option<&MetaFieldChoice> {
        self.fields.iter().find(|c| c.field == field)
    }

    pub fn chosen(&self, field: MetaField) -> Option<&MetaPick> {
        self.choice(field).and_then(|c| c.chosen.as_ref())
    }

    /// 採用した値の出どころ（`field_sources.<項目名>` に入れる）。
    pub fn source(&self, field: MetaField) -> Option<MetaSource> {
        self.chosen(field).map(|p| p.source)
    }

    pub fn text(&self, field: MetaField) -> Option<&str> {
        match self.chosen(field).map(|p| &p.value) {
            Some(MetaValue::Text(s)) => Some(s),
            _ => None,
        }
    }

    pub fn number(&self, field: MetaField) -> Option<i64> {
        match self.chosen(field).map(|p| &p.value) {
            Some(MetaValue::Number(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn authors(&self) -> &[CandidateAuthor] {
        match self.chosen(MetaField::Authors).map(|p| &p.value) {
            Some(MetaValue::Authors(a)) => a,
            _ => &[],
        }
    }

    /// 書影の URL を、試す順に並べたもの（採用した取得元、続いて他の取得元。URL の重複は除く）。
    pub fn covers_in_order(&self) -> Vec<CandidateCover> {
        let mut out: Vec<CandidateCover> = Vec::new();
        if let Some(choice) = self.choice(MetaField::Cover) {
            for pick in choice.chosen.iter().chain(choice.others.iter()) {
                if let MetaValue::Covers(list) = &pick.value {
                    for c in list {
                        if !out.iter().any(|x| x.url == c.url) {
                            out.push(c.clone());
                        }
                    }
                }
            }
        }
        out
    }

    /// 書誌の値が 1 つもない（どの取得元にも収録がなかった）。書影の URL は数えない
    /// （URL があっても画像が使えるとは限らず、書誌の値ではないため）。
    pub fn is_empty(&self) -> bool {
        self.fields
            .iter()
            .filter(|f| f.field != MetaField::Cover)
            .all(|f| f.chosen.is_none())
    }
}

/// 候補を統合する。`candidates` の各取得元は 1 つずつ（同じ取得元が複数あれば先のものだけ使う）。
pub fn merge(
    isbn13: &str,
    candidates: &[Candidate],
    second_tier: Option<&MetaSecondTier>,
    priorities: &MetaPriorities,
) -> MetaMerged {
    // 取得元 → 候補。2段目は Cコードと価格だけを持つ疑似的な候補にする。
    let mut sources: Vec<(MetaSource, Candidate)> = Vec::new();
    if let Some(tier) = second_tier {
        let mut c = Candidate::empty(crate::types::ProviderId::Openbd, isbn13);
        c.c_code = tier.c_code.clone().filter(|s| !s.is_empty());
        c.list_price = tier.list_price;
        sources.push((MetaSource::Barcode, c));
    }
    for c in candidates {
        let source = MetaSource::from(c.provider);
        if !sources.iter().any(|(s, _)| *s == source) {
            sources.push((source, c.clone()));
        }
    }
    let find = |s: MetaSource| sources.iter().find(|(x, _)| *x == s).map(|(_, c)| c);

    let mut fields: Vec<MetaFieldChoice> = Vec::with_capacity(MetaField::ALL.len());
    let mut title_source: Option<MetaSource> = None;
    for field in MetaField::ALL {
        let order = priorities.order(field.group());
        let all: Vec<MetaPick> = order
            .iter()
            .filter_map(|s| {
                let value = find(*s)?.value(field)?;
                Some(MetaPick { value, source: *s })
            })
            .collect();

        let mut chosen: Option<MetaPick> = all.first().cloned();
        if field == MetaField::Title {
            title_source = chosen.as_ref().map(|p| p.source);
        }
        if field == MetaField::Subtitle
            && let Some(ts) = title_source
        {
            // 副書名は採用した書名と同じ取得元から（なければ空）
            chosen = all.iter().find(|p| p.source == ts).cloned();
        }

        let mut others: Vec<MetaPick> = Vec::new();
        for p in &all {
            let same_as_chosen = chosen.as_ref().is_some_and(|c| c.value == p.value);
            let is_chosen = chosen.as_ref().is_some_and(|c| c.source == p.source);
            if is_chosen || same_as_chosen || others.iter().any(|o| o.value == p.value) {
                continue;
            }
            others.push(p.clone());
        }
        fields.push(MetaFieldChoice {
            field,
            chosen,
            others,
        });
    }

    strip_volume_from_title(&mut fields);
    MetaMerged {
        isbn13: isbn13.to_string(),
        fields,
    }
}

/// 採用した書名が、採用した巻次で終わっていれば、書名から巻次を除く。
fn strip_volume_from_title(fields: &mut [MetaFieldChoice]) {
    let volume = fields
        .iter()
        .find(|f| f.field == MetaField::Volume)
        .and_then(|f| f.chosen.as_ref())
        .and_then(|p| match &p.value {
            MetaValue::Text(t) => Some(t.clone()),
            _ => None,
        });
    let Some(volume) = volume else { return };
    let Some(title) = fields.iter_mut().find(|f| f.field == MetaField::Title) else {
        return;
    };
    let Some(pick) = title.chosen.as_mut() else {
        return;
    };
    if let MetaValue::Text(t) = &mut pick.value
        && let Some(head) = t.strip_suffix(volume.as_str())
        && head.ends_with([' ', '.'])
        && !head.trim_matches([' ', '.']).is_empty()
    {
        *t = head.trim_end_matches([' ', '.']).to_string();
    }
}
