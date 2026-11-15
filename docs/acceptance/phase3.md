# フェーズ3 受け入れ確認（機能拡張）

- 対象: 要件書 12 章フェーズ3。機能ごとに、実装の前に受け入れ基準の案を作って承認を得て、ここに足していく（`docs/decisions/0022` §3）
- これまでの機能: グリッド表示と巻をまとめた表示（2026-10-17。`docs/decisions/0024`）、作品の手動のまとめ・外し（2026-10-18。`docs/decisions/0026`）、キーワード検索（2026-10-18。`docs/decisions/0025`）、一括再取得（2026-11-14。`docs/decisions/0027`）、汎用 CSV 取り込み（2026-11-14。`docs/decisions/0028`）、PC カメラでの読み取り（2026-11-15。`docs/decisions/0029`）

## 1. 受け入れ基準

### グリッド表示・巻をまとめた表示（AT-30〜AT-35）

| ID | 内容 | 確認方法 | 状態 |
|---|---|---|---|
| AT-30 | グリッドへの切り替え・並べ替え・再起動 | `MainScreen.test.tsx`（グリッドに切り替えると、カードと並べ替えのメニューを出し、`layoutStore` に入る／保存してあった形で開く）、`SortMenu.test.tsx`、`settings.rs` と `settingsBridge.test.ts`・`persist.test.ts`（`list.viewMode` の保存と、壊れた値の扱い） | 自動で確認済み。実機の見た目は確認待ち |
| AT-31 | グリッドの操作が表と同じ | `BookGrid.test.tsx`（クリック・⌘+クリック・Shift+クリック、上下左右の矢印と短い最後の段、Shift+矢印、Home・End・PageUp・PageDown・⌘A・Esc、Enter・ダブルクリックで編集、表と同じ右クリックのメニュー、ゴミ箱のキー、選択行へのスクロール）、`gridLayout.test.ts` | 自動で確認済み。実機は確認待ち |
| AT-32 | 作品の判定と、畳んだ行の表示 | `works.test.ts`（巻次の外し方の全パターン、`坂の上`・`1Q84` などの外さない例、全角・半角）、`grouping.test.ts`（`スローループ` 1〜7 巻＋巻次なしの `スローループ 8` → 8 冊の 1 作品、巻の順、著者の違う同名の本は別、著者のない本の扱い）、`BookTable.test.tsx`（▶・書名・冊数・巻の範囲・最初の巻の書影）、`MainScreen.test.tsx`（「巻をまとめる」の保存） | 自動で確認済み。実機は確認待ち |
| AT-33 | 絞り込み・検索・並べ替えとの関係 | `grouping.test.ts`（1 冊だけ残れば普通の行、書名以外の並べ替えでも最初の巻の位置、作品の中は巻次の順）、`MainScreen.test.tsx`（件数は冊数のまま） | 自動で確認済み |
| AT-34 | 選択・キーでの開閉・検索欄の ISBN | `BookTable.test.tsx`（→・←・Enter・ダブルクリック・▶ のクリックでの開閉、作品の行のゴミ箱のキーで全巻の ID、作品の行では「編集」が使えない、`treegrid`）、`BookGrid.test.tsx`（作品のカード、札で開くと次の段から帯に並ぶ、Enter で開閉）、`MainScreen.test.tsx`（畳んだ作品の巻の ISBN で、作品が開いてその巻が選ばれる、作品の行を選ぶと選択数は冊数、巻に focus があるまま畳むと focus が作品の行へ移る） | 自動で確認済み。実機は確認待ち |
| AT-35 | 5,000 冊でのスクロールと、まとめる処理の速さ | `perf.test.ts`（2 万冊）と、2 章の計測（ブラウザのモックと、書影つきのダミー 5,000 冊のアプリ） | 確認済み（2.3 の判定。メモリの最大の数 MB の超過は許容） |

### 作品の手動のまとめ・外し（AT-36〜AT-40）

| ID | 内容 | 確認方法 | 状態 |
|---|---|---|---|
| AT-36 | 自動ではまとまらない本をまとめ、あとで同じ作品の巻を登録する | `MainScreen.works.test.tsx`（モックで、`スローループ` 1〜7 の作品の行と著者の違うアンソロジーを選んでまとめると 8 冊の 1 行になり、その行が畳まれたまま選ばれる。あとで `スローループ 8` を登録すると 9 冊）、`grouping.test.ts`「作品の手動の指定（0026）」（手動の作品の鍵と巻の順、巻次のない本は最後、上書きのない 1 冊が手動の作品に入る）、`ListContextMenu.test.tsx`（畳んだ作品の行で全巻を渡す）、`crates/core/tests/works.rs`（新しい UUIDv7 で対象に 1 回ずつ書く）、`ipc_tests.rs` の `merging_works_announces_exactly_the_changed_books` | 自動で確認済み。実機は確認待ち |
| AT-37 | 作品から外す・自動に戻す | `MainScreen.works.test.tsx`（外すと普通の行になって選ばれ、自動に戻すと作品に戻る。1 冊だけなら「まとめる」が押せない。オフのときは項目が出ない）、`ListContextMenu.test.tsx`（3 項目の表示と、押せる・押せない）、`grouping.test.ts`（`"none"` の本は書名と著者が同じでも入らない）、`works.rs` の `update_books_sets_none_a_valid_id_or_clears_the_work_without_provenance`・`update_books_rejects_a_malformed_work_id_and_writes_nothing` | 自動で確認済み。実機は確認待ち |
| AT-38 | 手動の作品の巻 1 冊と別の本をまとめる。絞り込みで巻を隠してまとめる。2 つの作品に分ける | `works.rs`（対象の手動の作品を使い続け、ほかの本だけ書く。2 つの手動の作品は冊数の多い方にそろえ、対象でない本・ゴミ箱の本も書き換える。同じ冊数なら小さい ID。完全削除した本は数えない）、`grouping.test.ts`（違う ID の手動の作品は混ざらない、2 つの手動の作品につながる自動の作品はどちらにも入らない、絞り込みで隠れたときのまとまり方、著者のない本はつながらない） | 自動で確認済み（分ける手順の通しは実機の M3） |
| AT-39 | 2 台での同期と、古い版の端末 | `convergence.rs`（収束の性質テストに `work_id` を足した）、`migrate.rs` の `upgrading_v3_to_v4_moves_a_received_work_id_from_extra_json_into_the_column`（古い版が `extra_json` に入れた値を列に移し、時計はそのまま。あとから届いた古い Op は負け、新しい Op は勝つ）・`a_corrupt_extra_json_does_not_stop_the_work_id_migration`。取り込んだ変更の通知は、どの項目でも本の ID を知らせる（0016） | 自動で確認済み。2 台の実機は確認待ち（M5・M6） |
| AT-40 | 書き出しと、まとめる処理の速さ | `export.rs` の `csv_has_the_manual_work_column_right_after_the_volume_sort_column`（列の位置と 3 つの値、数式になりうる値に `'`）、JSON は `json_export_values_can_be_replayed_as_ops`（項目の一覧から作るので `work_id` を含む）、`perf.test.ts`（2 万行のうち約 2% が手動の作品、約 1% が `"none"`。2.1） | 自動で確認済み |

