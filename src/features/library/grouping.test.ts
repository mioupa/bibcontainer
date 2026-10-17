import { describe, expect, it } from "vitest";
import {
  buildWorkLayout,
  countBooks,
  expandToBookIds,
  groupRows,
  groupSummary,
  isGroupKey,
  layoutEntries,
  type GroupedList,
  type ListEntry,
} from "@/features/library/grouping";
import { sortRows } from "@/features/library/sort";
import { makeRow } from "@/test/factories";
import type { ListRow } from "@/types/contracts";

const NONE: ReadonlySet<string> = new Set();
const author = (name: string) => ({ name, kana: null, role: null });

/** 巻つきの本。`id` は `${title}-${volume}` のようにして、読みやすくする。 */
function vol(
  title: string,
  volume: number | null,
  authors: string[] = ["山田 太郎"],
  extra: Partial<ListRow> = {},
): ListRow {
  return makeRow({
    id: `${title}-${volume ?? "-"}-${authors.join("+")}`,
    title,
    sortTitle: title,
    volume: volume === null ? null : String(volume),
    volumeSort: volume,
    authors: authors.map(author),
    ...extra,
  });
}

const keysOf = (grouped: GroupedList) => grouped.entries.map((entry) => entry.key);
const kinds = (entries: ListEntry[]) =>
  entries.map((e) => (e.kind === "group" ? `G${e.group.members.length}` : `B${e.level}`)).join(" ");

