# 0025: キーワード検索（書名・著者で探す）

## 状態

採用（2026-10-17 にユーザーが設計と受け入れ基準の案を承認した（案では AT-36〜AT-40 としたが、並行した 0026 が先に使ったので AT-41〜AT-45 にした）。Google を含め、そのために opener のプラグインで外部のページを開く形も承認済み。jsdom のテストと、偽の HTTP・実際の応答のフィクスチャでのテストまで。Google の実際の検索は、ユーザーの環境の live テストで確かめた（2026-10-18）。実機での見た目・操作は未確認。要件書は「影響・今後」の案のとおり 0.9 に反映した、2026-10-18）

## 日付

2026-10-17〜18

## 背景

0022 §1 の 2 番目（FR-META-12 [SHOULD]）。ISBN のない本（古書・同人誌など）や、バーコードが読めない本を、書名・著者で探して登録フォームに入れる。0022 の決定は「手動登録フォームに『書名・著者で探す』を置く。NDL・Google・Open Library を探し、選ぶとフォームに入れる。ISBN があれば、ふだんの取得で詳しい書誌を取り直す」。楽天ブックスは見送った（0023）ので、検索先はこの 3 つ。openBD には検索の窓口がない。

取得元のトレイト（0007 §2）には、`search` の枠だけ（`Unsupported`）があった。

確認した公式情報（2026-10-17 に取得）:

- NDLサーチ: 外部提供インタフェース仕様書 第 1.5 版（2026.9.25。SRU の章、表 3-2・3-3）、別紙 1（提供元の一覧、2026.6.24）、別紙 2（資料種別の一覧、2024.4.1）、https://ndlsearch.ndl.go.jp/help/api （利用条件）
- Google Books: https://developers.google.com/books/docs/v1/using （Performing a search）、reference/volumes/list、https://developers.google.com/books/branding （2011-05-09 更新）、/books/terms（2012-04-25）、Google APIs Terms of Service（2021-11-09）
- Open Library: https://openlibrary.org/dev/docs/api/search 、/developers/api、/search/howto

## 決定

### 1. 公式情報と実際の応答で分かったこと

#### NDLサーチ（SRU）

| 項目 | 確認結果 |
|---|---|
| 索引 | `title`・`creator`・`publisher` などは**部分一致**。1 つの値に空白で区切った語が複数あれば AND で、そのときは値を `"` で囲む。`isbn` は完全一致。演算子は `=`・`any`・`all` だけで、完全一致・前方一致の指定はできない |
| 絞り込み | `mediatype = "books"` が図書、`dpid = "iss-ndl-opac"` が NDL 自身の書誌（R100000002）。SRU は、提供元をまたいで同定済みのレコードを探す（仕様書 2(6)） |
| 件数 | `maximumRecords` は既定 200、最大 500 |
| 並び順 | 何も指定しないと、書名・刊行年と無関係（「スローループ」は 10, 11, 1, 2, … 9 巻の順。「こころ 夏目漱石」は ISBN のない戦前の版から並ぶ） |
| 並べ替え | 仕様書は `version=1.2` の `sortBy`（`title`・`creator`・`issued_date`・`created_date`・`modified_date`）を示す。**問い合わせの末尾に ` sortBy issued_date/sort.descending` と付けると、最後の条件の値に巻き込まれてエラーか 0 件になる。`AND sortBy = "issued_date/sort.descending"` と、ほかの条件と同じ形で連ねると効く**（「こころ 夏目漱石」の 101 件のうち、2025.10 から新しい順に返った） |
| `AND`・`OR` の罠 | 仕様書は「検索語に AND・OR を含むと（andy・organic なども）検索エラーになる」とし、`=` の前後に空白を入れて引用符で囲む書き方を案内している。`title = "organic"`（18,938 件）、`title = "war and peace"`（1,791 件）は、その書き方でエラーにならなかった |
| 書名の索引の広さ | 収録作品名・並列書名にも当たる（「ハリー・ポッター 賢者の石」で、別の本に収められた短い文章や、中国語版、ピアノ譜が出る） |
| 利用条件 | 数値の上限は示されていない。同時接続に上限があり、大量の連続アクセスは遮断されうるので、並列にしない（0007 と同じ） |

#### Google Books（`volumes.list`）

