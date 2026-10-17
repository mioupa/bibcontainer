// 開発用のダミーデータ。バックエンド（Rust の IPC）がなくても、5,000〜20,000 冊の一覧を動かせるようにする。
// 統合時は、取得を src/features/library/api.ts で IPC に差し替える。このファイルはテストと性能計測に残す。
//
// 乱数は種つきで、同じ (count, seed) なら同じ結果を返す（テストの再現性のため）。
import { PROVISIONAL_C_CODE_TABLE } from "@/data/cCodeTable";
import type { Author, ListRow, Location } from "@/types/contracts";

/** Rust 側が作る並べ替え用のキー（NFKC → 小文字 → カタカナをひらがなに）の、フロント版の代用。 */
export function normalizeSortKey(text: string): string {
  return text
    .normalize("NFKC")
    .toLowerCase()
    .replace(/[ァ-ヶ]/g, (ch) => String.fromCharCode(ch.charCodeAt(0) - 0x60));
}

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

type Rng = () => number;
const pick = <T>(rng: Rng, list: readonly T[]): T => list[Math.floor(rng() * list.length)] as T;
const chance = (rng: Rng, p: number) => rng() < p;
const int = (rng: Rng, min: number, max: number) => min + Math.floor(rng() * (max - min + 1));
const hex = (rng: Rng, length: number) =>
  Array.from({ length }, () => Math.floor(rng() * 16).toString(16)).join("");
const pad = (n: number, width = 2) => String(n).padStart(width, "0");
/** UUIDv7 に似た形の ID（本の ID と、手動の作品の ID に使う）。 */
const uuidOf = (rng: Rng) =>
  `${hex(rng, 8)}-${hex(rng, 4)}-7${hex(rng, 3)}-${hex(rng, 4)}-${hex(rng, 12)}`;

