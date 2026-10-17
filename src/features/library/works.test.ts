import { describe, expect, it } from "vitest";
import {
  normalizeAuthorName,
  normalizeWorkTitle,
  parseKanjiNumber,
  stripTrailingVolume,
  volumeLabel,
  volumeOrder,
  workInfo,
} from "@/features/library/works";
import { makeRow } from "@/test/factories";

describe("stripTrailingVolume（巻次の欄があるとき）", () => {
  // [書名, 巻次, 作品名]
  const cases: [string, string, string][] = [
    ["スローループ 7", "7", "スローループ"],
    ["One piece 巻1", "巻1", "One piece"],
    ["スローループ　7", "7", "スローループ"], // 全角空白
    ["Foo.7", "7", "Foo"], // ピリオドの直後
    ["X vol.7", "7", "X"], // 巻のラベルも除く
    ["X No. 12", "12", "X"],
    ["スローループ ７", "7", "スローループ"], // 全角の数字（NFKC で同じ）
    ["スローループ 7", "７", "スローループ"],
    ["Ｆａｔｅ　ＺＥＲＯ ３", "3", "Ｆａｔｅ　ＺＥＲＯ"],
    ["ガンダム  第12巻", "第12巻", "ガンダム"],
  ];
  it.each(cases)("%s（巻次 %s）→ %s。取り除いた番号は読み取らない", (title, volume, base) => {
    expect(stripTrailingVolume(title, volume)).toEqual({ baseTitle: base, impliedVolume: null });
  });

  it("書名が巻次で終わらない、または区切りがないときは、巻次の欄では取らない（次の規則を試す）", () => {
    // 区切りのない 4 → 規則 b も当たらない
    expect(stripTrailingVolume("1Q84", "4")).toEqual({ baseTitle: "1Q84", impliedVolume: null });
    // 巻次が書名に含まれない → 規則 b（巻次の欄があるので、番号は読み取らない）
    expect(stripTrailingVolume("スローループ 8", "7")).toEqual({
      baseTitle: "スローループ",
      impliedVolume: null,
    });
    expect(stripTrailingVolume("スローループ", "7")).toEqual({
      baseTitle: "スローループ",
      impliedVolume: null,
    });
  });

  it("取り除くと空になるときは、元の書名のまま", () => {
    expect(stripTrailingVolume("7", "7").baseTitle).toBe("7");
    expect(stripTrailingVolume(".7", "7").baseTitle).toBe(".7");
  });

  it("巻次が空白だけなら、ないものとして扱う（規則 b で番号を読み取る）", () => {
    expect(stripTrailingVolume("スローループ 8", "  ")).toEqual({
      baseTitle: "スローループ",
      impliedVolume: 8,
    });
  });
});

