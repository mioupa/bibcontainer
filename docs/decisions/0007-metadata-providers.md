# 0007: 書誌の取得元（openBD・NDL・Google Books）と書影の処理

## 状態

採用（NDL の書影は、ユーザー判断により 2026-10-14 に削除した。「改訂」を参照。未解決の点は「影響・今後」を参照）

## 日付

2026-10-14

## 背景

フェーズ 1 の「書誌の取得と書影の処理」を `crates/metadata`（`bibcontainer-metadata`）に実装した。要件書 0 章 4 に従い、実装の前に 3 つの外部 API（openBD、NDLサーチ、Google Books）の公式情報と実際の応答を確認した。確認の結果、要件書 11 章の記述と違う点がいくつか見つかったので、差異をここに記録する（要件書は直接直さず、修正案を「影響・今後」に書く）。

確認した公式情報（すべて 2026-10-14 に取得）:

- openBD: https://openbd.jp/ 、v1 提供終了の告知（2023-07-25）、代替書誌情報の告知（2023-08-29）、利用規約（/terms/）
- NDLサーチ: https://ndlsearch.ndl.go.jp/help/api （利用条件）、外部提供インタフェース仕様書 第 1.5 版（2026.9.25、SRU の章）、API 提供対象データプロバイダの一覧
- Google Books: https://developers.google.com/books/docs/v1/using 、getting_started、reference/volumes/list、/books/terms、/books/branding

## 改訂（2026-10-14、ユーザー判断）

実装後にユーザーが次のとおり判断した。本文のうち NDL の書影に関する記述は、確認の経緯を残すために消さず、削除済みである旨を添えてある。

- **NDL の書影 API は使わない（ユーザー判断により削除、2026-10-14）。** `Referer` のないリクエストが 403 になる件（「未解決 1」）は、`Referer` の偽装がアクセス制限の回避にあたるため、書影を諦めて解決とした。`crates/metadata` から `NdlThumbnail` 取得元（`providers/ndl_thumbnail.rs`）と `ProviderId::NdlThumbnail`・`MetaSource::NdlThumbnail` を削除し、書影の既定の優先順位は **openBD > Google** にした。取得元は 3 つ（`openbd`・`ndl`・`google`）。NDL の書誌（SRU）はそのまま使う。書影の取得（`acquire_cover`）に変更はなく、ダミー画像・画像でない応答（`Content-Type` が `image/*` でない）の判定は、openBD・Google の書影にも働く。NDL 書影の 403 の本文をおさめたフィクスチャ（`ndl_thumbnail_403.html`）も削除した。`ProviderInfo.needs_network` は、今の取得元がすべて `true` だが、通信しない取得元を足すときのために残した。
- **Google Books は API キーを使う。** ユーザーがキーを取得して、設定画面から入れる。キーがないときは Google を使わない。これを反映して、`GoogleBooks` の `ProviderInfo.auth` を `Optional` から **`Required`** に変えた（FR-META-03: 認証が必要な取得元は、キーが設定されたときだけ有効にできる。呼び出し側がキーの有無で有効・無効を決める）。「未解決 2」はこの判断で解決した。
- **openBD は使い続ける。** 実装は変えない。「未解決 3」の規約との整合は、引き続き記録として残す。
- **TLS の暗号プロバイダは ring にした。** reqwest の `rustls` 機能は aws-lc-rs を使い、Windows のビルドで NASM や CMake が要ることがあるため、reqwest を `rustls-no-provider` にして、`rustls`（`ring` 機能）を直接の依存にした。ring の既定のプロバイダは `http::install_crypto_provider()`（`ReqwestClient::with_timeout` が 1 回だけ呼ぶ）でプロセスの既定として入れる。`cargo tree` で `aws-lc-sys`・`aws-lc-rs` が依存から消えたことを確かめた。

## 決定

### 1. 公式情報の確認結果と、要件書 11 章との差異

#### openBD

