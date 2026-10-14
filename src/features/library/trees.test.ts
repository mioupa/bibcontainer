import { describe, expect, it } from "vitest";
import { UNSET } from "@/features/library/keys";
import {
  buildContentTree,
  buildLocationIndex,
  buildLocationPaths,
  buildLocationTree,
  buildTagTree,
  collectTagPaths,
  flattenLocations,
  tagPrefixes,
  topEntries,
} from "@/features/library/trees";
import type { Location } from "@/types/contracts";

const locations: Location[] = [
  { id: "home", name: "自宅", parentId: null, sortOrder: 1 },
  { id: "study", name: "書斎", parentId: "home", sortOrder: 2 },
  { id: "living", name: "リビング", parentId: "home", sortOrder: 1 },
  { id: "shelf", name: "本棚", parentId: "study", sortOrder: 1 },
  { id: "lab", name: "研究室", parentId: null, sortOrder: 2 },
];

describe("保管場所", () => {
  it("子孫の集合（自分を含む）を引ける", () => {
    const index = buildLocationIndex(locations);
    expect([...index.descendantsOf("home")].sort()).toEqual(["home", "living", "shelf", "study"]);
    expect([...index.descendantsOf("lab")]).toEqual(["lab"]);
    expect([...index.descendantsOf("unknown")]).toEqual(["unknown"]);
  });

  it("親子のループがあっても止まる", () => {
    const loop: Location[] = [
      { id: "a", name: "A", parentId: "b", sortOrder: null },
      { id: "b", name: "B", parentId: "a", sortOrder: null },
    ];
    expect([...buildLocationIndex(loop).descendantsOf("a")].sort()).toEqual(["a", "b"]);
    expect(buildLocationPaths(loop).get("a")).toBe("B > A");
    expect(buildLocationTree(loop, new Map()).length).toBeGreaterThan(0);
  });

  it("パスを作る", () => {
    const paths = buildLocationPaths(locations);
    expect(paths.get("shelf")).toBe("自宅 > 書斎 > 本棚");
    expect(paths.get("lab")).toBe("研究室");
  });

  it("ツリーは sortOrder 順で、件数は子孫を足す", () => {
    const tree = buildLocationTree(
      locations,
      new Map([
        ["shelf", 3],
        ["living", 1],
        ["lab", 2],
        ["home", 1],
      ]),
    );
    expect(tree.map((n) => n.name)).toEqual(["自宅", "研究室"]);
    const home = tree[0];
    expect(home?.count).toBe(5);
    expect(home?.children.map((n) => n.name)).toEqual(["リビング", "書斎"]);
    expect(home?.children[1]?.count).toBe(3);
    expect(tree[1]?.count).toBe(2);
  });

  it("親が見つからない場所は根として扱う", () => {
    const tree = buildLocationTree(
      [{ id: "x", name: "X", parentId: "gone", sortOrder: null }],
      new Map(),
    );
    expect(tree.map((n) => n.id)).toEqual(["x"]);
  });
});

describe("タグ", () => {
  it("階層ごとのパスに分ける", () => {
    expect(tagPrefixes("研究/SiC/論文")).toEqual(["研究", "研究/SiC", "研究/SiC/論文"]);
    expect(tagPrefixes("小説")).toEqual(["小説"]);
  });

  it("ツリーを作る（親がなくても、階層の途中まで作られる）", () => {
    const paths = collectTagPaths([{ tags: ["研究/SiC", "小説"] }, { tags: ["研究/論文"] }]);
    const tree = buildTagTree(
      paths,
      new Map([
        ["研究", 3],
        ["研究/SiC", 1],
        ["小説", 2],
      ]),
    );
    expect(tree.map((n) => n.name)).toEqual(["小説", "研究"].sort());
    const research = tree.find((n) => n.path === "研究");
    expect(research?.count).toBe(3);
    expect(research?.children.map((n) => [n.path, n.name, n.count])).toEqual([
      ["研究/SiC", "SiC", 1],
      ["研究/論文", "論文", 0],
    ]);
  });
});

describe("内容（Cコード）の分類", () => {
  it("大分類の下に中分類を並べる", () => {
    const tree = buildContentTree(
      new Map([
        ["9", 3],
        ["5", 1],
        [UNSET, 2],
      ]),
      new Map([
        ["93", 2],
        ["91", 1],
        ["55", 1],
      ]),
    );
    expect(tree.map((n) => n.major)).toEqual([
      "0",
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
      UNSET,
    ]);
    expect(tree.find((n) => n.major === "9")?.minors).toEqual([
      { code: "91", count: 1 },
      { code: "93", count: 2 },
    ]);
    expect(tree.find((n) => n.major === "0")?.count).toBe(0);
  });
});

describe("topEntries", () => {
  it("件数の多い順（同数は名前順）に上位だけを返し、選択中の項目は必ず含める", () => {
    const counts = new Map([
      ["b", 5],
      ["a", 5],
      ["c", 1],
      ["d", 1],
      [UNSET, 9],
      ["zero", 0],
    ]);
    expect(topEntries(counts, 2).map((e) => e.key)).toEqual(["a", "b"]);
    expect(topEntries(counts, 2, "d").map((e) => e.key)).toEqual(["a", "b", "d"]);
    expect(topEntries(counts, 3, "a").map((e) => e.key)).toEqual(["a", "b", "c"]);
  });
});

describe("flattenLocations（選択欄に出す保管場所の一覧）", () => {
  it("深さ優先の順（並び順 sortOrder、同じなら名前）で、パスと深さを付ける", () => {
    expect(flattenLocations(locations).map((o) => [o.path, o.depth])).toEqual([
      ["自宅", 0],
      ["自宅 > リビング", 1],
      ["自宅 > 書斎", 1],
      ["自宅 > 書斎 > 本棚", 2],
      ["研究室", 0],
    ]);
  });

  it("指定した保管場所とその子孫は除く（移動先に、自分の下を選ばせない）", () => {
    const exclude = buildLocationIndex(locations).descendantsOf("study");
    expect(flattenLocations(locations, exclude).map((o) => o.id)).toEqual([
      "home",
      "living",
      "lab",
    ]);
  });

  it("親子のループがあっても、止まって、すべて出す", () => {
    const looped: Location[] = [
      { id: "a", name: "A", parentId: "b", sortOrder: 1 },
      { id: "b", name: "B", parentId: "a", sortOrder: 1 },
    ];
    expect(
      flattenLocations(looped)
        .map((o) => o.id)
        .sort(),
    ).toEqual(["a", "b"]);
  });
});
