# 0018: Google Books の ISBN 検索が 0 件のときの回避策（Dynamic Links → volumes/<ID>）

## 状態

採用（ユーザー指示、2026-10-16。要件書は 0.5 で 11.3 に反映した。実際のキーでの `volumes/<ID>` の応答は、この作業では確かめられていない。「影響・今後」を参照）

## 日付

2026-10-16

## 背景

2026-10-01 ごろから、Google Books API の `volumes?q=isbn:<ISBN>&key=…` が、有効なキーでも HTTP 200・`totalItems: 0` を返す（0017 の「背景」）。Google の報告窓口（Issue Tracker 569667437）は「Won't Fix (Infeasible)」で閉じられ、直る見込みが立たない。

0017 で Open Library を加えたが、Google にしかない書影・書誌（和書の書影の一部、洋書の大きい書影）は取れないままだった。2026-10-15 の調査で、Google の別の 2 つの窓口が ISBN で正しく動くことが分かった。

- **Dynamic Links**（公式。https://developers.google.com/books/docs/dynamic-links ）: `https://books.google.com/books?bibkeys=ISBN:<ISBN>&jscmd=viewapi&callback=cb` が、ISBN から Google の volume ID（`info_url` の `id=`）を返す（JSONP）。キーは要らない。
- **volume の取得**: `GET https://www.googleapis.com/books/v1/volumes/<ID>?key=…` は、ID を指定すれば書誌を返す（2026-10-15 にユーザーのキーで確認。キーなしは 429・割り当て 0）。

確認した公式情報（2026-10-16 に取得）: Dynamic Links のページ。要点:

- 書式は `bibkeys`（`ISBN:`・`OCLC:`・`LCCN:`。ISBN は 10 桁・13 桁とも可）、`jscmd=viewapi`、`callback`。応答は `bib_key`・`info_url`・`preview_url`・`thumbnail_url`・`preview`（`full`・`partial`・`noview`）・`embeddable`。
- 「キーなどの認可は要らない」。数値のレート制限の記載はなく、「問い合わせが多すぎると captcha を求められる」。
- 「サーバー側やオフラインの問い合わせ向けには作られていない」（閲覧の可否が利用者の場所で変わるため）。
- 「Google Books への帰属とリンクを保つこと」。廃止の告知はない（以前の名前は Book Viewability API）。

## 決定

### 1. 取得の流れ（`GoogleBooks::fetch_raw`）

1. これまでどおり `volumes?q=isbn:<ISBN>&key=…` を問い合わせる。HTTP エラーはこれまでどおり。書誌が取れた（`parse_google` が `Some`）か、解析できない応答なら、その応答を返す（回避策に進まない）。
2. **該当なしのときだけ**、Dynamic Links に問い合わせる（キーは送らない）。JSONP の括弧の中を JSON として読み、`ISBN:<ISBN-13>` の項目（なければ唯一の項目）の `info_url`（なければ `preview_url`・`thumbnail_url`）から `id` を取る。ID は `^[A-Za-z0-9_-]{4,32}$` のものだけを URL に使う（それ以外は解析エラー）。`cb({});`（項目なし）なら該当なし。HTTP エラー（captcha の 429 などを含む）や JSONP でない応答は、Google の取得全体のエラーにする（キャッシュされない）。
3. `volumes/<ID>?key=…` を取る。404 なら該当なし。ほかの HTTP エラーは、検索のエラーと同じに意味づけする（接続テストの「API キーが無効」などがそのまま働く）。200 なら、`volumeInfo.industryIdentifiers` にこの ISBN（ISBN-10 は 13 桁に直して比べる）があるか、識別子がまったくないときだけ採用する（検索の規則と同じ）。合わなければ該当なし。
- 生の応答（`RawResponse.body`）は、書誌が取れたときは volume の JSON そのまま、該当なしのときは検索の応答（`totalItems: 0`）。`parse_google` は検索の応答と 1 件の volume の両方を読む。以前のキャッシュ（検索の応答）も、そのまま読める。
- 通信はすべて `google` のグループ（1 秒に 1 回、同時接続 1）。障害の間は、1 冊につき検索・Dynamic Links・volume の 3 回（2 秒以上）。Google が直れば、Google にある本は 1 回目で見つかり、回避策は使われなくなる。Google にない本は、そのあとも Dynamic Links に 1 回多く問い合わせる（`cb({});` で終わる）。

