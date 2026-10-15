//! AT-S7: 3 台を模擬した性質テスト。
//!
//! ランダムな操作（本・保管場所・タグ・シリーズ・本棚の作成・変更・削除・復元）、ランダムな書き出しと取り込みの順序、
//! ファイルが遅れて見える・途中まで見える・衝突コピーができる、途中参加（新規作成・参加・統合）、解除と再参加、
//! スナップショット、セグメントの統合、時計の逆行・大きなずれ、知らない `ent`・項目を含む外部の端末のセグメントを混ぜる。
//! 最後に、全端末で書き出しと取り込みを十分に繰り返したあと、`dump_canonical` が全端末で一致し、
//! 未書き出しの Op がなく、参照されている書影が全端末に届いていること。

mod common;

use bibcontainer_core::backup::{self, BackupKind};
use bibcontainer_core::schema::Entity;
use bibcontainer_core::{db, sync_state};
use bibcontainer_sync::engine::SyncOptions;
use bibcontainer_sync::{Delivery, DeliveryPolicy, SetupMode, SyncError};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use serde_json::{Value, json};

use common::*;

const N: usize = 3;
const NAMES: [&str; N] = ["A", "B", "C"];

#[derive(Debug, Clone)]
struct EditSpec {
    ent: u8,
    id: usize,
    picks: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
enum Step {
    Edit {
        dev: usize,
        edit: EditSpec,
    },
    Export(usize),
    Import(usize),
    Sync(usize),
    Snapshot(usize),
    Join(usize),
    Leave(usize),
    Maintain(usize),
    Tick {
        dev: usize,
        delta_ms: i64,
    },
    AdvanceDays {
        dev: usize,
        days: i64,
    },
    Policy {
        dev: usize,
        kind: u8,
    },
    Deliver {
        dev: usize,
        pick: usize,
        mode: u8,
        cut: usize,
    },
    ConflictCopy {
        pick: usize,
    },
    Foreign,
    /// バックアップを取る（DB がファイルの世界だけ）。
    Backup(usize),
    /// 最後に取ったバックアップから復元する（新しい端末 ID になり、そのあとの変更を失う）。
    Restore(usize),
}

fn arb_edit() -> impl Strategy<Value = EditSpec> {
    (
        0u8..10,
        0usize..4,
        prop::collection::vec((0usize..32, 0usize..8), 1..=4),
    )
        .prop_map(|(ent, id, picks)| EditSpec { ent, id, picks })
}

fn arb_step(restore_weight: u32) -> impl Strategy<Value = Step> {
    let dev = 0..N;
    prop_oneof![
        8 => (dev.clone(), arb_edit()).prop_map(|(dev, edit)| Step::Edit { dev, edit }),
        3 => dev.clone().prop_map(Step::Export),
        3 => dev.clone().prop_map(Step::Import),
        2 => dev.clone().prop_map(Step::Sync),
        1 => dev.clone().prop_map(Step::Snapshot),
        2 => dev.clone().prop_map(Step::Join),
        1 => dev.clone().prop_map(Step::Leave),
        1 => dev.clone().prop_map(Step::Maintain),
        2 => (dev.clone(), -3000i64..60_000).prop_map(|(dev, delta_ms)| Step::Tick { dev, delta_ms }),
        1 => (dev.clone(), 31i64..60).prop_map(|(dev, days)| Step::AdvanceDays { dev, days }),
        2 => (dev.clone(), 0u8..3).prop_map(|(dev, kind)| Step::Policy { dev, kind }),
        4 => (dev, 0usize..64, 0u8..3, 0usize..4096)
            .prop_map(|(dev, pick, mode, cut)| Step::Deliver { dev, pick, mode, cut }),
        1 => (0usize..64).prop_map(|pick| Step::ConflictCopy { pick }),
        1 => Just(Step::Foreign),
        restore_weight => (0..N).prop_map(Step::Backup),
        restore_weight => (0..N).prop_map(Step::Restore),
    ]
}

fn arb_plan() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(arb_step(1), 1..70)
}

/// バックアップと復元を多めに混ぜた計画。
fn arb_restore_plan() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(arb_step(5), 1..70)
}

