// 「同じ作品の巻」を見分けるための、書名・著者の正規化（FR-LIST-08）。React に依存しない純粋関数。
//
// 書名の末尾の巻の表記（`スローループ 7`、`X 第3巻`、`X(7)`、`X vol.7`、`罪と罰 上` など）を取り除いた「作品名」と、
// 並べ替えに使う比べ方（`titleKey`）、著者の比べ方（`authorKeys`）を、行ごとに 1 度だけ作る（行オブジェクトで WeakMap に覚える。
// 行は、変わると別のオブジェクトに差し替わる）。同じ作品かどうかの判定（グループ化）は grouping.ts。
//
// 取りすぎないことを優先する。区切りの空白（か `.`・括弧）のない末尾は巻とみなさない（`坂の上`・`1Q84`・`2001年宇宙の旅`）。
import type { ListRow } from "@/types/contracts";

export interface WorkInfo {
  /** 末尾の巻の表記を取り除いた書名（表示用）。 */
  baseTitle: string;
  /**
   * `row.volume` が空で、書名の末尾から巻の表記を取り除けたとき、その巻の番号（`スローループ 8` の 8）。
   * それ以外は null。巻の並べ替えと範囲の表示に使う。
   */
  impliedVolume: number | null;
  /** 作品名の比べ方（正規化済み）。空なら、その行はグループにしない。 */
  titleKey: string;
  /** 著者ごとの比べ方（正規化済み。空のものは除く）。 */
  authorKeys: readonly string[];
}

// ---- 巻の表記のパターン ----

const NUM = "[0-9０-９]+(?:[.．][0-9０-９]+)?";
const KANJI_NUM = "[一二三四五六七八九十百千〇零]+";
const OPEN = "[(（\\[［]";
const CLOSE = "[)）\\]］]";

/** `vol` のような英字の語を、大文字・小文字・全角のどれでも読めるパターンにする。 */
function word(text: string): string {
  return [...text]
    .map((ch) => {
      const upper = ch.toUpperCase();
      const full = (c: string) => String.fromCharCode(c.charCodeAt(0) + 0xfee0);
      return `[${ch}${upper}${full(ch)}${full(upper)}]`;
    })
    .join("");
}

// 末尾の文字が、どのパターンの終わりにもなりえないなら、正規表現を試さない（大半の書名はここで終わる）
const MAY_END_WITH_VOLUME = /[0-9０-９)）\]］巻集部編上中下]$/;

interface Pattern {
  regex: RegExp;
  /** マッチから巻の番号を取り出す。 */
  volume: (match: RegExpExecArray) => number | null;
}

const PATTERNS: readonly Pattern[] = [
  // ラベルつき（P1 の「空白 + 数」より先に試す）
  // 空白 + vol.|vol|volume|no.|no|#|巻 + 空白? + 数   例: X vol.7 / X Vol 3 / X No.2 / X #5 / X 巻1
  {
    regex: new RegExp(
      `\\s+(?:${word("volume")}|${word("vol")}[.．]?|${word("no")}[.．]?|[#＃]|巻)\\s*(${NUM})$`,
    ),
    volume: (m) => parseNumber(m[1]),
  },
  // 空白 + 第? + 数 + 巻|集|部|編?   例: X 7 / X 第7巻 / X 7巻 / X ７
  { regex: new RegExp(`\\s+第?(${NUM})(?:巻|集|部|編)?$`), volume: (m) => parseNumber(m[1]) },
  // 括弧に入った 第? + 数 + 巻?   例: X(7) / X（7） / X [7]
  {
    regex: new RegExp(`\\s*${OPEN}\\s*第?\\s*(${NUM})\\s*巻?\\s*${CLOSE}$`),
    volume: (m) => parseNumber(m[1]),
  },
  // 空白 + 第 + 漢数字 + 巻   例: X 第三巻
  {
    regex: new RegExp(`\\s+第(${KANJI_NUM})巻$`),
    volume: (m) => parseKanjiNumber(m[1] ?? ""),
  },
  // 空白 + 上|中|下|前編|後編|上巻|中巻|下巻   例: 罪と罰 上
  {
    regex: /\s+(上巻|中巻|下巻|前編|後編|上|中|下)$/,
    volume: (m) => PART_VOLUME[m[1] ?? ""] ?? null,
  },
];