// [表記, ヨミ]
const PREFIXES: readonly (readonly [string, string])[] = [
  ["はじめての", "はじめての"],
  ["やさしい", "やさしい"],
  ["実践", "じっせん"],
  ["図解", "ずかい"],
  ["最新", "さいしん"],
  ["決定版", "けっていばん"],
  ["完全", "かんぜん"],
  ["基礎から学ぶ", "きそからまなぶ"],
  ["", ""],
  ["", ""],
];
const NOUNS: readonly (readonly [string, string])[] = [
  ["数学", "すうがく"],
  ["物理学", "ぶつりがく"],
  ["半導体", "はんどうたい"],
  ["炭化ケイ素", "たんかけいそ"],
  ["電子回路", "でんしかいろ"],
  ["機械学習", "きかいがくしゅう"],
  ["統計学", "とうけいがく"],
  ["プログラミング", "ぷろぐらみんぐ"],
  ["日本史", "にほんし"],
  ["世界史", "せかいし"],
  ["経済学", "けいざいがく"],
  ["料理", "りょうり"],
  ["おにぎり", "おにぎり"],
  ["小説", "しょうせつ"],
  ["旅", "たび"],
  ["哲学", "てつがく"],
  ["宇宙", "うちゅう"],
  ["化学", "かがく"],
  ["医学", "いがく"],
  ["建築", "けんちく"],
  ["写真", "しゃしん"],
  ["音楽", "おんがく"],
  ["英語", "えいご"],
  ["Rust", "らすと"],
  ["TypeScript", "たいぷすくりぷと"],
  ["アルゴリズム", "あるごりずむ"],
  ["猫", "ねこ"],
  ["星", "ほし"],
  ["海", "うみ"],
  ["森", "もり"],
];
const SUFFIXES: readonly (readonly [string, string])[] = [
  ["入門", "にゅうもん"],
  ["の教科書", "のきょうかしょ"],
  ["の基礎", "のきそ"],
  ["ハンドブック", "はんどぶっく"],
  ["事典", "じてん"],
  ["講義", "こうぎ"],
  ["の世界", "のせかい"],
  ["物語", "ものがたり"],
  ["の冒険", "のぼうけん"],
  ["ノート", "のーと"],
  ["", ""],
];
const SURNAMES: readonly (readonly [string, string])[] = [
  ["佐藤", "さとう"],
  ["鈴木", "すずき"],
  ["高橋", "たかはし"],
  ["田中", "たなか"],
  ["渡辺", "わたなべ"],
  ["伊藤", "いとう"],
  ["山本", "やまもと"],
  ["中村", "なかむら"],
  ["小林", "こばやし"],
  ["加藤", "かとう"],
  ["吉田", "よしだ"],
  ["山田", "やまだ"],
  ["松本", "まつもと"],
  ["井上", "いのうえ"],
  ["木村", "きむら"],
  ["林", "はやし"],
  ["清水", "しみず"],
  ["山崎", "やまざき"],
  ["森", "もり"],
  ["池田", "いけだ"],
];
const GIVEN_NAMES: readonly (readonly [string, string])[] = [
  ["太郎", "たろう"],
  ["花子", "はなこ"],
  ["一郎", "いちろう"],
  ["美咲", "みさき"],
  ["健", "けん"],
  ["恵", "めぐみ"],
  ["大輔", "だいすけ"],
  ["彩", "あや"],
  ["翔", "しょう"],
  ["優子", "ゆうこ"],
];
const PUBLISHERS: readonly string[] = [
  "岩波書店",
  "講談社",
  "新潮社",
  "文藝春秋",
  "角川書店",
  "集英社",
  "小学館",
  "筑摩書房",
  "早川書房",
  "みすず書房",
  "オーム社",
  "技術評論社",
  "翔泳社",
  "日経BP",
  "丸善出版",
  "朝倉書店",
  "共立出版",
  "森北出版",
  "ポット出版",
  "ちくま書房",
  "河出書房新社",
  "白水社",
  "平凡社",
  "有斐閣",
  "東京大学出版会",
];
const TAGS: readonly string[] = [
  "研究/SiC",
  "研究/パワー半導体",
  "研究/論文",
  "小説",
  "漫画",
  "技術書/Rust",
  "技術書/TypeScript",
  "技術書/数学",
  "料理",
  "積読",
  "仕事/参考書",
  "仕事",
];
const COVER_PROBABILITY = 0.25;
/**
 * 同じ作品の巻（FR-LIST-08）を始める確率（1 行ごと）。作品は平均 21 冊（2〜40）なので、全体の約 25% の行が作品の巻になる。
 * p × 21 ÷ (p × 21 + 1 − p) = 0.25 なら p ≈ 0.0156。偶然に同じ書名・著者になった本のぶんを見込んで、少し下げる。
 */
const WORK_START_PROBABILITY = 0.014;
const WORK_MIN_VOLUMES = 2;
const WORK_MAX_VOLUMES = 40;
/** 作品の巻のうち、巻次の欄を持たず、書名の末尾に巻を書く本の割合。 */
const WORK_TITLE_VOLUME_PROBABILITY = 0.1;

/** 手動の作品（0026）に入れる行の割合（全体に対する）。1 つの作品は 2〜5 冊で、行は種つきの乱数で無作為に選ぶ。 */
const MANUAL_WORK_SHARE = 0.02;
const MANUAL_WORK_MIN = 2;
const MANUAL_WORK_MAX = 5;
/** 「作品から外す」（`"none"`）にする行の割合。 */
const WORK_NONE_SHARE = 0.01;

