// サイドバーのツリー（保管場所・タグ・内容分類）を作る純粋関数。
import { UNSET } from "@/features/library/keys";
import type { Location } from "@/types/contracts";

// ===== 保管場所 =====

export interface LocationIndex {
  /** 保管場所 id と、その子孫の id（自分を含む）の集合を返す。知らない id は自分だけ。 */
  descendantsOf: (id: string) => ReadonlySet<string>;
}

/** 親子のループ（同期の衝突で起こりうる）があっても無限ループにしない。 */
export function buildLocationIndex(locations: readonly Location[]): LocationIndex {
  const children = new Map<string, string[]>();
  for (const loc of locations) {
    if (loc.parentId === null) continue;
    const list = children.get(loc.parentId);
    if (list) list.push(loc.id);
    else children.set(loc.parentId, [loc.id]);
  }
  const cache = new Map<string, ReadonlySet<string>>();
  return {
    descendantsOf(id) {
      const cached = cache.get(id);
      if (cached) return cached;
      const result = new Set<string>([id]);
      const stack = [id];
      while (stack.length > 0) {
        const current = stack.pop();
        if (current === undefined) break;
        for (const child of children.get(current) ?? []) {
          if (!result.has(child)) {
            result.add(child);
            stack.push(child);
          }
        }
      }
      cache.set(id, result);
      return result;
    },
  };
}

/** 「自宅 > 書斎の本棚 > 3段目」の形のパス。一覧の列と詳細ペインで使う。 */
export function buildLocationPaths(
  locations: readonly Location[],
  separator = " > ",
): ReadonlyMap<string, string> {
  const byId = new Map(locations.map((l) => [l.id, l]));
  const paths = new Map<string, string>();
  for (const loc of locations) {
    const names: string[] = [];
    const seen = new Set<string>();
    let current: Location | undefined = loc;
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      names.unshift(current.name);
      current = current.parentId === null ? undefined : byId.get(current.parentId);
    }
    paths.set(loc.id, names.join(separator));
  }
  return paths;
}

export interface LocationNode {
  id: string;
  name: string;
  /** この場所と子孫の冊数。 */
  count: number;
  children: LocationNode[];
}