| 項目 | 確認結果 |
|---|---|
| 書式 | `q` だけが必須。`maxResults` は既定 10、最大 40。`printType=books`。`intitle:`・`inauthor:` は 2026-10 初めから 0 件を返す障害が続いている（0017・0018。フォーラムの最後の投稿は 10-09 で、Google の返答はない）。普通の文字検索は動く |
| キー | キーなしは 429（割り当て 0）。2026-10-17 も同じ |
| ブランドの規約 | 「Google Books の結果を 1 件以上表示するときは、『powered by Google』のロゴを結果のそばに置く」「表示する結果の 1 件ごとに、Google Books のその本のページ（またはプレビューのあるページ）への目立つリンクを置く」「Google Books API の結果を並べ替えたり、そのほか改変したりしてはならない」。準拠した実装の例は「Google の検索結果を第三者の結果と混ぜていない」。「Google のロゴを、競合する検索サービスのロゴと同じページに出さない」 |
| 規約（API 全般） | 「キャッシュのヘッダーが許すより長くコピーを持たない・データベースを作らない」（Google APIs Terms of Service 5e）。登録した本に Google 由来の値を保存するのは、ISBN の取得（0007）からの扱いと同じ |

#### Open Library（`search.json`）

| 項目 | 確認結果 |
|---|---|
| 書式 | `title=`・`author=` は AND で効き、`author=` も部分一致（`martin` で `Robert C. Martin`）。`fields` で返す項目を選ぶ（`*` は重いので避けるよう案内。`*` でも `editions` は返らない） |
| 版 | 作品（work）ごとに、条件に合う版が `editions.docs` に**1 つだけ**返る（ドキュメントも「今は 1 版だけ」） |
| ISBN | 作品単位の `isbn` は数千件になることがある（Pride and Prejudice で 6,117 件）。版の `isbn` を使う。返った版に ISBN がないこともある |
| 和書 | ほとんどない（「スローループ」は 1 巻だけで著者・出版社なし。「こころ」はローマ字の出版社、ISBN のない版） |
| 作法 | 名乗らないリクエストは 1 秒に 1 回（0017 と同じく、メールを入れないのでこれに従う）。キャッシュを勧め、大量取得を禁じる |

### 2. 探し方（`crates/metadata`）

- トレイトの `search` は `Result<SearchResults, SearchError>` を返す。`SearchQuery { title, author }`（以前の `limit` は外し、件数は取得元ごとの定数にした）。`SearchQuery::validated()` が NFKC・前後の空白・連続する空白を整え、書名も著者も空、またはどちらかが 200 文字を超えるときは `SearchError::InvalidQuery`（新しい種類）。
- 結果の型（IPC に出す）: `SearchHit { provider, isbn13: Option<String>, candidate: Candidate, link: Option<String> }`、`SearchResults { total, hits }`。`isbn13` はチェックデジットの合う ISBN-13 だけ（ISBN-10 は直す）。候補の `isbn13`（`String`）は、ISBN がなければ空にした（`Candidate` の型は変えない）。`link` はその取得元のサイトの、その本のページ（https）。
- `MetadataService::search(provider, query)`。NDL・Google・Open Library の `supports_search` を真にした（openBD は偽のまま）。
- 通信は、ISBN の取得と同じレート制限のグループ（`ndl`・`google`・`openlibrary`。1 秒に 1 回、同時接続 1）を通る。1 回の検索で、取得元ごとに 1 回だけ問い合わせる。検索の結果はキャッシュしない（選んだあとの ISBN の取得は、ふだんのキャッシュを使う）。