/** 保管場所（ツリー）のダミー。 */
export const MOCK_LOCATIONS: readonly Location[] = (() => {
  const list: Location[] = [];
  const add = (id: string, name: string, parentId: string | null, sortOrder: number) =>
    list.push({ id, name, parentId, sortOrder });
  add("loc-home", "自宅", null, 1);
  add("loc-study", "書斎", "loc-home", 1);
  add("loc-study-a", "本棚A", "loc-study", 1);
  add("loc-study-a1", "1段目", "loc-study-a", 1);
  add("loc-study-a2", "2段目", "loc-study-a", 2);
  add("loc-study-a3", "3段目", "loc-study-a", 3);
  add("loc-study-b", "本棚B", "loc-study", 2);
  add("loc-study-b1", "1段目", "loc-study-b", 1);
  add("loc-study-b2", "2段目", "loc-study-b", 2);
  add("loc-living", "リビング", "loc-home", 2);
  add("loc-living-shelf", "飾り棚", "loc-living", 1);
  add("loc-lab", "研究室", null, 2);
  add("loc-lab-shelf1", "書架1", "loc-lab", 1);
  add("loc-lab-shelf2", "書架2", "loc-lab", 2);
  add("loc-parents", "実家", null, 3);
  add("loc-parents-closet", "押入れ", "loc-parents", 1);
  return list;
})();

const LEAF_LOCATION_IDS = MOCK_LOCATIONS.filter(
  (l) => !MOCK_LOCATIONS.some((c) => c.parentId === l.id),
).map((l) => l.id);

const C_CODE_CONTENTS = Object.keys(PROVISIONAL_C_CODE_TABLE.content);

function isbn13Of(rng: Rng): string {
  const body = `9784${Array.from({ length: 8 }, () => int(rng, 0, 9)).join("")}`;
  let sum = 0;
  for (let i = 0; i < 12; i++) sum += Number(body[i]) * (i % 2 === 0 ? 1 : 3);
  return `${body}${(10 - (sum % 10)) % 10}`;
}

function publishedOnOf(rng: Rng): string {
  const year = int(rng, 1985, 2026);
  const r = rng();
  if (r < 0.1) return String(year);
  if (r < 0.3) return `${year}-${pad(int(rng, 1, 12))}`;
  return `${year}-${pad(int(rng, 1, 12))}-${pad(int(rng, 1, 28))}`;
}

function createdAtOf(rng: Rng): string {
  return `${int(rng, 2020, 2026)}-${pad(int(rng, 1, 12))}-${pad(int(rng, 1, 28))}T${pad(int(rng, 0, 23))}:${pad(int(rng, 0, 59))}:${pad(int(rng, 0, 59))}Z`;
}

/** 同じ作品の巻をまとめて作るための計画。1 つの作品の全巻が、書名・著者・出版社を共有する。 */
interface WorkPlan {
  title: string;
  kana: string;
  hasKana: boolean;
  authors: Author[];
  publisher: string | null;
  seriesName: string | null;
  /** 全巻の数。 */
  total: number;
  /** 次に作る巻の番号（1 始まり）。 */
  next: number;
}

function makeAuthors(rng: Rng, count: number): Author[] {
  return Array.from({ length: count }, (_, k) => {
    const surname = pick(rng, SURNAMES);
    const given = pick(rng, GIVEN_NAMES);
    return {
      name: `${surname[0]} ${given[0]}`,
      kana: `${surname[1]} ${given[1]}`,
      role: k === 0 ? "著" : "共著",
    };
  });
}

function startWork(rng: Rng): WorkPlan {
  const prefix = pick(rng, PREFIXES);
  const noun = pick(rng, NOUNS);
  const suffix = pick(rng, SUFFIXES);
  return {
    title: `${prefix[0]}${noun[0]}${suffix[0]}`,
    kana: `${prefix[1]}${noun[1]}${suffix[1]}`,
    hasKana: chance(rng, 0.8),
    // 巻をまとめる表示を確かめるので、著者のない作品は作らない
    authors: makeAuthors(rng, chance(rng, 0.85) ? 1 : int(rng, 2, 3)),
    publisher: pick(rng, PUBLISHERS),
    seriesName: chance(rng, 0.6) ? `${noun[0]}シリーズ` : null,
    total: int(rng, WORK_MIN_VOLUMES, WORK_MAX_VOLUMES),
    next: 1,
  };
}

