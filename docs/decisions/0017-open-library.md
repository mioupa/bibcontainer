# 0017: 書誌の取得元に Open Library を加える

## 状態

採用（取得元の追加はユーザーが承認済み、2026-10-15。要件書は「影響・今後」の案のとおり 0.4 に反映した、2026-10-16）

## 日付

2026-10-15

## 背景

2026-10 初めから、Google Books API の `q=isbn:…` の検索が、有効なキーでも HTTP 200・`totalItems: 0` を返すようになった（`intitle:`・`inauthor:` も同じ。ふつうの文字検索は動く）。ユーザーの環境（複数のネットワーク、別の PC）で再現し、Google Developer forums と GitHub（FireBall1725/librarium-api #130）にも同じ報告がある。Google 側の不具合で、10-15 の時点で返答はない。確かめ方は `scripts/check-google-books.sh`。

和書は openBD・NDL で足りるが、洋書は Google がほぼ唯一の取得元だったため、書影と書誌が取れなくなった。そこで、キーの要らない Open Library を取得元に加える（ユーザー承認、2026-10-15）。Google が直れば、Google が優先される。

確認した公式情報（すべて 2026-10-15 に取得）:

- https://openlibrary.org/developers/api （利用の作法、レート制限）
- https://openlibrary.org/dev/docs/api/books （`/isbn/` と、Books API が「legacy で、いずれ廃止されうる」こと）
- https://openlibrary.org/dev/docs/api/covers （書影の URL、レート制限、作法）
- https://openlibrary.org/developers/licensing （データの権利）
- 検索 API（`/search.json`）の実際の応答

## 決定

### 1. 公式情報の確認結果

| 項目 | 確認結果 |
|---|---|
| レート制限（書誌） | 名乗らないリクエストは 1 秒に 1 回。User-Agent にアプリ名と**メールか電話番号**を入れると 1 秒に 3 回。アプリの User-Agent は連絡先が URL（`bibcontainer/<版> (+<URL>)`）なので、1 秒に 1 回に従う。利用者のメールアドレスは User-Agent に入れない |
| 大量取得 | API での書誌の大量取得は禁止（データのダンプを使うよう案内）。違反すると強いレート制限か遮断。複数の IP に分散させない |
| 書影の URL | `https://covers.openlibrary.org/b/<key>/<value>-<S|M|L>.jpg`。key は `isbn`・`olid`・`id`（書影 ID）など。`?default=false` を付けると、書影がないとき空白の画像ではなく 404 |
| 書影のレート制限 | **書影 ID と OLID 以外**（ISBN など）での取得は、IP ごとに 5 分で 100 回まで（超えると 403）。書影 ID での取得は制限されない |
| 書影の作法 | 「書影 API をクロールしない」「公開のページに出すなら covers.openlibrary.org を指す URL を使う」「書影 API は公開のウェブサイトでの表示のためのもので、大量ダウンロード用ではない（大量なら archive.org の一括ダウンロード）」「Open Library へのリンクを、本のページか『このアプリについて』・フッターに置いてもらえるとありがたい」 |
| データの権利 | Internet Archive は、データベースの内容に新たな著作権などを主張しない（寄稿の一部に既存の権利の問題がありうる、とも書いてある）。書影の権利については記載なし |
| `/api/books` | 「legacy で、いずれ廃止されうる」。2026-09-26 に全 ISBN で 404 になったという報告もあるので使わない |
| `/isbn/<ISBN>.json` | 版（edition）の JSON へ 302 で転送される。未収録は 404 |

実際の応答で分かったこと:

- **版の JSON には著者名がない。** `authors` は `{"key": "/authors/OL…A"}` の参照だけで、`Clean Code`（9780132350884）は版に `authors` 自体がなく、作品（work）の側にある。著者名を版の JSON から得るには、作品と著者の JSON を 1 件ずつ取ることになり、1 冊で 3〜7 回の通信になる。
- **検索 API なら 1 回で著者名まで取れる。** `search.json?isbn=<ISBN>&fields=…,editions,editions.…` は、ISBN に合う版（`editions.docs`）について、書名・副書名・著者名・出版社・発行日・言語・書影 ID・ISBN を返す。ページ数と内容紹介は返らない。
- 1 つの ISBN に版が 2 つあることがある（`Pride and Prejudice` 9780141439518。`/isbn/` の転送先は `OL37076991M`、検索の版は `OL3700132M`）。
- 発行日は英語の自由な書式（`July 2008`、`2003`、`Mar 05, 2016` など）。
- `covers` には削除済みを表す `-1` が混じる。
- 2026-10-15 の確認: 9780132350884（Clean Code）、9780141439518（Pride and Prejudice）、9781493934362 は書誌も書影もある。9784003101117（こころ）は無い（和書はほとんど収録がない）。