| 項目 | 要件書 | 確認結果 |
|---|---|---|
| 提供状況 | v1 の提供終了が告知されたが、応答はある | 同じ。終了日は未定（「技術的な調整が完了次第」）。2023-08-28 から**書誌・書影の出どころを国立国会図書館の書誌に切り替え**、ONIX 形式に簡易変換して配信している（告知: 「一部情報が欠落している可能性」「書影の収録範囲は大幅に減少」）。代替 API は少なくとも 60 か月提供する予定と告知されている |
| 取れる項目 | 書名・著者・出版社・発行日・価格・Cコード・内容紹介・書影 | **版元ドットコム会員社の本だけ**。AT-10 の `9784780802047` は取れる。それ以外（`9784003101117` 岩波文庫、`9784101001012` 新潮文庫、`9784088725093` ONE PIECE）は NDL 由来で、Cコード・内容紹介・書影・ページ数がない |
| 著者 | `summary.author` | 会員社: `山田玲子／著 水野菜生／英訳`。NDL 由来: `夏目,漱石,1867-1916`、`Dostoyevsky,Fyodor,1821-1881 江川,卓,1927-2001`（役割なし、姓,名,生没年、人はスペース区切り）、全角英数や `米川／正夫` のゆれもある |
| 価格 | `PriceType` が `01` なら税抜 | NDL 由来は `310 (税込)` のように税込と明記されたものが `PriceType 01` に入る。`362` のように明記がないものもあり、税抜か税込かは判別できない |
| 書名 | `summary.title` | NDL 由来は `One piece 巻1`（巻次込み）、`人はなんで生きるか : 他四篇 : トルストイ民話集`（副書名込み） |
| 発行日 | — | `20140408`、`198905`、`1975-04`、`昭和32-01`、`1959-6-1959-11`、`1957 17刷-01`、`2002-11-5`、空 など |
| 規約 | 販促・紹介のために利用できる、改変しない、削除依頼に応じる | 同じ。加えて「キャッシュする場合は、変更をできるだけ早く反映する」「利用権の譲渡禁止」。クレジット表記とアクセス頻度の規定は規約ページになく、「API 仕様書」に従うとある（仕様書の数値は見つからなかった） |

#### NDLサーチ

| 項目 | 要件書 | 確認結果 |
|---|---|---|
| SRU のパラメータ | `recordSchema=dcndl&maximumRecords=5` | `recordSchema` は `dc`・`dcndl`・`dcndl_v3`。`maximumRecords` は既定 200、最大 500。`isbn=` は完全一致で、10 桁・13 桁のどちらを入れても両方に変換して探す。`onlyBib=true` で所蔵（`dcndl:Item`）を省ける。`recordPacking` の既定は `string`（XML がエスケープされた文字列）なので `xml` を指定する。→ **`recordSchema=dcndl&recordPacking=xml&onlyBib=true&maximumRecords=10`** を使う（応答が約 1/3 になる） |
| 応答の形 | `dcndl:volume`、`dcndl:seriesTitle`、NDC は `dcterms:subject` の NDC9／NDC10 | 巻次・シリーズ・書名は `rdf:Description/rdf:value`（と `dcndl:transcription`）の入れ子。NDC は `dcterms:subject rdf:resource=".../class/ndc9/596.3"`（`ndc10`・`ndc8` もある）か、`dc:subject`（型 `.../NDC8`）。著者は `dc:creator` が `山田玲子 著`、`川端康成著`、`尾田栄一郎∥著` のように役割付きで、ヨミは `dcterms:creator/foaf:Agent` の `foaf:name`（`山田, 玲子, 料理`）と `dcndl:transcription` にある |
| 複数レコード | 図書のレコードを選ぶ | 1 つの ISBN で NDL 自身（`R100000002`）、CiNii（NII）、公共図書館の MARC など複数返る。NII のレコードは価格・ページ数がないことがある。シリーズ全体を 1 件にまとめた NII のレコード（ISBN が 100 個並ぶ）も混じる |
| 誤った ISBN | — | `9784000000000` を探すと、別の本（`4-938853-04-3`）に**誤って登録された ISBN**（`dcterms:identifier` の型が `ErrorISBN`）で当たる。型が `ISBN` の識別子で一致するレコードだけを採用する |
| 該当なし | — | HTTP 200 で `numberOfRecords` がなく、`<diagnostics>` に `Record does not exist` |
| 全角 | — | 古い MARC 由来のレコードは `Ｏｎｅ　Ｐｉｅｃｅ`、`３９０円`、`２０７ｐ ; １８ｃｍ`、`１９９７．１２` のように全角（NFKC を先に掛ける） |
| 価格 | `dcndl:price` | `1300円`、`310円 (税込)`、`390円` |
| 利用条件 | 個人・非営利は申請不要、継続利用は届出を求められる、API 利用の表示が必要 | 同じ。アクセス頻度の数値、同時接続数の数値は示されていない。書影などコンテンツの条件は、書誌データの条件（CC BY 4.0 互換）とは別で、提供機関の条件を確認するよう案内されている |
| **書影 API**（ユーザー判断により削除、2026-10-14） | `GET https://ndlsearch.ndl.go.jp/thumbnail/{ISBN13}.jpg`、画像がなければ XML のエラー | **公式の API 仕様書・ヘルプに書影 API の記載は見つからなかった**（第三者の記事のみ）。実際には、`Referer` のないリクエスト（アプリからの直接アクセス）は CloudFront が **403（Request blocked）** で拒否した。この取得元は使わないことにした（「改訂」） |