| 取得元 | 問い合わせ | 結果 |
|---|---|---|
| NDL | `/api/sru?operation=searchRetrieve&version=1.2&recordSchema=dcndl&recordPacking=xml&onlyBib=true&maximumRecords=50&query=…`。条件は `title = "<書名>" AND creator = "<著者>" AND mediatype = "books" AND dpid = "iss-ndl-opac" AND sortBy = "issued_date/sort.descending"`（書名・著者はあるものだけ。`search_cql`）。値の `"`・`\` は空白にする。演算子と同じ綴りの語（`AND`・`OR`・`NOT`）は小文字にする（大文字の語を引用符の中に入れたときの動きは確かめていない） | 50 件まで。念のため、応答も刊行年の新しい順に安定に並べ直す（日付のないものは最後）。1 件ずつ、データ側の書誌（書名のある `BibResource`）を、ISBN の取得と同じ `candidate_from` で候補にする。ISBN は型が `ISBN` の識別子のうち最初の正しいもの。ISBN が 4 個を超える（シリーズ全体のレコード）か、ない（古い本）ときは `None`。同じ ISBN は先のものだけ。`link` は書誌の `rdf:about` の `#` より前（`https://ndlsearch.ndl.go.jp/books/R100000002-I…`） |
| Google | `/books/v1/volumes?q=<書名 著者>&maxResults=20&printType=books&key=…` | 20 件まで、**Google の順のまま**。項目を捨てない（`volumeInfo` のない、見せるもののない項目だけ除く）。候補の作り方は ISBN の取得と同じ関数に分けて共有した。`link` は `https://books.google.com/books?id=<ID>`（ID は `^[A-Za-z0-9_-]{4,32}$` のときだけ）。HTTP のエラーの意味づけは ISBN の取得と同じ |
| Open Library | `/search.json?title=…&author=…&fields=key,title,subtitle,author_name,cover_i,editions,editions.key,editions.title,editions.subtitle,editions.author_name,editions.isbn,editions.publisher,editions.publish_date,editions.language,editions.cover_i&limit=20`（あるものだけ。作品単位の `isbn` は取らない） | 20 件まで、Open Library の順のまま。作品ごとに、返った版（`editions.docs[0]`）の値で候補を作り、版になければ作品の値。ISBN は版の 13 桁、なければ 10 桁を直す。書影は書影 ID の URL（0017 §3）。`link` は版の `https://openlibrary.org/books/OL…M`、なければ作品の `/works/OL…W` |

- NDL の結果は、該当なし（`Record does not exist`・0 件）なら空。ほかの診断（エラー）は解析のエラー。

### 3. アプリへの組み込み（`src-tauri/src/metadata/search.rs`）

- **`search_metadata(provider, title?, author?) → ProviderSearchResult`**（`{ provider, outcome, total, hits, elapsedMs }`）。`outcome` は ISBN の取得の `ProviderOutcome`（1 件以上で `found`、0 件で `notFound`、通信・HTTP・解析の失敗はそれぞれの種類）。通信の失敗はエラーにせず `outcome` で返し、画面が取得元ごとに出す。条件が正しくない・取得元が使えない（無効にしている、Google のキーがない）・検索できない（openBD）ときだけ `validation`。DB にも書影にも触らない。
- **`fetch_search_hit(hit) → FetchMetadataResult`**（`fetch_metadata` と同じ形。DB には書かない）。
  - 画面から届いた結果を確かめる（`checked_hit`）: 候補の取得元が結果の取得元と同じ、ISBN があればチェックデジットの合う ISBN-13、書影の URL は https のものだけを残す（取得元は結果の取得元にそろえる。レート制限とダミー画像の判定が取得元で決まるため）。違えば `validation`。
  - **ISBN あり**: `fetch_metadata` と同じ流れ（有効な取得元・キャッシュ・ISBN で決まる優先順位の組。共通の `fetch_isbn13`）。選んだ取得元が、この取得で「取れた」にならなかったとき（該当なし・通信エラー・無効にした、など）だけ、検索結果の候補を足して統合し直す（`add_candidate`）。取れたときは、ISBN の取得の候補（項目が多い）を使う。
    - 足した取得元の報告が「該当なし」なら「取れた」に直す。直さないと、ISBN ではどの取得元にも見つからなかった本が、値があるのに `failed`（再取得は手動）になる。通信・HTTP・解析の失敗はそのまま残し、取得状態は 0010 §7 のとおり `pending`・`partial` になる（オフラインなら、検索結果の値が入ったまま取得待ちで保存でき、あとで自動で取り直される）。
  - **ISBN なし**: 検索結果の 1 件だけを、和書の組の優先順位（ISBN のない本は和書。0020 §1）で統合する。書影は候補の URL から取れれば保存し、取れなければなし（エラーにしない）。取得状態は入れるが、画面が ISBN のない本の下書きから外す（0011。`changes.ts` の `buildNewBookDraft`）。