### 2. 書影

- volume から取った書誌は、`volumeInfo.imageLinks` があるときだけ、ID から `https://books.google.com/books/content?id=<ID>&printsec=frontcover&img=1&zoom=0`、次に同じ URL の `zoom=1` を書影の候補にする。volume の `imageLinks` は `books/publisher/content?…&imgtk=…` の形で、動きを確かめていないので使わない。
- 2026-10-16 の確認:

  | ID | `zoom=0` | `zoom=1` |
  |---|---|---|
  | Clean Code `hjEFCAAAQBAJ` | 200 `image/jpeg` 2156×2774（本物） | 200 `image/jpeg` 128×170（本物） |
  | こころ `fz5TPAAACAAJ` | 200 `image/png` 575×750。SHA-256 が既知のダミー（`3efa8c43…`）と一致 | 200 `image/jpeg` 128×188（本物） |

  既存の「`zoom=0` を先に試し、ダミーなら次へ」（0007 §8）で、和書は `zoom=1` の本物に落ちる。

### 3. そのほか

- **Dynamic Links の HTTP エラーは「API キーが無効」と読まない。** キーを使わない窓口なので、メッセージの末尾に印（`DYNAMIC_LINKS_NOTE`）を付け、`classify_failure` は 401・403 でも `Other` にする。
- **誤った ISBN への備え。** `9784000000000`（NDL で別の本に誤登録された ISBN。0007 §1）を Dynamic Links に問い合わせると、別の本の ID（`zT3GEAAAQBAJ`）が返った。volume の ISBN を確かめるのはこのため。
- **内容紹介の HTML。** volume の `description` は検索の応答と違い HTML（`<p>`・`<br>`・`<i>`・`<ul>`・`<li>` など）を含む。`strip_html` を、段落（`<p>`・`<div>`・見出し・`<blockquote>`）は空行、`<br>` と箇条書きの項目は改行、そのほかのタグは除く形に直した。数値の文字参照（`&#NNN;`・`&#xHH;`）も戻す。空行は続けず、先頭・末尾の空行は除く。これまでの `"a<p>b</p><p>c</p>"` → `"ab\nc"` は `"a\n\nb\n\nc"` に変わった。
- テストのための接続先: `with_base_url` は API と Dynamic Links の両方を同じモックに向ける（既存のモックのテストが、該当なしの本で本物の books.google.com に問い合わせないように）。別々にするときは `with_base_urls`。

### 4. フィクスチャとテスト

