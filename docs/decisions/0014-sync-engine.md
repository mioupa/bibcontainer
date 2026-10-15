# 0014: 同期エンジンの実装（`crates/sync` とコアの追加）

## 状態

採用

## 日付

2026-10-15

## 背景

0013 で決めたフェーズ 2 の同期の設計に従って、同期エンジン（`crates/sync`、`bibcontainer-sync`）と、コア（`crates/core`）の DB 側の操作（`sync_state`）を実装した。アプリ（`src-tauri`）への組み込みは次の作業で行う。ここには、0013 では決めていなかった細部（トレイトの形、DB の操作の切り方、取り込み済みの範囲の扱い、スナップショットの書式と適用の規則、エラーの扱い）と、0013 から変えた点、組み込みに必要なことを記録する。

実装の途中、性質テストと筋書きのテストで、0013 の設計に穴が 2 つ見つかった（§5）。どちらも修正した。

## 決定

### 1. 構成と公開 API

- `crates/sync`（`bibcontainer-sync`、edition 2024、Tauri 非依存）。依存は `bibcontainer-core`、`rusqlite`（`Connection` の型）、`serde`・`serde_json`、`sha2`、`flate2`（純 Rust の gzip）、`ts-rs`、`uuid`（一時ファイル名）、`thiserror`、`tracing`。
- コアに `sync_state` モジュールを足した（§3）。マイグレーションは足していない（`local_meta`・`applied_segments`・`outbox`・`outbox_restored` で足りる）。
- 公開する型と関数:

| 区分 | 名前 |
|---|---|
| エンジン | `SyncEngine::new(db, transport, clock, covers_dir, DeviceInfo)`、`with_options(SyncOptions)` |
| 取り込み | `import()`（1 回分）、`import_all(max_rounds)`、`ImportReport` |
| 書き出し | `export()`、`ExportReport` |
| 同期 | `sync(max_rounds)`（スナップショットが必要なら先に書く → 取り込み → 書き出し）、`SyncReport` |
| 設定 | `inspect(library_folder)`、`setup(SetupMode)`、`leave()`、free 関数 `leave(db)`、`local_summary()`、`SetupReport` |
| スナップショット | `write_snapshot()`、`SnapshotInfo` |
| 手入れ | `maintain()`（`cleanup_tmp` + `consolidate_own_segments`）、`cleanup_tmp()`、`consolidate_own_segments()`、`gc_covers()` |
| 状態 | `local_counters()`、`devices()`、`read_device_files()`、`write_device_file(last_export_ms)`、`read_library_file()`、`device_id()` |
| 同期フォルダ | `SyncTransport`、`LocalFolder`（`resolve_root`）、`MemoryFolder`・`MemoryTransport`（テスト用）、`FileEntry`、`TransportError`・`TransportErrorKind` |
| DB | `SyncDb`、`ConnDb` |
| 画面の型（ts-rs、camelCase） | `SyncStatus`、`SyncState`、`SyncDevice`、`ClockSkew`、`FolderInspection`、`FolderKind`、`SetupMode` |
| エラー | `SyncError`（`state()` で `SyncState` にできる） |

- ts-rs の生成物は `src/bindings/` の `ClockSkew.ts`・`FolderInspection.ts`・`FolderKind.ts`・`SetupMode.ts`・`SyncDevice.ts`・`SyncState.ts`・`SyncStatus.ts`（新しいファイルだけ。既存のものは変えていない）。形は 0013 §9 のとおり。

### 2. トランスポート（`SyncTransport`）

