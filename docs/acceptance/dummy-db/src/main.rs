//! 受け入れ確認（AT-14）用に、ダミーの蔵書を入れた DB と書影を作る。
//!
//! ```text
//! dummy-db <アプリのデータフォルダ> [冊数（既定 5000）] [書影の種類の数（既定 500）] [乱数の種（既定 1）] [--works]
//! ```
//!
//! `--works`（引数のどこに書いてもよい）を付けると、同じ作品の巻（書名が同じで著者も同じ本）を約 3 割作る。
//! 巻をまとめた表示の確認用。付けなければ、乱数の流れも含めて、これまでと同じデータになる。
//!
//! `<アプリのデータフォルダ>` に `library.sqlite3` と `covers/` を作る（macOS の本物は
//! `~/Library/Application Support/io.github.mioupa.bibcontainer`）。**すでに `library.sqlite3` が
//! あるときは何もせず終わる**ので、本物のデータを上書きしない。コアの API（`create_books` など）で作るため、
//! 操作の記録（Op）・検索の索引・一覧の並べ替えキーは、アプリで登録したときと同じものが入る。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use bibcontainer_core::library::{self, Env, FieldChange};
use bibcontainer_core::normalize::isbn13_check_digit;
use bibcontainer_core::{SystemClock, open};
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// 再現できる擬似乱数（xorshift64*）。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }
    /// `percent` % の確率で true。
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// (表記, ヨミ)
type Pair = (&'static str, &'static str);

const TOPICS: &[Pair] = &[
    ("線形代数", "せんけいだいすう"),
    ("微分積分", "びぶんせきぶん"),
    ("統計学", "とうけいがく"),
    ("量子力学", "りょうしりきがく"),
    ("熱力学", "ねつりきがく"),
    ("半導体", "はんどうたい"),
    ("結晶成長", "けっしょうせいちょう"),
    ("炭化ケイ素", "たんかけいそ"),
    ("電磁気学", "でんじきがく"),
    ("材料科学", "ざいりょうかがく"),
    ("機械学習", "きかいがくしゅう"),
    ("データベース", "でーたべーす"),
    ("アルゴリズム", "あるごりずむ"),
    ("ネットワーク", "ねっとわーく"),
    ("オペレーティングシステム", "おぺれーてぃんぐしすてむ"),
    ("プログラミング", "ぷろぐらみんぐ"),
    ("ソフトウェア設計", "そふとうぇあせっけい"),
    ("暗号", "あんごう"),
    ("日本史", "にほんし"),
    ("世界史", "せかいし"),
    ("中世", "ちゅうせい"),
    ("近代", "きんだい"),
    ("経済学", "けいざいがく"),
    ("経営", "けいえい"),
    ("会計", "かいけい"),
    ("哲学", "てつがく"),
    ("倫理学", "りんりがく"),
    ("心理学", "しんりがく"),
    ("言語学", "げんごがく"),
    ("文章", "ぶんしょう"),
    ("俳句", "はいく"),
    ("短歌", "たんか"),
    ("料理", "りょうり"),
    ("パン", "ぱん"),
    ("珈琲", "こーひー"),
    ("旅", "たび"),
    ("山", "やま"),
    ("海", "うみ"),
    ("星空", "ほしぞら"),
    ("宇宙", "うちゅう"),
    ("生物", "せいぶつ"),
    ("医学", "いがく"),
    ("建築", "けんちく"),
    ("都市", "とし"),
    ("写真", "しゃしん"),
    ("音楽", "おんがく"),
    ("映画", "えいが"),
    ("美術", "びじゅつ"),
    ("茶", "ちゃ"),
    ("庭", "にわ"),
    ("夜", "よる"),
    ("朝", "あさ"),
    ("猫", "ねこ"),
    ("犬", "いぬ"),
    ("冬", "ふゆ"),
    ("夏", "なつ"),
    ("約束", "やくそく"),
    ("記憶", "きおく"),
    ("手紙", "てがみ"),
    ("雨", "あめ"),
    ("図書館", "としょかん"),
];

const ENGLISH: &[&str] = &[
    "Algorithms",
    "Database Systems",
    "Linear Algebra",
    "Statistical Learning",
    "Operating Systems",
    "Computer Networks",
    "Semiconductor Physics",
    "Crystal Growth",
    "Software Design",
    "Machine Learning",
];