#### Google Books

| 項目 | 要件書 | 確認結果 |
|---|---|---|
| API キー | 任意 | ドキュメントは「公開データへのリクエストにも API キーかアクセストークンが必要」としている。実際、**キーなしのリクエストは `429 RESOURCE_EXHAUSTED`（`Queries per day`、`quota_limit_value: "0"`、共有プロジェクト）で常に失敗した**（4 回とも）。実質的にキーは必須 |
| 割り当て | — | 確認したドキュメントのページに、1 日・1 分あたりの上限の記載はなかった（一般に言われる 1 日 1,000 件は、この調査では確認できていない） |
| 書影 URL | `imageLinks.thumbnail`、`http://` は `https://` にする | 同じ。さらに URL に `&edge=curl`（ページをめくった加工）が付くので外す。`zoom=1` が幅 128px 程度で、**`zoom=0` にすると大きい画像（実在の ID で 800x1237）が返る** |
| 「画像なし」 | 既知のハッシュと寸法で判定 | 存在しない ID の書影は、`zoom` によって 3 種類の「image not available」（PNG）が返る。下の「ダミー画像の判定」に SHA-256 と寸法を記録した |
| 規約 | — | ブランディング: 検索結果などには「Powered by Google」の帰属表示、結果の改変・並べ替えの禁止、Google Books ページへのリンク。「このアプリについて」のクレジットに入れる（`ProviderInfo.credit`） |

実際に取れた応答は、openBD（6 件 + 未収録）、NDL（5 件 + 未収録）、書影のダミー画像（3 種）。**Google Books の成功応答は取れなかった**（キーがないため）ので、`tests/fixtures/google_synthetic_*.json` は公式の `volumes` リソースの形に合わせた**合成**である。キーを環境変数 `GOOGLE_BOOKS_API_KEY` に入れて `cargo test -p bibcontainer-metadata --test live -- --ignored` を実行すると、実際の応答で確かめられる。

