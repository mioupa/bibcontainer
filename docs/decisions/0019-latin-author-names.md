# 0019: 洋書（ラテン文字）の著者名の解析を直す

## 状態

採用（ユーザー指示、2026-10-16。0007 §4 の「著者と役割」の規則のうち、ラテン文字の名前に関わる部分をこの記録で改める）

## 日付

2026-10-16

## 背景

ユーザーから、洋書の著者が細かく分かれると報告があった（2026-10-16）。

- Clean Code（9780132350884）の著者が「Robert」「C.」「Martin」「...」「[et」「al.]」「foreword by James O. Coplien」の 7 人になる。
- Pride and Prejudice（9780141439518）では「edited with an introduction and notes by Vivien Jones」が 1 人の名前になる。
- 和書と、著者が 1 人の洋書（Gary S. Was）は正しい。

原因は NDL の解析にあった。責任表示 `dc:creator`（`Robert C. Martin ... [et al.] ; foreword by James O. Coplien`）を、和書の書き方（「山田玲子／著 水野菜生／英訳」のように空白で人を区切る）の前提で `parse_authors` に通していた。役割の語（著・訳など）がない部分では、空白で区切った語の 1 つ 1 つを別の人にしていた。NDL は 1 人ずつの構造化された名前（`dcterms:creator/foaf:Agent/foaf:name`。`Martin, Robert C.`、`Coplien, James O.`）も返しているのに、使っていなかった。著者の優先順位は openBD > NDL > Google > Open Library なので、洋書では NDL の壊れた著者が先に採用されていた。

2026-10-16 に NDL から取った実際の応答:

| ISBN | `dc:creator` | `foaf:name` |
|---|---|---|
| 9780132350884 | `Robert C. Martin ... [et al.] ; foreword by James O. Coplien` | `Martin, Robert C.`、`Coplien, James O.` |
| 9780141439518（2003 年版） | `Jane Austen ; edited with an introduction and notes by Vivien Jones ; with the original Penguin Classics introduction by Tony Tanner` | `Austen, Jane`、`Jones, Vivien`、`Tanner, Tony` |
| 9780141439518（2014 年版） | `Jane Austen ; edited with an introduction and notes by Vivien Jones` | `Austen, Jane`、`Jones, Vivien` |

## 決定

### 1. NDL: 責任表示がラテン文字だけなら、構造化された名前から著者を作る

- レコードの `dc:creator` に漢字・ひらがな・カタカナがなく（空でもなく）、`foaf:name` が 1 つ以上あれば、`foaf:name` の順に著者を作る（`normalize::parse_latin_statement_authors`）。それ以外（和書、`ドストエーフスキイ作 ; 米川正夫譯` や `Robert C. Martin 著 ; 花井志生 訳` のような翻訳書）は、これまでの解析のまま。
- `dc:creator` にあって `foaf:name` にない人は使わない（`... [et al.]` で省かれた人など）。`foaf:name` を正とする。
- ヨミは付けない（洋書の `foaf:Agent` には `dcndl:transcription` がない）。
- 1 つの ISBN に複数のレコードがあるときの選び方は変えていない。Pride and Prejudice は、NDC のある 2014 年版のレコードが選ばれる。

### 2. 名前の形: ラテン文字の「姓, 名」は「名 姓」にする

- 末尾の生没年（`, 1775-1817`、`, 1952-`、`, -1817` など）を除き、最初のコンマで「姓」と「名」に分けて `名 姓` にする（`Martin, Robert C.` → `Robert C. Martin`、`Smith, John, Jr.` → `John Smith Jr.`）。コンマのない名前（団体など）はそのまま（`normalize::latin_person_name`）。
- Google Books と Open Library の書き方（`Robert C. Martin`）にそろえ、編集フォームの「他の取得元の値」（FR-META-05）で同じ人が同じ形で並ぶようにするため。
- **0007 §4 の変更**: openBD の NDL 由来の著者（`姓,名,生没年`）のうち、漢字・かな以外の名前は、これまで `Tolstoy, Leo` の形にしていたが、`Leo Tolstoy` の形にする。テストの期待値を `Dostoyevsky, Fyodor` → `Fyodor Dostoyevsky` などに直した（openBD の 9784003261354・9784003261460）。漢字・かなの名前（`夏目,漱石,1867-1916` → `夏目漱石`）は変えていない。

### 3. 役割: 責任表示の英語から読む

`dc:creator` を `;` で区切り、各人の姓（`foaf:name` のコンマより前）を単語として含む最初の区切りから、役割を決める。