### キーワード検索（AT-41〜AT-45）

| ID | 確かめ方 | 確認した場所 | 状態 |
|---|---|---|---|
| AT-41 | 「探す」を押して書名で探す。取得元ごとの欄、並び、Google の表示 | `KeywordSearchDialog.test.tsx`（欄の順（かな・漢字の有無）、取得元ごとに別々に届く、Google の順のまま・各行のリンク・powered by Google（検索の欄のそばと結果の見出し）、キーのない Google は呼ばずに知らせる、件数「全 X 件のうち N 件」）、`searchModel.test.ts`、`crates/metadata/tests/search.rs`（NDL の実際の応答: 「スローループ」11 件が新しい順、`sortBy` の句、ISBN の取り出し・重複、Google の順とリンク、Open Library の版）、`search_tests.rs`（検索は DB に書かない） | 自動で確認済み。Google・NDL・Open Library の実際の検索は live テストで確認済み（2026-10-18）。実機は確認待ち |
| AT-42 | ISBN のある結果を選ぶ | `BookEditorDialog.test.tsx`（`fetch_search_hit` に結果を渡し、ISBN が入り、書名・著者は置き換わり、手で直した出版社は残る。他の取得元の値を選び直せる）、`fetchApply.test.ts`、`search_tests.rs`（ふだんの取得と同じ結果、選んだ取得元が ISBN で見つからなければ検索結果の値を足し、`failed` にしない） | 自動で確認済み。実機は確認待ち |
| AT-43 | ISBN のない結果（古い版など）を選ぶ | `BookEditorDialog.test.tsx`（項目が入り、保存する下書きに取得状態が付かない。ISBN を入れてあれば変えずに取得待ち）、`search_tests.rs`（候補の値だけで、由来はその取得元。書影は取れれば保存し、取れなくても続ける） | 自動で確認済み。実機は確認待ち |
| AT-44 | 取得元の失敗・オフライン | `KeywordSearchDialog.test.tsx`（欄ごとの「通信できませんでした」「取得元がエラーを返しました（HTTP 503）」・エラーの文、ほかの欄は出る）、`search_tests.rs`（通信・HTTP・解析の失敗はエラーでなく状態、オフラインで選ぶと値が入ったまま `pending`）、`BookEditorDialog.test.tsx`（オフラインの知らせと `pending` での保存） | 自動で確認済み |
| AT-45 | 語の扱いとキー操作 | `crates/metadata/tests/search.rs`（`"`・`\` を除く、`AND`・`OR`・`NOT` の語、NFKC、空・200 文字を超える条件は通信しない）、`KeywordSearchDialog.test.tsx`（両方空なら探さない、Enter で探す、↑↓・Home・End・Enter・ダブルクリック・「入れる」、日本語入力の確定の Enter を無視、古い検索の応答を捨てる）、`BookEditorDialog.test.tsx`（Esc は検索だけを閉じる、検索の中の Enter・⌘Enter はフォームに取られない、外側のクリックで閉じない、リンクは `open_link` を呼んで行を選ばない）、`system.rs`（`open_link` の URL の検査） | 自動で確認済み。実機（WKWebView・WebView2 の日本語入力）は確認待ち |

### 一括再取得（AT-46〜AT-49）

| ID | 確かめ方 | 確認した場所 | 状態 |
|---|---|---|---|
| AT-46 | 表示中の本をまとめて再取得する。確認のダイアログ、裏で進むこと、進み具合、結果、手で直した項目 | `refetch_job_tests.rs`（`at46_start_returns_at_once_and_progress_is_reported_per_chunk`: すぐ `running` で戻り、1・10・10…冊ごとに `refetch-progress`、最後に `finished` と件数、手で直した項目は書かず `keptManualFields` に数える。`start_with_no_usable_network_provider_stops_at_once`、`an_empty_job_finishes_at_once`）、`MainScreen.test.tsx`（帯の「再取得…」がゴミ箱・0 冊・実行中は押せない、確定で表示中の本の ID を `start_refetch` に渡す）、`RefetchDialog.test.tsx`（ISBN のない冊数、10 冊以上の目安、300 冊以上の注意）、`StatusBar.test.tsx`（「N / M 冊」、棒、残りの時間）、`refetch.test.ts`・`refetchJob.test.tsx`（終わりの通知は作業ごとに 1 回、画面を開き直しても出し直さない、すぐ終わった作業も通知する） | 自動で確認済み。実機は確認待ち |
| AT-47 | 実行中にやめる。実行中の、ほかの再取得・自動再取得・登録の取得 | `refetch_job_tests.rs`（`at47_cancel_during_a_slow_fetch_stops_at_once_and_writes_nothing_for_that_chunk`、`at47_registration_fetch_and_pending_refetch_do_not_wait_for_the_job`、`a_job_waits_for_the_pending_refetch_that_holds_the_lock`、`cancel_while_waiting_for_the_lock_ends_the_job`、`cancel_without_a_job_does_nothing`。実行中の 2 つ目は `conflict`）、`StatusBar.test.tsx`（「やめる」で `cancel_refetch`）、`ListContextMenu`・`DetailPane` の再取得は実行中に押せない（`MainScreen.test.tsx`） | 自動で確認済み。実機は確認待ち |
| AT-48 | 取得元の制限・拒否、1 日の上限、通信が切れる。もう一度実行したときの順番 | `refetch_job_tests.rs`（`at48_a_refusal_stops_the_job_and_leaves_the_refused_books_unwritten`（2 回目は取れなかった本から取る）、`at48_google_daily_quota_stops_the_job_with_its_message`、`at48_going_offline_stops_the_job_without_writing_that_chunk`、`the_pending_auto_refetch_still_writes_what_it_got_on_a_refusal`）、`refetch_job.rs`（`books_are_ordered_by_the_oldest_cache_row_with_missing_rows_first`、`ties_keep_the_given_order`）、`crates/metadata/tests/bulk_batch.rs`（制限を返した取得元に残りを問い合わせない: NDL の 429、Google の検索の 403・volume の 429、Open Library の 429、Dynamic Links の 429 では検索は続ける）、`refetch.test.ts`（止まった理由の文、1 日の上限の見分け、閉じるまで残る） | 自動で確認済み |
| AT-49 | 洋書・和書 20 冊をまとめて取る（偽の通信） | `crates/metadata/tests/bulk_batch.rs`（`at49_twenty_isbns_are_batched_and_give_the_same_results_as_one_by_one`: Open Library の検索 2 回・Dynamic Links 2 回、結果とキャッシュは 1 冊ずつと同じ。実際の応答のフィクスチャでの ISBN ごとの分け方、`limit` で切れたときの取り直し、1 冊は今の流れ）。実際の通信: `crates/metadata/tests/live.rs` の `live_open_library_batch_search_equals_single_searches`・`live_google_dynamic_links_batch_equals_single_lookups` | 自動で確認済み。live テストは 2026-11-14 に通った |

### 汎用 CSV 取り込み（AT-50〜AT-57）

| ID | 確かめ方 | 確認した場所 | 状態 |
|---|---|---|---|
| AT-50 | UTF-8（BOM の有無とも）と Shift_JIS（漢字・かな・①・髙）を開く。判定を手で変える | `csv_import/tests.rs`（`decodes_utf8_with_and_without_bom`、`decodes_shift_jis_including_extended_characters`、`auto_detection_errors_on_bytes_that_are_neither_utf8_nor_shift_jis`、`forced_encoding_is_used_even_if_the_bytes_do_not_fit`、`preview_reports_encoding_and_whether_it_was_detected`）、`tests/csv_import.rs`（`imports_shift_jis_files`）、`CsvImportDialog.test.tsx`（文字コードを選ぶと、その文字コードで開き直す） | 自動で確認済み。実機は確認待ち |
| AT-51 | 書き出しの列名・別名の見出しの CSV を開き、対応を変える。「1 行目は見出し」を外す | `csv_import/tests.rs`（`suggests_every_header_of_the_exporter`、`suggests_aliases_ignoring_width_case_and_spaces`、`rejects_a_target_mapped_twice_or_a_column_out_of_range`、`without_header_the_first_record_is_data`）、`model.test.ts`（1 つの項目は 1 列だけ、全項目に名前と分類がちょうど 1 つ）、`CsvImportDialog.test.tsx`（対応の案と例、見出しを外すと「1 列目」、ISBN も書名もないと「次へ」が押せない） | 自動で確認済み。実機は確認待ち |
| AT-52 | 確認の画面の件数・理由・作る保管場所とシリーズ。取り込むまで DB が変わらない | `tests/csv_import.rs`（`plan_does_not_write_to_the_database`、`classifies_rows_in_order_with_every_skip_kind`、`id_column_is_only_for_duplicate_detection_and_never_written`）、`ipc_tests.rs`（`csv_import_commands_open_check_run_and_save_rejected`: 確認では書かず、通知も出ない）、`CsvImportDialog.test.tsx`（件数（0 の内訳は出さない）、理由の全種類と Excel の注意、絞り込み、500 行で切る、0 冊なら取り込めない、「戻る」で対応が残る） | 自動で確認済み。実機は確認待ち |
| AT-53 | ISBN のある行を取り込む。取得待ちの再取得のあと、手動の再取得、上書きを選んだ再取得 | `tests/csv_import.rs`（`execute_writes_import_origin_pending_status_ops_and_created_at`: 由来が `import`、ISBN のある本だけ `pending`、Op）、`metadata/tests.rs`（`a_csv_imported_book_keeps_its_values_when_the_pending_refetch_fills_the_rest`: CSV の書名・本体価格は残り、出版社は取得元から入り `ok` に。手動の再取得でも残り、上書きを選ぶと置き換わる、`trigger_now_ignores_the_minimum_gap`）、`refetch.rs`（`plan_keeps_values_imported_from_csv_unless_override_is_chosen`: 書名・巻次と並べ替え用の数値・シリーズ・Cコード）。オフラインのときは、取得待ちの本の扱い（AT-11、0010 §9）のまま | 自動で確認済み。書影が入ることと、オフラインで取り込んで復帰したあとは実機で確認待ち |
| AT-54 | 著者・タグ・保管場所・シリーズ・列挙値・日付・金額・評価を取り込む | `csv_import/tests.rs`（`authors_split_on_semicolons_and_japanese_commas_not_ascii_commas`、`tags_split_on_all_separators`、`location_is_split_on_gt_with_nfkc_and_trimmed`、`enums_accept_keys_and_japanese_labels`、`dates_accept_many_forms_and_zero_pad`、`integers_accept_yen_commas_and_page_suffixes`、`rating_accepts_1_to_5_and_treats_zero_as_empty`、`volume_sort_work_id_and_id`、`trashed_column_values`、`created_at_formats`）、`tests/csv_import.rs`（`resolves_locations_by_path_unique_leaf_or_creates_the_missing_tail`、`series_are_matched_by_normalized_name_and_new_ones_are_deduplicated`） | 自動で確認済み |
| AT-55 | アプリの書き出しを空の蔵書に取り込み、書き出し直す。同じ蔵書に取り込む | `tests/csv_import.rs`（`export_then_import_round_trips_into_an_empty_library`: ID・書影・取得状態の列を除いて一致（登録日時も）、`importing_an_export_into_its_own_library_skips_every_row`） | 自動で確認済み。実機は確認待ち |
| AT-56 | 取り込めなかった行を保存し、直して取り込む | `tests/csv_import.rs`（`rejected_rows_file_has_header_reason_column_bom_and_crlf_and_can_be_fixed_and_reimported`、`rejected_rows_without_header_have_no_header_row`）、`ipc_tests.rs`（保存）、`CsvImportDialog.test.tsx`（既定の名前「<元の名前>-取り込めなかった行.csv」、取り消し、エラー、エラーがなければボタンを出さない） | 自動で確認済み。実機は確認待ち |
| AT-57 | 5,000 行の確認と取り込みの速さ。途中で失敗させる。20 MB を超えるファイル | `tests/csv_import.rs`（`perf_5000_rows_plan_and_execute`（`#[ignore]`。2.4）、`a_failure_in_the_middle_rolls_everything_back`: 本・保管場所・シリーズのどれも残らない）、`ipc_tests.rs`（`csv_import_rejects_files_over_the_size_limit`）、`csv_import/tests.rs`（`rejects_input_over_the_size_limit`） | 自動で確認済み |