### 2. 取得元のトレイト（FR-META-01）

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;                 // openbd / ndl / google
    fn info(&self) -> ProviderInfo;             // 表示名・認証・取れる項目・レート制限・クレジット・対応状況
    async fn fetch_raw(&self, transport: &Transport, isbn13: &str) -> Result<RawResponse, FetchError>;
    async fn fetch_raw_batch(&self, transport: &Transport, isbns: &[String]) -> Vec<(String, Result<RawResponse, FetchError>)>;  // 既定は 1 件ずつ。openBD は 100 件ずつ 1 回で
    fn parse(&self, raw: &RawResponse) -> Result<Option<Candidate>, ParseError>;   // Ok(None) は該当なし
    async fn search(&self, transport: &Transport, query: &SearchQuery) -> Result<Vec<Candidate>, SearchError>;  // 既定は Unsupported（フェーズ 3）
}
```

- `fetch_raw` と `parse` を分ける。生の応答（`RawResponse.body` の文字列）を保存すれば、マッピングを直したときに通信なしで `parse` をやり直せる（FR-META-10）。openBD の生の応答は「その ISBN の要素 1 つの JSON」（未収録は `null`）、NDL は SRU の XML 全体、Google は JSON 全体。
- 取得元は 3 つ: `OpenBd`、`Ndl`、`GoogleBooks`（API キーは `GoogleBooks::new(Some(key))` で呼び出し側が渡す。認証は `Required`）。`default_providers(key)` で 3 つを作る。当初は NDL の書影（`NdlThumbnail`。書誌では通信せず、書影の URL を組み立てるだけ）もあったが、ユーザー判断により削除した（「改訂」）。
- 接続先を変える `with_base_url` がある（モックサーバでのテスト用）。

### 3. 生の応答のキャッシュ（FR-META-10）

- `RawCache { get, put }` を呼び出し側が実装する。`src-tauri` では `provider_cache(provider, isbn13, fetched_at, status, payload)` に対応させる: `provider` は `ProviderId::as_str()`、`payload` は `CacheEntry.body`、`status` は `found` が真なら `ok`、偽なら `not_found`、`fetched_at` は UNIX 秒（TEXT 列には RFC 3339 に直して入れてよい）。
- 有効期限は既定 30 日（`DEFAULT_CACHE_TTL`）。**該当なしは 1 日**（`DEFAULT_NOT_FOUND_TTL`。新刊は少し遅れて収録されるため）。`MetadataService::with_ttl` で変えられる。期限内なら通信しない。
- 保存するのは「解析に成功した応答」と「該当なし」だけ。通信エラー・HTTP エラー・解析エラーの応答は保存しない。キャッシュの応答が解析できなければ（マッピングを直したあとなど）使わずに取り直す。キャッシュの読み書きが失敗しても取得は止めない。
- `FetchRequest::force_refresh()` でキャッシュを読まずに取り直す（FR-META-13）。

### 4. 候補（`Candidate`）と正規化（FR-META-06）

`Candidate`: 書名、副書名、書名のヨミ、著者（名前・ヨミ・役割）、出版社、発行日、本体価格、参考の税込価格、Cコード、NDC、ページ数、大きさ、言語、内容紹介、シリーズ名、巻次、書影の URL（取得元つき・複数可）。

| 項目 | 規則 |
|---|---|
| 短い文字列（書名・著者名・出版社・シリーズ・巻次） | 前後の空白を除き、NFKC にそろえ、連続する空白を 1 つにする |
| 内容紹介 | NFKC は掛けない（句読点・記号が変わるため）。改行を `\n` にそろえて前後の空白を除くだけ。Google の簡単な HTML（`<br>`、`<b>`、`&amp;`）は除く |
| 書名と副書名 | 副書名が別にあればそのまま。なければ ISBD の区切り（` : `、最初の 1 か所）で分ける（NDL と、NDL 由来の openBD） |
| 著者と役割 | 下記 |
| 日付 | `YYYY`・`YYYY-MM`・`YYYY-MM-DD`。`20100701`、`201007`、`2010-07`、`2010.7`、`2010/7/1`、`2010年7月`、`1997.12-`、和暦（`昭和32-01`）、範囲（先頭の年月だけ使う）、年のあとに余計な文字が続くもの（年だけ）。読めなければ空（推測しない） |
| 価格 | 円だけ。`(税込)` と明記されていれば**参考の税込価格**、そうでなければ**本体価格**とみなす。明記がなければ税抜か税込か判別できないので、openBD の `PriceType 01` と NDL の価格は本体価格として扱う（推測であり、「影響・今後」に書いた） |
| ページ数・大きさ | NDL の `dcterms:extent`（`218p ; 15cm`、`xi, 210p ; 22cm`、`12, 356p`、`1冊 ; 21cm`、`19×26cm`）から分ける。ページ数は最後の「数字 + p」 |
| 言語 | ISO 639-1（`jpn` → `ja`）にそろえる |
| Cコード | 4 桁の数字だけ |
| NDC | NDC10 > NDC9 > NDC8 の順に新しい版を使う |
| 書名のヨミ | かなを含むものだけ（`One piece` のような綴りは入れない） |
| 書影の URL | `http://` は `https://` にする。Google は `edge=curl` を外し、`zoom=0`（大きい）を `zoom=1` より先にする |

著者の役割（`parse_authors`）:

- 扱う形（NFKC 後）: `山田玲子／著 水野菜生／英訳`、`山田玲子 著`、`川端康成著`、`尾田栄一郎∥著`、`米川, 正夫（訳・解説）`、`[岩波書店] [編]`、`ドストエーフスキイ作 ; 米川正夫譯`（`;`・`、`・改行で人を区切る）、`夏目,漱石,1867-1916`（役割なし。`姓,名,生没年` は生没年を捨てて `夏目漱石`、漢字・ひらがなだけなら詰め、それ以外は `Tolstoy, Leo` の形）。
- 役割の語: `著`・`作`・`文`・`絵`・`画`・`訳`（`譯` は `訳` にする）・`編`・`編著`・`監修`・`監訳`・`原作`・`作画`・`漫画`・`イラスト`・`解説`・`校注`・`英訳`・`〇語訳` など。`／` の右が役割の語でなければ（`米川／正夫`）名前の区切りとみなす。
- 名前の末尾の 1 文字が役割と同じ（`優作`、`美絵`）ときに誤って切らないよう、`作`・`文`・`絵`・`画` がくっついた形は、前がカタカナ・英字だけのときだけ役割とみなす。
- ヨミは、openBD は ONIX の `Contributor.PersonName.collationkey`、NDL は `foaf:Agent` の `dcndl:transcription` から、名前の照合キー（英数字とかなだけ）が合う著者に付ける。かなを含まないヨミは付けない。