- **`open_link(url)`**: 既定のブラウザで開く。https だけ、利用者情報・明示のポートなし、ホストが `books.google.com`・`ndlsearch.ndl.go.jp`・`openlibrary.org`・`openbd.jp`・`github.com` のどれかに完全に一致するときだけ（`commands/system.rs`。ほかは `validation`）。`github.com` は連絡先の既定の URL のため。opener のプラグイン（`tauri-plugin-opener`）は、ログのフォルダを開く処理のためにすでに入っていた。Rust から `OpenerExt::open_url` で呼ぶので、capabilities の追加も JS のパッケージも要らない（権限の検査は、プラグインの JS 向けのコマンドの側にだけある）。

### 4. 画面

- **入口**: 登録フォームの手動登録（`new`）のときだけ、書名の欄の右に「探す」（`tabs.tsx` の `trailing`）。編集（`edit`）と連続登録の行の手直し（`draft`）には出さない（バーコードを読んだ本は ISBN で取れるため）。押すと、フォームの書名と最初の著者の名前を入れた「書名・著者から探す」のダイアログが開き、どちらかが入っていれば、すぐに探す。どちらも空なら書名の欄にフォーカスを置き、「探す」は押せない。
- **ダイアログ**（`src/features/editor/search/KeywordSearchDialog.tsx`、決まりは `searchModel.ts`）: 書名・著者の欄と「探す」。探すのは Enter か「探す」のときだけ。取得元は `list_providers` のうち、検索でき（`supportsSearch`）有効なもの。使える（`active`）ものには取得元ごとに別々に `search_metadata` を呼び、届いた欄から出す。有効だがキーのない Google は、「API キーがないので探していません」と出して、呼ばない。新しく探すと前の結果を消し、前の検索の遅れた応答は捨てる。
- **欄の順**: 書名・著者にかな・漢字があれば NDL → Google → Open Library、なければ Google → Open Library → NDL（0020 の和書・洋書と同じ考え）。欄の中は取得元の返した順のまま。
- **欄の見出し**: 取得元の名前（Google は「Google Books の検索結果」）と、「探しています…」・「N 件」・「全 X 件のうち N 件」・「見つかりませんでした」・「通信できませんでした」・「取得元がエラーを返しました（HTTP 503）」・「応答を読めませんでした」・エラーの文。Google の欄は、1 件以上あるとき見出しの右に「powered by Google」の画像。Google を探すときは、検索の欄のそばにも同じ画像を置く（§5）。
- **行**: 書名・副書名・巻次、著者（役割を（）で）、出版社・発行日、ISBN（なければ「ISBN なし」）、取得元のページへのリンク（「NDL サーチで見る」「Google Books で見る」「Open Library で見る」。`open_link`）。書影の画像は出さない。
- **選び方**: 結果全体が 1 つの `listbox`（欄は `group`、行は `option`）。クリックで選び、ダブルクリック・「入れる」・一覧での Enter で入れる。書名・著者の欄での ↑↓ は、選択を動かして一覧にフォーカスを移す。Home・End も使える。日本語入力の確定の Enter・矢印は無視する（既存の `isImeCommitKey`）。行のリンクは、選んでいる行のものだけ Tab で届く。Esc は検索のダイアログだけを閉じる（フォームは閉じず、破棄の確認も出ない）。ダイアログはフォームの `<form>` の外（`FormProvider` の中の兄弟）に置いたので、フォームの Enter の扱い・⌘Enter の保存は検索の中では働かない。
- **選んだあと**（`BookEditorForm.tsx` の `chooseHit`）: ダイアログを閉じ、「書誌を取得しています」の表示のまま `fetch_search_hit` を呼ぶ。フォームへの入れ方は「書誌を取得」と同じ関数（`applyFetched`）。入れる項目は `fetchApply.ts` の `keysToFill`: 手で直していない項目と、**結果に値があれば書名・著者**（`SEARCH_REPLACED_KEYS`。手で入れていても置き換える）。ほかの手で入れた項目は残す。取得元の表示と「他の取得元の値」も、結果に差し替わる。ISBN は、取得が終わってから、結果に ISBN があるときだけ入れ、取得済みとして扱う（取得状態は ISBN の取得のとおり）。ISBN のない結果を選んだときは ISBN の欄を変えない（フォームにすでに ISBN があれば、その本は取得待ちで保存される）。オフラインなら、いまの「通信できませんでした」の知らせを出し、値は入ったまま取得待ちで保存できる。
- **「このアプリについて」のクレジット**（0022 §4 の残り）: クレジットの文の中の https の URL を、押すと既定のブラウザで開くリンクにした（`src/lib/linkify.ts`、`open_link`）。
- ブラウザのモック（`pnpm dev`）: `list_providers` と検索にダミーの結果を足した（`src/dev/mockSearch.ts`。書名か著者に「エラー」を含むと、Google が HTTP 503、Open Library が通信エラー）。結果を選ぶのは、`fetch_metadata` と同じく「モックでは使えません」。リンクは `window.open`。