/// 項目の表（名前、値の候補）。候補はどれも、その項目に記録できる値。
fn field_table(ent: Entity) -> Vec<(&'static str, Vec<Value>)> {
    let series = [json!(SERIES_1), Value::Null];
    let locs = [json!(LOC_1), Value::Null];
    match ent {
        Entity::Book => vec![
            (
                "title",
                vec![json!("a"), json!("b"), json!("日本語"), json!("")],
            ),
            ("rating", vec![json!(1), json!(2), json!(3), Value::Null]),
            ("memo", vec![json!("x"), json!("y"), Value::Null]),
            ("deleted", vec![json!(true), json!(false)]),
            ("purged", vec![json!(false), json!(false), json!(true)]),
            (
                "tags",
                vec![json!([]), json!(["a"]), json!(["a", "研究/SiC"])],
            ),
            (
                "authors",
                vec![
                    json!([]),
                    json!([{"name": "n"}]),
                    json!([{"name": "m", "role": "著"}]),
                ],
            ),
            ("series_id", series.to_vec()),
            ("location_id", locs.to_vec()),
            (
                "volume_sort",
                vec![json!(1), json!(1.5), json!(10.0), Value::Null],
            ),
            ("ownership", vec![json!("owned"), json!("wanted")]),
            ("created_at", vec![json!("2026-01-01T00:00:00.000Z")]),
            (
                "field_sources.title",
                vec![json!("openbd"), json!("manual"), Value::Null],
            ),
            ("manual_fields.title", vec![json!(true), Value::Null]),
            ("manual_fields.memo", vec![json!(true), Value::Null]),
            (
                "cover_hash",
                vec![json!(0), json!(1), json!(2), Value::Null],
            ),
        ],
        Entity::Series => vec![
            ("name", vec![json!("シリーズ a"), json!("シリーズ b")]),
            ("memo", vec![json!("m"), Value::Null]),
            ("deleted", vec![json!(true), json!(false)]),
        ],
        Entity::Location => vec![
            ("name", vec![json!("書斎"), json!("居間")]),
            ("parent_id", vec![json!(LOC_1), Value::Null]),
            ("sort_order", vec![json!(1.0), json!(2.5), Value::Null]),
            ("deleted", vec![json!(true), json!(false)]),
        ],
        Entity::Tag => vec![
            ("name", vec![json!("t1"), json!("t2")]),
            ("color", vec![json!("#fff"), Value::Null]),
            ("deleted", vec![json!(true), json!(false)]),
        ],
        Entity::Shelf => vec![
            ("name", vec![json!("本棚 a"), json!("本棚 b")]),
            ("query", vec![json!({}), json!({"q": "a"})]),
            ("sort_order", vec![json!(1), Value::Null]),
            ("deleted", vec![json!(true), json!(false)]),
        ],
    }
}

fn entity_and_id(spec: &EditSpec) -> (Entity, String) {
    match spec.ent {
        0..=5 => (Entity::Book, book_id(spec.id % 4)),
        6 => (Entity::Series, SERIES_1.to_owned()),
        7 => (Entity::Location, LOC_1.to_owned()),
        8 => (
            Entity::Tag,
            [TAG_1, "0192f3a0-6c1e-7cc3-9b1a-2f4e8d1c0b02"][spec.id % 2].to_owned(),
        ),
        _ => (Entity::Shelf, SHELF_1.to_owned()),
    }
}

struct World {
    backend: Backend,
    /// DB をファイルにしているか（バックアップと復元を試せる）。
    disk: bool,
    backups: [Option<String>; N],
    devs: Vec<Dev>,
    joined: [bool; N],
    options: SyncOptions,
    foreign_done: bool,
}

/// 一時的に起こりうるエラー（ライブラリの情報がまだ届いていない・途中まで届いている）。
fn transient(err: &SyncError) -> bool {
    matches!(
        err,
        SyncError::LibraryMissing | SyncError::BadLibraryFile(_)
    )
}

impl World {
    fn new(backend: Backend, disk: bool) -> Self {
        let devs = NAMES
            .iter()
            .map(|n| {
                if disk {
                    Dev::new_on_disk(n, &backend)
                } else {
                    Dev::new(n, &backend)
                }
            })
            .collect();
        World {
            backend,
            disk,
            backups: [None, None, None],
            devs,
            joined: [false; N],
            options: SyncOptions {
                segment_max_ops: 3,
                max_files_per_import: 3,
                snapshot_every_ops: 6,
                snapshot_batch: 2,
                max_cover_fetch: 2,
                ..SyncOptions::default()
            },
            foreign_done: false,
        }
    }

    fn memory(&self) -> Option<&bibcontainer_sync::MemoryFolder> {
        match &self.backend {
            Backend::Memory(m) => Some(m),
            Backend::Local(_) => None,
        }
    }

    fn engine(
        &self,
        d: usize,
    ) -> bibcontainer_sync::SyncEngine<
        '_,
        bibcontainer_sync::ConnDb,
        dyn bibcontainer_sync::SyncTransport,
    > {
        self.devs[d].engine_with(self.options.clone())
    }