### 5. 統合（FR-META-04／05）

`merge(isbn13, &candidates, second_tier, &priorities) -> MetaMerged`。

- 項目ごとに、優先順位の順に値のある取得元を探して最初のものを採用し（`chosen`）、ほかの取得元の値を優先順位の順に `others` に残す（採用した値と同じものは残さない）。値がない項目も結果に入る。
- 優先順位は `MetaPriorities`（`MetaGroup` ごとの `MetaSource` の並び）。引数で渡す。設定にない取得元は、既定の順で末尾に足す（取得元が増えても古い設定のまま動く）。既定は FR-META-04 の表から、未実装の楽天・Amazon を除いたもの:

  | 行 | 既定の順 |
  |---|---|
  | 書名・副書名、著者、出版社、言語 | openBD > NDL > Google |
  | 書名のヨミ、NDC | NDL |
  | 発行日 | openBD > NDL > Google（楽天を除く） |
  | 本体価格 | **2段目（barcode）** > openBD > NDL |
  | 参考の税込価格 | openBD > NDL（要件書にない行。税込と明記された価格だけ） |
  | Cコード | **2段目** > openBD |
  | ページ数・大きさ | NDL > openBD > Google |
  | シリーズ・巻次 | openBD > NDL |
  | 内容紹介 | openBD > Google |
  | 書影 | openBD > Google（NDL 書影は削除。「改訂」） |

- 書名と副書名は同じ取得元から取る（別の取得元の副書名を混ぜると書名と合わなくなる）。採用した書名の取得元に副書名がなければ副書名は空で、他の取得元の副書名は `others` に回る。
- 2段目のバーコードは `MetaSecondTier { c_code, list_price }` で渡し、`MetaSource::Barcode` として扱う。価格が「不明」（`None`）なら、次の取得元の価格を使う。
- 採用した書名が採用した巻次で終わっているとき（openBD の `One piece 巻1` と NDL の巻次 `巻1`）は、書名から巻次を除く。
- 採用した取得元は `MetaSource::as_str()`（`barcode`・`openbd`・`ndl`・`google`）で `field_sources.<項目名>`（項目名は `MetaField::key()`）に使える。
- 手で直した項目を上書きしない（FR-META-07）のは呼び出し側の役目。`MetaMerged` は全項目を返すので、呼び出し側が `manual_fields` を見て捨てる。

### 6. 取得の流れ（`MetadataService`）

- `fetch(&FetchRequest) -> MetaFetchResult`、`fetch_many(&[FetchRequest])`。有効な取得元（`FetchRequest.enabled`）に**並行して**問い合わせ、結果を統合する。ISBN は ISBN-13 にそろえる（ハイフン・全角・ISBN-10 も可）。ISBN として読めなければ通信しない。
- `fetch_many` は、まとめて取れる取得元（openBD）を 1 回の通信（100 件まで）にし、そうでない取得元は 1 件ずつ、レート制限を守って取る。
- 取得元ごとの状態 `ProviderOutcome`: `found`（成功）、`notFound`（該当なし）、`networkError`、`httpError {status}`、`parseError`。あわせて `fromCache` と `network`（書誌で通信する取得元か）を持つ。
- **`offline`**: 書誌で通信する取得元（`network` が真）が 1 つ以上あり、**そのすべてが `networkError`** のとき真。呼び出し側はこれで `meta_status` を `pending` にする（AT-11）。HTTP エラーが返ってきた（サーバにはつながっている）ときは偽。`has_error()`（1 つでも失敗した）と `found_count()`、`merged.is_empty()`（書誌の値が 1 つもない。書影の URL は数えない）で `partial`・`failed` の判断ができる。
- 1 つの取得元が落ちても、ほかの結果で続ける。

### 7. 通信の作法（`Transport`、FR-META-08／09／11）

- レート制限は「グループ」ごと。リクエストの開始間隔と同時接続数を守る。

  | 取得元 | グループ | 間隔 | 同時接続 |
  |---|---|---|---|
  | openBD | `openbd` | 1 秒 | 1 |
  | NDL | `ndl` | 1 秒 | **1（並列にしない）** |
  | Google Books（書誌・書影） | `google` | 1 秒 | 1 |
  | ユーザーが入力した画像 URL | `user_url` | なし | 8 |

  openBD の 1 秒は、規約・仕様書に数値の上限が見つからなかったため、要件書 11.1 の「1 秒に 1 回程度」に従った決め。