| 区切りの形 | 役割 |
|---|---|
| `by` がない（`Jane Austen`、`Robert C. Martin ... [et al.]`）、`by Jane Austen`、`written by` | 著 |
| `by` の前に `edited`・`editor(s)`・`ed.`・`compiled`・`compiler` | 編 |
| `translated`・`translator(s)`・`trans.` | 訳 |
| `illustrated`・`illustrations`・`illustrator(s)` | 絵 |
| `foreword`・`preface` | 序文 |
| `introduction`・`afterword`・`notes`・`commentary` | 解説 |
| `adapted` | 翻案 |
| 上のどれでもない、どの区切りにも名前がない | なし |

`by` の前に語が複数あるときは、最初に出てくる語で決める（`edited with an introduction and notes by Vivien Jones` は「編」）。

- 結果: Clean Code は `Robert C. Martin`（著）、`James O. Coplien`（序文）。Pride and Prejudice は `Jane Austen`（著）、`Vivien Jones`（編）。2003 年版のレコードなら、さらに `Tony Tanner`（解説）。
- 役割は自由な文字列（コアの検証は前後の空白を除くだけ、編集フォームの役割の候補（`<datalist>`）は入力の補助）なので、候補にない「序文」「翻案」もそのまま保存・表示できる。候補への追加はしていない。

### 4. 構造化された名前がないとき: ラテン文字の部分を空白で切らない

`parse_author_segment`（openBD の要約の著者など、ほかの呼び出し元も通る）で、漢字・かなを含まない区切りは、次のように読む（`parse_latin_segment`）。

- `...`・`…`・`[et al.]`・`et al.` を除く。
- 先頭の `<語> by ` を役割として外す（3 の表。`by` の前に語がない `by Jane Austen` は「著」）。`by` のない区切りは役割なし（これまでどおり）。
- 人は ` and `・` & ` だけで分ける（コンマでは分けない。`John Smith, Jr.` を分けないため）。
- 和書の区切りはこれまでどおり。

### 5. openBD の NDL 由来の著者: 名前の途中の空白で人を切らない

openBD の版元以外の本の `summary.author` は、人を空白で区切り、各人が `姓,名,生没年`（`Dostoyevsky,Fyodor,1821-1881 江川,卓,1927-2001`）。名に空白がある洋名（`Martin,Robert C.,1952-`）が人の区切りで切れないよう、空白で区切った語が「前の語がラテン文字で、この語がラテン文字で、コンマを含まないか、最初のコンマのあとが年・年の範囲だけ」のときは、前の人の続きとしてつなぐ（`group_person_tokens`）。漢字・かなの語（`米川/正夫` など）はつながない。

- ヨミの付け方（`attach_kana`）は、`名 姓` の形に変えた洋名でも ONIX の `collationkey` と照合できるようにした（9784003261460 の `Dostoevskii` のカタカナのヨミが消えないように）。

### 6. Google Books・Open Library は変えていない

どちらも、配列の 1 要素を 1 人として扱い、分解の処理に通していない（`google.rs`、`openlibrary.rs` の `names()`）。それを固定するテストを足した。

### 7. フィクスチャとテスト

- NDL: `ndl_9780132350884.xml`、`ndl_9780141439518.xml`（上の表の実際の応答。2003 年版の結果は、フィクスチャから 1 件目のレコードだけを残してテストする）、`ndl_9784048930598.xml`（Clean Code の日本語訳。下記）。
- openBD: `openbd_9784048930598.json`（同じ本。`hanmoto` の連絡先などは他の openBD のフィクスチャと同じく除いた）。
- 単体テスト: 名前の形（生没年、反転、`Jr.`、団体）、役割の語の表、ラテン文字の区切り（`Robert C. Martin ... [et al.]` → 1 人、`edited by A and B` → 2 人で「編」、`foreword by James O. Coplien` → 1 人で「序文」）、openBD の続きの規則、`Robert C. Martin 著 ; 花井志生 訳` → `Robert C. Martin` が 1 人のまま、Google と Open Library の名前が分かれないこと。

### 8. 日本語訳の本で、洋名の欠けた空白を補う（2026-10-16 追記、ユーザー指示）

**背景**: 日本語訳の本（978-4）では、元のデータ自体で洋名の空白が欠けている。Clean Code の日本語訳（9784048930598）では次のとおりだった。

| 取得元 | 著者の文字列 | それまでの結果 | 同じ応答の中の構造化された名前 |
|---|---|---|---|
| openBD | `summary.author` が `Martin,RobertC. 花井,志生` | `RobertC. Martin` | ONIX の `Contributor[].PersonName` が `Martin, Robert C.` |
| NDL（自館のレコード。選ばれる方） | `ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ 著 ; 花井志生 訳`。`foaf:name` も `ＲｏｂｅｒｔＣ．Ｍａｒｔｉｎ` | `RobertC.Martin`（著） | 同じ応答の CiNii のレコードの `foaf:name` が `Martin, Robert C.` |