describe("stripTrailingVolume（巻次の欄がないとき。書名の末尾から読み取る）", () => {
  // [書名, 作品名, 読み取る番号]
  const cases: [string, string, number | null][] = [
    // 空白 + 第? + 数 + 巻|集|部|編?
    ["X 7", "X", 7],
    ["X 第7巻", "X", 7],
    ["X 7巻", "X", 7],
    ["X ７", "X", 7],
    ["X　７", "X", 7],
    ["X 第３集", "X", 3],
    ["X 2部", "X", 2],
    ["X 4編", "X", 4],
    ["X 7.5", "X", 7.5],
    ["X ７．５", "X", 7.5],
    ["スローループ 8", "スローループ", 8],
    ["スローループ    12", "スローループ", 12],
    // 括弧
    ["X(7)", "X", 7],
    ["X（7）", "X", 7],
    ["X [7]", "X", 7],
    ["X［７］", "X", 7],
    ["X (第3巻)", "X", 3],
    ["X（第３巻）", "X", 3],
    // vol. / No. / # / 巻
    ["X vol.7", "X", 7],
    ["X Vol 3", "X", 3],
    ["X VOL.12", "X", 12],
    ["X volume 4", "X", 4],
    ["X No.2", "X", 2],
    ["X no 5", "X", 5],
    ["X #5", "X", 5],
    ["X ＃５", "X", 5],
    ["X 巻1", "X", 1],
    ["X Ｖｏｌ．７", "X", 7],
    // 第 + 漢数字 + 巻
    ["X 第三巻", "X", 3],
    ["X 第十二巻", "X", 12],
    ["X 第二十巻", "X", 20],
    ["X 第〇巻", "X", 0],
    // 上・中・下・前編・後編
    ["罪と罰 上", "罪と罰", 1],
    ["罪と罰 中", "罪と罰", 2],
    ["罪と罰 下", "罪と罰", 3],
    ["X 前編", "X", 1],
    ["X 後編", "X", 2],
    ["X 上巻", "X", 1],
    ["X 中巻", "X", 2],
    ["X 下巻", "X", 3],
    // 1 つだけ取る
    ["X 1 2", "X 1", 2],
    ["X 第2巻 (3)", "X 第2巻", 3],
  ];
  it.each(cases)("%s → %s（番号 %s）", (title, base, implied) => {
    expect(stripTrailingVolume(title, null)).toEqual({ baseTitle: base, impliedVolume: implied });
    // 前後の空白は無視する
    expect(stripTrailingVolume(`  ${title}  `, null).baseTitle).toBe(base);
  });

  // 区切り（空白・括弧）のない末尾は、巻とみなさない
  const negatives = [
    "坂の上",
    "1Q84",
    "ムーミン谷の十一月",
    "2001年宇宙の旅",
    "1984",
    "X7",
    "第三の男",
    "上",
    "ルパン三世 第3の男",
    "銀河鉄道の夜",
    "吾輩は猫である",
    "7",
    "(7)",
    "第3巻",
    "Vol.7",
    "X vol.",
    "X 第",
    "X 十",
  ];
  it.each(negatives)("取らない: %s", (title) => {
    expect(stripTrailingVolume(title, null)).toEqual({ baseTitle: title, impliedVolume: null });
  });

  it("取り除くと空になるときは、元の書名のまま", () => {
    expect(stripTrailingVolume("（7）", null)).toEqual({ baseTitle: "（7）", impliedVolume: null });
    expect(stripTrailingVolume(" (7)", null)).toEqual({ baseTitle: "(7)", impliedVolume: null });
  });
});

describe("parseKanjiNumber", () => {
  const cases: [string, number | null][] = [
    ["三", 3],
    ["十", 10],
    ["十二", 12],
    ["二十", 20],
    ["二十三", 23],
    ["百", 100],
    ["百五", 105],
    ["百二十", 120],
    ["千", 1000],
    ["〇", 0],
    ["零", 0],
    ["二〇", 20],
    ["一二", 12],
    ["", null],
    ["A", null],
  ];
  it.each(cases)("%s → %s", (text, value) => {
    expect(parseKanjiNumber(text)).toBe(value);
  });
});

describe("normalizeWorkTitle", () => {
  it("NFKC・小文字・カタカナをひらがなに・空白と中黒を除く（長音符はそのまま）", () => {
    expect(normalizeWorkTitle("スローループ")).toBe("すろーるーぷ");
    expect(normalizeWorkTitle("ｽﾛｰﾙｰﾌﾟ")).toBe(normalizeWorkTitle("スローループ"));
    expect(normalizeWorkTitle("すろーるーぷ")).toBe(normalizeWorkTitle("スローループ"));
    expect(normalizeWorkTitle("ＯＮＥ ＰＩＥＣＥ")).toBe("onepiece");
    expect(normalizeWorkTitle("ONE PIECE")).toBe("onepiece");
    expect(normalizeWorkTitle("one piece")).toBe("onepiece");
    expect(normalizeWorkTitle("ハリー・ポッター")).toBe(normalizeWorkTitle("ハリーポッター"));
    expect(normalizeWorkTitle("ハリー･ポッター")).toBe(normalizeWorkTitle("ハリーポッター"));
    expect(normalizeWorkTitle("ハリー·ポッター")).toBe(normalizeWorkTitle("ハリーポッター"));
    expect(normalizeWorkTitle("A　B\tC")).toBe("abc");
  });

  it("濁点つき・小さい字・ヴ・繰り返し記号も、ひらがなにそろえる", () => {
    expect(normalizeWorkTitle("ヴァイオレット")).toBe("ゔぁいおれっと");
    expect(normalizeWorkTitle("ガッコウ")).toBe("がっこう");
    expect(normalizeWorkTitle("ヽヾ")).toBe("ゝゞ");
  });

  it("空白だけなら空（グループにしない）", () => {
    expect(normalizeWorkTitle(" 　 ")).toBe("");
    expect(normalizeWorkTitle("")).toBe("");
  });

  it("別の作品は別のキーになる", () => {
    expect(normalizeWorkTitle("スローループ")).not.toBe(normalizeWorkTitle("スローループ2"));
  });
});

