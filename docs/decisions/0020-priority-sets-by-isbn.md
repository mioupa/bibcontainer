# 0020: 統合の優先順位を ISBN の国番号で 2 組に分ける（和書・洋書など）

## 状態

採用（ユーザー決定、2026-10-16。要件書は「影響・今後」の案のとおり 0.5 で FR-META-04・FR-CONF-02 に反映した）

## 日付

2026-10-16

## 背景

洋書では、NDL（CiNii Research 経由のデータ）の書名が図書館の流儀で小文字になる（`Clean code`、副書名 `a handbook of agile software craftsmanship : pbk.`）。内容紹介と書影もない。統合の優先順位は openBD > NDL > Google > Open Library なので、洋書でも NDL の書名・著者が先に採用されていた。一方、和書・漫画は openBD と NDL が正確で、今の順位が合っている。

ユーザーの決定:

- ISBN が 978-4 で始まる本（日本の出版社）は、今の優先順位のまま。
- それ以外の本（洋書など）は、書名・副書名・著者・出版社・発行日・内容紹介・言語・書影で Google と Open Library を優先し、NDL・openBD はその後ろの補いにする。
- 本体価格・Cコード（2段目と openBD）、NDC（NDL）、ヨミ（NDL）は、どちらでも今のまま。
- ISBN のない本は、和書の順位を使う。
- 洋書の既定の順位の叩き台が示され、実際の応答の質を見て調整してよい、とされた。

## 決定

### 1. 区分（`PriorityProfile`）

- `Japanese`（和書）: ISBN-13 が `9784` で始まる（ISBN-10 の `4` で始まるものは 13 桁に直すとこれになる）。ISBN がない・読めない本もこちら。
- `Foreign`（洋書など）: それ以外の ISBN（978-0、978-1、978-3、979-… など）。日本の出版社が出した英語の本（978-4）は和書の区分になる。
- `PriorityProfile::for_isbn(Option<&str>)` が決める（既存の ISBN の正規化を使うので、ハイフン・全角・ISBN-10 も可）。

### 2. 既定の順位

和書の区分は、これまでの既定（0007 §5、0017 §4）のまま。洋書の区分は次のとおり。行に入る取得元は和書と同じで、順だけを変えた。

| 行 | 和書（今のまま） | 洋書など |
|---|---|---|
| 書名・副書名、著者、出版社、言語 | openBD > NDL > Google > Open Library | **Google > Open Library > NDL > openBD** |
| 発行日 | openBD > NDL > Google > Open Library | **Google > Open Library > NDL > openBD** |
| 内容紹介 | openBD > Google > Open Library | **Google > Open Library > openBD** |
| 書影 | openBD > Google > Open Library | **Google > Open Library > openBD** |
| シリーズ・巻次 | openBD > NDL | **NDL > openBD** |
| ページ数・大きさ | NDL > openBD > Google > Open Library | 同じ |
| 書名のヨミ、NDC | NDL | 同じ |
| 本体価格 | 2段目 > openBD > NDL | 同じ |
| 参考の税込価格 | openBD > NDL | 同じ |
| Cコード | 2段目 > openBD | 同じ |

**叩き台から変えたところ**（指示文は変えてよいとされている）:

- **シリーズ・巻次**: 叩き台は「NDL > Open Library > Google > openBD」。Google と Open Library は、今の実装ではシリーズを返さない（Open Library の `series` は叢書名で、このアプリのシリーズと意味が違うので取っていない。0017 §3）。そのため、行に入るのは NDL と openBD だけで、NDL > openBD にした。
- **内容紹介・書影**: 叩き台は書名などと同じ順（NDL を含む）。NDL は内容紹介と書影を返さないので、行に入らない。
- 楽天・Amazon は未実装なので、どちらの区分にも入れていない（0007 §5 と同じ）。

### 3. 設定（`local_settings` の `providers`）

- `ProviderSettings.priorities` を `MetaPrioritySets` にした。保存の形は `"priorities": { "japanese": { "groups": {…} }, "foreign": { "groups": {…} } }`。
- 読み出し:
  - **以前の形**（`priorities` の直下に `groups` があり、`japanese`・`foreign` がない）は、和書の組として読み、洋書の組には既定を入れる。
  - 新しい形は、組ごとに、その区分の既定で補う（欠けた行・設定にない取得元を既定の順で末尾に足す、知らない取得元や重複を捨てる、という 0010 §2 の作法を、区分ごとの既定で行う）。
  - 片方の組がない・壊れているときは、その組だけ既定にする。`groups` と `japanese`・`foreign` の両方があるときは、新しい形として読む。
- 統合（`MetadataService`）は、ISBN ごとに組を選んでから統合する。`fetch_many` に和書と洋書が混ざっていても、1 冊ずつ正しい組を使う。`merge()` は 1 組の `MetaPriorities` を受け取るまま。
- `MetaFetchResult` に、使った区分（`profile`）を足した。画面ではまだ使っていない。

### 4. 画面（設定の「取得元」の節）

- 項目ごとの優先順位の並べ替えの上に、「和書（ISBN 978-4）」「洋書など（それ以外）」のタブを置いた（既存の `Tabs`。登録・編集フォームと同じ部品）。最初は和書。最後に見たタブは画面の状態だけに持つ（保存しない）。
- 片方の組を並べ替えても、もう片方は変わらない。保存では両方を送る。
- 説明: 「ISBN が 978-4 で始まる本は「和書」の順、それ以外の ISBN の本は「洋書など」の順を使います。ISBN のない本は「和書」の順です。」

### 5. 登録済みの洋書の直し方