/**
 * ダミーの一覧用の行を作る。約 25% の行は、同じ作品の巻（同じ書名・著者・出版社で、巻が 1〜n）。
 * 巻の大半は巻次の欄（`volume`・`volumeSort`）を持ち、約 10% は巻次がなく、書名が「書名 n」の形。
 */
export function generateMockRows(count: number, seed = 20261014): ListRow[] {
  const rng = mulberry32(seed);
  const rows: ListRow[] = [];
  let work: WorkPlan | null = null;
  for (let i = 0; i < count; i++) {
    if (work === null && chance(rng, WORK_START_PROBABILITY)) work = startWork(rng);
    // 作品の巻なら、その作品の何巻か（作品の最後の巻を作ったら、次の行からは作品ではない）
    let workVolume: number | null = null;
    let workTitleVolume = false;
    const plan: WorkPlan | null = work;
    if (plan !== null) {
      workVolume = plan.next;
      workTitleVolume = chance(rng, WORK_TITLE_VOLUME_PROBABILITY);
      plan.next += 1;
      if (plan.next > plan.total) work = null;
    }

    const prefix = pick(rng, PREFIXES);
    const noun = pick(rng, NOUNS);
    const suffix = pick(rng, SUFFIXES);
    let title = `${prefix[0]}${noun[0]}${suffix[0]}`;
    const kana = `${prefix[1]}${noun[1]}${suffix[1]}`;
    let titleKana: string | null = chance(rng, 0.8) ? kana : null;

    const authorCount = chance(rng, 0.1) ? 0 : chance(rng, 0.8) ? 1 : int(rng, 2, 3);
    let authors: Author[] = makeAuthors(rng, authorCount);

    let inSeries = chance(rng, 0.25);
    let volumeNumber: number | null = inSeries ? int(rng, 1, 30) : null;
    let seriesName: string | null = inSeries ? `${noun[0]}シリーズ` : null;
    if (plan !== null && workVolume !== null) {
      title = workTitleVolume ? `${plan.title} ${workVolume}` : plan.title;
      titleKana = plan.hasKana
        ? workTitleVolume
          ? `${plan.kana} ${workVolume}`
          : plan.kana
        : null;
      authors = plan.authors.map((author) => ({ ...author }));
      volumeNumber = workTitleVolume ? null : workVolume;
      seriesName = plan.seriesName;
      inSeries = seriesName !== null;
    }
    const owned = chance(rng, 0.88);
    const ownership: ListRow["ownership"] = owned
      ? "owned"
      : chance(rng, 0.65)
        ? "wanted"
        : "released";
    const readStatus: ListRow["readStatus"] =
      ownership === "wanted"
        ? "unread"
        : pick(rng, ["unread", "unread", "reading", "read", "read", "read", "paused"] as const);
    const readFinishedOn =
      readStatus === "read"
        ? `${int(rng, 2020, 2026)}-${pad(int(rng, 1, 12))}-${pad(int(rng, 1, 28))}`
        : null;
    const hasCCode = chance(rng, 0.92);
    const cCode = hasCCode
      ? `${pick(rng, [0, 0, 0, 1, 2, 3, 3, 8, 9] as const)}${pick(rng, [0, 0, 1, 1, 2, 9] as const)}${pick(rng, C_CODE_CONTENTS)}`
      : null;
    const metaRoll = rng();
    const metaStatus: ListRow["metaStatus"] =
      metaRoll < 0.9 ? "ok" : metaRoll < 0.94 ? "pending" : metaRoll < 0.97 ? "partial" : "failed";
    const tagCount = chance(rng, 0.35) ? 0 : chance(rng, 0.7) ? 1 : 2;
    const tags = [...new Set(Array.from({ length: tagCount }, () => pick(rng, TAGS)))];
    const randomPublisher = chance(rng, 0.95) ? pick(rng, PUBLISHERS) : null;
    const publisher = plan === null ? randomPublisher : plan.publisher;
    const listPrice = chance(rng, 0.9) ? int(rng, 4, 40) * 100 : null;

    rows.push({
      id: uuidOf(rng),
      isbn13: chance(rng, 0.97) ? isbn13Of(rng) : null,
      title,
      titleKana,
      subtitle: chance(rng, 0.2) ? "新装版" : null,
      volume: volumeNumber === null ? null : String(volumeNumber),
      volumeSort: volumeNumber,
      seriesId: inSeries && seriesName !== null ? `series-${seriesName}` : null,
      seriesName,
      workId: null,
      authors,
      publisher,
      publishedOn: chance(rng, 0.97) ? publishedOnOf(rng) : null,
      cCode,
      ndc: chance(rng, 0.4) ? `${int(rng, 100, 999)}` : null,
      listPrice,
      pages: chance(rng, 0.85) ? int(rng, 80, 800) : null,
      ownership,
      wishPriority: ownership === "wanted" ? pick(rng, ["high", "mid", "low"] as const) : null,
      locationId: ownership === "owned" && chance(rng, 0.9) ? pick(rng, LEAF_LOCATION_IDS) : null,
      tags,
      readStatus,
      readFinishedOn,
      rating: readStatus === "read" && chance(rng, 0.8) ? int(rng, 1, 5) : null,
      purchasedOn: ownership === "owned" && chance(rng, 0.7) ? createdAtOf(rng).slice(0, 10) : null,
      purchasePrice:
        ownership === "owned" && chance(rng, 0.6) && listPrice !== null
          ? Math.round(listPrice * 1.1)
          : null,
      purchaseStore:
        ownership === "owned" && chance(rng, 0.5)
          ? pick(rng, ["紀伊國屋書店", "丸善", "Amazon", "古書店"])
          : null,
      coverHash: chance(rng, COVER_PROBABILITY) ? hex(rng, 64) : null,
      metaStatus,
      deleted: chance(rng, 0.015),
      createdAt: createdAtOf(rng),
      sortTitle: normalizeSortKey(titleKana ?? title),
      sortAuthors: normalizeSortKey(authors[0]?.kana ?? authors[0]?.name ?? ""),
    });
  }
  applyWorkOverrides(rows, seed);
  return rows;
}