### カメラ読み取り（0029。AT-58〜AT-62）

| AT | 確かめ方 | 自動テスト | 状態 |
|---|---|---|---|
| AT-58 | 「カメラで読む」をオンにして本の上段のバーコードを映す → 映像が出て ISBN が読まれ、リーダーと同じに取得が始まる。外部（CDN）への通信はしない | `CameraPanel.test.tsx`（オンにすると `getUserMedia`（音声なし）を呼び、読めた値を `onDigits` に渡す、開いたときはオフ）。`pnpm build` で wasm が `dist/assets/zxing_reader-*.wasm` に出ること、`locateFile` の上書き（0029 §2）。node で、ISBN の EAN-13 を zxing-wasm の `readBarcodes`（`formats: ["EAN13"]`）が読むこと | 自動で確認済み（読み取りは差し替え）。実機の映像と通信なしは確認待ち（V1・V2） |
| AT-59 | 上段と下段を続けて、または同時に映す → 1 冊として組になり、C コードと価格が入る | `dedupe.test.ts`（2 段同時は ISBN → 192 の順、192 が先に採用されても渡す）、`CameraPanel.test.tsx`（ISBN → 2段目の順に渡す）。組み合わせはリーダーと同じ `usePairing`（AT-05〜09）。node で、ISBN と 192 の 2 本を縦に並べた画像から 2 本とも読めること | 自動で確認済み。実機の映像は確認待ち（V3） |
| AT-60 | 同じ本を映したままにする。いったん外してもう一度映す → 映したままでは 1 回だけ読まれ音も 1 回。外してから映すともう 1 回読まれる | `dedupe.test.ts`（連続 2 回で採用、映したままは 1 回、消えて 1.5 秒後は再び渡す、1.5 秒未満は渡さない、間に読めない回があれば連続とは数えない）、`CameraPanel.test.tsx`（映したままでも 1 回） | 自動で確認済み。実機は確認待ち（V4） |
| AT-61 | 登録フォームや確認ダイアログを開いたまま映す。画面を離れる。ウィンドウを隠す → 開いている間は読まれず、閉じた直後に前のコードが入らない。離れる・隠すとカメラが止まる | `dedupe.test.ts`（止めている間に読んだ値は再開直後に入らない、止めている間に消えた本は再開後に映せば渡す）、`CameraPanel.test.tsx`（`listening` が偽・モーダルを開いている間は渡さない、閉じても入らない、オフ・アンマウント・`visibilitychange` で `track.stop`、見えるようになれば開き直す、アンマウントのあとで届いた `getUserMedia` はすぐ stop） | 自動で確認済み。実機は確認待ち（V5・V6） |
| AT-62 | カメラの許可を断る。カメラのない端末で開く。雑誌や食品のバーコードを映す → 理由と直し方が出て、リーダーと手入力は使える。書籍以外はリーダーと同じ扱い（EAN-13 以外は読まない） | `CameraPanel.test.tsx`（許可を断られた・カメラがない・使用中・安全な接続でない・wasm の読み込みの失敗の文、覚えていたカメラがなければ既定でやり直す）、`dedupe.test.ts`（13 桁の数字以外は渡さない）。雑誌・食品の判定はリーダーと同じ `classify_code`（AT-05〜09）。EAN-13 以外は `formats: ["EAN13"]` で読まない | 自動で確認済み。実機の許可のダイアログと、実物の雑誌・食品は確認待ち（V7・V8） |