const SURNAMES: &[Pair] = &[
    ("夏目", "なつめ"),
    ("芥川", "あくたがわ"),
    ("太宰", "だざい"),
    ("宮沢", "みやざわ"),
    ("森", "もり"),
    ("谷崎", "たにざき"),
    ("川端", "かわばた"),
    ("三島", "みしま"),
    ("村上", "むらかみ"),
    ("吉本", "よしもと"),
    ("山田", "やまだ"),
    ("佐藤", "さとう"),
    ("鈴木", "すずき"),
    ("高橋", "たかはし"),
    ("田中", "たなか"),
    ("伊藤", "いとう"),
    ("渡辺", "わたなべ"),
    ("中村", "なかむら"),
    ("小林", "こばやし"),
    ("加藤", "かとう"),
];

const GIVEN: &[Pair] = &[
    ("漱石", "そうせき"),
    ("龍之介", "りゅうのすけ"),
    ("治", "おさむ"),
    ("賢治", "けんじ"),
    ("鷗外", "おうがい"),
    ("潤一郎", "じゅんいちろう"),
    ("康成", "やすなり"),
    ("由紀夫", "ゆきお"),
    ("春樹", "はるき"),
    ("ばなな", "ばなな"),
    ("太郎", "たろう"),
    ("花子", "はなこ"),
    ("一郎", "いちろう"),
    ("次郎", "じろう"),
    ("美咲", "みさき"),
    ("健", "けん"),
    ("誠", "まこと"),
    ("愛", "あい"),
    ("翔", "しょう"),
    ("葵", "あおい"),
];

const PUBLISHERS: &[&str] = &[
    "岩波書店",
    "新潮社",
    "講談社",
    "文藝春秋",
    "集英社",
    "小学館",
    "角川書店",
    "筑摩書房",
    "中央公論新社",
    "オライリー・ジャパン",
    "技術評論社",
    "翔泳社",
    "日経BP",
    "ポット出版",
    "みすず書房",
    "河出書房新社",
    "共立出版",
    "丸善出版",
    "東京大学出版会",
    "ミネルヴァ書房",
];

const STORES: &[&str] = &[
    "紀伊國屋書店",
    "丸善",
    "ジュンク堂書店",
    "Amazon",
    "楽天ブックス",
    "ブックオフ",
    "古書店",
    "大学生協",
];

const TAGS: &[&str] = &[
    "小説",
    "研究/SiC",
    "研究/論文",
    "研究/装置",
    "技術",
    "料理",
    "旅行",
    "歴史",
    "哲学",
    "漫画",
    "雑誌",
    "洋書",
    "古書",
    "積読",
    "再読",
    "名著",
    "仕事/設計",
    "仕事/資料",
    "趣味",
];

/// (名前, 親の名前)。親は先に出てくること。
const LOCATIONS: &[(&str, Option<&str>)] = &[
    ("自宅", None),
    ("書斎", Some("自宅")),
    ("書斎の本棚 A", Some("書斎")),
    ("書斎の本棚 B", Some("書斎")),
    ("書斎の本棚 C", Some("書斎")),
    ("リビング", Some("自宅")),
    ("リビングの棚", Some("リビング")),
    ("寝室", Some("自宅")),
    ("研究室", None),
    ("研究室の本棚 1", Some("研究室")),
    ("研究室の本棚 2", Some("研究室")),
    ("研究室の本棚 3", Some("研究室")),
    ("実家", None),
    ("実家の押入れ", Some("実家")),
    ("貸出中", None),
];

fn isbn(i: usize) -> String {
    let first12 = format!("9784{i:08}");
    let check = isbn13_check_digit(&first12).unwrap();
    format!("{first12}{check}")
}

fn date(rng: &mut Rng, from_year: usize, to_year: usize) -> String {
    format!(
        "{}-{:02}-{:02}",
        from_year + rng.below(to_year - from_year + 1),
        1 + rng.below(12),
        1 + rng.below(28)
    )
}