const PART_VOLUME: Record<string, number> = {
  上: 1,
  上巻: 1,
  前編: 1,
  中: 2,
  中巻: 2,
  後編: 2,
  下: 3,
  下巻: 3,
};

function parseNumber(text: string | undefined): number | null {
  if (text === undefined) return null;
  const value = Number(text.normalize("NFKC"));
  return Number.isFinite(value) ? value : null;
}

const KANJI_DIGIT: Record<string, number> = {
  〇: 0,
  零: 0,
  一: 1,
  二: 2,
  三: 3,
  四: 4,
  五: 5,
  六: 6,
  七: 7,
  八: 8,
  九: 9,
};
const KANJI_UNIT: Record<string, number> = { 十: 10, 百: 100, 千: 1000 };

/** 漢数字を数にする（`三` → 3、`十二` → 12、`二十` → 20、`百五` → 105、`二〇` → 20）。読めなければ null。 */
export function parseKanjiNumber(text: string): number | null {
  if (text === "") return null;
  // 十・百・千を含まなければ、位取りの書き方（二〇 → 20）
  if (![...text].some((ch) => ch in KANJI_UNIT)) {
    let value = 0;
    for (const ch of text) {
      const digit = KANJI_DIGIT[ch];
      if (digit === undefined) return null;
      value = value * 10 + digit;
    }
    return value;
  }
  let total = 0;
  let current = 0;
  for (const ch of text) {
    const digit = KANJI_DIGIT[ch];
    const unit = KANJI_UNIT[ch];
    if (digit !== undefined) current = digit;
    else if (unit !== undefined) {
      total += (current === 0 ? 1 : current) * unit;
      current = 0;
    } else return null;
  }
  return total + current;
}

// ---- 正規化 ----

/** NFKC で字が変わりうる文字（全角の英数・半角カナ・全角空白・丸数字・互換文字など）。これがなければ NFKC を試さない。 */
const NFKC_SENSITIVE =
  /[\u00a0\u00a8\u00aa\u00b2-\u00b5\u00b8-\u00ba\u00bc-\u00be\u2000-\u200a\u2070-\u209f\u2100-\u218f\u2460-\u24ff\u3000\u3036\u3038-\u303a\u3200-\u33ff\uf900-\ufaff\ufb00-\ufb4f\ufe30-\ufe6f\uff00-\uffef]/;

const WHITESPACE_AND_DOTS = /[\s・･·]/g;
const AUTHOR_NOISE = /[\s・･·,，、.．]/g;
const KATAKANA = /[ァ-ヶヽヾ]/g;

/** 作品名の比べ方: NFKC → 小文字 → カタカナをひらがなに（長音符 ー はそのまま）→ 空白と中黒を除く。 */
export function normalizeWorkTitle(text: string): string {
  return text
    .normalize("NFKC")
    .toLowerCase()
    .replace(KATAKANA, (ch) => String.fromCharCode(ch.charCodeAt(0) - 0x60))
    .replace(WHITESPACE_AND_DOTS, "");
}

/** 著者名の比べ方: NFKC → 小文字 → 空白・中黒・読点・ピリオドを除く（`山田 太郎` と `山田太郎`、`J.K. Rowling` と `JK ROWLING` が同じになる）。 */
export function normalizeAuthorName(text: string): string {
  return text.normalize("NFKC").toLowerCase().replace(AUTHOR_NOISE, "");
}

// ---- 末尾の巻の表記を取り除く ----

/** 末尾の巻のラベル（`vol`・`No`・`#`・`第`・`巻`）。巻の番号だけを取ったあとに残ったものを片づける。 */
const TRAILING_LABEL = new RegExp(
  `\\s+(?:${word("volume")}|${word("vol")}[.．]?|${word("no")}[.．]?|[#＃第巻])$`,
);

/** `rest`（書名から巻を除いた残り）が空白か `.` で終わるなら、その区切りと、巻のラベルを除いた書名。そうでなければ null。 */
function cutSeparator(rest: string): string | null {
  const last = rest.charAt(rest.length - 1);
  let base: string;
  if (last === ".") base = rest.slice(0, -1);
  else if (/\s/.test(last)) base = rest;
  else return null;
  base = base.replace(/\s+$/, "").replace(TRAILING_LABEL, "").replace(/\s+$/, "");
  return base === "" ? null : base;
}