- タイムアウトは 10 秒（接続も 10 秒）。リトライは、タイムアウト・応答の受信の失敗・429・500・502・503・504 を、1 秒・2 秒・4 秒の指数バックオフで最大 3 回（最初の 1 回を入れて 4 回）。**接続そのものが確立できないエラー（DNS の解決失敗・接続拒否・到達不能）はリトライしない**（オフラインで 1 冊ごとに 7 秒待たないため。0010 §13）。`Retry-After` があれば、それ以上待つ（最長 60 秒）。やり直しても 5xx・429 のままなら、その応答を返す（`httpError`）。応答が得られなければ `networkError`。**429 のうち「1 日あたりの上限」（Google のキーなし）は、待っても回復しないのでリトライしない。**
- User-Agent は `user_agent(版, 連絡先 URL)` で作った `bibcontainer/<版> (+<URL>)`。連絡先は呼び出し側が渡す（既定は 0005 のとおり `https://github.com/mioupa/bibcontainer`）。`Referer` は付けない。
- ログと結果に出す URL・エラーメッセージは、`key=`・`token=`・`access_token=`・`applicationId=` などの値を `***` に置き換える。reqwest のエラーからは URL を外してから文章にする。
- 応答本体の上限は、テキスト 8 MiB、画像 20 MiB。
- TLS は rustls（暗号プロバイダは ring。「改訂」）。`system-proxy` を有効にしている（大学などのプロキシ環境のため）。

### 8. 書影（FR-COVER-01〜05）

- **取得**: `MetaMerged::covers_in_order()` の URL を順に試し、最初に使える画像を保存する（`acquire_cover`）。ダミー画像・画像でない応答・HTTP エラーなら次の URL へ進む。結果は試した URL ごとの状態（`saved`・`placeholder`・`notAnImage`・`networkError`・`httpError`・`decodeError`）と、`offline`（試した URL がすべて通信エラー）。
- **ダミー画像の判定**:
  - NDL: `Content-Type` が `image/*` でなければ（XML のエラーなど）画像なし。
  - Google Books: 確認した「image not available」の PNG と、**ダウンロードした元のバイト列の SHA-256 と寸法が両方合う**ものをダミーとする（`KNOWN_PLACEHOLDERS`）。再エンコードした画像は別物として扱う。

    | `zoom` | 寸法 | SHA-256 |
    |---|---|---|
    | 1（thumbnail） | 128x170 | `e3f8c414b288cbdf4e6d1e00eb6d3826157d10a5b5628b9318f726ea490eca12` |
    | 0・2・3・4 | 575x750 | `3efa8c43e5b4348f303a528c81adf435f0111ea752fe9f0f6241478b60987fa6` |
    | 5（smallThumbnail） | 57x80 | `3abd7c366bd89e9e5e2fb472424b1245aacc948a34a751abca63381067628585` |

    この 3 つは `tests/fixtures/google_noimage_*.png` に実物を置き、`live` のテスト（`--ignored`）で今も一致するか確かめられる。
  - そのほか、長辺 40px 未満の画像（追跡用の 1px 画像など）は書影として使わない。
- **変換**: JPEG・PNG・GIF・WebP を読み、長辺 800px 以下（小さい画像は拡大しない）、JPEG 品質 85 に変換する。デコードして再エンコードするので EXIF・XMP・IPTC・コメントは残らない。EXIF の向きは先に画素へ反映する。透過は白地に重ねる。ICC プロファイルは残さない（sRGB 以外の画像は色が変わり得る）。幅・高さの上限 16384px、メモリ 512 MiB でデコードし、巨大な画像で止まらないようにする。
- **保存**: `<dir>/<ハッシュの先頭 2 文字>/<ハッシュ>.jpg`。ハッシュは**変換後の JPEG の SHA-256**（小文字 16 進 64 桁）。同じハッシュのファイルがあれば書かない。一時ファイルに書いてから改名する。`dir` は引数（`app_local_data_dir/covers`、0005 の 7）。
- **サムネイル**: `ensure_thumbnail(cover_dir, cache_dir, hash, 160 または 320)` が、`<cache_dir>/<長辺>/<先頭 2 文字>/<hash>.jpg` を返す。なければ書影から作る（品質 80、拡大しない）。ハッシュは 64 桁の小文字 16 進だけを受け付ける（パスの組み立てに使うため）。
- **手動の差し替え**: `import_cover_bytes`（ファイルのドロップ・クリップボード）、`import_cover_url`（ユーザーが入力した http・https の URL。画像でない応答は拒否）。どちらも同じ変換・保存。

