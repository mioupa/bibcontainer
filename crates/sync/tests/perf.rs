//! 性能の計測（5,000 冊のライブラリ）。普段は動かさない（`#[ignore]`）。次のコマンドで、リリースビルドで実行する。
//!
//! ```text
//! cargo test --release -p bibcontainer-sync --test perf -- --ignored --nocapture
//! ```
//!
//! 測るもの: スナップショットの書き出し（状態の読み出し・JSON 化・gzip・書き込み）、新しい端末での読み込み
//! （読み出し・展開・適用）、セグメントからの取り込み、ファイルの大きさ、書影の受け渡し。

mod common;

use std::time::{Duration, Instant};

use bibcontainer_core::library::{self, Env, FieldChange};
use bibcontainer_core::normalize::isbn13_check_digit;
use bibcontainer_core::{Clock, SystemClock, db};
use bibcontainer_sync::engine::{DeviceInfo, SyncEngine, SyncOptions};
use bibcontainer_sync::format::sha256_hex;
use bibcontainer_sync::layout::cover_local_path;
use bibcontainer_sync::{ConnDb, LocalFolder, SetupMode};
use rusqlite::Connection;
use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

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

const BOOKS: usize = 5_000;

struct Node {
    _dir: TempDir,
    db: ConnDb,
    covers: TempDir,
    transport: LocalFolder,
    info: DeviceInfo,
}

impl Node {
    fn new(name: &str, library_root: &std::path::Path) -> Self {
        Self::with_covers(name, library_root, tempdir().unwrap())
    }

    fn with_covers(name: &str, library_root: &std::path::Path, covers: TempDir) -> Self {
        let dir = tempdir().unwrap();
        let (conn, _) = db::open(
            &dir.path().join("library.sqlite3"),
            &dir.path().join("backups"),
        )
        .unwrap();
        Node {
            _dir: dir,
            db: ConnDb::new(conn),
            covers,
            transport: LocalFolder::new(library_root),
            info: DeviceInfo {
                name: name.to_owned(),
                os: "perf".to_owned(),
                app_version: "0.1.0".to_owned(),
            },
        }
    }

    /// 書影のフォルダの中身を新しい一時フォルダにコピーする。
    fn covers_dir_clone(&self) -> TempDir {
        let to = tempdir().unwrap();
        copy_dir(self.covers.path(), to.path());
        to
    }

    fn engine<'a>(&'a self, clock: &'a SystemClock) -> SyncEngine<'a, ConnDb, LocalFolder> {
        SyncEngine::new(
            &self.db,
            &self.transport,
            clock,
            self.covers.path(),
            self.info.clone(),
        )
    }
}

fn build_library(conn: &mut Connection, n: usize, with_covers: Option<&std::path::Path>) {
    let clock = SystemClock;
    let env = Env::new(&clock);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let series: Vec<String> = (0..100)
        .map(|i| library::find_or_create_series(conn, &env, &format!("ダミーシリーズ{i}")).unwrap())
        .collect();
    for chunk_start in (0..n).step_by(1000) {
        let end = (chunk_start + 1000).min(n);
        let batch: Vec<Vec<FieldChange>> = (chunk_start..end)
            .map(|i| {
                let mut changes = dummy_book(&mut rng, i, &series);
                // 半分の本に書影（8KB ほどのダミー）をつける
                if let Some(dir) = with_covers
                    && i % 2 == 0
                {
                    let mut bytes: Vec<u8> = (0..8192usize)
                        .map(|k| ((i * 31 + k * 7) % 251) as u8)
                        .collect();
                    bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
                    let hash = sha256_hex(&bytes);
                    let path = cover_local_path(dir, &hash);
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, &bytes).unwrap();
                    changes.push(FieldChange::provider("cover_hash", json!(hash), "openbd"));
                }
                changes
            })
            .collect();
        library::create_books(conn, &env, &batch).unwrap();
    }
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn dir_size(path: &std::path::Path) -> (u64, usize) {
    let mut total = 0;
    let mut files = 0;
    if let Ok(read) = std::fs::read_dir(path) {
        for e in read.flatten() {
            let p = e.path();
            if p.is_dir() {
                let (t, f) = dir_size(&p);
                total += t;
                files += f;
            } else if let Ok(m) = e.metadata() {
                total += m.len();
                files += 1;
            }
        }
    }
    (total, files)
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let out = f();
    let d = t.elapsed();
    eprintln!("{label}: {d:.2?}");
    (out, d)
}