和書の区分（0020）では openBD・NDL が先に採用されるので、優先順位では直らない。

**決定**（`normalize::restore_latin_name_spacing`。openBD と NDL の解析の最後に、漢字・かなを含まない著者名だけにかける。Google・Open Library にはかけない）:

1. **同じ応答の中の構造化された名前で置き換える。** openBD は ONIX の `Contributor[].PersonName`（文字列か `.content`）、NDL は応答の**全レコード**の `dcterms:creator/foaf:Agent/foaf:name`（選ばれなかったレコードも含む。出版社などは使わない）のうち、漢字・かなを含まないものを、`latin_person_name` で `名 姓` にして参照にする。照合キー（NFKC のあと小文字にし、英数字だけを残す。`robertcmartin`）が合う参照があれば、その綴りに置き換える。役割・ヨミは変えない（ヨミは置き換えの前に付ける）。
   - キーが合う参照が、空白・句読点だけが違う形で複数あれば、空白の多いものを使う（同数なら先のもの）。それ以外の違い（大文字小文字、アクセントなど）があれば、決められないので置き換えない。
   - キーが空（英数字がない。キリル文字など）の名前は照合しない。
2. **そのあと、頭文字のまわりの空白だけを補う**（`space_latin_initials`）。補うのは、小文字のすぐあとに頭文字（大文字 1 字とピリオド）が続く形（`RobertC.` → `Robert C.`）と、語頭の頭文字のすぐあとに大文字で始まる語が続く形（`C.Martin` → `C. Martin`、`J.R.R.Tolkien` → `J.R.R. Tolkien`）の 2 つだけ。`McDonald`・`DeMarco`・`MacArthur`・`St.John`・`O'Brien`、すでに空白のある名前は変えない。参照の綴りにも空白の欠けたもの（`Robert C.Martin`）がありうるので、置き換えたあとにもかける（整った名前には何もしない）。
3. 置き換えで同じ著者（名前・ヨミ・役割が同じ）が重なったら、先のものだけを残す。

**結果**:

| フィクスチャ | それまで | いま |
|---|---|---|
| openBD 9784048930598 | `RobertC. Martin` | `Robert C. Martin`（役割・ヨミなしは同じ。ONIX の `collationkey` が空のため） |
| NDL 9784048930598 | `RobertC.Martin`（著、ヨミ マーティン） | `Robert C. Martin`（著、ヨミ マーティン）。CiNii のレコードの `foaf:name` から |
| openBD 9784003261460 | `FyodorMikhailovich Dostoevskii` | `Fyodor Mikhailovich Dostoevskii`（ONIX の `Ｄｏｓｔｏｅｖｓｋｉｉ，Ｆｙｏｄｏｒ　Ｍｉｋｈａｉｌｏｖｉｃｈ` から。ヨミはそのまま） |
| openBD 9784003261354 | `Fyodor Dostoyevsky` | 同じ |

- テスト: 上の表、CiNii のレコードを除いた NDL の応答（2 の補いだけで `Robert C. Martin`）、ONIX の `Contributor` を除いた openBD の応答（同じ）、照合キー・置き換え（合う・合わない・決められない・空白の多い方）・頭文字の補い（直す形と、変えない形）の単体テスト。
- 頭文字の補いは推測を含むが、頭文字（大文字 1 字とピリオド）に接する 2 つの形に限ったので、正しい綴りを壊す例は見つけていない。`Ph.D.Smith` は `Ph.D. Smith` になる。

## 理由

- NDL がすでに 1 人ずつの名前を返しているので、文字列を推測で分解するより確実。和書の解析は実データで検証済み（0007）なので、責任表示がラテン文字だけのときに限って新しい経路に分けた。
- 名前の形を `名 姓` にそろえるのは、取得元のあいだで同じ人を同じ形にするため（Google・Open Library はもともと `名 姓`）。
- 役割を英語の責任表示から読むのは、和書と同じく「著」「編」「訳」などで一覧・書き出しに出すため。読めない語は推測しない（役割なし）。

## 影響・今後

- **既存の本は、「書誌を再取得」で直る**（手動の再取得はキャッシュを使わずに取り直す。0010 §9）。手で直した著者は上書きしない（`overrideManual` を選んだときだけ上書き）。
- **日本語訳の本で、元のデータの洋名の空白が欠けている件は、§8 で直した**（同じ日に追加）。
- 要件書の修正案: FR-META-06 の著者の例に「洋書は NDL の構造化された名前を使い、`名 姓` の形にそろえる（例: `Martin, Robert C.` → Robert C. Martin〔著〕、`foreword by James O. Coplien` → James O. Coplien〔序文〕）」を足す。
