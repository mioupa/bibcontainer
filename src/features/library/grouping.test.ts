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