## 2. 性能

### 2.1 まとめる処理（`src/features/library/perf.test.ts`、2 万冊、7 回の最速。開発機の Mac）

| 処理 | 時間 |
|---|---|
| まとめる（行ごとの作品の情報が計算済み。絞り込みを変えたとき） | 8.9ms |
| まとめる（全行が新しい。初めてまとめたとき・全件を読み直したとき） | 24.2ms |
| 開いている作品だけを変える（`layoutEntries`） | 0.1ms |

2026-10-18（0026 のあと。2 万行のうち約 2% が手動の作品、約 1% が `"none"`）: 計算済み 8.0ms、全行が新しい 25ms、上書きのない行だけ 6.5ms、`layoutEntries` 0.1ms。上書きのない行だけのときは、0024 と同じ処理の道筋（手動の作品の表を作らない）。

### 2.2 スクロール（ブラウザのモック。2026-10-17）

- ヘッドレスの Chrome 155、1440 × 900、倍率 1。Vite の開発ビルド（React は開発版なので、アプリより遅めに出る）。書影は CDP で同じ画像を返して、`<img>` が DOM に残る状態にした。
- 一覧のスクロール欄を、1 フレームに 8・60・200px ずつ JS で動かした（5,000 冊と 2 万冊、各 2 回）。