### 9. IPC に出す型（ts-rs）

`Candidate`、`CandidateAuthor`、`CandidateCover`、`ProviderId`、`ProviderInfo`、`ProviderAuth`、`ProviderRateLimit`、`ProviderOutcome`、`ProviderReport`、`MetaSource`、`MetaField`、`MetaGroup`、`MetaPriorities`、`MetaSecondTier`、`MetaValue`、`MetaPick`、`MetaFieldChoice`、`MetaMerged`、`MetaFetchResult`、`MetaCoverResult`、`MetaCoverAttempt`、`MetaCoverAttemptResult`、`MetaSavedCover`。`cargo test` で `src/bindings/` に出る。フィールド名は camelCase、タグつきの列挙は `kind`（`MetaValue` は `type` と `value`）。

### 10. フィクスチャ（`crates/metadata/tests/fixtures/`）

- openBD: `openbd_batch.json`（4 ISBN の実際の応答）と、ISBN ごとの要素（`9784780802047`、`9784003101117`、`9784101001012`、`9784088725093`、複数著者の `9784003261354`・`9784003261460`）、未収録（`openbd_null.json`、`openbd_notfound_response.json`）。版元の連絡先・書評者名を含む `hanmoto.hanmotoinfo`・`hanmoto.reviews`・`hanmoto.jyuhan` は取り除いた（解析では使わない）。
- NDL: `ndl_<ISBN>.xml`（`onlyBib=true` の実際の応答。4 ISBN と、誤 ISBN で当たる `9784000000000`）、`ndl_notfound.xml`（該当なし）。
- NDL 書影: 削除した（「改訂」）。403 の本文のフィクスチャ（`ndl_thumbnail_403.html`）も削除し、書影の取得のテストは、403・XML のエラー・ダミー画像を、テスト内で作った応答で確かめている。
- Google: `google_quota_exceeded.json`（キーなしで実際に返った 429。プロジェクト番号は伏せた）、`google_noimage_*.png`（実物のダミー画像 3 種）。`google_synthetic_*.json` は**合成**（上記）。
- 実在の表紙画像は置かない。画像処理のテストは、テスト内で作った画像を使う。

## 理由

- 取得元の応答が要件書の想定と大きく違っていた（openBD の NDL 由来、NDL の複数レコードと誤 ISBN、Google のキー必須、NDL 書影の 403）。実際の応答をフィクスチャにして正規化・選択・統合のルールを決めたので、ルールは実データで検証されている。
- 生の応答を保存して `parse` を分けたのは、要件書 FR-META-10 のとおり。openBD のように中身が今後も変わりそうな取得元で、マッピングの修正が通信なしで済む。
- 2段目を「取得元」として統合に入れたのは、要件書の優先順位表（2段目が本体価格と Cコードで最優先）をそのまま設定で変えられるようにするため。
- 書名と副書名を同じ取得元から取るのは、NDL 由来の openBD と版元データで副書名の粒度が違い、混ぜると不自然になるため。
- ダミー画像をハッシュと寸法の両方で判定するのは、要件書 FR-COVER-02 のとおり。ハッシュだけだと別の画像との衝突、寸法だけだと本物の小さい書影を誤って捨てる恐れがある。

## 影響・今後

### 未解決（ユーザーの判断が要る）

1. **（解決済み。ユーザー判断で NDL の書影は削除、2026-10-14）NDL の書影 API が使えない。** `Referer` のないリクエストは 403。`Referer: https://ndlsearch.ndl.go.jp/` を付けると 200（`image/jpeg`）で返ったが、アプリの連絡先 URL や別のサイトの `Referer` では 403 のままなので、NDL 自身のページからの参照だけを許す設定（ホットリンク対策）とみられる。`Referer` を偽装すれば取れるが、アクセス制限の回避にあたるので**実装していない**（実装は `Referer` を付けない）。選択肢は、(a) NDL に利用を申し出る（継続利用の届出の窓口がある。13 章）、(b) 偽装を許容する、(c) NDL の書影は諦め、openBD と Google の書影を使う。ユーザーは (c) を選んだ。確認のため、診断として `Referer` 付きのリクエストを数回送っている（成功・失敗の切り分けだけで、画像は保存していない）。
2. **（解決済み。ユーザーがキーを取得して設定画面から入れる、2026-10-14）Google Books は API キーが実質必須。** 要件書の「任意」を「実質必須（キーなしは常に 429）」に直す提案をする。キーを入れるまで Google は使わない。キーを入れた状態の実際の応答は未確認（`live` のテストで確認できる）。
3. **openBD の利用規約との整合。** 規約は「データの任意の改変禁止」「利用目的は本の販促・紹介」「キャッシュしたら変更をできるだけ早く反映」。要件書の正規化（NFKC、役割の分離）は書式の整形で内容の改変ではないという解釈で実装した（生の応答は無改変でキャッシュに残る）。個人の蔵書管理が「販促・紹介」にあたるかは、規約から判断できない。
4. **価格が本体か税込かの判別。** 明記のない価格を本体価格として扱っているが、NDL 由来の古い本（例: 新潮文庫 `雪国` 362 円）では税込の可能性がある。2段目（バーコードの価格）が最優先なので、通常の運用（ISBN と 2段目をそろえて読む）では影響が小さい。