### 5. Google Books のブランドの規約への対応

| 規約 | 対応 |
|---|---|
| 「powered by Google」のロゴを結果のそばに置く。Google を検索する検索欄にも帰属を示す（ブランドのページの「Search boxes」） | Google が配る画像（`https://books.google.com/googlebooks/images/poweredby.png`。62×30 の PNG、透過、3,441 バイト、SHA-256 `edf04c8a8d59da479bb8be1eed30057f7f7679b18249a2b54f69013817a2bc84`）を `src/assets/google/powered-by-google.png` に同梱し、手を加えずに、Google の欄の見出し（1 件以上あるとき）と、Google を探すときの検索の欄のそばに置く。暗い表示でも読めるよう、白い角丸の下地に載せる（画像そのものは変えない） |
| 1 件ごとに Google Books のページへの目立つリンク | Google の各行に「Google Books で見る」（`https://books.google.com/books?id=<ID>`、`open_link`） |
| 並べ替え・改変をしない | Google の結果は Google の順のまま、捨てない（`volumeInfo` のない項目だけ除く）。ほかの取得元の結果と混ぜず、Google の欄にまとめる |
| 結果の呼び名 | 見出しは「Google Books の検索結果」（規約の「Google Books search results」） |
| 競合するサービスのロゴと並べない | NDL・Open Library は名前の文字だけで、ロゴを出さない |

### 6. テストとフィクスチャ

- 実際の応答（2026-10-17。`crates/metadata/tests/fixtures/`）: `ndl_search_slowloop.xml`（11 件。並べ替えなしの順）、`ndl_search_kokoro_soseki.xml`（101 件のうち 20 件。ISBN のない版、ISBN-10 の形）、`ndl_search_war_and_peace_sorted.xml`（`sortBy` を連ねた形の応答）、`ndl_search_error_diagnostic.xml`（`sortBy` を末尾に付けて失敗した応答）、`openlibrary_searchkw_clean_code_martin.json`・`openlibrary_searchkw_kokoro.json`・`openlibrary_searchkw_slowloop.json`。
- **合成**: `google_synthetic_search_clean_code.json`（`volumes.list` の形。ID は `SYNTHETIC…`）。キーがないため、Google の検索の成功応答は取れていない。
- `crates/metadata/tests/search.rs`（条件の組み立てと整え方、`"`・`\`・`AND` などの語、空・長すぎる条件、NDL の結果の並び・ISBN・重複・該当なし・エラー、Google の順・リンク・ISBN-10・識別子のない項目・429/400、Open Library の版・リンク・書影、ISBN の取得が変わらないこと）。`src-tauri/src/metadata/search_tests.rs`（検索が DB に書かないこと、取得元ごとの状態、使えない取得元、ISBN ありの結果の取り直しと候補の足し方、ISBN ではどこにも見つからない本が `ok` になること、オフラインで取得待ち、ISBN なしの下書き、書影の取得、壊れた結果の拒否）。`open_link` の URL の検査の表。
- 実際の通信のテスト（`#[ignore]`）: `crates/metadata/tests/live.rs` の検索の 3 件（Google は `GOOGLE_BOOKS_API_KEY` が要る）。実装中に送った実際の問い合わせは、NDL の SRU が 10 回（応答の形の確認 6 回と、並べ替え・`organic`・`"` を除いた形・`war and peace` の 4 回）、Open Library が 6 回、Google がキーなしで 2 回（どちらも 429）。

## 理由