    fn check<T>(&self, d: usize, what: &str, r: Result<T, SyncError>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) if transient(&e) => None,
            Err(e) => panic!("{} の {what} に失敗: {e}", NAMES[d]),
        }
    }

    fn edit(&mut self, d: usize, spec: &EditSpec) {
        let (ent, id) = entity_and_id(spec);
        let table = field_table(ent);
        let mut set = serde_json::Map::new();
        for &(f, v) in &spec.picks {
            let (name, values) = &table[f % table.len()];
            let mut value = values[v % values.len()].clone();
            if *name == "cover_hash"
                && let Some(n) = value.as_u64()
            {
                let hash = self.devs[d].make_cover(format!("cover {n}").as_bytes());
                value = json!(hash);
            }
            set.insert((*name).to_owned(), value);
        }
        self.devs[d].clock.advance(1);
        let dev = &self.devs[d];
        bibcontainer_core::record_change_committed(&mut dev.db.lock(), &dev.clock, ent, &id, set)
            .unwrap();
    }

    fn join(&mut self, d: usize) {
        if self.joined[d] {
            return;
        }
        let engine = self.engine(d);
        let visible = match engine.read_library_file() {
            Ok(v) => v,
            Err(e) if transient(&e) => return,
            Err(e) => panic!("{}: {e}", NAMES[d]),
        };
        let has_data = sync_state::has_local_data(&self.devs[d].db.lock()).unwrap();
        let mode = match (visible, self.backend.read_raw("library.json").is_some()) {
            (Some(_), _) if has_data => SetupMode::JoinMerge,
            (Some(_), _) => SetupMode::Join,
            (None, false) => SetupMode::Create,
            // 実際にはあるが、まだこの端末には見えない。あとでやり直す。
            (None, true) => return,
        };
        if let Some(_report) = self.check(d, "設定", engine.setup(mode)) {
            self.joined[d] = true;
        }
    }

    fn foreign(&mut self) {
        if self.foreign_done {
            return;
        }
        self.foreign_done = true;
        let x = "d-ffffffffffff";
        let ops = [
            foreign_op(
                x,
                1,
                T0 + 5,
                Entity::Book,
                &book_id(0),
                json!({"title": "外部の書名", "zz_future": {"k": [1, 2]}, "rating": 2}),
            ),
            foreign_op(
                x,
                2,
                T0 + 6,
                Entity::Tag,
                TAG_1,
                json!({"name": "外部のタグ", "zz_tag": "x"}),
            ),
            foreign_op(
                x,
                3,
                T0 + 7,
                Entity::Book,
                &book_id(1),
                json!({"memo": "外部のメモ", "field_sources.memo": "ndl"}),
            ),
        ];
        let mut lines: Vec<String> = ops.iter().map(|o| o.to_json_line().unwrap()).collect();
        // 知らない ent と、新しい形式の Op
        lines.push(format!(
            r#"{{"v":1,"dev":"{x}","seq":4,"hlc":"{:015}:00000:{x}","ent":"collection","id":"c1","set":{{"name":"新しい種類"}}}}"#,
            T0 + 8
        ));
        lines.push(format!(
            r#"{{"v":2,"dev":"{x}","seq":5,"hlc":"{:015}:00000:{x}","ent":"book","id":"{}","set":{{"title":{{"future":true}}}}}}"#,
            T0 + 9,
            book_id(2)
        ));
        self.backend.write_raw(
            &bibcontainer_sync::layout::segment_path(x, 1, 5),
            &sealed(&lines),
        );
    }

    fn apply(&mut self, step: &Step) {
        match step {
            Step::Edit { dev, edit } => self.edit(*dev, edit),
            Step::Export(d) if self.joined[*d] => {
                let r = self.engine(*d).export();
                self.check(*d, "書き出し", r);
            }
            Step::Import(d) if self.joined[*d] => {
                let r = self.engine(*d).import_all(100);
                self.check(*d, "取り込み", r);
            }
            Step::Sync(d) if self.joined[*d] => {
                let r = self.engine(*d).sync(100);
                self.check(*d, "同期", r);
            }
            Step::Snapshot(d) if self.joined[*d] => {
                let r = self.engine(*d).check_and_snapshot();
                self.check(*d, "スナップショット", r);
            }
            Step::Join(d) => self.join(*d),
            Step::Leave(d) if self.joined[*d] => {
                self.engine(*d).leave().unwrap();
                self.joined[*d] = false;
            }
            Step::Maintain(d) if self.joined[*d] => {
                let r = self.engine(*d).maintain();
                self.check(*d, "手入れ", r);
            }
            Step::Tick { dev, delta_ms } => self.devs[*dev].clock.advance(*delta_ms),
            Step::AdvanceDays { dev, days } => self.devs[*dev].clock.advance(days * DAY),
            Step::Policy { dev, kind } => {
                if let Some(m) = self.memory() {
                    let policy = match kind {
                        0 => DeliveryPolicy::Immediate,
                        1 => DeliveryPolicy::Hidden,
                        _ => DeliveryPolicy::Partial { percent: 55 },
                    };
                    m.transport(NAMES[*dev]).set_policy(policy);
                }
            }
            Step::Deliver {
                dev,
                pick,
                mode,
                cut,
            } => {
                if let Some(m) = self.memory() {
                    let t = m.transport(NAMES[*dev]);
                    let pending = t.pending();
                    if !pending.is_empty() {
                        let (path, _) = &pending[pick % pending.len()];
                        let len = m.read_raw(path).map_or(0, |d| d.len());
                        match mode {
                            0 => t.deliver(path),
                            1 => t.set_delivery(path, Delivery::Partial(cut % (len + 1))),
                            _ => t.set_delivery(path, Delivery::Hidden),
                        }
                    }
                }
            }
            Step::ConflictCopy { pick } => {
                let paths: Vec<String> = self
                    .backend
                    .paths()
                    .into_iter()
                    .filter(|p| p.starts_with("ops/") || p.starts_with("snapshots/"))
                    .filter(|p| !p.contains(" (1)"))
                    .collect();
                if !paths.is_empty() {
                    self.backend.make_conflict_copy(&paths[pick % paths.len()]);
                }
            }
            Step::Foreign => self.foreign(),
            Step::Backup(d) if self.disk => {
                let dev = &self.devs[*d];
                let dir = dev
                    .dir
                    .as_ref()
                    .expect("ファイルの DB")
                    .path()
                    .to_path_buf();
                let info = backup::create_backup(
                    &dev.db.lock(),
                    &dir.join("manual"),
                    BackupKind::Manual,
                    &dev.clock,
                )
                .unwrap();
                self.backups[*d] = Some(info.path);
            }
            Step::Restore(d) if self.disk => {
                if let Some(path) = self.backups[*d].clone() {
                    let dev = &self.devs[*d];
                    let dir = dev
                        .dir
                        .as_ref()
                        .expect("ファイルの DB")
                        .path()
                        .to_path_buf();
                    // 接続をすべて閉じてから復元する
                    dev.db
                        .replace(rusqlite::Connection::open_in_memory().unwrap());
                    let db_path = dir.join("library.sqlite3");
                    backup::restore_backup(
                        std::path::Path::new(&path),
                        &db_path,
                        &dir.join("safety"),
                        &dev.clock,
                    )
                    .unwrap();
                    let (conn, _) = db::open(&db_path, &dir.join("backups")).unwrap();
                    dev.db.replace(conn);
                    // バックアップの時点で参加していなければ、参加していない状態に戻る
                    self.joined[*d] = sync_state::library_id(&dev.db.lock()).unwrap().is_some();
                }
            }
            // 参加していない端末の同期操作は何もしない
            Step::Export(_)
            | Step::Import(_)
            | Step::Sync(_)
            | Step::Snapshot(_)
            | Step::Leave(_)
            | Step::Maintain(_)
            | Step::Backup(_)
            | Step::Restore(_) => {}
        }
    }

    /// 遅れ・途中・衝突コピーの影響をなくし、全員を参加させて、十分に同期する。
    fn finish(&mut self) {
        if let Some(m) = self.memory() {
            for n in NAMES {
                let t = m.transport(n);
                t.set_policy(DeliveryPolicy::Immediate);
                t.deliver_all();
            }
        }
        for (d, name) in NAMES.iter().enumerate() {
            self.join(d);
            assert!(self.joined[d], "{name} が参加できない");
        }
        let mut last: Option<Vec<Value>> = None;
        for round in 0..10 {
            for d in 0..N {
                let r = self.engine(d).sync(100);
                self.check(d, "最後の同期", r)
                    .expect("最後の同期は成功する");
            }
            let dumps: Vec<Value> = self.devs.iter().map(Dev::dump).collect();
            let stable = last.as_ref() == Some(&dumps);
            let same = dumps.iter().all(|d| *d == dumps[0]);
            if same && stable {
                return;
            }
            last = Some(dumps);
            assert!(round < 9, "10 回の同期でそろわなかった");
        }
    }

    fn check_final(&self) {
        let refs: Vec<&Dev> = self.devs.iter().collect();
        assert_converged(&refs);
        for d in &self.devs {
            assert_eq!(d.pending(), 0, "{} に未書き出しの Op が残っている", d.name);
            let hashes = sync_state::referenced_cover_hashes(&d.db.lock()).unwrap();
            for h in hashes {
                assert!(d.has_cover(&h), "{} に書影 {h} が届いていない", d.name);
            }
            // 派生データ（検索の索引）が、全体の作り直しと一致する
            let incremental = derived_rows(&d.db.lock());
            let mut conn = d.db.lock();
            bibcontainer_core::derived::rebuild(&mut conn).unwrap();
            assert_eq!(incremental, derived_rows(&conn), "{} の派生データ", d.name);
        }
    }
}