/// 書影のダミー（グラデーションと帯と矩形）。実物と同じく長辺 800px の JPEG（品質 85）にする。
fn make_cover(rng: &mut Rng) -> Vec<u8> {
    let (w, h) = (560_u32, 800_u32);
    let c1 = [
        u8::try_from(40 + rng.below(180)).unwrap(),
        u8::try_from(40 + rng.below(180)).unwrap(),
        u8::try_from(40 + rng.below(180)).unwrap(),
    ];
    let c2 = [255 - c1[0] / 2, 255 - c1[1] / 2, 255 - c1[2] / 2];
    let mut img = RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let t = (f64::from(y) * 0.8 + f64::from(x) * 0.2) / f64::from(h);
        let mix = |a: u8, b: u8| {
            let v = f64::from(a) * (1.0 - t) + f64::from(b) * t;
            // 0..=255 に収まる
            u8::try_from(v.round() as i64).unwrap_or(255)
        };
        *px = Rgb([mix(c1[0], c2[0]), mix(c1[1], c2[1]), mix(c1[2], c2[2])]);
    }
    // 帯・矩形（タイトルのブロックのつもり）
    for _ in 0..(3 + rng.below(4)) {
        let rw = 80 + u32::try_from(rng.below(400)).unwrap();
        let rh = 20 + u32::try_from(rng.below(120)).unwrap();
        let rx = u32::try_from(rng.below(usize::try_from(w - rw).unwrap())).unwrap();
        let ry = u32::try_from(rng.below(usize::try_from(h - rh).unwrap())).unwrap();
        let color = Rgb([
            u8::try_from(rng.below(256)).unwrap(),
            u8::try_from(rng.below(256)).unwrap(),
            u8::try_from(rng.below(256)).unwrap(),
        ]);
        for y in ry..ry + rh {
            for x in rx..rx + rw {
                img.put_pixel(x, y, color);
            }
        }
    }
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 85)
        .write_image(img.as_raw(), w, h, ExtendedColorType::Rgb8)
        .unwrap();
    jpeg
}

/// 書影を `covers/<先頭 2 文字>/<SHA-256>.jpg` に書き、ハッシュを返す（アプリの保存形式と同じ）。
fn write_cover(covers_dir: &Path, jpeg: &[u8]) -> String {
    let hash: String = Sha256::digest(jpeg)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let dir = covers_dir.join(&hash[..2]);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{hash}.jpg")), jpeg).unwrap();
    hash
}

const USAGE: &str = "使い方: dummy-db <アプリのデータフォルダ> [冊数（既定 5000）] [書影の種類の数（既定 500）] [乱数の種（既定 1）] [--works]";

/// `--works` で作る「作品」。巻の書名は、この `base` から作る。
struct Work {
    /// 巻の表記を除いた書名
    base: String,
    kana: String,
    publisher: &'static str,
    /// 全巻で同じ著者（`authors` の値）
    authors: Value,
    series_id: String,
}

/// 巻の書き方。
#[derive(Clone, Copy)]
enum VolumeStyle {
    /// 書名は `base`、`volume` は `"{n}"`
    Plain,
    /// 書名は `base`、`volume` は `"第{n}巻"`
    Kan,
    /// 書名が `"{base} {n}"`（`volume` なし。openBD だけの本のつもり）
    TitleSpace,
    /// 書名が `"{base}（{n}）"`（`volume` なし）
    TitleParen,
}

/// 作品の 1 冊としての位置。
struct WorkSlot {
    work: usize,
    volume: usize,
    style: VolumeStyle,
}

struct WorkPlan {
    works: Vec<Work>,
    /// 本の通し番号ごと。作品の巻でない本は `None`
    slots: Vec<Option<WorkSlot>>,
}

impl WorkPlan {
    fn for_book(&self, i: usize) -> Option<(&Work, &WorkSlot)> {
        self.slots[i]
            .as_ref()
            .map(|slot| (&self.works[slot.work], slot))
    }
}

/// 1 作品の巻数の上限。
const MAX_VOLUMES: usize = 40;

fn shuffle<T>(rng: &mut Rng, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        items.swap(i, rng.below(i + 1));
    }
}