- 取得元ごとに欄を分けるのは、Google の規約が Google の結果の並べ替え・改変を禁じ、準拠した例も第三者の結果と混ぜていないため。ISBN でまとめて 1 行にすると、Google の結果の改変・並べ替えにあたる。
- NDL だけ並べ替えるのは、NDL の既定の順が使えない（先頭の 50 件が当てにならない）ため。新しい順にしたのは、登録したい本が新しい版であることが多く、漫画の巻も見つけやすいから。
- 文字を打つたびに探さないのは、NDL の並列・大量アクセスの注意と Open Library の作法に沿うため（1 回の操作で取得元ごとに 1 回）。
- ISBN のある結果をふだんの取得で取り直すのは、0022 の決定のとおり。検索の結果は取得元 1 つ分で項目も少ない（Open Library の検索はページ数・内容紹介を返さない、Google の検索は和書の出版社が空のことが多い）。取り直せなかったときに検索結果の値を足すのは、選んだ本の値が失われないようにするため。
- 書名と著者だけを選んだ本の値に置き換えるのは、検索の語として打った値（「スローループ 7」など）がそのまま残るのを避けるため。ほかの項目は「書誌を取得」（0011）と同じく、手で入れた値を残す。

## 影響・今後

### 要件書の修正案

（ユーザーの承認を得て、0.9 に反映した。2026-10-18）

- 7.3 FR-META-12 に、入口（手動登録の書名の横の「探す」。Enter か「探す」で探し、打つたびには探さない）、検索先（NDL・Google Books（キーがあるとき）・Open Library。取得元ごとに 1 回）、表示（取得元ごとに分け、Google の結果は混ぜず並べ替えない）、選んだとき（ISBN があればふだんの取得、なければ候補の値だけ。書名・著者は置き換え、手で入れたほかの項目は残す）を足す。
- 9.2 に「手動登録では、書名の横に『探す』（FR-META-12）を置く」。
- 10.6 に、Google Books の検索結果の表示の規則（ロゴ、1 件ごとのリンク、混ぜない・並べ替えない）と、「このアプリについて」のクレジットの URL を押すと既定のブラウザで開くことを足す。
- 11.2〜11.4 に、それぞれのキーワード検索の書式と注意（§2 の表）。
- 12 章のフェーズ 3 の受け入れ基準に AT-41〜AT-45。

### 確認したこと・未確認（ユーザーの環境で確かめる）

1. **（確認済み、2026-10-18）Google の検索の実際の応答。** ユーザーのターミナルで live のテスト 3 件を流し、すべて通った。Google は `clean code`・`martin` で 20 件（`totalItems` 350）が返り、どの結果にも書名があり、ISBN のある結果もあった。NDL は「スローループ」で 11 件、Open Library は 3 件。フィクスチャは合成のまま（実際の応答は保存していない）。手順（fish）:
   - キーは `set -x GOOGLE_BOOKS_API_KEY (security find-generic-password -s io.github.mioupa.bibcontainer -a google -w)` で読む。macOS の「ターミナル」で流し、キーチェーンの確認の画面で許可する。確認の画面を出せない環境（Claude のアプリのターミナルの欄など）では、`security` が終了コード 36（`errSecInteractionNotAllowed`）で何も返さない。fish の `env 名前=(…)` は、`(…)` が何も出力しないと引数ごと消えるので、エラーにならずに「キーがないので飛ばす」になる。
   - そのあと `cargo test -p bibcontainer-metadata --test live -- --ignored --test-threads=1 --nocapture keyword_search`。終わったら `set -e GOOGLE_BOOKS_API_KEY`。
2. 実機での見た目と操作（`docs/acceptance/phase3.md` の確認表）。

### そのほか

- NDL の新着の書誌（`iss-ndl-opac-inprocess`。受け入れから約 4 日で出る）は探していない。新しい本はバーコードがあるのがふつうで、ISBN の取得で取れるため。
- 検索の結果に書影の小さな画像は出していない（CSP を広げずに済み、和書の主な検索先の NDL には書影がないため）。必要になったら、書影 ID の URL（Open Library）と Google の書影だけを `img-src` に足す形を考える。
- 次の 50 件（NDL）・次の 20 件（Google・Open Library）を取る機能はない。見つからなければ語を足して絞る。