/// 5,000 冊で、スナップショット経由の参加と、セグメントだけからの参加の時間を測る。
#[test]
#[ignore = "性能の計測。--ignored で実行する"]
fn five_thousand_books_snapshot_and_segments() {
    let clock = SystemClock;
    eprintln!("---- {BOOKS} 冊 ----");

    // ---- スナップショットで共有する ----
    let folder_a = tempdir().unwrap();
    let root_a = folder_a.path().join("bibcontainer-library");
    let a = Node::new("A", &root_a);
    let (_, build_time) = timed(
        "作成（Op 記録・派生データの更新込み）",
        || build_library(&mut a.db.lock(), BOOKS, Some(a.covers.path())),
    );
    let _ = build_time;
    let ops = bibcontainer_core::sync_state::pending_count(&a.db.lock()).unwrap();
    eprintln!("未書き出しの Op: {ops}");
    let state_entities = bibcontainer_core::sync_state::export_state(&a.db.lock())
        .unwrap()
        .len();
    eprintln!("エンティティ: {state_entities}");

    // 書影を除いた時間を知るため、書影の受け渡しは別に測る
    let (_, t_state) = timed("状態の書き出し（export_state のみ）", || {
        bibcontainer_core::sync_state::export_state(&a.db.lock())
            .unwrap()
            .len()
    });
    let _ = t_state;
    let (created, t_create) = timed(
        "新しいライブラリを作る（スナップショットを書く。書影の写しを含む）",
        || a.engine(&clock).setup(SetupMode::Create).unwrap(),
    );
    let _ = (created, t_create);
    let (snap_bytes, snap_files) = dir_size(&root_a.join("snapshots"));
    eprintln!(
        "スナップショットのファイル: {snap_files} 個、{:.2} MB（{snap_bytes} バイト）",
        mb(snap_bytes)
    );
    let (cover_bytes, cover_files) = dir_size(&root_a.join("covers"));
    eprintln!(
        "同期フォルダの書影: {cover_files} 個、{:.1} MB",
        mb(cover_bytes)
    );

    // 2 回目のスナップショット（書影は写し済みなので、書き出しそのものの時間）
    let (info, _) = timed(
        "2 回目のスナップショット（書影は写し済み）",
        || a.engine(&clock).write_snapshot().unwrap(),
    );
    eprintln!(
        "  含まれるエンティティ {}、{:.2} MB",
        info.entities,
        mb(info.bytes as u64)
    );

    // 新しい端末が参加する
    let b = Node::new("B", &root_a);
    let (setup, _) = timed(
        "新しい端末が参加（スナップショットの読み込み・適用、書影の取得を含む）",
        || b.engine(&clock).setup(SetupMode::Join).unwrap(),
    );
    let import = setup.import.unwrap();
    eprintln!(
        "  適用したスナップショット {}、写した書影 {}、届いていない書影 {}",
        import.snapshots_applied, import.covers_fetched, import.missing_covers
    );
    assert_eq!(import.snapshots_applied, 1);
    let books: i64 =
        b.db.lock()
            .query_row("SELECT count(*) FROM books", [], |r| r.get(0))
            .unwrap();
    assert_eq!(books as usize, BOOKS);

    // 書影が手元にそろっている端末の参加（スナップショットの読み込み・展開・適用だけの時間）
    let b2 = Node::with_covers("B2", &root_a, b.covers_dir_clone());
    let (setup, _) = timed(
        "書影がそろっている端末が参加（スナップショットの読み込み・展開・適用）",
        || b2.engine(&clock).setup(SetupMode::Join).unwrap(),
    );
    assert_eq!(setup.import.unwrap().covers_fetched, 0);

    // 同期していない状態の取り込み（何もすることがない）
    let (_, _) = timed(
        "取り込み（変化なし。一覧・確認だけ）",
        || b.engine(&clock).import().unwrap(),
    );

    // ---- セグメントだけで共有する（スナップショットなし） ----
    let folder_c = tempdir().unwrap();
    let root_c = folder_c.path().join("bibcontainer-library");
    let c = Node::new("C", &root_c);
    // スナップショットを書かないようにして、セグメントだけで共有する
    let no_snapshot = SyncOptions {
        snapshot_every_ops: i64::MAX,
        ..SyncOptions::default()
    };
    c.engine(&clock)
        .with_options(no_snapshot.clone())
        .setup(SetupMode::Create)
        .unwrap();
    build_library(&mut c.db.lock(), BOOKS, None);
    let (export, _) = timed("セグメントの書き出し（書影なし）", || {
        c.engine(&clock)
            .with_options(no_snapshot.clone())
            .export()
            .unwrap()
    });
    let (seg_bytes, seg_files) = dir_size(&root_c.join("ops"));
    eprintln!(
        "  {} Op、セグメント {seg_files} 個、{:.2} MB",
        export.ops_exported,
        mb(seg_bytes)
    );
    let d = Node::new("D", &root_c);
    let (setup, _) = timed(
        "新しい端末が参加（セグメントだけから取り込み）",
        || d.engine(&clock).setup(SetupMode::Join).unwrap(),
    );
    let import = setup.import.unwrap();
    eprintln!(
        "  適用したセグメント {}、Op {}",
        import.files_applied, import.ops_seen
    );
    let books: i64 =
        d.db.lock()
            .query_row("SELECT count(*) FROM books", [], |r| r.get(0))
            .unwrap();
    assert_eq!(books as usize, BOOKS);
    let _ = clock.now_ms();
}