function compareText(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** 直接の冊数（`direct`）から、子孫を足した冊数つきのツリーを作る。並びは sortOrder、同じなら名前。 */
export function buildLocationTree(
  locations: readonly Location[],
  direct: ReadonlyMap<string, number>,
): LocationNode[] {
  const ids = new Set(locations.map((l) => l.id));
  const childrenOf = new Map<string | null, Location[]>();
  for (const loc of locations) {
    // 親が見つからない（消えた）ものは根として扱う
    const parent = loc.parentId !== null && ids.has(loc.parentId) ? loc.parentId : null;
    const list = childrenOf.get(parent);
    if (list) list.push(loc);
    else childrenOf.set(parent, [loc]);
  }
  const sorted = (list: readonly Location[]) =>
    [...list].sort(
      (a, b) =>
        (a.sortOrder ?? Number.MAX_SAFE_INTEGER) - (b.sortOrder ?? Number.MAX_SAFE_INTEGER) ||
        compareText(a.name, b.name),
    );

  const build = (loc: Location, path: Set<string>): LocationNode => {
    const nextPath = new Set(path).add(loc.id);
    const children = sorted(childrenOf.get(loc.id) ?? [])
      .filter((child) => !nextPath.has(child.id))
      .map((child) => build(child, nextPath));
    const count = children.reduce((sum, c) => sum + c.count, direct.get(loc.id) ?? 0);
    return { id: loc.id, name: loc.name, count, children };
  };
  const roots = sorted(childrenOf.get(null) ?? []);
  // 親子のループだけで構成された場所は根に現れないので、見つからないものを根に足す
  const reached = new Set<string>();
  const markReached = (node: LocationNode) => {
    reached.add(node.id);
    node.children.forEach(markReached);
  };
  const tree = roots.map((root) => build(root, new Set()));
  tree.forEach(markReached);
  for (const loc of sorted(locations)) {
    if (!reached.has(loc.id)) {
      const node = build(loc, new Set());
      markReached(node);
      tree.push(node);
    }
  }
  return tree;
}

export interface LocationOption {
  id: string;
  name: string;
  /** 「自宅 > 書斎の本棚 > 3段目」の形のパス。 */
  path: string;
  /** ルートが 0。 */
  depth: number;
}

/**
 * 保管場所を、ツリーの深さ優先の順（並びは sortOrder、同じなら名前）の一覧にする。
 * 選択欄（フォーム・一括変更・移動先）に出す。`exclude` の保管場所とその子孫は除く（移動先に自分の下を選ばせない）。
 */
export function flattenLocations(
  locations: readonly Location[],
  exclude: ReadonlySet<string> = new Set(),
  separator = " > ",
): LocationOption[] {
  const result: LocationOption[] = [];
  const visit = (nodes: readonly LocationNode[], parentPath: string, depth: number) => {
    for (const node of nodes) {
      if (exclude.has(node.id)) continue;
      const path = parentPath === "" ? node.name : `${parentPath}${separator}${node.name}`;
      result.push({ id: node.id, name: node.name, path, depth });
      visit(node.children, path, depth + 1);
    }
  };
  visit(buildLocationTree(locations, new Map()), "", 0);
  return result;
}

// ===== タグ =====

/** タグ名の `/` 区切りで、すべての階層のパス（"研究/SiC" なら "研究" と "研究/SiC"）を返す。 */
export function tagPrefixes(tag: string): string[] {
  const parts = tag.split("/");
  const result: string[] = [];
  let path = "";
  for (const part of parts) {
    path = path === "" ? part : `${path}/${part}`;
    result.push(path);
  }
  return result;
}

export interface TagNode {
  /** タグの階層パス（"研究/SiC"）。絞り込みの値になる。 */
  path: string;
  /** 表示名（パスの最後の部分）。 */
  name: string;
  count: number;
  children: TagNode[];
}

/** すべての行のタグから、階層ごとのパスの集合を作る（絞り込み中も項目が消えないようにするための母集団）。 */
export function collectTagPaths(rows: readonly { tags: readonly string[] }[]): Set<string> {
  const paths = new Set<string>();
  for (const row of rows) {
    for (const tag of row.tags) {
      for (const path of tagPrefixes(tag)) paths.add(path);
    }
  }
  return paths;
}

/** パスの集合と冊数（`counts`）から、タグのツリーを作る。冊数がないパスは 0。 */
export function buildTagTree(
  allPaths: Iterable<string>,
  counts: ReadonlyMap<string, number>,
): TagNode[] {
  const nodes = new Map<string, TagNode>();
  const roots: TagNode[] = [];
  const sortedPaths = [...new Set(allPaths)].sort(compareText);
  for (const path of sortedPaths) {
    const slash = path.lastIndexOf("/");
    const node: TagNode = {
      path,
      name: slash < 0 ? path : path.slice(slash + 1),
      count: counts.get(path) ?? 0,
      children: [],
    };
    nodes.set(path, node);
    const parent = slash < 0 ? undefined : nodes.get(path.slice(0, slash));
    if (parent) parent.children.push(node);
    else roots.push(node);
  }
  return roots;
}

// ===== 内容（Cコードの大分類 → 中分類） =====

export interface ContentNode {
  /** 大分類（3 桁目）。UNSET は Cコードなし。 */
  major: string;
  count: number;
  /** 中分類（3〜4 桁目の 2 桁）と冊数。 */
  minors: { code: string; count: number }[];
}

export function buildContentTree(
  majorCounts: ReadonlyMap<string, number>,
  minorCounts: ReadonlyMap<string, number>,
): ContentNode[] {
  const nodes: ContentNode[] = [];
  for (let digit = 0; digit <= 9; digit++) {
    const major = String(digit);
    const count = majorCounts.get(major) ?? 0;
    const minors = [...minorCounts.entries()]
      .filter(([code]) => code.startsWith(major))
      .map(([code, n]) => ({ code, count: n }))
      .sort((a, b) => compareText(a.code, b.code));
    nodes.push({ major, count, minors });
  }
  const unset = majorCounts.get(UNSET) ?? 0;
  if (unset > 0) nodes.push({ major: UNSET, count: unset, minors: [] });
  return nodes;
}

/** 件数の多い順（同数なら名前順）に並べ、上位 `limit` 件を返す。`keep` は必ず含める（選択中の項目を隠さない）。 */
export function topEntries(
  counts: ReadonlyMap<string, number>,
  limit: number,
  keep?: string | null,
): { key: string; count: number }[] {
  const all = [...counts.entries()]
    .filter(([key, count]) => key !== UNSET && count > 0)
    .map(([key, count]) => ({ key, count }))
    .sort((a, b) => b.count - a.count || compareText(a.key, b.key));
  const top = all.slice(0, limit);
  if (keep && !top.some((e) => e.key === keep)) {
    const kept = all.find((e) => e.key === keep);
    if (kept) top.push(kept);
  }
  return top;
}