| 場面 | DOM の行・カードの最大 | DOM の `<img>` の最大（5,000 / 2 万冊） | 1 フレームあたりのメインスレッドの時間（8 / 60 / 200px） |
|---|---|---|---|
| 表、まとめない | 42 | 16 / 20 | 2.2〜2.8 / 4.1〜5.3 / 5.0〜6.1ms |
| 表、まとめる（すべて畳む） | 42 | 17 / 20 | 2.2〜3.4 / 4.4〜4.7 / 5.0〜5.7ms |
| 表、まとめる（すべて開く） | 42 | 17 / 18 | 2.3〜3.2 / 4.3〜4.7 / 5.2〜5.8ms |
| グリッド、まとめない | 55 | 19 / 24 | 0.5〜0.9 / 2.1〜3.0 / 3.3〜3.8ms |
| グリッド、まとめる（20 作品を開く） | 42〜55 | 19 / 24 | 0.5〜0.6 / 2.0〜2.8 / 3.5〜4.0ms |

- どの場面でも、フレームの間隔は 16.7ms（60Hz）のままで、50ms を超えるフレームと長いタスク（50ms 以上）はなかった。CPU を 4 倍遅くしても、グリッドは 50ms を超えるフレームがなかった。表は 200px/フレームのときだけ、まとめる・まとめないに関係なく 2,000 フレームのうち 16〜25 回が 50ms を超えた（まとめた表示で悪くなったのではない）。
- 「巻をまとめる」の切り替えは、5,000 冊で 48〜72ms（長いタスク 1 回、最大 62ms）、2 万冊で 98〜133ms（長いタスク 1 回、最大 120ms）。初めてまとめるときに、全行の作品の情報を計算するため。表とグリッドの切り替えは 46〜88ms。どれも 1 回の操作のあとに 1 度だけで、スクロール中には起きない。

### 2.3 スクロールとメモリ（アプリ、書影つきのダミー 5,000 冊。2026-10-17）

**環境と測り方**

- Mac mini（M4、24GB）、macOS 27、内蔵 SSD。
- 画面は 60Hz、倍率 1（Retina ではない）。ウィンドウは CSS で 1280 × 768 で、詳細ペインを開いている。グリッドは 4 列。
- 計測したもの: `a52f398` のリリースビルドに、計測用のコード（自動のスクロール、フレームとタイマーの記録、物理フットプリントの 1 秒ごとの記録）を足したコピー（scratchpad）。計測用のコードは、リポジトリのソースには入れていない。
- 画面はロックされていた。そのため、フェーズ 1 の 3.5 と同じく、計測用のコピーにだけ WKWebView の非公開 API でウィンドウの遮蔽の判定をやめさせた。全計測の始めと終わりに `visibilityState` が `visible` で、60fps で動いていることを確かめた。
- ダミー: `dummy-db … 5000 5000 --works`。一覧は 4,944 行（ゴミ箱の 56 冊を除く）で、そのうち書影のある本は 2,823 冊、作品は 150（巻は 1,500 冊）。
- サムネイルは、先に 1 周して作っておいた。サムネイルがまだない初回のグリッドも 1 回だけ流し、フレームの間隔は最大 23ms だった。
- スクロールは、1 フレームに 8px（先頭から 6 秒）、60px と 200px（どちらも一番下まで）で、各 3 回。

**スクロール**

| 場面 | フレームの間隔（99% 点 / 最大） | 50ms を超えたフレーム | タイマーの遅れの最大 | DOM の `<img>` の最大 |
|---|---|---|---|---|
| 表、まとめない | 19 / 20〜24ms | 0 | 13〜16ms | 35〜39 |
| 表、まとめる（すべて畳む） | 19 / 19〜23ms | 0 | 13〜18ms | 35〜39 |
| 表、まとめる（すべて開く） | 19 / 20〜22ms | 0 | 13〜15ms | 35〜39 |
| グリッド、まとめない | 19 / 20〜24ms | 0 | 9〜13ms | 34〜40 |
| グリッド、まとめる（約 20 作品を開く） | 19〜21 / 19〜24ms | 0 | 10〜13ms | 35〜40 |

- 中央値はどれも 17ms（60Hz）。8 万フレームを超えるうち、25ms を超えたフレームは 0。50ms 以上のタイマーの遅れも 0。
- 「巻をまとめる」の切り替えと、表とグリッドの切り替え: 操作から描画までは、中央値約 20ms、最大 50ms。その直後のフレームの間隔は、最大 57ms（約 3 フレーム）。

**メモリ（物理フットプリントの合計。アプリ本体 + WebKit の GPU・WebContent・Networking。MB。中央値と範囲、各 3 回）**

| | 書影 2,500 枚のあと、落ち着いた値 | 同、最大 | 全冊（2,823 枚）のあと、落ち着いた値 | 同、最大 | 計測全体の最大 |
|---|---|---|---|---|---|
| グリッド（前後に余分に描く段: 3） | 205（191〜211） | 292（281〜294） | 186（177〜193） | 293（278〜302） | 293〜305 |
| 表（同じ条件で） | 193（182〜196） | 270（267〜276） | 190（181〜192） | 267（251〜270） | 280〜286 |