fn derived_rows(conn: &rusqlite::Connection) -> Vec<(String, String, String)> {
    conn.prepare(
        "SELECT i.book_id, i.sort_title, f.title FROM book_idx i JOIN books_fts f ON f.rowid = i.idx ORDER BY i.book_id",
    )
    .unwrap()
    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

trait SnapshotNow {
    fn check_and_snapshot(&self) -> Result<bibcontainer_sync::SnapshotInfo, SyncError>;
}

impl<D: bibcontainer_sync::SyncDb, T: bibcontainer_sync::SyncTransport + ?Sized> SnapshotNow
    for bibcontainer_sync::SyncEngine<'_, D, T>
{
    /// ライブラリを確かめてから、スナップショットを書く。
    fn check_and_snapshot(&self) -> Result<bibcontainer_sync::SnapshotInfo, SyncError> {
        self.read_library_file()?.ok_or(SyncError::LibraryMissing)?;
        self.write_snapshot()
    }
}

fn run_plan(backend: Backend, disk: bool, plan: &[Step]) {
    let mut world = World::new(backend, disk);
    for step in plan {
        world.apply(step);
    }
    world.finish();
    world.check_final();
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 120,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("proptest-regressions"))),
        ..ProptestConfig::default()
    })]

    /// 3 台の端末が、メモリ上の同期フォルダ（遅れる・途中まで見える・衝突コピー）を介して収束する。
    #[test]
    fn at_s7_three_devices_converge_on_a_flaky_memory_folder(plan in arb_plan()) {
        run_plan(Backend::memory(), false, &plan);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("proptest-regressions"))),
        ..ProptestConfig::default()
    })]

    /// ローカルのフォルダ（衝突コピーあり。遅れ・途中の再現はメモリ上の実装だけ）でも収束する。
    #[test]
    fn at_s7_three_devices_converge_on_a_local_folder(plan in arb_plan()) {
        run_plan(Backend::local(), false, &plan);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 30,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("proptest-regressions"))),
        ..ProptestConfig::default()
    })]

    /// DB をファイルにして、バックアップと復元（新しい端末 ID、そのあとの変更の喪失）も混ぜても収束する。
    #[test]
    fn at_s7_three_devices_converge_with_backups_and_restores(plan in arb_restore_plan()) {
        run_plan(Backend::memory(), true, &plan);
    }
}