describe("normalizeAuthorName", () => {
  it("NFKC・小文字・空白・中黒・読点・ピリオドを除く", () => {
    expect(normalizeAuthorName("山田 太郎")).toBe("山田太郎");
    expect(normalizeAuthorName("山田　太郎")).toBe("山田太郎");
    expect(normalizeAuthorName("山田太郎")).toBe("山田太郎");
    expect(normalizeAuthorName("J.K. Rowling")).toBe("jkrowling");
    expect(normalizeAuthorName("ＪＫ ＲＯＷＬＩＮＧ")).toBe("jkrowling");
    expect(normalizeAuthorName("Rowling, J.K.")).toBe("rowlingjk");
    expect(normalizeAuthorName("ヴィクトル・ユーゴー")).toBe("ヴィクトルユーゴー");
    expect(normalizeAuthorName("ﾔﾏﾀﾞ ﾀﾛｳ")).toBe("ヤマダタロウ");
    expect(normalizeAuthorName("、 ・")).toBe("");
  });
});

describe("workInfo", () => {
  it("巻次の欄があれば、それを取り除く。番号は読み取らない", () => {
    const info = workInfo(makeRow({ title: "スローループ 7", volume: "7", volumeSort: 7 }));
    expect(info.baseTitle).toBe("スローループ");
    expect(info.impliedVolume).toBeNull();
    expect(info.titleKey).toBe(normalizeWorkTitle("スローループ"));
  });

  it("巻次の欄がなく、書名の末尾に巻があれば、取り除いて番号を読み取る", () => {
    const info = workInfo(makeRow({ title: "スローループ 8", volume: null }));
    expect(info.baseTitle).toBe("スローループ");
    expect(info.impliedVolume).toBe(8);
  });

  it("著者の比べ方は、空のものを除く。同じ著者の重なりも 1 つにする", () => {
    const info = workInfo(
      makeRow({
        authors: [
          { name: "山田 太郎", kana: null, role: null },
          { name: "  ", kana: null, role: null },
          { name: "山田太郎", kana: null, role: null },
          { name: "J.K. Rowling", kana: null, role: null },
        ],
      }),
    );
    expect(info.authorKeys).toEqual(["山田太郎", "jkrowling"]);
  });

  it("行ごとに 1 度だけ計算する（同じ行は同じ結果のオブジェクト。差し替わった行は計算し直す）", () => {
    const row = makeRow({ title: "A 1" });
    expect(workInfo(row)).toBe(workInfo(row));
    expect(workInfo({ ...row })).not.toBe(workInfo(row));
  });
});

describe("volumeLabel / volumeOrder", () => {
  it("巻次の欄があればそれ、なければ書名から読み取った番号", () => {
    expect(volumeLabel(makeRow({ title: "A 1", volume: "1", volumeSort: 1 }))).toBe("1");
    expect(volumeLabel(makeRow({ title: "A 8", volume: null }))).toBe("8");
    expect(volumeLabel(makeRow({ title: "A", volume: null }))).toBeNull();
    expect(volumeLabel(makeRow({ title: "A 1", volume: " 上 " }))).toBe("上");
    expect(volumeLabel(makeRow({ title: "A 7.5", volume: null }))).toBe("7.5");
  });

  it("並べ替えは volumeSort、なければ書名から読み取った番号", () => {
    expect(volumeOrder(makeRow({ title: "A 1", volume: "1", volumeSort: 3 }))).toBe(3);
    expect(volumeOrder(makeRow({ title: "A 8", volume: null }))).toBe(8);
    expect(volumeOrder(makeRow({ title: "A", volume: null }))).toBeNull();
    // 巻次はあるが並べ替えの値がなければ、番号は読み取らない
    expect(volumeOrder(makeRow({ title: "A 2", volume: "上", volumeSort: null }))).toBeNull();
  });
});