- 表は、フェーズ 1 の 3.5（落ち着いた値 190〜191MB）と同じ水準。
- グリッドの落ち着いた値は表と同じくらい。ただし、描画中の最大が表より 15〜25MB 高く、3 回のうち 2 回で 300MB を超えた（302MB と 305MB）。最大のときの内訳は、本体 40〜50MB、GPU 79〜82MB、WebContent 164〜170MB、Networking 6MB。
- 前後に余分に描く段を減らす案（1・2 段）も測りかけたが、計測のウィンドウが作業の邪魔になるため、途中でやめた。**数 MB の超過は許容する**（2026-10-17 にユーザーが判断）。余分に描く段は 3 のまま。

**AT-35 の判定**: スクロールは、どの場面でも引っかからない（10.1 の 60fps の目安を満たす）。DOM の書影は、見えている段と前後の数段だけ（グリッドで最大 40 枚）。まとめる処理は 2 万冊で 50ms 未満（手元の Mac。最初の読み込みの 1 回だけは、CI の機械で最大 100ms。テストの上限は 300ms。0024）。メモリは、グリッドで最大が 300MB を数 MB 超えることがある（上のとおり許容）。

### 2.4 CSV 取り込み（`crates/core/tests/csv_import.rs` の `perf_5000_rows_plan_and_execute`。2026-11-14、開発機の Mac、release）

5,000 行の CSV（既存 1,000 冊のファイルの DB、うち 500 行が登録済みの ISBN）。

| 処理 | 目標 | 結果 |
|---|---|---|
| 確認（`plan`） | 2 秒未満 | 約 13ms |
| 取り込み（`execute`。4,500 冊・保管場所 40・シリーズ 100 を作る） | 10 秒未満 | 約 0.9 秒 |

```fish
cargo test --release -p bibcontainer-core --test csv_import -- --ignored --nocapture
```

## 3. 実機確認チェックリスト（グリッド表示・巻をまとめた表示・作品の手動のまとめ・キーワード検索・一括再取得・CSV 取り込み・カメラ読み取り）

準備: 本物の蔵書で見てよい（表示を切り替えるだけで、データは変えない。ただし、右クリックの一括変更とゴミ箱は本物のデータを変える）。ダミーの蔵書で試すなら、`docs/acceptance/phase1.md` 5 章の手順で、`--works` を付けて作る。

```fish
cargo build --release --manifest-path docs/acceptance/dummy-db/Cargo.toml
set DUMMY_HOME (mktemp -d)
set DATA_DIR "$DUMMY_HOME/Library/Application Support/io.github.mioupa.bibcontainer"
docs/acceptance/dummy-db/target/release/dummy-db $DATA_DIR 5000 5000 --works
env HOME=$DUMMY_HOME pnpm tauri dev
```

### グリッド表示

- [ ] G1. 一覧の上の帯の左の切り替えで、表とグリッドが入れ替わる。グリッドの書影がぼけない（Retina の画面）。
- [ ] G2. ウィンドウの幅を変えると、列の数が変わる。サイドバーと詳細ペインの開閉でも並び直す。
- [ ] G3. 設定の「表示」で文字サイズを変えると、カードも大きさが変わり、重なったり切れたりしない。
- [ ] G4. クリック、⌘+クリック、Shift+クリック、矢印（上下左右）、Shift+矢印、Enter、ダブルクリック、右クリックのメニュー、⌘⌫ が、表と同じに効く。選んだ本が詳細ペインに出る。
- [ ] G5. 帯の「並べ替え」で並びが変わる。表に戻っても同じ並べ替えになっている。
- [ ] G6. 速くスクロールしても引っかからない。書影の出遅れ（サムネイルを毎回読み直すようにした）が気にならない。
- [ ] G7. 詳細ペインの書影が、本を選び直したときにちらつかない（詳細ペインの書影も、毎回読み直すようにした）。
- [ ] G8. アプリを終了して起動し直しても、グリッドのまま。
- [ ] G9. ライト・ダークのどちらでも、選んだカードと focus の輪が見える。

### 巻をまとめた表示

- [ ] W1. 帯の「巻をまとめる」で、同じ作品の巻（漫画など）が「▶ 書名　N 冊」の 1 行になる。巻次の列に範囲（1〜7）、最初の巻の書影が出る。起動し直しても、まとめたまま。
- [ ] W2. ▶ のクリック、→・←、Enter、ダブルクリックで開閉する。開いた巻は字下げされ、巻次の順に並ぶ。
- [ ] W3. 本物の蔵書で、まとまるべき作品がまとまり、別の作品がまとまっていない。おかしな例があれば、書名・巻次・著者を教えてほしい（判定の規則は 0024 §4・§5）。
- [ ] W4. 作品の行を選び、右クリックの一括変更（読書状況など）で、全巻が変わる。ステータスバーの選択数が冊数になる。
- [ ] W5. 検索欄に、畳んだ作品の巻の ISBN を入れて Enter を押すと、作品が開いて、その巻が選ばれる。
- [ ] W6. サイドバーの絞り込みや検索で巻が減ると、冊数が減る。1 冊だけになると普通の行になる。
- [ ] W7. グリッドでも作品が 1 枚（重ねた書影と冊数の札）にまとまり、札のクリック・Enter・ダブルクリックで開くと、巻が次の段から帯の中に並ぶ。

### 作品の手動のまとめ・外し（0026）

準備: まとめる・外す・自動に戻すは、本物のデータ（books の `work_id`）を変え、同期で他の端末にも届く。どれも「自動に戻す」で元に戻せる。気になるなら、上のダミーの蔵書で試す。「巻をまとめる」をオンにしておく。