- 実際の応答（2026-10-16）: `google_dynamiclinks_9780132350884.jsonp`（ID `hjEFCAAAQBAJ`）、`google_dynamiclinks_9784003101117.jsonp`（`fz5TPAAACAAJ`）、`google_dynamiclinks_9784000000000.jsonp`（誤 ISBN。`zT3GEAAAQBAJ`）、`google_dynamiclinks_notfound.jsonp`（`cb({});`。9784123456784）。どれも `200 application/javascript; charset=ISO-8859-1`。
- **合成**: `google_synthetic_volume_hjEFCAAAQBAJ.json`・`google_synthetic_volume_fz5TPAAACAAJ.json`（volume の形。HTML の内容紹介、`publisher/content…imgtk` の `imageLinks`、ISBN-10 と 13）。この作業の環境ではキーチェーンのキーを読めなかった（`errSecInteractionNotAllowed`）ため。検索の該当なしは、既存の `google_synthetic_notfound.json`。
- 流れのテスト（`crates/metadata/tests/google_fallback.rs`、偽の HTTP）: 検索で見つかれば 1 回、Dynamic Links が `{}` なら 2 回で該当なし、volume が合えば 3 回で取れる（書影は `zoom=0`→`zoom=1`、内容紹介の HTML を除く）、ISBN が合わない・404 は該当なし、Dynamic Links の 500・JSONP でない応答はエラーでキャッシュされない、volume の 400・403 は検索と同じ意味づけ、不正な ID は 3 回目を送らない、キーは Dynamic Links に送られない（wiremock で 2 つの接続先を分けて確かめる）。`src-tauri` の接続テストのテスト（回避策で見つかる、volume の 400 が `invalidKey`、Dynamic Links の 403 が `httpError`）。
- 実際の通信のテスト: `crates/metadata/tests/live.rs` の `live_google_books_finds_a_book_even_while_isbn_search_is_broken`（`#[ignore]`、`GOOGLE_BOOKS_API_KEY` が要る）。**まだ実行していない。**

## 理由

- Google の ISBN 検索が直る見込みがなく、Open Library では補えない書影・書誌があるため。
- 検索が該当なしのときだけ回避策に進むので、Google が直れば余分な通信はほぼなくなり、コードを戻す必要もない。
- Dynamic Links の「サーバー側・オフライン向けではない」は、閲覧の可否が利用者の場所で変わることへの注意で、利用者の PC から本を 1 冊ずつ問い合わせるこのアプリは、想定の範囲内と判断した。1 秒に 1 回の間隔を守り、大量に問い合わせない。
- volume の ISBN を確かめるのは、Dynamic Links が誤登録の ISBN でも別の本を返すため（実際に確かめた）。

## 影響・今後

### 未確認（ユーザーの環境で確かめる）

1. **実際のキーでの `volumes/<ID>` の応答で、解析が正しく働くか。** フィクスチャは合成。ユーザーのターミナル（キーチェーンの許可の確認が出せる）で、`live` のテストを実行する（fish。キーがないとテストは飛ばして成功になるので、`--nocapture` で「飛ばす」と出ていないことを見る）:
   `env GOOGLE_BOOKS_API_KEY=(security find-generic-password -s io.github.mioupa.bibcontainer -a google -w) cargo test -p bibcontainer-metadata --test live -- --ignored --test-threads=1 --nocapture live_google_books_finds_a_book_even_while_isbn_search_is_broken`必要なら、合成のフィクスチャを実際の応答に置き換え、内容紹介の文字列を比べるテストを直す。
2. アプリでの確認: 洋書（Clean Code など）と和書（こころなど）をスキャン登録し、Google の書影・書誌が入るか。設定画面の Google の接続テストが「接続できました」になるか（回避策で 3 回の通信、2 秒ほど）。

### 要件書の修正案（0.5 で反映済み、2026-10-16）

- 11.3 Google Books に「`q=isbn:` が 0 件のときは、Dynamic Links（`https://books.google.com/books?bibkeys=ISBN:<ISBN>&jscmd=viewapi`、キー不要）で volume ID を得て、`volumes/<ID>` を取る（2026-10 の障害への回避策。`docs/decisions/0018`）。volume の `industryIdentifiers` の ISBN が合うものだけを使う。書影は `books/content?id=<ID>&printsec=frontcover&img=1&zoom=0`（ダミーなら `zoom=1`）」を足す。

### そのほか

- Dynamic Links が captcha を返すようになったら（一括の再取得などで）、Google の取得はエラー（`partial`）になる。そうなったら、Dynamic Links の間隔を広げるか、一括の再取得では回避策を使わない形を考える。
- Google が直ったかは、これまでどおり `bash scripts/check-google-books.sh` で確かめる。直ったら、この回避策は使われなくなるだけなので、急いで消す必要はない。