/// 作品ごとの巻数。合計は `count` の 3 割ほどで、長い作品（最大 40 巻）と短い作品（2 巻）が混ざる。
fn work_sizes(rng: &mut Rng, count: usize) -> Vec<usize> {
    let target = count * 30 / 100;
    let n_works = 150.max(target / 20).min(target / 2).min(1000);
    let mut sizes = vec![2; n_works];
    let mut remaining = (target - 2 * n_works).min(n_works * (MAX_VOLUMES - 2));
    while remaining > 0 {
        // 前のほうの作品に偏らせる
        let upper = rng.below(n_works) + 1;
        let w = rng.below(upper);
        if sizes[w] < MAX_VOLUMES {
            sizes[w] += 1;
            remaining -= 1;
        }
    }
    sizes
}

/// 作品の書名（巻の表記なし）とそのヨミ。
fn work_title(rng: &mut Rng) -> (String, String) {
    let a = *rng.pick(TOPICS);
    let b = *rng.pick(TOPICS);
    match rng.below(7) {
        0 => (
            format!("{}の{}物語", a.0, b.0),
            format!("{}の{}ものがたり", a.1, b.1),
        ),
        1 => (format!("{}戦記", a.0), format!("{}せんき", a.1)),
        2 => (format!("{}探偵団", a.0), format!("{}たんていだん", a.1)),
        3 => (
            format!("{}と{}の日々", a.0, b.0),
            format!("{}と{}のひび", a.1, b.1),
        ),
        4 => (format!("{}の冒険", a.0), format!("{}のぼうけん", a.1)),
        5 => (format!("{}異聞録", a.0), format!("{}いぶんろく", a.1)),
        _ => {
            let s = *rng.pick(SURNAMES);
            (format!("{}家の{}", s.0, a.0), format!("{}けの{}", s.1, a.1))
        }
    }
}

fn work_author(rng: &mut Rng, role: &str) -> (String, Value) {
    let (s, sk) = *rng.pick(SURNAMES);
    let (g, gk) = *rng.pick(GIVEN);
    let name = format!("{s} {g}");
    let value = json!({ "name": name, "kana": format!("{sk} {gk}"), "role": role });
    (name, value)
}

/// 作品と、どの本がどの作品の何巻かを決める（`--works` のときだけ呼ぶ）。
fn plan_works(
    rng: &mut Rng,
    count: usize,
    mut create_series: impl FnMut(&str) -> Result<String, String>,
) -> Result<WorkPlan, String> {
    let sizes = work_sizes(rng, count);
    let mut works = Vec::with_capacity(sizes.len());
    let mut used_titles = HashSet::new();
    for _ in &sizes {
        // 作品どうしで書名が重ならないようにする
        let (base, kana) = loop {
            let t = work_title(rng);
            if used_titles.insert(t.0.clone()) {
                break t;
            }
        };
        // 著者は 1 人が 7 割。2 人のときは原作と作画
        let authors = if rng.chance(70) {
            vec![work_author(rng, "著").1]
        } else {
            let (first, v1) = work_author(rng, "原作");
            let v2 = loop {
                let (name, v) = work_author(rng, "作画");
                if name != first {
                    break v;
                }
            };
            vec![v1, v2]
        };
        let publisher = *rng.pick(PUBLISHERS);
        let series_id = create_series(&format!("{base}シリーズ"))?;
        works.push(Work {
            base,
            kana,
            publisher,
            authors: json!(authors),
            series_id,
        });
    }

    // 作品の巻にする位置を散らし、巻の番号は位置の順に 1, 2, ... と振る
    let total: usize = sizes.iter().sum();
    let mut positions: Vec<usize> = (0..count).collect();
    shuffle(rng, &mut positions);
    positions.truncate(total);
    positions.sort_unstable();
    let mut owners: Vec<usize> = sizes
        .iter()
        .enumerate()
        .flat_map(|(w, &n)| std::iter::repeat_n(w, n))
        .collect();
    shuffle(rng, &mut owners);
    let mut next_volume = vec![1; sizes.len()];
    let mut slots: Vec<Option<WorkSlot>> = (0..count).map(|_| None).collect();
    for (&pos, &work) in positions.iter().zip(&owners) {
        let volume = next_volume[work];
        next_volume[work] += 1;
        // 巻の書き方: 書名に入れる 15%（そのうち「（n）」が 5%）、「第 n 巻」10%、残りは数字だけ
        let style = match rng.below(100) {
            0..=9 => VolumeStyle::TitleSpace,
            10..=14 => VolumeStyle::TitleParen,
            15..=24 => VolumeStyle::Kan,
            _ => VolumeStyle::Plain,
        };
        slots[pos] = Some(WorkSlot {
            work,
            volume,
            style,
        });
    }
    Ok(WorkPlan { works, slots })
}