- [ ] M1. 自動ではまとまらない本（特装版・アンソロジー・書名の表記が違う巻など）を、作品の行と一緒に選び（⌘+クリック）、右クリックの「1 つの作品にまとめる」で 1 行になる。その行が畳まれたまま選ばれている。グリッドでも 1 枚にまとまる。
- [ ] M2. 作品を開いて巻を 1 冊選び、「作品から外す」で普通の行になる。その本を選んで「自動に戻す」で、作品に戻る。外した本では「作品から外す」が、上書きのない本では「自動に戻す」が押せない。
- [ ] M3. 自動で 1 つにまとまる作品を 2 つに分ける: 片方の巻を選んで「作品から外す」→ その巻を選んで「1 つの作品にまとめる」→ 残りの巻（作品の行）を選んで「1 つの作品にまとめる」。2 つの行になる。
- [ ] M4. まとめた作品と書名・著者が同じ巻を新しく登録すると、その作品に入る。
- [ ] M5. Mac でまとめた・外した結果が、同期のあと Windows でも同じに見える（逆も）。
- [ ] M6. 同期の前に、2 台で重なる本を別々にまとめる（片方の端末をネットワークから外すなどして）。同期のあと、両方の端末で同じ形になる（重なった本はあとでまとめた方の作品に入る）。もう一度まとめれば 1 つになる。
- [ ] M7. 「巻をまとめる」をオフにすると、右クリックに 3 つの項目が出ない。表示中の本を CSV に書き出すと、`作品（手動）` の列に、`まとめない` や作品の ID が出る。

### キーワード検索（0025）

準備: 探すだけならデータは変わらない。結果を選ぶとフォームに入るだけで、「登録」を押すまで本は増えない。Google を試すには、設定の「取得元」で API キーを入れておく。

- [ ] K1. メイン画面の「手動登録」（⌘N）で、書名の欄の右に「探す」がある。編集のフォームと、スキャン登録の行の手直しには出ない。
- [ ] K2. 書名に「スローループ」と入れて「探す」を押すと、すぐ探し始め、NDL・Google・Open Library の欄が別々に埋まる。NDL は新しい巻から並ぶ。Google の欄には「powered by Google」の画像と、各行に「Google Books で見る」がある（検索の欄の右下にも同じ画像）。
- [ ] K3. 英語の書名（例: `clean code`、著者 `martin`）で探すと、Google → Open Library → NDL の順に欄が並ぶ。
- [ ] K4. 行の「NDL サーチで見る」「Google Books で見る」「Open Library で見る」で、既定のブラウザにその本のページが開く。
- [ ] K5. ISBN のある行を選んで「入れる」（またはダブルクリック、Enter）。ISBN が入り、書影・出版社などが入る。書名・著者は選んだ本の値になる。先に手で入れておいた出版社などは残る。「登録」で保存した本の詳細の取得元が、取得元の名前になる。
- [ ] K6. 「こころ」「夏目漱石」で探し、「ISBN なし」の古い版を選ぶ。書名・出版社・発行日が入り、ISBN は空のまま。保存できる。
- [ ] K7. 日本語入力をオンにしたまま、書名の欄で変換を確定する Enter を押しても、探し始めない（確定だけ）。もう一度 Enter で探す。↑↓ で行を選び、Enter で入れる。Esc で検索だけが閉じ、登録のフォームは残る。
- [ ] K8. 「このアプリについて」のクレジットの URL を押すと、既定のブラウザで開く。
- [ ] K9. Wi-Fi を切って探すと、各欄に「通信できませんでした」が出る（アプリは固まらない）。

### CSV 取り込み（0028）

準備: 取り込みは本物のデータに本・保管場所・シリーズを足し、同期で他の端末にも届く。元に戻す操作はない（消すならゴミ箱へ）。上のダミーの蔵書（`env HOME=$DUMMY_HOME pnpm tauri dev`）か、空の蔵書で試すとよい。試す CSV は、アプリの書き出し（設定の「バックアップ」→「書き出し」→ CSV）と、Excel・Numbers で作ったもの（Excel は「CSV UTF-8」と「CSV（コンマ区切り）」＝ Shift_JIS の両方）。

- [ ] C1. 設定の「バックアップ」に「取り込み」があり、「CSV から取り込む…」でファイルを選ぶとダイアログが開く。
- [ ] C2. Excel で保存した Shift_JIS の CSV と UTF-8 の CSV で、見出しと例が化けない。文字コードの欄が「自動（Shift_JIS）」などになる。わざと違う文字コードを選ぶと化け、戻すと直る。
- [ ] C3. アプリの書き出しを開くと、見出しから項目が自動で選ばれる（書影・取得状態は「取り込まない」）。列の項目を変えると、同じ項目だった列が「取り込まない」に戻る。
- [ ] C4. 「次へ」で確認の画面になり、取り込む・飛ばす・取り込めない・値を読めなかった件数、作る保管場所とシリーズ、行番号つきの理由が出る。ここで閉じても、蔵書は変わっていない。
- [ ] C5. 書き出しを同じ蔵書に取り込むと、全行が「飛ばす」（登録済みの ISBN・同じ ID の本・ゴミ箱の行）になり、取り込めない。
- [ ] C6. ISBN のある本を取り込むと、すぐ一覧に出て「取得待ち」になり、しばらくすると書影と、CSV になかった項目（出版社など）が入る。CSV に書いた書名は変わらない。その本で「書誌を再取得」しても書名は残り、「手で直した項目・取り込んだ項目も上書きする」をオンにすると取得元の書名になる。
- [ ] C7. Wi-Fi を切って取り込むと、取得待ちのまま登録される。Wi-Fi を戻すと（ウィンドウを前面にするか、5 分以内に）書誌と書影が入る。
- [ ] C8. ISBN 列に `9.78478E+12` のような値（Excel で数値になった ISBN）や、書名も ISBN もない行を混ぜると「取り込めない」になる。結果の画面の「取り込めなかった行を CSV に保存…」で保存したファイルを Excel で開くと、元の列と「取り込めなかった理由」が並ぶ。直して取り込むと、その行だけが入る。
- [ ] C9. 保管場所 `自宅 > 書斎 > 2段目` のような値で、ない場所が親から作られる。登録日時の列のある CSV では、取り込んだ本の登録日が CSV の値になる（一覧を登録日で並べると、その日時の位置に入る）。

### 一括再取得（0027）

準備: 再取得は本物のデータ（取得元由来の項目）を書き換え、同期で他の端末にも届く。手で直した項目は書き換えない。気になるなら、上のダミーの蔵書で試す（ダミーの本の ISBN は取得元に問い合わせるので、通信は起きる）。