- 操作は 0013 §1 のとおり: `list`・`read`・`write_atomic`・`exists`・`delete`・`create_dir_all`。パスは `bibcontainer-library/` からの相対で、区切りは常に `/`（`..`・先頭の `/`・`\`・`:` を含むパスは断る）。
- 足したもの: `check_writable()`（設定の前に、書き込めるかを、フォルダを作らずに確かめる。既定の実装は一時ファイルを書いて消す。`LocalFolder` は、いちばん近い既存のフォルダに書く）。
- `write_atomic` は、同じフォルダに `.tmp-<乱数>` で書き（`LocalFolder` は `sync_all` してから）、名前を変える。同じ名前があれば置き換える。名前の変更が権限エラーで失敗したら（他のプロセスが掴んでいる、Windows）、少し待って 5 回までやり直す。
- 失敗は `TransportError { kind, path, message }`。種類は `NotFound`・`Unavailable`（同期フォルダそのものがない。Google Drive 未接続など）・`PermissionDenied`・`StorageFull`・`Other`。`LocalFolder` は、ライブラリのフォルダがなくても親があれば `NotFound`（まだ作っていないだけ）、親もなければ `Unavailable` にする。書き込むときも、親がなければ作らない（Drive が未接続のとき、ローカルに空のフォルダを作ってしまわないため）。
- `LocalFolder::resolve_root(選んだフォルダ)`: 選んだフォルダの名前が `bibcontainer-library` か、`library.json` があればそのフォルダ。なければ、その下の `bibcontainer-library`。
- メモリ上の実装（`MemoryFolder`）は、共有のフォルダに端末ごとの窓（`MemoryTransport`）をつなぐ。ほかの端末が書いたファイルの見え方を、端末ごとの方針（`Immediate`・`Hidden`・`Partial { percent }`）と、ファイルごとの状態（`Full`・`Hidden`・`Partial(n)`）で決める。`pending()`・`deliver()`・`set_delivery()` で、テストが届く順を決める。`make_conflict_copy` で `… (1).jsonl` を作る。`set_unavailable`・`set_fail_writes(kind)` で、同期フォルダが使えない・容量不足を再現する。自分が書いたファイルは、すぐ自分に見える。
- 一覧で、`.` で始まる名前・`desktop.ini`・`Thumbs.db` は無視する（衝突コピーとして数えない）。

### 3. DB の操作の切り方

- **接続を借りる口は 1 つ**: `SyncDb::with_conn(|conn| …)`。エンジンは、同期フォルダの読み書きの間は接続を持たず、DB に触る短い処理だけをこのクロージャに入れる。アプリ側は、`Database` の Mutex を借りる形で実装する（§11）。テストや単体の利用には、接続を 1 本 Mutex で持つ `ConnDb`。
- 書き込みロックを長く持たない: セグメント 1 ファイル（最大 1,000 Op）は 1 トランザクション。スナップショットの適用は 500 エンティティずつトランザクションを分ける（途中で止まっても、項目ごとの LWW なので状態は正しいまま。取り込み済みの範囲は最後に記録するので、次の回に最初から適用し直す）。
- コアの `sync_state`:
  - ライブラリ ID: `library_id`・`set_library_id`・`clear_library_id`。`join_library(tx, id, needs_snapshot)`（別のライブラリに切り替えるときは、取り込み済みの範囲などを消す）、`leave_library(tx)`。
  - 「スナップショットが必要」の印: `needs_snapshot`・`set_needs_snapshot`。**`backup::restore_backup` が立てる**。立てた印は、スナップショットを書いたあとに同期が下ろす。
  - 取り込み済みの範囲: `record_applied`、`applied_ranges`（`AppliedRanges`。`covers`・`contiguous`）、`is_range_applied`、`contiguous_applied`、`clear_applied`。
  - outbox: `pending_ops_after(after_seq, limit)`、`pending_count`、`mark_flushed(from, to)`、`mark_flushed_through(seq)`、`current_seq`、`outbox_hlc_ms(seq)`、`clear_outbox_restored`。
  - 状態: `export_state`（全エンティティ × `field_clocks` に時計のある項目の値と HLC）、`apply_entity_state`、`export_snapshot_basis`（HLC の採番・seq・各端末の反映済みの位置・状態・書影のハッシュを 1 つの書き込みトランザクションで読む）、`EntityState`・`SnapshotBasis`。
  - そのほか: `has_local_data`、`data_summary`、`referenced_cover_hashes`（完全削除していない本）、`all_cover_hashes`（完全削除した本も含む）、`snapshot_seen`・`mark_snapshot_seen`（他端末のスナップショットを読んだ印）、`last_snapshot`・`set_last_snapshot`（自分が最後に書いた位置）。
- 状態の書き出し: 行は ID 順、時計は (entity_id, field) 順に並べて読み進める（行ごとに引かない）。値は項目の型どおり（Bool は真偽値、Json は JSON、NULL は null）。書籍の `field_sources.<項目名>`（取得元の文字列か null）と `manual_fields.<項目名>`（`true` か null）は、保存形式（`field_sources_json`・`manual_fields_json`）から項目ごとの値に分けて出す。未知の項目は `extra_json` の値。**項目がすべて不正で時計のない行も、項目が空の行として出す**（行を作って、全端末の `dump_canonical` をそろえるため）。

### 4. 取り込み済みの範囲（`applied_segments`）

- 主キーは `(device_id, seq_from)`。同じ `seq_from` を記録し直すときは `seq_to` の大きい方を残す（自分のセグメントを統合した大きなファイルを、あとから読んだとき）。読むときは、重なる範囲・隣り合う範囲をまとめる。
- 記録するもの: (a) セグメントを適用したら、そのファイルの範囲。(b) スナップショットを適用したら、見出しの `covers` の各端末について `1..=seq`（自分の端末は除く）。
- 「ある範囲が覆われているか」は、まとめた範囲のどれか 1 つに含まれるか。覆われているセグメントは読まない。
- 「1 から切れ目なく適用した seq」は、1 から始まる最初の範囲の終わり（1 から始まっていなければ 0）。スナップショットの `covers` に使う。
- **記録しない場合**: 知らない `ent`・`v` の Op を含むセグメント（スナップショット）。知っている分だけ適用し、範囲は記録しない。アプリを更新したあとに、全部適用し直せる。代わりに、アプリを更新するまで毎回読み直す（読むのは、その種類のファイルだけ）。
- 同期を解除するとき、ライブラリを切り替えるときに全部消す。バックアップから復元した DB は、バックアップの時点の範囲を持ったまま。それでよい（その時点の状態と対になっているため）。

### 5. スナップショット

**書式（0013 §6 のとおり。細部）**

- gzip（既定の圧縮）した JSON Lines。1 行目は見出し `{"v":1,"kind":"snapshot","libraryId","dev","hlc","covers":{…}}`（キー順はこの順）。2 行目からエンティティごとに 1 行 `{"ent","id","fields":{"<項目名>":[<値>,"<HLC>"]}}`。最終行は終端レコード `{"end":true,"count":N,"sha256":"…"}`。`count` は終端レコードより前の行数（**見出しを含む**）、`sha256` は終端レコードより前の全行（`\n` を含む）。
- 読むとき、gzip として読めない・終端レコードがない・件数やハッシュが合わないものは、書き込み途中か同期途中（`Incomplete`）。完成していて壊れていないが使えないもの（`v` が新しい、種類が違う、見出しの `dev` がフォルダの端末 ID と違う）は `Unusable`（警告して飛ばす。取り込みは止めない）。HLC が読めない項目は、その項目だけ捨てる。知らない `ent` のエンティティは、数えて飛ばす。
- ファイル名は `<ms15>-<counter5>.jsonl.gz`（`:` を含まない）。**HLC は、書くたびに採番し直す**（`export_snapshot_basis` が `tick` して `local_meta` に保存する）ので、同じ端末のスナップショットの名前は重ならず、増えていく。時計が戻っても逆行しない。
- 展開後の大きさに上限（2GiB）を設ける。

**`covers`**: 見出しの `covers` は、自分の端末は現在の seq、他の端末は取り込み済みの範囲で「1 から切れ目なく適用した seq」（0 の端末は入れない）。

**書く契機**（`needs_snapshot` の印、または `export` の判定）

- (a) 新しいライブラリを作るとき（手元にデータがあれば）、(b) 統合のとき、(c) バックアップから復元した DB が同期を再開するとき（印は復元が立てる）。**状態を読んだ seq までの Op は `flushed = 1` にする**（セグメントには書かない）。復元で退避した `outbox_restored` は、スナップショットに含まれるので捨てる。
- (d) 前回のスナップショットから自分の Op が 2,000 件以上、または 7 日以上たって新しい Op がある（`export` が、セグメントを書いたあとに判定する）。前回の位置（`last_snapshot_seq`・`last_snapshot_ms`）がなければ、7 日の数え始めを初めての `export` にする。
- 自分のスナップショットは新しい 2 つだけ残す。
- 状態を読んだあとに作られた Op は `flushed = 0` のまま残り、次の書き出しでセグメントに書かれる。
- 書影は、スナップショットより先に同期フォルダへ写す。対象は**完全削除した本の書影も含む**全ての本の `cover_hash`（`all_cover_hashes`。理由は §5 の「修正した穴」）。

**適用**

- 他端末のスナップショットは、**端末ごとに最新のもの（完成していなければ 1 つ古いもの）を、1 回ずつ読んで適用する**。読んだ名前は `snapshot_seen`（端末 ID → ファイル名）に覚え、それより古い名前には戻らない。自分のスナップショットは読まない。別のライブラリのもの（見出しの `libraryId` が違う）は、覚えるだけで適用しない。
- 適用は、エンティティごとに、同じ HLC の項目をまとめた Op を作って `apply_op`。先に、状態に含まれる HLC（と見出しの HLC）の最大値を `observe_hlc` する。適用が済んだら、`covers` の範囲を取り込み済みとして記録し、読んだ名前を覚える。
- **手元の取り込み済みの範囲で覆われていても、適用する**（0013 の初案は、取り込み済みの範囲が何もないときの最初の取り込みだけ、最新を 1 つ読む、だった。§「0013 から変えた点」）。

**修正した穴（性質テストと筋書きのテストで見つかった）**

1. 書影の写し: スナップショットで写す書影を「完全削除していない本」の `cover_hash` だけにしていたため、ある端末が `purged = true` にした本を、別の端末の新しい `purged = false`（項目ごとの LWW）が取り消すと、書影が同期フォルダにないまま残った。完全削除した本の書影も写すことにした。届いていない書影の数・整理の対象は、これまでどおり完全削除していない本だけ。
2. スナップショットだけが運ぶ内容: 初案は、スナップショットの `covers` が、手元の取り込み済みの範囲で全部覆われているときは適用しない（「新しい内容がない」とみなす）ものだった。しかし、スナップショットには、`covers` が主張する Op では説明できない内容が入りうる。(i) バックアップから復元した端末の、`outbox_restored` の Op（古い端末 ID の Op で、セグメントに書かれていない）、(ii) ほかのライブラリから受け取った内容を持って、統合で参加した端末の内容。覆われているという判定では、これらが他の端末に届かない（復元・統合のテストで再現して確かめた）。そこで、読んでいない最新のスナップショットは、常に 1 回適用することにした。重ねて適用しても、項目ごとの LWW なので害はなく、費用は 5,000 冊で約 0.6 秒、1.4MB の読み込みが、スナップショットが書かれるたびに 1 回だけ。

### 6. 書き出し

- `export()` は、未書き出しの Op も、スナップショットの必要もなければ、同期フォルダに何も触れない（`library.json` も読まない）。
- 流れ: ライブラリの確認 →（印があれば）スナップショット → 未書き出しの Op を seq 順に最大 1,000 件ずつ読む → seq が連続している先頭の部分を 1 つのセグメントにする（欠番があればそこで切る）→ 参照する書影を先に写す → セグメントを書く → `flushed = 1` → …を未書き出しがなくなるまで → スナップショットの判定 → 何か書いたら `devices/<自分>.json` を書き直す。
- 重複した端末 ID の検出: 書こうとした範囲と重なる既存のファイルがあり、(a) 名前が同じで内容も同じなら、書かない（書いたあと `flushed` にする前に落ちた場合の再実行）、(b) それ以外は `DuplicateDevice` を返して止める。取り込みでも、自分のフォルダに、自分の seq より先のセグメントがあれば同じく止める（古い DB のコピーなど）。設定（参加）のときは、何も書く前に確かめる。
- outbox の Op の `dev` が自分でなければ、止める（`Internal`）。

### 7. 書影

- 書き出し側: 対象のハッシュを `covers/<xx>/` ごとに一覧して、ないものだけ、手元（`<covers_dir>/<xx>/<sha256>.jpg`）から読んで写す。手元の中身が名前のハッシュと合わなければ、警告して写さない。手元にないものは、警告して飛ばす（書き出しは止めない）。
- 取り込み側: 本が参照している書影（完全削除していない本）のうち手元にないものについて、同期フォルダの `covers/<xx>/` を一覧し、**実際にあるものだけ**を読む（まだ届いていない書影を何度も読みにいかない。届いていない書影があとの書影を後回しにしない）。中身の SHA-256 が名前と合わなければ（途中のファイル）使わず、次の回に確かめる。1 回の取り込みで写すのは 500 個まで。残りの数は `ImportReport.remaining_files` に足し、`import_all` が続けて取り込む。
- 手元への保存と同期フォルダへの書き込みは、どちらも一時ファイルに書いて `sync_all` してから名前を変える（内容がハッシュで決まる名前のファイルが、途中で壊れたまま残らないように）。1 個あたり約 4.5ms（`fsync` が支配的。§「性能」）。

### 8. 設定・判定・解除

- `inspect(library_folder)`: `library.json` を読み、`FolderKind` を決める。順は、更新が必要 > 別のライブラリ > 書き込めない > ライブラリあり／空。`library.json` がなければ、書き込めれば `Empty`、書き込めなければ `NotWritable`。何も残さない。端末の一覧は `devices/*.json`（読めないもの、端末 ID の書式でない名前を除く）。
- `setup(mode)`:
  - `Create`: フォルダにライブラリがあれば `LibraryExists`、この端末が別のライブラリに参加していれば `AlreadyInLibrary`。`library.json`（UUIDv7）を書き、**書いたあとに読み直して自分のものか確かめる**（同時に別の端末が作ったとき、あとから書いた方が残る。負けた側は何も記録せずに `LibraryExists`）。DB に記録し（手元にデータがあれば印を立てる）、データがあればスナップショットを書く。
  - `Join`／`JoinReplace`: 手元が空のとき、ライブラリ ID を記録して取り込む。手元にデータがあれば `LocalHasData`（同じライブラリへの再設定は除く）。`JoinReplace` は、アプリが手元の DB をバックアップして作り直した（新しい空の DB、新しい端末 ID）あとで呼ぶ。エンジンは `Join` と同じ手順を行う。
  - `JoinMerge`: ライブラリ ID と印を記録し、手元の状態をスナップショットとして書いてから取り込む。
  - この端末が既に同じライブラリに参加しているとき（フォルダの場所を変えた、解除せずに選び直した）は、取り込み済みの範囲・印を引き継ぎ、取り込み直す。別のライブラリに参加していれば `LibraryMismatch`（先に解除）。
  - 参加は、ライブラリ ID を記録したあとで取り込む。取り込みの途中で失敗しても、記録は残るので、次の `sync` が続きから行う。
  - `local_summary()` は、置き換えの確認の表示用（本・ゴミ箱の本・シリーズ・保管場所・タグ・本棚・未書き出しの Op の数）。
- 解除（`leave`、free 関数 `leave(db)`）: ライブラリ ID・取り込み済みの範囲・確かめたスナップショット・スナップショットを書いた位置・印を消す。手元のデータと同期フォルダの中身はそのまま。同期フォルダに触れないので、フォルダが使えなくても呼べる。解除したあと、同じライブラリに戻るときは、手元にデータがあるので `JoinMerge`（同じ ID の本は 2 冊にならない）。

### 9. 手入れ

- `cleanup_tmp()`: 自分の `ops/<自分>`・`snapshots/<自分>`・`devices/`・`covers/<xx>/` の `.tmp-*` のうち、更新から 24 時間以上たったものを消す（書き込み中のファイルを消さないため）。ほかの端末のフォルダは触らない。
- `consolidate_own_segments()`: 自分のセグメントのうち、書き出し済みで、最後の Op の HLC（outbox から引く。なければファイルの更新時刻）が 30 日より古いものを、seq が連続する範囲ごとに 1 つにまとめる（1 ファイル最大 10,000 Op）。まとめたファイルを書いてから、元のファイルを消す。2 つ以上のセグメントの組だけが対象。自分のファイルが壊れている・知らない Op を含むときは触らない。
- `gc_covers()`: どの本（完全削除していないもの）からも参照されておらず、ファイルの更新時刻が 30 日より前の書影を、手元と同期フォルダの両方から消す（FR-SYNC-13。手動の操作だけ）。同期フォルダのファイルの更新時刻が分からなければ消さない。他端末の変更を取り込んだあとに呼ぶこと。同期が設定されていなければ、手元だけ整理する。

### 10. エラーの扱い

- 取り込み・書き出しの関数は、次のときだけ `Err` を返す: 同期が設定されていない（`NotConfigured`）、ライブラリがない（`LibraryMissing`）、`library.json` が読めない（`BadLibraryFile`。途中までしか届いていないときなど。次の回にやり直す）、ライブラリ ID の不一致（`LibraryMismatch`）、新しいアプリが必要（`UpdateRequired`）、端末 ID の重複（`DuplicateDevice`）、同期フォルダが使えない・容量不足・書き込めない（`Transport`）、DB のエラー。`SyncError::state()` が、画面の `SyncState`（不一致は `mismatch`、更新は `updateRequired`、それ以外は `error`）にする。メッセージは日本語で、そのまま画面に出せる。
- ファイル 1 つの読み取りの失敗（読めない、一覧できない端末のフォルダ）、使えないスナップショットは、`ImportReport.errors` に入れて続ける。書影を写せなかったときも同じ。
- ローカルの操作は止めない。`Err` のとき、DB には、取り込み済みの範囲や `flushed` を記録しない（次の回にやり直せる）。

### 11. アプリへの組み込み（次の作業）

呼ぶ関数と順序:

1. **DB の口**: `SyncDb` を、`Database` の Mutex を同期的に借りる形で実装する（`Database::run` は async なので、同じ Mutex を借りる同期版を足す）。エンジンは `spawn_blocking`（別スレッド）の中で作って動かす。`SyncEngine` は借用だけを持つので、呼び出しごとに作ってよい。
   ```text
   SyncEngine::new(&db, &LocalFolder::new(root), &SystemClock, app_local_data_dir/covers,
                   DeviceInfo { name: 端末名, os: std::env::consts::OS, app_version: env!("CARGO_PKG_VERSION") })
   ```
2. **同時に 2 つ動かさない**: 取り込み・書き出し・設定・手入れは、アプリ側の 1 つのロック（`tokio::sync::Mutex` など）で順に実行する。
3. **設定画面**: `inspect_sync_folder(path)` → `LocalFolder::resolve_root(path)` で `bibcontainer-library` の場所を決め、`engine.inspect(その絶対パス)`。`setup_sync(path, mode)` → `engine.setup(mode)`。成功したあと、`local_settings` の `sync` セクションにフォルダを保存する（所属の根拠は DB のライブラリ ID）。`JoinReplace` は、先にアプリが DB を作り直す（`backup` で今の DB を残す → 新しい DB を作る。新しい端末 ID）。`leave_sync()` → `leave(&db)` と設定の削除。
4. **起動時**: フォルダが設定されていれば、`maintain()`（古い一時ファイルの掃除と統合。1 日 1 回ほどでよい）→ `sync(max_rounds)`。**バックアップから復元したあとも、すぐ `sync` を呼ぶ**（`needs_snapshot` の印が立っているので、先にスナップショットを書き、古い端末 ID のセグメントと他端末の変更を取り込み直す）。
5. **定期**: ウィンドウが前面に来たとき・ファイル監視（2 秒まとめる）・60 秒ごとに `import()`。`ImportReport.remaining_files > 0` なら、続けて `import()`。最後の変更から 5 秒後・終了時（最大 5 秒待つ）に `export()`。起動時に未書き出しがあれば `export()`。
6. **取り込みのあと**: `ImportReport.changed_book_ids` と `other_changed` を `library-changed` で知らせる。`clock_skew`・`missing_covers`・`errors` を状態に出す。
7. **状態（`SyncStatus`）**: `local_counters()`（ライブラリ ID・端末 ID・未書き出し・届いていない書影）に、アプリが持つフォルダのパス・最終書き出し・最終取り込み・直近のエラーを足す。`devices()` は同期フォルダを読むので、取り込みのあとに更新して覚えておく。エラーは `SyncError::to_string()`、状態は `SyncError::state()`。
8. **端末名の変更**: 設定に保存してから `write_device_file(None)`。
9. **書影の整理**: 取り込んだあとに `gc_covers()`（消した数は `local_removed + remote_removed`）。
10. 同期が設定されていないとき（ライブラリ ID がない）は、`import`・`export` は `NotConfigured` を返す。アプリは呼ばない。

### 12. 0013 から変えた点

- **スナップショットの適用**: 「取り込み済みの範囲が何もなければ、HLC が最新のものを 1 つ読む」を、「端末ごとに最新のものを 1 回ずつ、常に適用する」に変えた（§5）。
- **書影の写し**: スナップショットで写す書影に、完全削除した本の書影を含める（§5）。
- **スナップショットの名前の HLC**: 採番し直す（§5）。0013 は「HLC そのもの」とだけ書いていた。
- **`local_meta` のキー**: `library_id`・`needs_snapshot` に加えて、`snapshot_seen`・`last_snapshot_seq`・`last_snapshot_ms`。
- **セグメントの統合の古さ**: ファイルの更新時刻ではなく、最後の Op の HLC（outbox）で決める（Google Drive で更新時刻が変わってもよいように）。
- **取り込みの上限**: 書影の写しも 1 回の取り込みの上限に含め、`remaining_files` に足す。
- **`LocalFolder::resolve_root`**: 名前が `bibcontainer-library` のフォルダは、`library.json` がなくてもそのフォルダとする（0013 は `library.json` があれば）。
- **`SyncTransport::check_writable`** を足した。
- **取り込みでも**、自分のフォルダが自分の seq より先に進んでいれば、重複した端末 ID として止める（0013 は書き出しだけ）。
- **新しいライブラリを作るとき**、書いたあとに `library.json` を読み直して確かめる。
- 要件書の `(dev, seq)` で重複を除く（8.3）は、取り込み済みの範囲（`applied_segments`）による「覆われていれば読まない」で実現した（`applied_segments` は 0001 から用意されていた）。

### 13. テスト

- `crates/sync/tests/acceptance.rs`（メモリ上とローカルの両方）: `at_s1_ten_books_registered_on_a_appear_on_b`、`at_s2_different_fields_changed_offline_both_survive`、`at_s3_same_field_the_newer_hlc_wins_on_both`、`at_s4_deleted_stays_deleted_when_the_other_edits_later`、`at_s5_incomplete_segments_are_skipped_then_applied_when_complete`（終端なし・途中・件数・ハッシュ）、`at_s5_memory_transport_partial_hidden_and_conflict_copy`、`at_s6_new_device_joins_with_snapshot_and_remaining_segments`、`at_s6_join_without_any_snapshot_uses_segments`、`at_s8_other_librarys_folder_is_refused`。
- `crates/sync/tests/convergence.rs`（AT-S7。proptest）:
  - `at_s7_three_devices_converge_on_a_flaky_memory_folder`（120 件）: 3 台（A・B・C）が、ランダムな操作（本・シリーズ・保管場所・タグ・本棚の作成・変更・削除・復元、取得元・手修正の記録、書影）、書き出し・取り込み・同期・スナップショット・手入れをランダムな順で行い、新規作成・参加・統合・解除と再参加で途中参加し、時計が進む・戻る・30 日以上ずれる。ファイルが遅れて見える・途中まで見える・衝突コピーができる。知らない `ent`・`v`・項目を含む外部の端末のセグメントも混ぜる。最後に、全員を参加させ、遅れ・途中をなくして繰り返し同期し、`dump_canonical` が全端末で一致すること、未書き出しの Op がないこと、参照されている書影が全端末に届いていること、検索用の派生データが作り直しと一致することを確かめる。セグメントは 3 Op、取り込みは 3 ファイル、スナップショットは 6 Op、書影は 2 個ずつなど、上限を小さくして、分割・上限・繰り返しを起こしやすくしている。
  - `at_s7_three_devices_converge_on_a_local_folder`（24 件）: ローカルのフォルダ（衝突コピーあり）。
  - `at_s7_three_devices_converge_with_backups_and_restores`（30 件）: DB をファイルにして、バックアップと復元（新しい端末 ID、そのあとの変更の喪失）も混ぜる。
  - `a_fixed_scenario_with_merge_leave_and_clock_jumps_converges`: 統合・解除・時計のずれを含む決まった筋書き。
  - 最終のコードで、4 つとも 2,000 件ずつ実行した（release ビルド。約 3 分）。見つけた穴（§5）の 2 つ（復元・統合）は、修正前のコードで失敗することを確かめた。性質テストが実際に効いていることを、実装をわざと壊して（スナップショットの適用の条件、不完全なセグメントの扱い、書籍の手修正の記録の書き出し、復元の印）確かめた。
- `crates/sync/tests/engine.rs`（個別の機能。34 件）: フォルダの中身の判定（空・ライブラリあり・別のライブラリ・書き込めない・更新が必要）、新規作成（手元のデータをスナップショットで）、統合（両方残る・既に参加している端末にも届く）、置き換え、解除と再参加、同じライブラリへの再設定、書き出しの分割（1,000 Op）、書き出しの途中で落ちた場合、同じ端末 ID の検出（設定・書き出し）、取り込みの上限、衝突コピーと隠しファイルを読まない、時計のずれ（警告して適用・観測する・4 分は警告しない）、知らない `ent`・`v`、未知の項目と取得元の記録がスナップショットで運ばれる、スナップショットの契機と世代、途中のスナップショットから 1 つ前へ、使えないスナップショット、復元後の再開（新しい端末 ID・古い端末のセグメントの取り込み直し・書き出していない変更が届く）、前のライブラリの内容が統合で届く、完全削除した本の書影も写される、統合したセグメント、書影の整理、一時ファイルの掃除、書影のハッシュの確認と届いていない書影の数、同期フォルダが使えない・容量不足でもローカルの操作は止まらない、存在しない親のフォルダを作らない、`resolve_root`、状態の値。
- `crates/sync/src` の単体テスト（18 件）: ファイル名の規則、終端レコード（どこで切れても完成扱いにならない、件数・ハッシュ）、Op の分類（知らない・壊れている）、スナップショットの往復、バージョンの比較、ts-rs の出力。
- `crates/core/tests/sync_state.rs`（9 件）と `sync_state.rs` の単体テスト: ライブラリ ID・印・取り込み済みの範囲・outbox・状態の書き出しと適用（全エンティティ・時計・未知の項目・取得元の記録・時計のない行）・スナップショットの元・復元の印。
- 性能（`#[ignore]`。`cargo test --release -p bibcontainer-sync --test perf -- --ignored --nocapture`）: 5,000 冊（書影のある本 2,500 冊。エンティティ 5,100）、macOS（arm64）、ローカルの SSD。

| 項目 | 値 |
|---|---|
| 状態の書き出し（DB の読み出しだけ） | 約 80ms |
| スナップショットの書き出し（読み出し・JSON 化・gzip・書き込み。書影は写し済み） | 約 230ms |
| スナップショットのファイルの大きさ | 1.43MB（1,504,888 バイト。5,100 エンティティ。展開後は約 10MB） |
| 新しい端末が読み込む（書影がそろっている。読み込み・展開・適用） | 約 610ms |
| セグメントの書き出し（5,100 Op、6 ファイル、6.33MB） | 約 93ms |
| セグメントだけからの取り込み（新しい端末） | 約 480ms |
| 変化がないときの取り込み（一覧と確認だけ） | 約 2〜10ms |
| 書影 2,500 個（8KB）の写し（同期フォルダへ） | 約 10.8 秒（1 個 4.3ms。`fsync` が支配的） |
| 書影 2,500 個の取得（同期フォルダから） | 約 11.5 秒 |

## 理由

- 接続の借り方を `with_conn` の 1 つにしたのは、エンジンの側で「接続を持つのは、この短い処理の間だけ」を保ちやすく、アプリ側の実装が `Database::run` と同じ形（接続を借りてクロージャを実行）で済むため。DB の操作は `sync_state` の小さな関数に分けてあるので、アプリ側で個別に包む必要はない。
- スナップショットを毎回適用するのは、書き出していない Op を、状態だけが運ぶ場合があるため。取り込み済みの範囲で「新しい内容があるか」を判定する案は、復元と別ライブラリからの統合で、内容が届かなくなった。費用は小さく、安全側に倒した。
- スナップショットの HLC を採番し直すのは、同じ端末が続けて書いても名前が重ならず、「新しい順に読む」「確かめた名前より古いものは見ない」が成り立つため。
- 書影の取得で、同期フォルダを一覧してから読むのは、まだ届いていない書影を何度も読みにいって、遅い同期フォルダで時間を使わないため。
- セグメントの統合の古さを HLC で決めるのは、同期サービスがファイルの更新時刻を変えることがあるため。
- 取り込みで知らない `ent`・`v` の範囲を記録しないのは、アプリを更新したあとに、その Op を適用し直せるようにするため。

## 影響・今後

- **要件書の修正を提案する**（直接は直さない）:
  - 8.3 のスナップショットのファイル名を `<ミリ秒 15 桁>-<カウンタ 5 桁>.jsonl.gz` に（0013 と同じ）。見出しの `covers` と、書影を含めた受け渡しの説明を足す。
  - 8.3 の書影: FR-SYNC-04 の例外（全端末が書く。名前が内容の SHA-256 なので、同じ名前なら同じ内容）を書く。
  - 8.3 の「読む側は `(dev, seq)` で重複を除く」を、「取り込み済みの範囲で覆われているファイルは読まない」に。
  - FR-SYNC-13 の「作成から 30 日以上」は、実装では「ファイルの更新時刻が 30 日より前」。
- **未解決・注意**:
  - 同じ時刻に 2 台が別々に「新しいライブラリを作る」と、あとから書いた `library.json` が残る。負けた側は何も記録せずエラーになるが、勝った側が書く前に読み直していると、先に書いた側が不一致になる（次の取り込みで `mismatch` になる）。発生はまれ。
  - 知らない `ent`・`v` を含むファイルは、アプリを更新するまで、取り込みのたびに読み直す。`minAppVersion` を上げれば、古いアプリは `updateRequired` で止まる。
  - スナップショットの状態は、メモリに全部読み込む（5,000 冊でおよそ数十 MB。2 万冊では百数十 MB になりうる）。必要になったら、書き込みながら読む形にする。
  - 書影の写しは、1 個ごとに `fsync` するので、最初の同期で書影が多いと数十秒かかる（バックグラウンド）。1 個ずつの確認を保ったまま速くするなら、同期フォルダの側のファイルの大きさとの照合で済ませる案がある。
  - 復元では、バックアップのあとの変更のうち、書き出されなかったものは失われる（0013 のとおり、DB の修復の意味）。書き出されたものは、古い端末 ID のセグメントから戻る。
  - `outbox` の書き出し済みの行は、当面残す（0013 のとおり）。
  - Google Drive のストリーミングモードで、書影やセグメントの読み取りに時間がかかる場合のために、読み取りは別スレッドで行い、1 回の取り込みのファイル数に上限を設けてある。実機での確認は、組み込みのあと。