/**
 * 作品の手動の指定（`workId`。0026）を、約 2% の行（手動の作品）と約 1% の行（`"none"`）に付ける。
 * 手動の作品は、無作為に選んだ 2〜5 冊（別の作品の巻や、どの作品でもない本が混ざる）。行を作る乱数とは別の乱数を使い、
 * 上書きのない行の中身は、上書きを付ける前と変わらない。
 */
function applyWorkOverrides(rows: ListRow[], seed: number): void {
  const n = rows.length;
  if (n < MANUAL_WORK_MAX) return;
  const rng = mulberry32((seed ^ 0x5bd1e995) >>> 0);
  const taken = new Uint8Array(n);
  const claim = (): number | null => {
    for (let attempt = 0; attempt < 8; attempt++) {
      const index = Math.floor(rng() * n);
      if (taken[index] === 0) {
        taken[index] = 1;
        return index;
      }
    }
    return null;
  };
  let manualRows = 0;
  const manualTarget = Math.round(n * MANUAL_WORK_SHARE);
  while (manualRows < manualTarget) {
    const members: number[] = [];
    const size = int(rng, MANUAL_WORK_MIN, MANUAL_WORK_MAX);
    for (let k = 0; k < size; k++) {
      const index = claim();
      if (index !== null) members.push(index);
    }
    if (members.length < 2) break;
    const workId = uuidOf(rng);
    for (const index of members) (rows[index] as ListRow).workId = workId;
    manualRows += members.length;
  }
  const noneTarget = Math.round(n * WORK_NONE_SHARE);
  for (let k = 0; k < noneTarget; k++) {
    const index = claim();
    if (index !== null) (rows[index] as ListRow).workId = "none";
  }
}