/// 解除と再参加、時計のずれ、統合を含む、決まった筋書きでも収束する（性質テストの回帰用）。
#[test]
fn a_fixed_scenario_with_merge_leave_and_clock_jumps_converges() {
    let plan = vec![
        Step::Edit {
            dev: 2,
            edit: EditSpec {
                ent: 0,
                id: 0,
                picks: vec![(0, 0), (1, 0)],
            },
        },
        Step::Join(0),
        Step::Edit {
            dev: 0,
            edit: EditSpec {
                ent: 0,
                id: 0,
                picks: vec![(0, 1)],
            },
        },
        Step::Export(0),
        Step::Join(1),
        Step::Join(2), // C は手元にデータがあるので統合
        Step::AdvanceDays { dev: 1, days: 40 },
        Step::Edit {
            dev: 1,
            edit: EditSpec {
                ent: 0,
                id: 1,
                picks: vec![(3, 0)],
            },
        },
        Step::Sync(1),
        Step::Leave(0),
        Step::Edit {
            dev: 0,
            edit: EditSpec {
                ent: 6,
                id: 0,
                picks: vec![(0, 1)],
            },
        },
        Step::Join(0),
        Step::Snapshot(2),
        Step::Maintain(1),
        Step::Foreign,
    ];
    run_plan(Backend::memory(), false, &plan);
}