### 2. 取得のしかた: 検索 API → 版の JSON の 2 回

1. `GET https://openlibrary.org/search.json?isbn=<ISBN-13>&fields=key,title,author_name,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.publisher,editions.publish_date,editions.language,editions.cover_i,editions.isbn`
   - 該当なし（`docs` が空）なら、ここで終わる（1 回）。キャッシュには「該当なし」として残る（1 日）。和書の大半はこれ。
   - `docs[0].editions.docs` のうち、`isbn` にこの ISBN（13 桁か 10 桁）を含む版を選ぶ。なければ先頭の版。版がなければ「該当なし」。
2. `GET https://openlibrary.org<版のキー>.json`（例 `/books/OL26222911M.json`）。ページ数と内容紹介のため。
   - 404、または型が `/type/redirect`（統合された版）なら、版の JSON なしで続ける（検索の値だけを使う。転送先は追わない）。
   - 通信エラー・ほかの HTTP エラーは、取得全体のエラーにする（キャッシュに残らず、あとで取り直される）。
- 生の応答（`RawResponse.body`）は、`{"search": <検索の応答>, "edition": <版の JSON か null>}` の形にまとめて保存する。`parse` はこの形だけを読むので、マッピングを直したときに通信なしで作り直せる（FR-META-10）。
- レート制限のグループは `openlibrary`（1 秒に 1 回、同時接続 1）。書影の取得も同じグループにする。

**`/isbn/` を使わない理由**: 著者名を得るのに作品・著者の JSON が要り、通信が増えるため。検索 API を先にすると、未収録は 1 回、収録ありは 2 回で済む。検索の索引は DB より遅れて更新されることがあるが、新しく足された版が数日取れないだけで、「該当なし」は 1 日で取り直される。

### 3. 項目の対応

| 項目 | 取り方 |
|---|---|
| 書名・副書名 | 版の JSON の `title`（`title_prefix` があれば前に付ける）・`subtitle`。なければ検索の版の `title`・`subtitle`。書名と副書名は同じレコードから取る（0007 §5 と同じ考え） |
| 著者 | 検索の版の `author_name`、なければ作品の `author_name`。名前だけ（役割・ヨミなし） |
| 出版社 | 版の JSON の `publishers[0]`、なければ検索の版の `publisher[0]` |
| 発行日 | 版の JSON の `publish_date`、なければ検索の版の `publish_date[0]`。英語の月名（`July 2008`、`Mar 05, 2016`、`March 5, 2016`、`5 March 2016`）と `c1995`・`[2003]` を、共通の日付の正規化で読めるようにした |
| ページ数 | 版の JSON の `number_of_pages` |
| 言語 | 版の JSON の `languages[0].key`（`/languages/eng`）、なければ検索の版の `language[0]`。ISO 639-1 にそろえる |
| 内容紹介 | 版の JSON の `description`（文字列か `{type, value}`）。作品の内容紹介は取らない（作品の JSON を取らないため） |
| 書影 | 版の JSON の `covers` の最初の正の数、なければ検索の版の `cover_i`。URL は `https://covers.openlibrary.org/b/id/<書影 ID>-L.jpg?default=false` |
| 取らない | 書名のヨミ、NDC、Cコード、価格、大きさ、シリーズ・巻次（Open Library の `series` は叢書名で、このアプリのシリーズと意味が違う） |

**書影は ISBN ではなく書影 ID で取る。** ISBN での取得は 5 分で 100 回の制限があり、一括の再取得で超えると 403 になる。書影 ID での取得は制限がなく、書影 ID は書誌の取得で手に入る。

### 4. 統合の優先順位（既定）

Open Library は、値を持つ行のすべてで**最後**に置く（洋書の補い）。Google が直れば、Google の値が先に使われる。

| 行 | 既定の順 |
|---|---|
| 書名・副書名、著者、出版社、言語 | openBD > NDL > Google > Open Library |
| 発行日 | openBD > NDL > Google > Open Library |
| ページ数・大きさ | NDL > openBD > Google > Open Library |
| 内容紹介 | openBD > Google > Open Library |
| 書影 | openBD > Google > Open Library |