/// 書名・ヨミ・出版社・著者・シリーズ・巻を、作品の値で置き換える（そのほかはランダムなまま）。
fn apply_work(c: &mut Vec<FieldChange>, work: &Work, slot: &WorkSlot) {
    c.retain(|f| {
        !matches!(
            f.field.as_str(),
            "title" | "title_kana" | "publisher" | "authors" | "series_id" | "volume"
        )
    });
    let n = slot.volume;
    let (title, volume) = match slot.style {
        VolumeStyle::Plain => (work.base.clone(), Some(format!("{n}"))),
        VolumeStyle::Kan => (work.base.clone(), Some(format!("第{n}巻"))),
        VolumeStyle::TitleSpace => (format!("{} {n}", work.base), None),
        VolumeStyle::TitleParen => (format!("{}（{n}）", work.base), None),
    };
    c.push(FieldChange::provider("title", json!(title), "openbd"));
    if let Some(volume) = volume {
        // ヨミと巻は NDL 由来のつもり（書名に巻を入れた本は openBD だけで、ヨミも巻もない）
        c.push(FieldChange::provider("title_kana", json!(work.kana), "ndl"));
        c.push(FieldChange::provider("volume", json!(volume), "ndl"));
    }
    c.push(FieldChange::provider(
        "publisher",
        json!(work.publisher),
        "openbd",
    ));
    c.push(FieldChange::provider(
        "authors",
        work.authors.clone(),
        "openbd",
    ));
    c.push(FieldChange::provider(
        "series_id",
        json!(work.series_id),
        "ndl",
    ));
}

struct Catalog {
    series: Vec<String>,
    locations: Vec<String>,
    covers: Vec<String>,
}