### 要件書の修正案

- 7.3 FR-META-02: openBD の「主に取れる情報」に「版元ドットコム会員社の本のみ。それ以外は NDL 由来で、Cコード・内容紹介・書影がない」を足す。Google の認証を「任意」から「API キー（実質必須）」に。
- 11.1: 上の「openBD」の表の内容（NDL 由来の簡易変換、著者・価格・書名・日付の表記）。
- 11.2: SRU のパラメータ（`recordPacking=xml`、`onlyBib=true`）、応答の形（入れ子、NDC の表記）、複数レコードと誤 ISBN、該当なしの形。書影 API は公式の仕様書にないことと、403 の件。
- 11.3: キーなしは 429（割り当て 0）。書影 URL は `edge=curl` を外し、`zoom=0` で大きい画像が取れる。
- FR-COVER-02: Google のダミー画像が 3 種類あること（上の表）。

### 実装する側（`src-tauri`）への申し送り

- `RawCache` を `provider_cache` で実装する（3 の対応表）。`MetadataService::new(default_providers(key), Arc::new(Transport::new(Arc::new(ReqwestClient::new(&user_agent(版, 連絡先))?))))`。API キーはキーチェーンから読んで `default_providers` に渡す。キーをログ・DB・設定に出さない。
- `meta_status` の目安: `offline` → `pending`。書誌の値があり `has_error()` → `partial`。`merged.is_empty()` で、すべて `notFound` → `failed`（再取得は手動）、エラーがあれば `pending`。取得待ちの本を再取得するときは `force_refresh` は使わず、キャッシュ（`not_found` は 1 日）に任せる。
- 取得結果を `books` に書くときは、`manual_fields` の項目を除き、`field_sources.<MetaField::key()>` に `MetaSource::as_str()` を `record_change` で記録する。`series` は系列名なので、`series` テーブルの行に解決してから `series_id` を書く。`volume_sort` は `volume`（`巻1`、`巻一`、`上`）から作る（このクレートでは作らない）。
- 書影は、`acquire_cover` の結果の `saved.hash` を `cover_hash` に書く。`offline` なら書影も取得待ちにする。`covers/` と、サムネイルのキャッシュのフォルダは `src-tauri` が決めて、このクレートの関数に渡す。画像処理とファイルの書き込みはブロックするので、`import_cover_bytes_async` か `spawn_blocking` 経由で呼ぶ（`acquire_cover`・`import_cover_url` は内部でそうしている）。`ensure_thumbnail` は同期関数なので、`cover://` スキームの処理から呼ぶときは別スレッドで。
- 一括の再取得（FR-META-13）は `fetch_many`。NDL と Google は 1 件ずつ 1 秒あけるので、100 冊で 100 秒以上かかる。進捗を出すなら、数十件ずつに分けて呼ぶ。
- 「このアプリについて」のクレジットは `MetadataService::providers_info()` の `credit` を使う。

### そのほか

- TLS の暗号プロバイダは、Windows のビルドで C の依存（NASM・CMake）が要る aws-lc-rs を避けるため、ring にした（「改訂」）。ring も C のコードを含むが、MSVC があればビルドできる。CI の windows-latest で通るかは未確認。
- キーワード検索（FR-META-12）はフェーズ 3。トレイトの `search` が枠だけ（`Unsupported`）ある。
- 楽天・Amazon は未実装。トレイトに足すだけで、優先順位の表（`default_order`）に行を足せば統合に入る。
- 実際のネットワークを使うテストは `tests/live.rs`（`#[ignore]`）。`cargo test -p bibcontainer-metadata --test live -- --ignored --test-threads=1`。