- 登録済みの洋書は、「書誌を再取得」で、書名などが洋書の区分の順位で選び直される。**手動の再取得はキャッシュを使わずに取り直す**（0010 §9。指示文の「キャッシュの応答を読み直すので通信は少ない」は、今の実装と違うので直した）。1 冊につき、Google（障害の間は回避策で 3 回。0018）・Open Library（2 回。0017）・NDL・openBD に問い合わせるので、洋書が多いと 1 冊あたり数秒かかる。
- 手で直した項目は上書きしない（「手で直した項目も上書きする」を選んだときだけ上書き。0010 §9）。
- 取得元だけが変わって値が同じ項目（Open Library と Google がどちらも `Clean Code` を返すなど）は、コアが「値が変わらない変更」を書かないため、`field_sources` も前のまま（0006 の `ApplyCtx::apply`）。
- テスト（`src-tauri/src/metadata/tests.rs`）: NDL の値（`Clean code`）で登録した 978-0 の本を、手動の再取得で取り直すと、書名・副書名が Google・Open Library の値（`Clean Code`、`A Handbook of Agile Software Craftsmanship`）になり、`field_sources` が `google`（Google を有効にしたとき）か `openlibrary` になる。手で直した出版社は上書きされない（`skippedManual` に出る）。再取得は、前の取得のキャッシュがあっても、各取得元に 1 回ずつ問い合わせる。

### 6. テスト

- `crates/metadata/tests/merge.rs`: 区分の判定（9784…、4-…の ISBN-10、978-0・978-1・979-1、なし、読めない文字列）、和書の既定がこれまでと同じこと、洋書の既定、同じ候補でも 978-4 と 978-0 で書名・著者・書影・シリーズの採用元が変わり、ヨミ・NDC・価格・Cコード・ページ数は変わらないこと、洋書で Google → Open Library → NDL → openBD と順に落ちること、実際の Clean Code のフィクスチャで洋書の区分なら Google の書名になること、組ごとの補い。
- `crates/metadata/tests/service.rs`: 和書と洋書が混ざった `fetch_many`、片方の組の変更がもう片方に響かないこと、読めない ISBN は和書の区分。
- `src-tauri`: 設定の以前の形・新しい形・片方なし・壊れた値、和書の本は洋書の組を変えても影響を受けないこと、5 の再取得。
- 画面（Vitest）: 最初は和書のタブ、切り替えると洋書の順が出る、片方の並べ替えがもう片方を変えない、保存で両方を送る。

## 理由

- 和書と洋書で、データの質がよい取得元が逆になるため（和書は openBD・NDL が版元・図書館のデータで正確、洋書は NDL が CiNii 由来の小文字の書名で、Google・Open Library の方が出版物の表記に近い）。
- ISBN の国番号で分けるのは、取得する前に決まり、取得元の応答に左右されないため。言語（`language`）で分けると、どの取得元の言語を使うかで循環する。
- 設定を以前の形のまま和書の組として読むのは、利用者がすでに並べ替えた順位を失わないため。洋書の組は新しいので既定から始める。

## 影響・今後

### 要件書の修正案（0.5 で反映済み、2026-10-16）

- FR-META-04 の本文を「1 冊について、有効な取得元に並行して問い合わせ（取得元ごとのレート制限は守る）、項目ごとに優先順位に従って値を選ぶ。優先順位は、ISBN が 978-4 で始まる本（和書。ISBN のない本も含む）と、それ以外の本（洋書など）の 2 組を持つ。既定の優先順位は次のとおりで、設定画面で組ごと・項目ごとに並べ替えられる。」にする。
- 表を 2 列にする（和書・洋書など）:

  | 項目 | 和書（978-4、ISBN なし） | 洋書など（それ以外） |
  |---|---|---|
  | 書名・副書名 | openBD > NDL > 楽天 > Google > Open Library | Google > Open Library > NDL > openBD > 楽天 |
  | 書名のヨミ | NDL > 楽天 | NDL > 楽天 |
  | 著者（役割付き） | openBD > NDL > 楽天 > Google > Open Library | Google > Open Library > NDL > openBD > 楽天 |
  | 出版社 | openBD > NDL > 楽天 > Google > Open Library | Google > Open Library > NDL > openBD > 楽天 |
  | 発行日 | openBD > 楽天 > NDL > Google > Open Library | Google > Open Library > NDL > openBD > 楽天 |
  | 本体価格 | **2段目** > openBD > NDL（楽天の税込価格は参考値として別に保持） | 同じ |
  | Cコード | **2段目** > openBD | 同じ |
  | NDC | NDL | 同じ |
  | ページ数・大きさ | NDL > openBD > Google > Open Library | 同じ |
  | シリーズ・巻次 | openBD > NDL > 楽天 | NDL > openBD > 楽天 |
  | 内容紹介 | openBD > 楽天 > Google > Open Library | Google > Open Library > openBD > 楽天 |
  | 書影 | openBD > 楽天 > Google > Open Library > Amazon | Google > Open Library > openBD > 楽天 > Amazon |

- 表の下に「洋書などの組は、NDL（CiNii 由来）の書名が小文字になり、内容紹介・書影もないため、Google と Open Library を先にする（2026-10 ユーザー判断。`docs/decisions/0020`）」を足す。
- FR-CONF-02 の「項目ごとの優先順位」を「項目ごとの優先順位（和書・洋書などの 2 組）」にする。

### そのほか

- 編集フォームの「他の取得元の値」（FR-META-05）の並びも、その本の区分の順位になる（統合の結果の `others` の順）。
- 区分を本ごとに手で変える機能（978-4 の英語の本を洋書として扱うなど）は作っていない。必要になったら考える。