/// `work` があれば、ランダムに作ったあとで作品の値に置き換える（乱数の使い方は `work` の有無で変わらない）。
fn dummy_book(
    rng: &mut Rng,
    i: usize,
    cat: &Catalog,
    work: Option<(&Work, &WorkSlot)>,
) -> Vec<FieldChange> {
    let a = *rng.pick(TOPICS);
    let b = *rng.pick(TOPICS);
    let (title, kana): (String, Option<String>) = match rng.below(10) {
        0 => (format!("{}入門", a.0), Some(format!("{}にゅうもん", a.1))),
        1 => (
            format!("はじめての{}", a.0),
            Some(format!("はじめての{}", a.1)),
        ),
        2 | 3 => (
            format!("{}の{}", a.0, b.0),
            Some(format!("{}の{}", a.1, b.1)),
        ),
        4 => (
            format!("{}と{}", a.0, b.0),
            Some(format!("{}と{}", a.1, b.1)),
        ),
        5 => (
            format!("{}ハンドブック", a.0),
            Some(format!("{}はんどぶっく", a.1)),
        ),
        6 => (
            format!("{}の教科書 第{}版", a.0, 1 + rng.below(5)),
            Some(format!("{}のきょうかしょ", a.1)),
        ),
        7 => (format!("図解 {}", a.0), Some(format!("ずかい{}", a.1))),
        8 => (format!("{} {}", rng.pick(ENGLISH), 1 + rng.below(4)), None),
        _ => (
            format!("{}の{}", a.0, b.0),
            Some(format!("{}の{}", a.1, b.1)),
        ),
    };
    let mut c = vec![
        FieldChange::barcode("isbn13", json!(isbn(i))),
        FieldChange::provider("title", json!(title), "openbd"),
        FieldChange::provider("publisher", json!(*rng.pick(PUBLISHERS)), "openbd"),
        FieldChange::provider(
            "published_on",
            json!(format!("{}-{:02}", 1950 + rng.below(76), 1 + rng.below(12))),
            "openbd",
        ),
        FieldChange::barcode(
            "c_code",
            json!(format!(
                "{}{}{}{}",
                rng.below(2),
                rng.below(4),
                rng.below(10),
                rng.below(10)
            )),
        ),
        FieldChange::barcode("list_price", json!(500 + 10 * rng.below(300))),
        FieldChange::provider("pages", json!(80 + rng.below(700)), "ndl"),
        FieldChange::provider("language", json!("jpn"), "ndl"),
    ];
    if let Some(kana) = kana {
        c.push(FieldChange::provider("title_kana", json!(kana), "ndl"));
    }
    if rng.chance(20) {
        let sub = format!("{}から学ぶ{}", b.0, a.0);
        c.push(FieldChange::provider("subtitle", json!(sub), "ndl"));
    }
    let authors: Vec<Value> = (0..(1 + rng.below(3)))
        .map(|_| {
            let (s, sk) = *rng.pick(SURNAMES);
            let (g, gk) = *rng.pick(GIVEN);
            json!({
                "name": format!("{s} {g}"),
                "kana": format!("{sk} {gk}"),
                "role": if rng.chance(15) { "訳" } else { "著" },
            })
        })
        .collect();
    c.push(FieldChange::provider("authors", json!(authors), "openbd"));
    if rng.chance(40) {
        let text = format!(
            "{}と{}について、基本から応用までを丁寧に解説する。章末に演習問題と解答を収める。",
            a.0, b.0
        );
        c.push(FieldChange::provider("description", json!(text), "openbd"));
    }
    if rng.chance(85) {
        c.push(FieldChange::provider(
            "cover_hash",
            json!(cat.covers[rng.below(cat.covers.len())]),
            "openbd",
        ));
    }
    // 所有状態
    let kind = rng.below(100);
    if kind < 7 {
        c.push(FieldChange::manual("ownership", json!("wanted")));
        c.push(FieldChange::manual(
            "wish_priority",
            json!(*rng.pick(&["high", "mid", "low"])),
        ));
    } else {
        if kind < 9 {
            c.push(FieldChange::manual("ownership", json!("released")));
        }
        if rng.chance(92) {
            c.push(FieldChange::manual(
                "location_id",
                json!(cat.locations[rng.below(cat.locations.len())]),
            ));
        }
        c.push(FieldChange::manual(
            "purchased_on",
            json!(date(rng, 2005, 2026)),
        ));
        c.push(FieldChange::manual(
            "purchase_store",
            json!(*rng.pick(STORES)),
        ));
        c.push(FieldChange::manual(
            "purchase_price",
            json!(400 + 10 * rng.below(300)),
        ));
        c.push(FieldChange::manual(
            "purchase_cond",
            json!(if rng.chance(25) { "used" } else { "new" }),
        ));
    }
    let tags: Vec<&str> = (0..rng.below(4)).map(|_| *rng.pick(TAGS)).collect();
    c.push(FieldChange::manual("tags", json!(tags)));
    // 読書状況
    match rng.below(100) {
        0..=34 => {
            c.push(FieldChange::manual("read_status", json!("read")));
            c.push(FieldChange::manual(
                "read_started_on",
                json!(date(rng, 2005, 2025)),
            ));
            c.push(FieldChange::manual(
                "read_finished_on",
                json!(date(rng, 2005, 2026)),
            ));
            if rng.chance(70) {
                c.push(FieldChange::manual("rating", json!(1 + rng.below(5))));
            }
            if rng.chance(20) {
                c.push(FieldChange::manual(
                    "review",
                    json!("再読したい。図が多くて分かりやすい。"),
                ));
            }
        }
        35..=40 => c.push(FieldChange::manual("read_status", json!("reading"))),
        41..=43 => c.push(FieldChange::manual("read_status", json!("paused"))),
        _ => {}
    }
    if rng.chance(25) {
        c.push(FieldChange::manual(
            "memo",
            json!(format!("{}について。{}", a.0, b.0)),
        ));
    }
    // シリーズ（12%）
    if rng.chance(12) {
        c.push(FieldChange::provider(
            "series_id",
            json!(cat.series[rng.below(cat.series.len())]),
            "ndl",
        ));
        let n = 1 + rng.below(30);
        let volume = match rng.below(3) {
            0 => format!("{n}"),
            1 => format!("第{n}巻"),
            _ => format!("Vol.{n}"),
        };
        c.push(FieldChange::provider("volume", json!(volume), "ndl"));
    }
    if let Some((work, slot)) = work {
        apply_work(&mut c, work, slot);
    }
    c
}