describe("グループの作り方", () => {
  it("同じ作品名・同じ著者の本が 2 冊以上あれば、1 つのグループ。巻の順（書名の末尾の巻も読む）", () => {
    const rows = [
      ...[3, 1, 2, 5, 4, 7, 6].map((n) => vol("スローループ", n)),
      vol("スローループ 8", null),
    ];
    const grouped = groupRows(rows, true, NONE);
    expect(grouped.entries).toHaveLength(1);
    const entry = grouped.entries[0] as ListEntry;
    expect(entry.kind).toBe("group");
    if (entry.kind !== "group") return;
    expect(entry.expanded).toBe(false);
    expect(entry.group.members).toHaveLength(8);
    expect(entry.group.members.map((m) => m.volume)).toEqual([
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      null,
    ]);
    expect(entry.group.baseTitle).toBe("スローループ");
    expect(entry.group.firstVolume).toBe("1");
    expect(entry.group.lastVolume).toBe("8");
    // 巻の順 1..8（巻次のない 8 巻が最後）
    expect(entry.group.members.map((m) => m.title)).toEqual([
      ...Array.from({ length: 7 }, () => "スローループ"),
      "スローループ 8",
    ]);
    expect(entry.group.members.slice(0, 7).map((m) => m.volumeSort)).toEqual([1, 2, 3, 4, 5, 6, 7]);
    expect(grouped.bookIds).toEqual(entry.group.members.map((m) => m.id));
  });

  it("作品名が同じでも、著者が違えば別の作品（グループにならない）", () => {
    const rows = [
      vol("風", 1, ["山田 太郎"]),
      vol("風", 2, ["山田 太郎"]),
      vol("風", 1, ["鈴木 花子"]),
    ];
    const grouped = groupRows(rows, true, NONE);
    expect(kinds(grouped.entries)).toBe("G2 B1");
    const group = grouped.entries[0] as Extract<ListEntry, { kind: "group" }>;
    expect(group.group.members.every((m) => m.authors[0]?.name === "山田 太郎")).toBe(true);
  });

  it("同じ作品名・別の著者が、それぞれ 2 冊なら、2 つのグループ（キーが違う）", () => {
    const rows = [
      vol("風", 1, ["山田 太郎"]),
      vol("風", 1, ["鈴木 花子"]),
      vol("風", 2, ["山田 太郎"]),
      vol("風", 2, ["鈴木 花子"]),
    ];
    const grouped = groupRows(rows, true, NONE);
    expect(kinds(grouped.entries)).toBe("G2 G2");
    expect(new Set(keysOf(grouped)).size).toBe(2);
  });

  it("著者が 1 人でも同じなら同じ作品（共著の組み合わせが違っても、推移的につながる）", () => {
    const rows = [
      vol("共著", 1, ["A", "B"]),
      vol("共著", 2, ["B", "C"]),
      vol("共著", 3, ["C", "D"]),
      vol("共著", 4, ["X"]),
    ];
    const grouped = groupRows(rows, true, NONE);
    expect(kinds(grouped.entries)).toBe("G3 B1");
    expect(grouped.bookIds).toEqual(
      [rows[0], rows[1], rows[2], rows[3]].map((r) => (r as ListRow).id),
    );
  });

  it("書名・著者の違いのうち、全角・半角・大文字小文字・ひらがな／カタカナ・空白・中黒は同じとみなす", () => {
    const rows = [
      vol("スローループ", 1, ["山田 太郎"]),
      vol("ｽﾛｰﾙｰﾌﾟ", 2, ["山田太郎"]),
      vol("すろーるーぷ", 3, ["山田　太郎"]),
      vol("ONE PIECE", 1, ["尾田 栄一郎"]),
      vol("ＯＮＥ ＰＩＥＣＥ", 2, ["尾田栄一郎"]),
      vol("one piece", 3, ["尾田・栄一郎"]),
    ];
    const grouped = groupRows(rows, true, NONE);
    expect(kinds(grouped.entries)).toBe("G3 G3");
  });

  describe("著者のない本", () => {
    it("束の中の著者のある作品がちょうど 1 つなら、そこに入る", () => {
      const rows = [
        vol("風", 1, ["山田 太郎"]),
        vol("風", 2, []),
        vol("風", 3, ["山田 太郎"]),
        vol("風 4", null, []),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G4");
      const group = grouped.entries[0] as Extract<ListEntry, { kind: "group" }>;
      expect(group.group.members.map((m) => m.volumeSort ?? 4)).toEqual([1, 2, 3, 4]);
    });

    it("著者のある本が 1 冊もなければ、著者のない本どうしで 1 つの作品", () => {
      const rows = [vol("風", 1, []), vol("風", 2, []), vol("雲", 1, [])];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G2 B1");
    });

    it("著者のある作品が 2 つ以上あれば、どれにも入らず単独のまま", () => {
      const rows = [
        vol("風", 1, ["山田 太郎"]),
        vol("風", 2, ["山田 太郎"]),
        vol("風", 1, ["鈴木 花子"]),
        vol("風", 2, ["鈴木 花子"]),
        vol("風", 3, []),
        vol("風", 4, []),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G2 G2 B1 B1");
    });

    it("著者のある本が 1 冊だけで、そこに著者のない本が入れば、2 冊のグループになる", () => {
      const rows = [vol("風", 1, ["山田 太郎"]), vol("風", 2, [])];
      expect(kinds(groupRows(rows, true, NONE).entries)).toBe("G2");
    });
  });

  it("絞り込みの結果が 1 冊だけなら、グループにせず、ふつうの行", () => {
    const all = [vol("スローループ", 1), vol("スローループ", 2), vol("スローループ", 3)];
    expect(kinds(groupRows(all, true, NONE).entries)).toBe("G3");
    const subset = [all[1] as ListRow];
    const grouped = groupRows(subset, true, NONE);
    expect(kinds(grouped.entries)).toBe("B1");
    expect(grouped.groups.size).toBe(0);
    expect(grouped.groupOfBook.size).toBe(0);
  });

  it("作品名が空（空白だけ）の本は、グループにしない", () => {
    const rows = [vol(" 　 ", 1), vol(" 　 ", 2)];
    expect(kinds(groupRows(rows, true, NONE).entries)).toBe("B1 B1");
  });

  it("巻の表記のない書名が同じなだけの本（版違いなど）も、同じ著者なら 1 つにまとめる", () => {
    const rows = [
      vol("吾輩は猫である", null, ["夏目漱石"]),
      vol("吾輩は猫である", null, ["夏目漱石"]),
    ];
    const grouped = groupRows(rows, true, NONE);
    expect(kinds(grouped.entries)).toBe("G2");
    const group = grouped.entries[0] as Extract<ListEntry, { kind: "group" }>;
    expect(group.group.firstVolume).toBeNull();
    expect(group.group.lastVolume).toBeNull();
    // 巻の値がなければ、元の並びのまま
    expect(group.group.members.map((m) => m.id)).toEqual(rows.map((r) => r.id));
  });
});

describe("グループの key", () => {
  it("`work:` + 作品名の比べ方 + 区切り + 著者の比べ方のうち最小のもの。本の ID とは重ならない", () => {
    const rows = [vol("風", 1, ["鈴木 花子", "山田 太郎"]), vol("風", 2, ["山田 太郎"])];
    const grouped = groupRows(rows, true, NONE);
    const [key] = keysOf(grouped);
    expect(key?.startsWith("work:")).toBe(true);
    // 著者の比べ方のうち、文字列として最小のもの
    expect("山田太郎" < "鈴木花子").toBe(true);
    expect(key).toBe("work:風\u001f山田太郎");
    expect(isGroupKey(key)).toBe(true);
    for (const row of rows) expect(isGroupKey(row.id)).toBe(false);
    expect(grouped.groups.get(key as string)?.key).toBe(key);
  });

  it("著者のない作品の key は、区切りの後ろが空", () => {
    const grouped = groupRows([vol("風", 1, []), vol("風", 2, [])], true, NONE);
    expect(keysOf(grouped)).toEqual(["work:風\u001f"]);
  });

  it("同じ入力なら同じ key（並べ替えや展開の状態が変わっても変わらない）", () => {
    const rows = [vol("風", 1), vol("風", 2), vol("雲", 1)];
    const a = groupRows(rows, true, NONE);
    const b = groupRows([...rows].reverse(), true, new Set(keysOf(a)));
    expect([...a.groups.keys()]).toEqual([...b.groups.keys()]);
  });

  it("isGroupKey: null・本の ID は false", () => {
    expect(isGroupKey(null)).toBe(false);
    expect(isGroupKey(undefined)).toBe(false);
    expect(isGroupKey("0191d2a0-1234-7abc-8def-0123456789ab")).toBe(false);
    expect(isGroupKey("work:x")).toBe(true);
  });
});

describe("並び", () => {
  it("グループは、最初のメンバー（与えられた並びで先に出る本）の位置に出る", () => {
    // 並びは A1 B1 A2 C1 A3 → グループ A は先頭の位置、B・C はそのまま
    const a = [vol("A", 1), vol("A", 2), vol("A", 3)] as ListRow[];
    const rows = [a[0], vol("B", 1), a[1], vol("C", 1), a[2]] as ListRow[];
    const grouped = groupRows(rows, true, NONE);
    expect(kinds(grouped.entries)).toBe("G3 B1 B1");
    expect(grouped.entries.slice(1).map((e) => e.key)).toEqual([rows[1]?.id, rows[3]?.id]);
    // bookIds は、表示の順（グループの中は巻の順）
    expect(grouped.bookIds).toEqual(
      [a[0], a[1], a[2], rows[1], rows[3]].map((r) => (r as ListRow).id),
    );
  });

  it("書名以外の並べ替え（出版社の降順など）でも、グループは最初のメンバーの位置に出る", () => {
    const rows = [
      vol("A", 1, ["x"], { publisher: "あ社" }),
      vol("A", 2, ["x"], { publisher: "う社" }),
      vol("B", 1, ["y"], { publisher: "い社" }),
      vol("A", 3, ["x"], { publisher: "あ社" }),
    ];
    const sorted = sortRows(rows, [{ id: "publisher", desc: true }], { locationPaths: new Map() });
    // う社(A2) い社(B1) あ社(A1) あ社(A3)
    expect(sorted.map((r) => r.id)).toEqual([rows[1], rows[2], rows[0], rows[3]].map((r) => r?.id));
    const grouped = groupRows(sorted, true, NONE);
    expect(kinds(grouped.entries)).toBe("G3 B1");
    expect(grouped.entries[0]?.kind).toBe("group");
    expect(grouped.entries[1]?.key).toBe(rows[2]?.id);
    // グループの中は、並べ替えによらず巻の順
    const group = grouped.entries[0] as Extract<ListEntry, { kind: "group" }>;
    expect(group.group.members.map((m) => m.volumeSort)).toEqual([1, 2, 3]);
  });

  it("メンバーは volumeSort（なければ書名から読んだ番号）の昇順。値のない本は最後。同じなら元の並び", () => {
    const rows = [
      vol("A", null, ["x"], { id: "none-1" }),
      vol("A", 2, ["x"], { id: "two" }),
      vol("A", 1, ["x"], { id: "one-first" }),
      vol("A", 1, ["x"], { id: "one-second" }),
      vol("A 3", null, ["x"], { id: "implied-three" }),
      vol("A", null, ["x"], { id: "none-2" }),
    ];
    const grouped = groupRows(rows, true, NONE);
    const group = grouped.entries[0] as Extract<ListEntry, { kind: "group" }>;
    expect(group.group.members.map((m) => m.id)).toEqual([
      "one-first",
      "one-second",
      "two",
      "implied-three",
      "none-1",
      "none-2",
    ]);
    // 先頭・末尾の巻の表示は、巻をもつ本のうちの最初と最後
    expect(group.group.firstVolume).toBe("1");
    expect(group.group.lastVolume).toBe("3");
    // 書名の作品名は、巻の順で先頭の本のもの
    expect(group.group.baseTitle).toBe("A");
  });

  it("展開したグループは、グループの行のあとに、メンバーが level 2 で続く。畳んだら続かない", () => {
    const rows = [vol("B", 1, ["y"]), vol("A", 2), vol("A", 1), vol("C", 1, ["z"])];
    const closed = groupRows(rows, true, NONE);
    expect(kinds(closed.entries)).toBe("B1 G2 B1");
    const key = closed.entries[1]?.key as string;

    const open = groupRows(rows, true, new Set([key]));
    expect(kinds(open.entries)).toBe("B1 G2 B2 B2 B1");
    const group = open.entries[1] as Extract<ListEntry, { kind: "group" }>;
    expect(group.expanded).toBe(true);
    expect(open.entries.slice(2, 4).map((e) => e.key)).toEqual([rows[2]?.id, rows[1]?.id]);
    for (const entry of open.entries.slice(2, 4)) {
      expect(entry).toMatchObject({ kind: "book", level: 2, groupKey: key });
    }
    expect(open.entries[0]).toMatchObject({ kind: "book", level: 1, groupKey: null });
    // 展開しているかによらず、bookIds・グループの対応は同じ
    expect(open.bookIds).toEqual(closed.bookIds);
    expect([...open.groups.keys()]).toEqual([...closed.groups.keys()]);
    expect(open.expanded.has(key)).toBe(true);
  });

  it("グループの対応・行の対応が作られる", () => {
    const rows = [vol("A", 1), vol("A", 2), vol("B", 1, ["y"])];
    const grouped = groupRows(rows, true, NONE);
    const key = keysOf(grouped)[0] as string;
    expect(grouped.groupOfBook.get(rows[0]?.id as string)).toBe(key);
    expect(grouped.groupOfBook.get(rows[1]?.id as string)).toBe(key);
    expect(grouped.groupOfBook.has(rows[2]?.id as string)).toBe(false);
    expect(grouped.rowById.get(rows[2]?.id as string)).toBe(rows[2]);
    expect(grouped.rowById.size).toBe(3);
  });
});

describe("オフのとき", () => {
  it("すべてふつうの本の行（level 1）で、グループはない。展開の状態は無視する", () => {
    const rows = [vol("A", 1), vol("A", 2), vol("B", 1, ["y"])];
    const grouped = groupRows(rows, false, new Set(["work:あ\u001fx"]));
    expect(grouped.enabled).toBe(false);
    expect(grouped.entries).toEqual(
      rows.map((row) => ({ kind: "book", key: row.id, row, level: 1, groupKey: null })),
    );
    expect(grouped.groups.size).toBe(0);
    expect(grouped.groupOfBook.size).toBe(0);
    expect(grouped.bookIds).toEqual(rows.map((r) => r.id));
  });

  it("行がなければ、並びも空", () => {
    for (const enabled of [true, false]) {
      const grouped = groupRows([], enabled, NONE);
      expect(grouped.entries).toEqual([]);
      expect(grouped.bookIds).toEqual([]);
    }
  });
});

describe("layoutEntries（展開の状態だけを変えたとき、グループの計算は使い回す）", () => {
  it("同じレイアウトから、展開の状態ごとに並びを作る。グループ・ID の配列は共有する", () => {
    const rows = [vol("A", 1), vol("A", 2), vol("B", 1, ["y"])];
    const layout = buildWorkLayout(rows, true);
    const closed = layoutEntries(layout, NONE);
    const key = keysOf(closed)[0] as string;
    const open = layoutEntries(layout, new Set([key]));
    expect(kinds(closed.entries)).toBe("G2 B1");
    expect(kinds(open.entries)).toBe("G2 B2 B2 B1");
    expect([...open.groups.keys()]).toEqual([...closed.groups.keys()]);
    expect(open.bookIds).toBe(closed.bookIds);
    // 変わらない行のオブジェクトは同じ（行の再描画を避けられる）
    expect(open.entries[3]).toBe(closed.entries[1]);
  });
});

describe("expandToBookIds / countBooks", () => {
  const rows = [vol("B", 1, ["y"]), vol("A", 2), vol("A", 1), vol("A", 3), vol("C", 1, ["z"])];
  const grouped = groupRows(rows, true, NONE);
  const groupKey = keysOf(grouped)[1] as string;
  const id = (index: number) => (rows[index] as ListRow).id;

  it("グループの key は全メンバーの ID（巻の順）に、本の ID はそのままにする", () => {
    expect(expandToBookIds([groupKey], grouped)).toEqual([id(2), id(1), id(3)]);
    expect(expandToBookIds([id(0), id(4)], grouped)).toEqual([id(0), id(4)]);
  });

  it("表示の順（bookIds の順）に並べ、重なりは 1 つにする", () => {
    // 選んだ順に関わらず、表示の順
    expect(expandToBookIds([id(4), groupKey, id(0), id(1)], grouped)).toEqual([
      id(0),
      id(2),
      id(1),
      id(3),
      id(4),
    ]);
    expect(countBooks([id(4), groupKey, id(0), id(1)], grouped)).toBe(5);
  });

  it("並びにない key（消えた本）は含めない。空なら空", () => {
    expect(expandToBookIds(["no-such-book"], grouped)).toEqual([]);
    expect(expandToBookIds([], grouped)).toEqual([]);
    expect(countBooks(new Set([groupKey]), grouped)).toBe(3);
  });

  it("グループがない並び（オフ）では、本の ID がそのまま", () => {
    const plain = groupRows(rows, false, NONE);
    expect(expandToBookIds([id(3), id(0)], plain)).toEqual([id(0), id(3)]);
  });
});

// 本ごとの手動の指定（`workId`。0026）。"none" はどの作品にも入れない、それ以外は手動の作品の ID
describe("作品の手動の指定（0026）", () => {
  const W1 = "w-1";
  const W2 = "w-2";
  const KEY1 = "work:\u001fw-1";
  const KEY2 = "work:\u001fw-2";
  /** 手動の指定のある本。`id` は作品の ID・書名・巻・著者から作る。 */
  function man(
    workId: string | null,
    title: string,
    volume: number | null,
    authors: string[] = ["山田 太郎"],
    extra: Partial<ListRow> = {},
  ): ListRow {
    return vol(title, volume, authors, {
      id: `${workId ?? "auto"}:${title}-${volume ?? "-"}-${authors.join("+")}`,
      workId,
      ...extra,
    });
  }
  const groupAt = (grouped: GroupedList, index: number) =>
    grouped.entries[index] as Extract<ListEntry, { kind: "group" }>;
  const manualSlow = () => [1, 2, 3, 4, 5, 6, 7].map((n) => man(W1, "スローループ", n));

  describe("作品から外した本（none）", () => {
    it("書名・著者が同じ本があっても、どの作品にも入らない。束にも入れないので、相手の自動の本もグループにならない", () => {
      const rows = [vol("風", 1), man("none", "風", 2), vol("風", 3, ["山田 太郎"], { id: "x" })];
      // 自動の本は 2 冊（風 1・風 3）で、"none" の 1 冊は入らない
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G2 B1");
      expect(grouped.groupOfBook.has((rows[1] as ListRow).id)).toBe(false);
      expect(grouped.entries[1]?.key).toBe((rows[1] as ListRow).id);

      // 自動の本が 1 冊と "none" の本だけなら、どちらもふつうの行
      expect(kinds(groupRows([rows[0], rows[1]] as ListRow[], true, NONE).entries)).toBe("B1 B1");
      // "none" どうしも、まとめない
      expect(
        kinds(groupRows([man("none", "風", 1), man("none", "風", 2)], true, NONE).entries),
      ).toBe("B1 B1");
    });

    it("手動の作品と書名・著者が同じでも、none の本は、その作品に入らない", () => {
      const rows = [
        man(W1, "風", 1),
        man(W1, "風", 2),
        man("none", "風", 3),
        vol("風", 4, ["山田 太郎"], { id: "auto-4" }),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G3 B1");
      expect(groupAt(grouped, 0).group.members.map((m) => m.id)).toEqual([
        rows[0]?.id,
        rows[1]?.id,
        "auto-4",
      ]);
      expect(grouped.entries[1]?.key).toBe(rows[2]?.id);
    });
  });

  describe("手動の作品（同じ ID の本が 1 つの作品）", () => {
    it("書名・著者が違っても、同じ ID なら 1 つのグループ。key は `work:` + 区切り + ID。名前は巻の順で先頭の本のもの", () => {
      const rows = [
        man(W1, "スローループ 公式アンソロジー", null, ["別の 著者"]),
        man(W1, "スローループ", 2),
        man(W1, "スローループ", 1),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G3");
      const { group } = groupAt(grouped, 0);
      expect(group.key).toBe(KEY1);
      expect(isGroupKey(group.key)).toBe(true);
      // 巻のない本は最後。作品名は先頭の本（スローループ 1 巻）のもの
      expect(group.members.map((m) => m.volume)).toEqual(["1", "2", null]);
      expect(group.baseTitle).toBe("スローループ");
      expect(group.firstVolume).toBe("1");
      expect(group.lastVolume).toBe("2");
      expect(grouped.groupOfBook.get((rows[0] as ListRow).id)).toBe(group.key);
      expect(grouped.bookIds).toEqual(group.members.map((m) => m.id));
    });

    it("書名が空の本・著者のない本でも、手動の作品には入る", () => {
      const rows = [man(W1, " ", null, []), man(W1, "風", 1, [])];
      expect(kinds(groupRows(rows, true, NONE).entries)).toBe("G2");
    });

    it("グループの位置は、最初のメンバー（与えられた並びで先に出る本）の位置。ほかの本は、そのまま", () => {
      const rows = [
        vol("あ", 1, ["a"]),
        man(W1, "い", 1, ["b"]),
        vol("う", 1, ["c"]),
        man(W1, "え", 1, ["d"]),
        vol("お", 1, ["e"]),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("B1 G2 B1 B1");
      expect(keysOf(grouped)).toEqual([rows[0]?.id, KEY1, rows[2]?.id, rows[4]?.id]);
    });

    it("違う ID の手動の作品どうしは、書名・著者が同じでも、まとめない", () => {
      const rows = [
        man(W1, "風", 1),
        man(W1, "風", 2),
        man(W2, "風", 3),
        man(W2, "風", 4),
        // 両方の手動の作品とつながる自動の本は、どちらにも入らない
        vol("風", 5, ["山田 太郎"], { id: "auto-5" }),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G2 G2 B1");
      expect(keysOf(grouped)).toEqual([KEY1, KEY2, "auto-5"]);
    });

    it("表示中に 1 冊しかない手動の作品は、ふつうの行（絞り込みの結果。表示中の本だけで決まる）", () => {
      const all = [man(W1, "風", 1), man(W1, "風", 2), man(W1, "雲", 3, ["別の 著者"])];
      expect(kinds(groupRows(all, true, NONE).entries)).toBe("G3");
      expect(kinds(groupRows(all.slice(1), true, NONE).entries)).toBe("G2");
      const one = groupRows(all.slice(2), true, NONE);
      expect(kinds(one.entries)).toBe("B1");
      expect(one.groups.size).toBe(0);
    });
  });

  describe("自動の作品が、手動の作品に入る", () => {
    it("あとから登録した巻（自動の 1 冊。書名の末尾だけに巻がある）は、書名・著者が同じ手動の作品に入り、巻の順で最後に並ぶ", () => {
      const newest = vol("スローループ 8", null, ["山田 太郎"], { id: "new-8" });
      const grouped = groupRows([newest, ...manualSlow()], true, NONE);
      expect(kinds(grouped.entries)).toBe("G8");
      const { group } = groupAt(grouped, 0);
      expect(group.key).toBe(KEY1);
      expect(group.members.at(-1)?.id).toBe("new-8");
      expect(group.lastVolume).toBe("8");
      expect(group.baseTitle).toBe("スローループ");
      expect(grouped.groupOfBook.get("new-8")).toBe(KEY1);
    });

    it("著者が 1 人でも共通すれば入る。共通する著者がなければ入らない", () => {
      const rows = [
        ...manualSlow(),
        vol("スローループ 8", null, ["山田 太郎", "別の 著者"], { id: "co-author" }),
        vol("スローループ 9", null, ["他の 人"], { id: "stranger" }),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G8 B1");
      expect(groupAt(grouped, 0).group.members.map((m) => m.id)).toContain("co-author");
      expect(grouped.entries[1]?.key).toBe("stranger");
    });

    it("書名（作品名の比べ方）が違えば、著者が同じでも入らない", () => {
      const rows = [...manualSlow(), vol("スローループ 公式アンソロジー", null, ["山田 太郎"])];
      expect(kinds(groupRows(rows, true, NONE).entries)).toBe("G7 B1");
    });

    it("自動の作品が 2 冊以上あれば、まとめて入る（著者のない本も、その作品の一員なら一緒に）", () => {
      const rows = [
        ...manualSlow(),
        vol("スローループ", 8, ["山田 太郎"], { id: "auto-8" }),
        vol("スローループ", 9, ["山田 太郎", "絵 担当"], { id: "auto-9" }),
        vol("スローループ", 10, [], { id: "auto-10" }),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G10");
      expect(groupAt(grouped, 0).group.members.map((m) => m.volumeSort)).toEqual([
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
      ]);
    });

    it("著者のない自動の本だけの作品は、手動の作品とつながらない（自動の作品のまま）", () => {
      const single = [...manualSlow(), vol("スローループ", 8, [], { id: "no-author" })];
      const grouped = groupRows(single, true, NONE);
      expect(kinds(grouped.entries)).toBe("G7 B1");
      expect(grouped.entries[1]?.key).toBe("no-author");
      // 著者のない本どうしは、自動の作品としてまとまる（手動の作品には入らない）
      const two = [
        ...manualSlow(),
        vol("スローループ", 8, [], { id: "n8" }),
        vol("スローループ", 9, [], { id: "n9" }),
      ];
      expect(kinds(groupRows(two, true, NONE).entries)).toBe("G7 G2");
    });

    it("手動の作品の側に、書名か著者がない本しかなければ、つながりは作らない", () => {
      const noAuthors = [man(W1, "風", 1, []), man(W1, "風", 2, []), vol("風", 3, ["山田 太郎"])];
      expect(kinds(groupRows(noAuthors, true, NONE).entries)).toBe("G2 B1");
      const noTitle = [man(W1, " ", 1), man(W1, " ", 2), vol(" ", 3, ["山田 太郎"])];
      expect(kinds(groupRows(noTitle, true, NONE).entries)).toBe("G2 B1");
    });

    it("つながる手動の作品が 2 つ以上なら、どれにも入らない。自動の作品は自動のまま（2 冊以上ならグループ）", () => {
      const rows = [
        man(W1, "風", 1, ["A"]),
        man(W1, "風", 2, ["A"]),
        man(W2, "風", 3, ["B"]),
        man(W2, "風", 4, ["B"]),
        vol("風", 5, ["A", "B"], { id: "bridge-1" }),
        vol("風", 6, ["A"], { id: "bridge-2" }),
        vol("風", 7, ["B"], { id: "lonely" }),
      ];
      const grouped = groupRows(rows, true, NONE);
      // 風 5・6・7 は著者でつながって 1 つの自動の作品（3 冊）。A の作品にも B の作品にも触れるので、どちらにも入らない
      expect(kinds(grouped.entries)).toBe("G2 G2 G3");
      const auto = groupAt(grouped, 2).group;
      expect(auto.key).toBe("work:風\u001fa");
      expect(auto.members.map((m) => m.id)).toEqual(["bridge-1", "bridge-2", "lonely"]);
    });

    it("1 冊だけの自動の本が 2 つの手動の作品とつながるときも、どちらにも入らない", () => {
      const rows = [
        man(W1, "風", 1, ["A"]),
        man(W1, "風", 2, ["A"]),
        man(W2, "風", 3, ["B"]),
        man(W2, "風", 4, ["B"]),
        vol("風", 5, ["A", "B"], { id: "bridge" }),
      ];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G2 G2 B1");
      expect(grouped.entries[2]?.key).toBe("bridge");
    });

    it("表示中の手動の本が 1 冊だけでも、自動の本が入れば、2 冊のグループになる", () => {
      const rows = [man(W1, "風", 1), vol("風", 2, ["山田 太郎"], { id: "auto-2" })];
      const grouped = groupRows(rows, true, NONE);
      expect(kinds(grouped.entries)).toBe("G2");
      expect(groupAt(grouped, 0).group.key).toBe(KEY1);
      // 手動の本が 1 冊で、入る自動の本がなければ、ふつうの行
      expect(kinds(groupRows([rows[0] as ListRow], true, NONE).entries)).toBe("B1");
    });

    it("つながりは、表示中の手動の本だけで決まる（絞り込みで手動の本が隠れたら、自動の作品に戻る）", () => {
      const manual = [man(W1, "風", 1, ["A"]), man(W1, "雲", 2, ["B"])];
      const autos = [
        vol("雲", 3, ["B"], { id: "cloud-3" }),
        vol("雲", 4, ["B"], { id: "cloud-4" }),
      ];
      // 雲 2（手動）が見えていれば、雲 3・雲 4 は手動の作品に入る
      expect(kinds(groupRows([...manual, ...autos], true, NONE).entries)).toBe("G4");
      // 雲 2 が絞り込みで隠れると、雲 3・雲 4 は自動の作品（手動の風 1 は 1 冊で、ふつうの行）
      const hidden = groupRows([manual[0] as ListRow, ...autos], true, NONE);
      expect(kinds(hidden.entries)).toBe("B1 G2");
      expect(groupAt(hidden, 1).group.key).toBe("work:雲\u001fb");
    });
  });

  it("「巻をまとめる」がオフなら、workId は使わない（すべてふつうの行）", () => {
    const rows = [man(W1, "風", 1), man(W1, "雲", 2, ["別の 著者"]), man("none", "風", 3)];
    const grouped = groupRows(rows, false, NONE);
    expect(grouped.entries.map((e) => e.kind)).toEqual(["book", "book", "book"]);
    expect(grouped.groups.size).toBe(0);
    expect(grouped.bookIds).toEqual(rows.map((r) => r.id));
  });

  it("手動の作品の key で開閉できる（layoutEntries）。開くと、メンバーが level 2 で続く", () => {
    const rows = [man(W1, "風", 1), man(W1, "雲", 2, ["別の 著者"]), vol("嵐", 1, ["x"])];
    const layout = buildWorkLayout(rows, true);
    expect(kinds(layoutEntries(layout, NONE).entries)).toBe("G2 B1");
    const open = layoutEntries(layout, new Set([KEY1]));
    expect(kinds(open.entries)).toBe("G2 B2 B2 B1");
    expect(open.entries[1]).toMatchObject({ level: 2, groupKey: KEY1 });
  });

  it("expandToBookIds: 手動の作品の key は、表示中の全メンバー（入った自動の本を含む。隠れた本は含まない）に広がる", () => {
    const rows = [
      vol("嵐", 1, ["x"]),
      man(W1, "風", 1),
      man(W1, "雲", 2, ["別の 著者"]),
      vol("風", 3, ["山田 太郎"], { id: "absorbed" }),
      man(W1, "霧", 9, ["別の 著者"]),
    ];
    // 絞り込みで、霧 9 は隠れている
    const grouped = groupRows(rows.slice(0, 4), true, NONE);
    expect(keysOf(grouped)).toEqual([rows[0]?.id, KEY1]);
    expect(expandToBookIds([KEY1], grouped)).toEqual([rows[1]?.id, rows[2]?.id, "absorbed"]);
    expect(expandToBookIds([KEY1, rows[0]?.id as string], grouped)).toEqual([
      rows[0]?.id,
      rows[1]?.id,
      rows[2]?.id,
      "absorbed",
    ]);
    expect(countBooks([KEY1], grouped)).toBe(3);
  });
});

describe("groupSummary（グループの行に出す値）", () => {
  const ctx = { locationPaths: new Map([["loc-1", "自宅 > 書斎"]]) };
  function summaryOf(rows: ListRow[]) {
    const grouped = groupRows(rows, true, NONE);
    const entry = grouped.entries[0] as Extract<ListEntry, { kind: "group" }>;
    return groupSummary(entry.group, ctx);
  }

  it("全員が同じ値の列は、その値。ばらばらの列は空（blank）", () => {
    const { row, blank } = summaryOf([
      vol("A", 1, ["x"], { publisher: "岩波書店", rating: 3, isbn13: "9784000000001" }),
      vol("A", 2, ["x"], { publisher: "岩波書店", rating: 4, isbn13: "9784000000002" }),
      vol("A", 3, ["x"], { publisher: "岩波書店", rating: 3, isbn13: "9784000000003" }),
    ]);
    expect(blank.has("publisher")).toBe(false);
    expect(row.publisher).toBe("岩波書店");
    expect(blank.has("rating")).toBe(true);
    expect(blank.has("isbn13")).toBe(true);
    // 著者は同じ
    expect(blank.has("authors")).toBe(false);
  });

  it("値がないことで揃っている列（null どうし）は空のまま。揃っていなければ blank", () => {
    const { blank } = summaryOf([
      vol("A", 1, ["x"], { locationId: null }),
      vol("A", 2, ["x"], { locationId: "loc-1" }),
    ]);
    expect(blank.has("location")).toBe(true);
    expect(blank.has("tags")).toBe(false);
  });

  it("書名は作品名、巻は範囲（片方だけならその値、なければ空）。blank に入れない", () => {
    const withRange = summaryOf([vol("A", 1, ["x"]), vol("A", 7, ["x"])]);
    expect(withRange.row.title).toBe("A");
    expect(withRange.row.volume).toBe("1〜7");
    expect(withRange.blank.has("title")).toBe(false);
    expect(withRange.blank.has("volume")).toBe(false);
    expect(withRange.row.subtitle).toBeNull();

    const single = summaryOf([vol("A", 1, ["x"]), vol("A", null, ["x"])]);
    expect(single.row.volume).toBe("1");

    const none = summaryOf([vol("A", null, ["x"]), vol("A", null, ["x"])]);
    expect(none.row.volume).toBeNull();
  });

  it("シリーズ名は、全員が同じ（空でない）ときだけ残す", () => {
    const same = summaryOf([
      vol("A", 1, ["x"], { seriesName: "Aシリーズ" }),
      vol("A", 2, ["x"], { seriesName: "Aシリーズ" }),
    ]);
    expect(same.row.seriesName).toBe("Aシリーズ");
    const different = summaryOf([
      vol("A", 1, ["x"], { seriesName: "Aシリーズ" }),
      vol("A", 2, ["x"], { seriesName: null }),
    ]);
    expect(different.row.seriesName).toBeNull();
    expect(different.blank.has("series")).toBe(true);
    expect(summaryOf([vol("A", 1, ["x"]), vol("A", 2, ["x"])]).row.seriesName).toBeNull();
  });

  it("代表の行は先頭の本をもとにする（書影など並べ替えのキーのない列は先頭の本の値）。全員ゴミ箱なら deleted", () => {
    const rows = [
      vol("A", 2, ["x"], { coverHash: "bb", deleted: true }),
      vol("A", 1, ["x"], { coverHash: "aa", deleted: true }),
    ];
    const { row } = summaryOf(rows);
    expect(row.coverHash).toBe("aa");
    expect(row.id).toBe(rows[1]?.id);
    expect(row.deleted).toBe(true);
    expect(summaryOf([vol("A", 1, ["x"], { deleted: true }), vol("A", 2, ["x"])]).row.deleted).toBe(
      false,
    );
  });
});