- [ ] R1. サイドバーの「情報不足」（または「取得待ち」）を選び、一覧の上の帯の「再取得…」を押す。確認のダイアログに冊数と、ISBN のない冊数・時間の目安（10 冊以上のとき）が出る。「再取得」で閉じ、ステータスバーに「書誌を再取得中 N / M 冊・残り約 X 分」と細い棒、「やめる」が出る。
- [ ] R2. 動いている間も、本を選ぶ・詳細を見る・編集する・スキャン登録の画面に行って戻る、ができる。戻ったときも進み具合が出ている。帯の「再取得…」、右クリックの「書誌を再取得…」、詳細ペインの「書誌を再取得」は押せない。
- [ ] R3. 終わると通知に結果（更新・変化なし・飛ばした冊数など）が出る。書き換わった本の一覧の行・詳細が、開き直さなくても新しい値になる。手で直した項目（書名など）は変わっていない。
- [ ] R4. もう一度始めて、すぐ「やめる」を押す。数秒以内に止まり、「再取得をやめました。N 冊済み・残り M 冊」と出る。同じ絞り込みでもう一度始めると、前に済まなかった本から進む（ステータスバーの冊数で分かる）。
- [ ] R5. 動いている間に Wi-Fi を切る。少しして「通信できなくなりました」と残りの冊数の通知が出て止まり、閉じるまで残る。アプリは固まらない。
- [ ] R6. 洋書が多い絞り込み（例: 自動分類の発行形態や出版社で洋書の出版社を選ぶ）で再取得し、1 冊あたり 2 秒前後で進む。途中で Google の制限（「Google Books の 1 日の上限に達しました」や「制限を返しました（HTTP 429）」）が出たら、そのときの冊数を教えてほしい。
- [ ] R7. 「すべて」で数百冊を再取得してみてよければ、全部終わるまでの時間と、止まったならその理由を教えてほしい（Google の 1 日の割り当ての目安になる）。

### カメラ読み取り（0029）

準備: Mac のカメラ（内蔵、または連係カメラの iPhone）と、ISBN の下に 192 から始まる 2 段目のバーコードがある本、雑誌、食品のパッケージを用意する。登録は本物の蔵書に入るので、確かめたあとで消すか、ダミーの蔵書（上の手順）で試す。`pnpm tauri dev` で起動する。

- [ ] V1. スキャン登録画面を開く。右の「カメラで読む」はオフで、カメラは動いていない（Mac のメニューバーにカメラの使用中の印がない）。「カメラをオンにする」を押すと、macOS のカメラの許可のダイアログが出る（出ない場合は、どのアプリの許可が要るかを教えてほしい）。許可すると映像が出る。押したあと、カーソルは手入力欄に戻っている。
- [ ] V2. 本の上段（ISBN）のバーコードを映す。数秒以内に読まれ、リーダーと同じに直前に読んだ本のカードに ISBN が出て、書誌の取得が始まり、音が 1 回鳴る。映像の枠の色が一瞬変わる。通信は取得元だけで、外部の CDN（jsdelivr など）への通信がない（ネットワークを切ってカメラをオンにしても、読み取りは動く）。
- [ ] V3. 2 段目つきの本を、上段と下段が両方入る距離で映す（または、上段、下段を続けて映す）。1 冊の組になり、C コードと本体価格が入る。
- [ ] V4. 同じ本を映したままにしても、1 回だけ読まれ、音も 1 回。いったん外して 1.5 秒以上たってから映すと、もう 1 回読まれる（所有済みの通知が出る）。外して 1 秒もしないで映し直したときは読まれない。
- [ ] V5. バーコードを映したまま、登録フォーム（1 冊ずつのとき）や、一覧を空にする確認を開く。開いている間は読まれず、閉じた直後にも、映したままの本のコードは入らない。バーコードを外して 1.5 秒以上たって映すと、また読まれる。
- [ ] V6. カメラをオンにしたまま、メイン画面に戻る。Mac のカメラの使用中の印が消える。スキャン登録画面を開き直すと、カメラはオフになっている。ウィンドウを最小化（または別のアプリを前面にして隠す）すると、使用中の印が消え、戻すと（オンのままなら）映像が再開する。
- [ ] V7. 「システム設定 → プライバシーとセキュリティ → カメラ」で bibcontainer（開発モードではターミナルなど）を外してオンにすると、許可のしかたの文が出る。リーダーと手入力は使える。許可し直して、もう一度オンにすると映る。カメラのない Mac や、ほかのアプリがカメラを使っているときの文も、わかれば確かめる。
- [ ] V8. 雑誌（491・492 の定期刊行物コード）や食品のバーコード（49・45 など）を映すと、リーダーと同じに「雑誌は非対応です」「書籍以外は非対応です」と出る。QR コードなど、EAN-13 以外は何も起きない。
- [ ] V9. iPhone の連係カメラが使える環境では、オンにしたあと、「使うカメラ」の選択欄に iPhone が出て、選ぶと映像が切り替わる。選んだカメラは、アプリを終了して開き直し、オンにしたときも使われる（オン・オフそのものは覚えない）。連係カメラの映像でも読める。

### Windows

- [ ] X1. Windows でも G1・G4・G6・W1・W2・W7 が同じに動く（WebView2）。
- [ ] X2. Windows でも M1・M2・M7 が同じに動く。
- [ ] X3. Windows でも K2・K4・K5・K7・K8 が同じに動く（WebView2。日本語入力は Microsoft IME）。
- [ ] X4. Windows でも C1・C2・C6・C8 が同じに動く（Excel の Shift_JIS の CSV、保存のダイアログ）。
- [ ] X5. Windows でも R1〜R5 が同じに動く（WebView2）。
- [ ] X6. Windows でも V1〜V8 が同じに動く（WebView2）。許可のダイアログの出方（Edge と同じ形か、毎回出るか、一度許すと記憶されるか）と、`http://tauri.localhost` で `getUserMedia` が使えること（secure context）を確かめる。