- 取得元 ID・由来の名前は `openlibrary`（`ProviderId::OpenLibrary`・`MetaSource::OpenLibrary`）。表示名は `Open Library`。認証は不要。
- 保存済みの設定（`local_settings` の `providers`）に `openlibrary` がなくても、既定の順で末尾に足され（0007 §5、0010 §2）、**有効**として扱われる。利用者は設定画面で無効にできる。

### 5. アプリへの組み込み

- 接続テストの ISBN（0010 §15）: `9780141439518`（Pride and Prejudice）、`9780132350884`（Clean Code）、`9781493934362`。キーを使わない取得元なので、見つからないときの文言は「API キーは有効です」を含まないもの。
- 「このアプリについて」のクレジットは `書誌情報・書影: Open Library (https://openlibrary.org/)`。今の画面（`AboutSection.tsx`）はクレジットを文字で出すだけで、URL はリンクにならない（外部のページを開くプラグインを入れていない）。書影の作法の依頼（リンク）は、URL を文字で載せることで当面は満たすとした。
- 保存済みの設定に `openlibrary` がない場合のテスト（`src-tauri/src/metadata/settings.rs`）: Open Library が有効になり、取れる項目の行の末尾に足され、利用者が並べ替えた順と無効にした取得元はそのまま残る。

### 6. 構成

| 場所 | 内容 |
|---|---|
| `crates/metadata/src/providers/openlibrary.rs` | `OpenLibrary`（`with_base_url`、書影を別の接続先にする `with_base_urls`）。2 回の取得、包みの形、解析、クレジット、レート制限 |
| `crates/metadata/src/types.rs` | `ProviderId::OpenLibrary`・`MetaSource::OpenLibrary`（serde 名は `openlibrary`。既定の snake_case だと `open_library` になるため明示した） |
| `crates/metadata/src/merge.rs` | 既定の優先順位（§4） |
| `crates/metadata/src/normalize.rs` | 英語の日付（月名の全綴り・3 文字の略記（末尾の `.` は任意）・`Sept`・`5th` のような序数、`c1995`・`c.1995`・`[2003]`・`(2003)`・`©1995`）。ページ数は `448 pages` も読む。`strip_html` を Google と共有するため、ここに移した |
| `src-tauri/src/metadata/settings.rs`・`providers.rs` | 設定の読み出し（`openlibrary`）、接続テストの ISBN |
| `src/i18n/ja/settings.ts`・`edit.ts` | 取得元の表示名 |

- 版の key（`/books/OL…M`）は、URL に入れる前に形を確かめる（`^/books/OL\d+M$` 以外は解析エラーにして、2 回目の通信をしない）。
- `pagination` からページ数を読むのは、`number_of_pages` がないときだけ。既存の規則（最後の「数字 + p」）を使うので、`xii, 345 p., 16 p. of plates` は 16 になる（特別扱いしていない）。

### 7. フィクスチャとテスト

- `crates/metadata/tests/fixtures/openlibrary_search_<ISBN>.json`（9780132350884・9780141439518・9781493934362・未収録の 9784003101117）と、検索が返した版の JSON `openlibrary_edition_<OLID>.json`（`OL26222911M`・`OL3700132M`・`OL34386079M`）。2026-10-15 の実際の応答を、整形だけして保存した。`OL26222911M` は目次（358 項目）を含むため 64 KB ある。
- `title_prefix`、`covers` の `-1`、統合された版（`/type/redirect`）、文字列の `description` は、実際の 3 件に例がないので、テスト（`crates/metadata/tests/openlibrary.rs`）の中で作った JSON で確かめている。
- 取得の流れ（偽の HTTP と wiremock）: 未収録は通信 1 回、収録ありは 2 回、版が 404 なら検索の値だけで「取れた」、版が 500 なら取得のエラー（キャッシュされない）、検索が通信エラーなら `networkError`。
- 実際の通信のテストは `crates/metadata/tests/live.rs` の `live_open_library_*`（`#[ignore]`）。実装時に送ったのは、フィクスチャの取得と live のテストを合わせて 12 回（1.3 秒以上あけた）。
- 取得元が 4 つになったので、既存のテスト（`service.rs`・`http_wiremock.rs`・`src-tauri` の `tests.rs`・`SettingsScreen.test.tsx`）の件数を直した。`service.rs` の `cache_avoids_the_network_until_it_expires` は、29 日前の取得で Open Library だけが取り直される（該当なしの有効期限は 1 日）形に直した。

## 理由

