//! AT-14（性能）: ダミーの 5,000 冊・20,000 冊で、一覧の取得と検索の時間を測る。
//!
//! 普段は動かさない（`#[ignore]`）。次のコマンドで、リリースビルドで実行する。
//!
//! ```text
//! cargo test --release -p bibcontainer-core --test perf -- --ignored --nocapture
//! ```
//!
//! 目標（要件書 10.1）: 検索の応答 100ms 以内（5,000 冊）、300ms 以内（20,000 冊）。

// 検査の都合で `&[x.clone()]` と書くほうが読みやすいので、この lint は切る。
#![allow(clippy::cloned_ref_to_slice_refs)]

use std::time::{Duration, Instant};

use bibcontainer_core::library::{self, Env, FieldChange};
use bibcontainer_core::normalize::isbn13_check_digit;
use bibcontainer_core::{SystemClock, derived, get_rows, list_rows, open, search_books};
use rusqlite::Connection;
use serde_json::{Value, json};
use tempfile::tempdir;

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
    fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const WORDS: &[&str] = &[
    "吾輩",
    "猫",
    "坊っちゃん",
    "こころ",
    "三四郎",
    "プログラミング",
    "入門",
    "Rust",
    "データベース",
    "設計",
    "物語",
    "世界",
    "歴史",
    "科学",
    "数学",
    "物理",
    "化学",
    "宇宙",
    "SiC",
    "半導体",
    "結晶",
    "成長",
    "評価",
    "技術",
    "文学",
    "小説",
    "随筆",
    "詩集",
    "全集",
    "解説",
    "アルゴリズム",
    "ネットワーク",
    "経済",
    "哲学",
    "心理学",
    "言語",
    "料理",
    "旅",
    "山",
    "海",
    "夜",
    "朝",
    "春",
    "夏",
    "秋",
    "冬",
    "東京",
    "京都",
    "戦争",
    "平和",
    "学校",
    "医療",
    "建築",
    "写真",
    "音楽",
    "映画",
    "漫画",
    "辞典",
    "Introduction",
    "Guide",
    "Handbook",
    "Systems",
    "Design",
    "ＡＢＣ",
    "ﾃﾞｰﾀ",
    "シリーズ",
    "完全版",
];
const SURNAMES: &[&str] = &[
    "夏目", "芥川", "太宰", "宮沢", "森", "谷崎", "川端", "三島", "村上", "吉本", "山田", "佐藤",
    "鈴木", "高橋", "田中", "伊藤", "渡辺", "中村", "小林", "加藤",
];
const GIVEN: &[&str] = &[
    "漱石",
    "龍之介",
    "治",
    "賢治",
    "鷗外",
    "潤一郎",
    "康成",
    "由紀夫",
    "春樹",
    "ばなな",
    "太郎",
    "花子",
    "一郎",
    "次郎",
    "美咲",
    "健",
    "誠",
    "愛",
    "翔",
    "葵",
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
];
const TAGS: &[&str] = &[
    "小説",
    "研究/SiC",
    "研究/論文",
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
const KANA: &[&str] = &[
    "あ", "い", "う", "え", "お", "か", "き", "く", "け", "こ", "さ", "し", "す", "せ", "そ", "た",
    "ち", "つ", "て", "と", "な", "に", "ぬ", "ね", "の", "は", "ひ", "ふ", "へ", "ほ", "ま", "み",
    "む", "め", "も", "ら",
];

fn isbn(i: usize) -> String {
    let first12 = format!("9784{:08}", i);
    let check = isbn13_check_digit(&first12).unwrap();
    format!("{first12}{check}")
}

fn dummy_book(rng: &mut Rng, i: usize, series: &[String]) -> Vec<FieldChange> {
    let mut title = String::new();
    for k in 0..(2 + rng.below(3)) {
        if k > 0 {
            title.push(if rng.below(3) == 0 { ' ' } else { 'の' });
        }
        title.push_str(rng.pick(WORDS));
    }
    let mut changes = vec![
        FieldChange::provider("title", json!(title), "openbd"),
        FieldChange::barcode("isbn13", json!(isbn(i))),
        FieldChange::provider("publisher", json!(rng.pick(PUBLISHERS)), "openbd"),
        FieldChange::provider(
            "published_on",
            json!(format!("{}-{:02}", 1950 + rng.below(76), 1 + rng.below(12))),
            "openbd",
        ),
        FieldChange::barcode(
            "c_code",
            json!(format!(
                "0{}{}{}",
                rng.below(2),
                rng.below(10),
                rng.below(10)
            )),
        ),
        FieldChange::barcode("list_price", json!(500 + 10 * rng.below(300))),
        FieldChange::provider("pages", json!(80 + rng.below(700)), "ndl"),
    ];
    if rng.below(10) < 7 {
        let kana: String = (0..(6 + rng.below(14))).map(|_| rng.pick(KANA)).collect();
        changes.push(FieldChange::provider("title_kana", json!(kana), "ndl"));
    }
    let authors: Vec<Value> = (0..(1 + rng.below(3)))
        .map(
            |_| json!({"name": format!("{}{}", rng.pick(SURNAMES), rng.pick(GIVEN)), "role": "著"}),
        )
        .collect();
    changes.push(FieldChange::provider("authors", json!(authors), "openbd"));
    let tags: Vec<&str> = (0..rng.below(4)).map(|_| rng.pick(TAGS)).collect();
    changes.push(FieldChange::manual("tags", json!(tags)));
    if rng.below(10) < 3 {
        changes.push(FieldChange::manual(
            "memo",
            json!(format!("{} について。{}", rng.pick(WORDS), rng.pick(WORDS))),
        ));
    }
    if rng.below(10) == 0 {
        changes.push(FieldChange::manual(
            "series_id",
            json!(series[rng.below(series.len())]),
        ));
        changes.push(FieldChange::manual(
            "volume",
            json!(format!("{}", 1 + rng.below(30))),
        ));
    }
    if rng.below(4) == 0 {
        changes.push(FieldChange::manual("read_status", json!("read")));
        changes.push(FieldChange::manual("rating", json!(1 + rng.below(5))));
    }
    changes
}

fn build(conn: &mut Connection, n: usize) -> Duration {
    let clock = SystemClock;
    let env = Env::new(&clock);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let series: Vec<String> = (0..100)
        .map(|i| library::find_or_create_series(conn, &env, &format!("ダミーシリーズ{i}")).unwrap())
        .collect();
    let started = Instant::now();
    for chunk_start in (0..n).step_by(1000) {
        let end = (chunk_start + 1000).min(n);
        let batch: Vec<Vec<FieldChange>> = (chunk_start..end)
            .map(|i| dummy_book(&mut rng, i, &series))
            .collect();
        library::create_books(conn, &env, &batch).unwrap();
    }
    started.elapsed()
}

/// `f` を何回か実行して、中央値を返す。
fn median(runs: usize, mut f: impl FnMut() -> usize) -> (Duration, usize) {
    let mut times = Vec::new();
    let mut hits = 0;
    for _ in 0..runs {
        let t = Instant::now();
        hits = f();
        times.push(t.elapsed());
    }
    times.sort();
    (times[times.len() / 2], hits)
}

fn run(n: usize, limit: Duration) {
    let dir = tempdir().unwrap();
    let (mut conn, _) = open(
        &dir.path().join("perf.sqlite3"),
        &dir.path().join("backups"),
    )
    .unwrap();
    let build_time = build(&mut conn, n);
    eprintln!("---- {n} 冊 ----");
    eprintln!(
        "作成（検証・Op 記録・派生データの更新込み）: {build_time:.2?}（1 冊あたり {:.0?}）",
        build_time / u32::try_from(n).unwrap()
    );

    let (t, rows) = median(7, || list_rows(&conn).unwrap().len());
    eprintln!("list_rows（全 {rows} 件）: {t:.2?}");
    let ids: Vec<String> = list_rows(&conn)
        .unwrap()
        .into_iter()
        .take(50)
        .map(|r| r.id)
        .collect();
    let (t, _) = median(7, || get_rows(&conn, &ids).unwrap().len());
    eprintln!("get_rows（50 件）: {t:.2?}");

    let queries: &[(&str, String)] = &[
        ("3 文字以上（多い語）", "プログラミング".to_owned()),
        ("3 文字以上（2 語の AND）", "データベース 入門".to_owned()),
        ("3 文字以上（英字・大小無視）", "RUST".to_owned()),
        ("3 文字以上（ヨミ・ひらがな）", "あいう".to_owned()),
        ("3 文字以上（該当少）", "SiC".to_owned()),
        ("2 文字以下（2 文字）", "小説".to_owned()),
        ("2 文字以下（1 文字）", "猫".to_owned()),
        ("2 文字以下（2 文字 + 3 文字以上）", "海 半導体".to_owned()),
        ("ISBN（13 桁）", isbn(n / 2)),
        ("ISBN（ハイフン入り）", {
            let i = isbn(n / 3);
            format!(
                "{}-{}-{}-{}-{}",
                &i[..3],
                &i[3..4],
                &i[4..6],
                &i[6..12],
                &i[12..]
            )
        }),
        ("ISBN（ISBN-10）", {
            let i = isbn(n / 4);
            bibcontainer_core::normalize::isbn13_to_isbn10(&i).unwrap()
        }),
        ("検索語なし（全 ID）", String::new()),
    ];
    let mut worst = Duration::ZERO;
    for (label, q) in queries {
        let (t, hits) = median(9, || search_books(&conn, q).unwrap().len());
        eprintln!("search_books {label}: {t:.2?}（{hits} 件）");
        worst = worst.max(t);
    }
    eprintln!("検索の最大（中央値）: {worst:.2?}（目標 {limit:.0?}）");

    let t = Instant::now();
    let built = derived::rebuild(&mut conn).unwrap();
    eprintln!(
        "派生データの全体の作り直し（{built} 冊）: {:.2?}",
        t.elapsed()
    );

    assert!(
        worst <= limit,
        "検索が目標を超えた: {worst:.2?} > {limit:.0?}（{n} 冊）"
    );
}

#[test]
#[ignore = "性能の計測。cargo test --release -p bibcontainer-core --test perf -- --ignored --nocapture"]
fn at14_5000_books_search_within_100ms() {
    run(5_000, Duration::from_millis(100));
}

#[test]
#[ignore = "性能の計測。cargo test --release -p bibcontainer-core --test perf -- --ignored --nocapture"]
fn at14_20000_books_search_within_300ms() {
    run(20_000, Duration::from_millis(300));
}