fn run(
    data_dir: &Path,
    count: usize,
    cover_variants: usize,
    seed: u64,
    works: bool,
) -> Result<(), String> {
    let db_path = data_dir.join("library.sqlite3");
    if db_path.exists() {
        return Err(format!(
            "{} がすでにあります。上書きしないので、別のフォルダを指定してください。",
            db_path.display()
        ));
    }
    let started = Instant::now();
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let (mut conn, _) = open(&db_path, &data_dir.join("backups")).map_err(|e| e.to_string())?;
    let clock = SystemClock;
    let env = Env::new(&clock);

    // 保管場所
    let mut location_ids: Vec<(&str, String)> = Vec::new();
    let mut leaf_ids = Vec::new();
    for (name, parent) in LOCATIONS {
        let parent_id = parent.and_then(|p| {
            location_ids
                .iter()
                .find(|(n, _)| *n == p)
                .map(|(_, id)| id.clone())
        });
        let id = library::create_location(&mut conn, &env, name, parent_id.as_deref())
            .map_err(|e| e.to_string())?;
        let has_child = LOCATIONS.iter().any(|(_, p)| *p == Some(*name));
        if !has_child {
            leaf_ids.push(id.clone());
        }
        location_ids.push((*name, id));
    }

    // シリーズ
    let series = (0..100)
        .map(|i| {
            let t = *rng.pick(TOPICS);
            library::find_or_create_series(&mut conn, &env, &format!("{}シリーズ {i}", t.0))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    // 書影
    let covers_dir = data_dir.join("covers");
    let covers: Vec<String> = (0..cover_variants)
        .map(|_| {
            let jpeg = make_cover(&mut rng);
            write_cover(&covers_dir, &jpeg)
        })
        .collect();

    let cat = Catalog {
        series,
        locations: leaf_ids,
        covers,
    };

    // 作品の巻（`--works` のときだけ。乱数は、ここ以降の本の生成の前に使う）
    let work_plan = if works {
        Some(plan_works(&mut rng, count, |name| {
            library::find_or_create_series(&mut conn, &env, name).map_err(|e| e.to_string())
        })?)
    } else {
        None
    };

    // 本（500 冊ずつ 1 トランザクション）
    let mut all_ids = Vec::with_capacity(count);
    for start in (0..count).step_by(500) {
        let end = (start + 500).min(count);
        let batch: Vec<Vec<FieldChange>> = (start..end)
            .map(|i| {
                let work = work_plan.as_ref().and_then(|p| p.for_book(i));
                dummy_book(&mut rng, i, &cat, work)
            })
            .collect();
        let ids = library::create_books(&mut conn, &env, &batch).map_err(|e| e.to_string())?;
        all_ids.extend(ids);
    }

    // 1% をゴミ箱へ
    let trashed: Vec<String> = all_ids.iter().filter(|_| rng.chance(1)).cloned().collect();
    library::trash_books(&mut conn, &env, &trashed).map_err(|e| e.to_string())?;

    // タグの色（2 つ）
    library::set_tag_color(&mut conn, &env, "小説", Some("blue")).map_err(|e| e.to_string())?;
    library::set_tag_color(&mut conn, &env, "研究/SiC", Some("green"))
        .map_err(|e| e.to_string())?;

    println!(
        "作成しました: {count} 冊（ゴミ箱 {} 冊）、保管場所 {}、書影 {} 枚。{:.1?}\n  DB: {}",
        trashed.len(),
        LOCATIONS.len(),
        cover_variants,
        started.elapsed(),
        db_path.display()
    );
    if let Some(plan) = &work_plan {
        let books = plan.slots.iter().flatten().count();
        println!("  作品: {} 件（巻 {books} 冊）", plan.works.len());
    }
    Ok(())
}

fn main() -> ExitCode {
    // `--works` は引数のどこにあってもよい（位置の引数は、それを除いた並びで数える）
    let mut works = false;
    let mut args: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--works" {
            works = true;
        } else if arg.starts_with("--") {
            eprintln!("知らないオプションです: {arg}\n{USAGE}");
            return ExitCode::from(2);
        } else {
            args.push(arg);
        }
    }
    let Some(dir) = args.first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let count = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5000);
    let cover_variants = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500)
        .max(1);
    let seed = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    match run(&PathBuf::from(dir), count, cover_variants, seed, works) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}