- Google の ISBN 検索が直る見込みが立たず、洋書の書影と書誌が取れないため。Open Library はキーが要らず、洋書の収録が多い。
- 最後に置くのは、Open Library の書誌は利用者の編集で作られ、項目のゆれ（`physical_format` が `Taschenbuch` など）があるため。既存の取得元が値を持つ項目は、そちらを使う。
- 検索 API を先にするのは、通信の回数を減らし（和書は 1 回）、Open Library のレート制限と大量取得の禁止に沿うため。

## 影響・今後

### 未解決（ユーザーの判断が要る）

1. **書影の作法との整合。** Open Library は「書影 API は公開のウェブサイトでの表示のためのもので、大量ダウンロード用ではない」としている。このアプリは、本を登録・再取得したときに 1 冊ずつ書影を取り、端末に保存する（ほかの取得元と同じ）。1 秒に 1 回の間隔で、クロールはしないので、大量ダウンロードにはあたらないと考えて実装した。一括の再取得（FR-META-13）で洋書を何百冊も取り直すと、量としては大きくなる。
2. **User-Agent の連絡先。** メールか電話番号を入れると 1 秒に 3 回まで使えるが、利用者の個人情報を外に出すことになるので、URL のまま 1 秒に 1 回にした。
3. **Open Library の不調が取得状態に響く。** 取得状態の規則（0010 §7）はそのままなので、Open Library がエラー（5xx・タイムアウトなど）を返すと、ほかの取得元で取れた本も `partial` になる。和書でも、Open Library には毎回 1 回問い合わせる（ふだんは「該当なし」で、`ok` のまま）。Open Library は Internet Archive の障害で長く止まったことがあり、Google より止まりやすいとみている。止まっている間は、5xx のリトライ（1・2・4 秒）の分だけ登録が遅くなり、登録した本が `partial` になる。設定で Open Library を無効にすれば避けられる。0010「未解決・今後」の案（`partial` は「書名が取れていない」だけにし、取得元のエラーは別に知らせる）に変えるかは、実際に困ってから決める。

### そのほか

- 1 冊を取るのにかかる時間: 収録ありの本は、検索と版の JSON の間で 1 秒待つので、1 秒以上かかる（ほかの取得元と並行なので、登録の待ち時間はおおむねこれで決まる）。一括の再取得は、洋書 1 冊につき 1〜2 秒増える。
- 「このアプリについて」のクレジットを押せるリンクにするには、外部のページを開くプラグイン（`tauri-plugin-opener`）と権限が要る。ほかの取得元のクレジットにも効くので、入れるなら別の作業にする。

### 要件書の修正案

（ユーザーの承認を得て、要件書 0.4 に反映した。2026-10-16。あわせて 2 章の表と 6 章の構成図にも Open Library を足した）

- 7.3 FR-META-02 の表に、Google Books の次の行として「Open Library ｜ [SHOULD] ｜ 不要 ｜ 書名、著者、出版社、発行日、ページ数、言語、内容紹介、書影（主に洋書。和書はほとんど収録がない）」を足す。表の下に「Open Library は、Google Books の ISBN 検索が 2026-10 初めから 0 件を返す障害を受けて、洋書の書誌と書影を補うために加えた（2026-10 ユーザー判断）」を足す。あわせて、表の途中にある NDL の書影の注記を表の下へ移す（今は注記で表が切れ、楽天・Amazon の行が表から外れている）。
- 7.3 FR-META-04 の表: 書名・副書名、著者、出版社の行の末尾に「> Open Library」、発行日を「openBD > 楽天 > NDL > Google > Open Library」、ページ数・大きさを「NDL > openBD > Google > Open Library」、内容紹介を「openBD > 楽天 > Google > Open Library」、書影を「openBD > 楽天 > Google > Open Library > Amazon」にする。
- 7.3 FR-META-08: 「既定: NDL・Google・楽天・Open Library は 1 秒に 1 件」にする。
- 10.6: 括弧内に Open Library を足し、「Open Library は、『このアプリについて』に Open Library へのリンクを置く（書影 API の作法による依頼）」を足す。
- 11 章に「11.4 Open Library」を足し（楽天・Amazon は 11.5・11.6 に繰り下げる）、本記録の 1〜3 の要点（窓口、検索 API → 版の JSON、書影 ID の URL、レート制限、作法）を書く。
- 版を 0.4 にし、変更履歴に「Open Library を取得元に加えた（`docs/decisions/0017`）」を足す。