/** 書名の末尾が `volume`（巻次の欄）と同じなら、それと区切りを除いた書名。NFKC で同じになる場合（全角の数字など）も読む。 */
function stripByVolume(title: string, volume: string): string | null {
  if (title.endsWith(volume)) return cutSeparator(title.slice(0, title.length - volume.length));
  // 素の比較で合わなかったときだけ、NFKC で比べる（全角と半角の数字・英字の違いなど）
  if (!NFKC_SENSITIVE.test(title) && !NFKC_SENSITIVE.test(volume)) return null;
  const normalizedVolume = volume.normalize("NFKC");
  if (!title.normalize("NFKC").endsWith(normalizedVolume)) return null;
  // 元の書名の、どこから後ろが巻次に当たるか（NFKC で長さが変わるので、後ろから 1 文字ずつ伸ばして探す）
  for (let k = 1; k <= title.length; k++) {
    if (title.slice(-k).normalize("NFKC") === normalizedVolume) {
      return cutSeparator(title.slice(0, title.length - k));
    }
  }
  return null;
}

/**
 * 書名の末尾の巻の表記を取り除く。
 * - `volume`（巻次の欄）が空でなく、書名がそれで終わる（直前が空白か `.`）なら、それを取り除く。`impliedVolume` は null。
 * - そうでなければ、末尾の巻の表記（PATTERNS）を 1 つだけ取り除く。巻次の欄が空のときは、取り除いた巻の番号を `impliedVolume` に入れる。
 * - 取り除くと空になるときは、元の書名のまま（巻の表記なしとして扱う）。
 */
export function stripTrailingVolume(
  title: string,
  volume: string | null,
): { baseTitle: string; impliedVolume: number | null } {
  const trimmed = title.trim();
  const unchanged = { baseTitle: trimmed, impliedVolume: null };
  const volumeText = volume?.trim() ?? "";

  if (volumeText !== "") {
    const base = stripByVolume(trimmed, volumeText);
    if (base !== null) return { baseTitle: base, impliedVolume: null };
  }

  if (!MAY_END_WITH_VOLUME.test(trimmed)) return unchanged;
  for (const pattern of PATTERNS) {
    const match = pattern.regex.exec(trimmed);
    if (match === null) continue;
    const base = trimmed.slice(0, match.index).trimEnd();
    if (base === "") return unchanged;
    return { baseTitle: base, impliedVolume: volumeText === "" ? pattern.volume(match) : null };
  }
  return unchanged;
}

const cache = new WeakMap<ListRow, WorkInfo>();

/** 行の作品の情報。行ごとに 1 度だけ計算する（行が差し替わるまで同じ結果）。 */
export function workInfo(row: ListRow): WorkInfo {
  const cached = cache.get(row);
  if (cached !== undefined) return cached;

  const { baseTitle, impliedVolume } = stripTrailingVolume(row.title, row.volume);
  const authorKeys: string[] = [];
  for (const author of row.authors) {
    const key = normalizeAuthorName(author.name);
    if (key !== "" && !authorKeys.includes(key)) authorKeys.push(key);
  }
  const info: WorkInfo = {
    baseTitle,
    impliedVolume,
    titleKey: normalizeWorkTitle(baseTitle),
    authorKeys,
  };
  cache.set(row, info);
  return info;
}

/** 巻の表示（巻次の欄。空なら、書名から読み取った番号）。どちらもなければ null。 */
export function volumeLabel(row: ListRow, info: WorkInfo = workInfo(row)): string | null {
  const own = row.volume?.trim() ?? "";
  if (own !== "") return own;
  return info.impliedVolume === null ? null : String(info.impliedVolume);
}

/** 巻の並べ替えの値（`volumeSort`。なければ書名から読み取った番号）。どちらもなければ null。 */
export function volumeOrder(row: ListRow, info: WorkInfo = workInfo(row)): number | null {
  return row.volumeSort ?? info.impliedVolume;
}
