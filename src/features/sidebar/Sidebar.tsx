// 左のサイドバー（FR-LIST-04）。絞り込みの項目と、そのときの件数を出す。
// 絞り込みの計算と件数の集計は features/library/filter.ts の純粋関数。ここは表示と、選択の操作だけ。
import { ChevronDownIcon, ChevronRightIcon, MoreHorizontalIcon, PlusIcon } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import type { TagInfo } from "@/bindings/TagInfo";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  LIBRARY_VIEWS,
  READ_STATUSES,
  isFilterActive,
  type AutoFilter,
  type Facets,
} from "@/features/library/filter";
import { UNSET } from "@/features/library/keys";
import {
  buildContentTree,
  buildLocationTree,
  buildTagTree,
  topEntries,
  type LocationNode,
  type TagNode,
} from "@/features/library/trees";
import { ManageDialogs, type ManageDialog } from "@/features/sidebar/ManageDialogs";
import { tagColorVar } from "@/features/sidebar/tagColor";
import { ja } from "@/i18n/ja";
import { formatInteger } from "@/lib/format";
import { cn } from "@/lib/utils";
import { sameAuto, useViewStore } from "@/stores/viewStore";
import type { CCodeTable, Location } from "@/types/contracts";

/** 出版社・著者など、数が多くなる分類で表示する上限。 */
const AUTO_LIST_LIMIT = 100;

interface SidebarProps {
  facets: Facets;
  locations: readonly Location[];
  /** すべての本のタグのパス（絞り込み中も項目が消えないようにするための母集団）。 */
  allTagPaths: ReadonlySet<string>;
  cCodes: CCodeTable;
  /** タグの色など（`list_tags`）。なければ色は出さない。 */
  tags?: readonly TagInfo[];
}

/** 項目の「…」メニュー（と右クリック）に出す操作。 */
interface MenuEntry {
  label: string;
  onSelect: () => void;
  destructive?: boolean;
}

export function Sidebar({ facets, locations, allTagPaths, cCodes, tags = [] }: SidebarProps) {
  const [dialog, setDialog] = useState<ManageDialog | null>(null);
  const tagColors = useMemo(() => new Map(tags.map((tag) => [tag.name, tag.color])), [tags]);
  const filter = useViewStore((s) => s.filter);
  const setLibrary = useViewStore((s) => s.setLibrary);
  const toggleReadStatus = useViewStore((s) => s.toggleReadStatus);
  const toggleLocation = useViewStore((s) => s.toggleLocation);
  const toggleTag = useViewStore((s) => s.toggleTag);
  const toggleAuto = useViewStore((s) => s.toggleAuto);
  const clearFilters = useViewStore((s) => s.clearFilters);

  const locationTree = useMemo(
    () => buildLocationTree(locations, facets.locations),
    [locations, facets.locations],
  );
  const tagTree = useMemo(() => buildTagTree(allTagPaths, facets.tags), [allTagPaths, facets.tags]);
  const contentTree = useMemo(
    () => buildContentTree(facets.auto.cMajor, facets.auto.cMinor),
    [facets.auto.cMajor, facets.auto.cMinor],
  );

  const autoActive = (auto: AutoFilter) => sameAuto(filter.auto, auto);
  const selectedAutoValue = (kind: AutoFilter["kind"]) =>
    filter.auto?.kind === kind ? filter.auto.value : null;

  return (
    <nav aria-label={ja.sidebar.ariaLabel} className="flex h-full min-h-0 flex-col text-sm">
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-1 p-2">
          {isFilterActive(filter) && (
            <Button variant="ghost" size="xs" className="self-start" onClick={clearFilters}>
              {ja.sidebar.clearFilters}
            </Button>
          )}

          <Group title={ja.sidebar.library.heading} defaultOpen>
            {LIBRARY_VIEWS.map((view) => (
              <Item
                key={view}
                label={ja.sidebar.library[view]}
                count={facets.library[view]}
                active={filter.library === view}
                onClick={() => setLibrary(view)}
              />
            ))}
          </Group>

          <Group title={ja.sidebar.readStatus.heading} defaultOpen>
            {READ_STATUSES.map((status) => (
              <Item
                key={status}
                label={ja.sidebar.readStatus[status]}
                count={facets.readStatus[status]}
                active={filter.readStatus === status}
                onClick={() => toggleReadStatus(status)}
              />
            ))}
          </Group>

          <Group
            title={ja.sidebar.locations.heading}
            defaultOpen
            action={
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={ja.sidebar.manage.locations.add}
                title={ja.sidebar.manage.locations.add}
                onClick={() => setDialog({ kind: "createLocation", parentId: null })}
              >
                <PlusIcon />
              </Button>
            }
          >
            {locationTree.map((node) => (
              <LocationItem
                key={node.id}
                node={node}
                depth={0}
                selectedId={filter.locationId}
                onSelect={toggleLocation}
                onManage={setDialog}
              />
            ))}
            <Item
              label={ja.sidebar.locations.unset}
              count={facets.locations.get(UNSET) ?? 0}
              active={filter.locationId === UNSET}
              onClick={() => toggleLocation(UNSET)}
            />
          </Group>

          <Group title={ja.sidebar.tags.heading} defaultOpen>
            {tagTree.map((node) => (
              <TagItem
                key={node.path}
                node={node}
                depth={0}
                selectedPath={filter.tag}
                onSelect={toggleTag}
                colors={tagColors}
                onManage={setDialog}
              />
            ))}
            <Item
              label={ja.sidebar.tags.unset}
              count={facets.tags.get(UNSET) ?? 0}
              active={filter.tag === UNSET}
              onClick={() => toggleTag(UNSET)}
            />
          </Group>

          <Group title={ja.sidebar.auto.heading}>
            <SubGroup title={ja.sidebar.auto.content}>
              {contentTree.map((node) => {
                const majorName =
                  node.major === UNSET
                    ? ja.sidebar.auto.unset
                    : `${node.major} ${cCodes.contentMajor[node.major] ?? ""}`.trim();
                return (
                  <CollapsibleItem
                    key={node.major}
                    label={majorName}
                    count={node.count}
                    active={autoActive({ kind: "cMajor", value: node.major })}
                    onClick={() => toggleAuto({ kind: "cMajor", value: node.major })}
                    hasChildren={node.minors.length > 0}
                    depth={0}
                  >
                    {node.minors.map((minor) => (
                      <Item
                        key={minor.code}
                        depth={1}
                        label={`${minor.code} ${cCodes.content[minor.code] ?? ja.cCode.undefinedContent(minor.code)}`}
                        count={minor.count}
                        active={autoActive({ kind: "cMinor", value: minor.code })}
                        onClick={() => toggleAuto({ kind: "cMinor", value: minor.code })}
                      />
                    ))}
                  </CollapsibleItem>
                );
              })}
            </SubGroup>

            <SubGroup title={ja.sidebar.auto.format}>
              {[...facets.auto.format.entries()]
                .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
                .map(([key, count]) => (
                  <Item
                    key={key}
                    label={
                      key === UNSET ? ja.sidebar.auto.unset : (cCodes.format[key] ?? `（${key}）`)
                    }
                    count={count}
                    active={autoActive({ kind: "format", value: key })}
                    onClick={() => toggleAuto({ kind: "format", value: key })}
                  />
                ))}
            </SubGroup>

            <RankedGroup
              title={ja.sidebar.auto.publisher}
              counts={facets.auto.publisher}
              selected={selectedAutoValue("publisher")}
              onSelect={(value) => toggleAuto({ kind: "publisher", value })}
            />
            <RankedGroup
              title={ja.sidebar.auto.author}
              counts={facets.auto.author}
              selected={selectedAutoValue("author")}
              onSelect={(value) => toggleAuto({ kind: "author", value })}
            />

            <SubGroup title={ja.sidebar.auto.year}>
              {[...facets.auto.year.entries()]
                .filter(([key]) => key !== UNSET)
                .sort(([a], [b]) => (a < b ? 1 : a > b ? -1 : 0))
                .map(([key, count]) => (
                  <Item
                    key={key}
                    label={key}
                    count={count}
                    active={autoActive({ kind: "year", value: key })}
                    onClick={() => toggleAuto({ kind: "year", value: key })}
                  />
                ))}
            </SubGroup>

            <SubGroup title={ja.sidebar.auto.meta} defaultOpen>
              <Item
                label={ja.sidebar.auto.metaPending}
                count={facets.auto.metaPending}
                active={autoActive({ kind: "meta", value: "pending" })}
                onClick={() => toggleAuto({ kind: "meta", value: "pending" })}
              />
              <Item
                label={ja.sidebar.auto.metaIncomplete}
                count={facets.auto.metaIncomplete}
                active={autoActive({ kind: "meta", value: "incomplete" })}
                onClick={() => toggleAuto({ kind: "meta", value: "incomplete" })}
              />
            </SubGroup>
          </Group>
        </div>
      </ScrollArea>
      <ManageDialogs dialog={dialog} locations={locations} onClose={() => setDialog(null)} />
    </nav>
  );
}

// ===== 部品 =====

function Group({
  title,
  defaultOpen = false,
  action,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  /** 見出しの右に置く操作（保管場所の追加など）。 */
  action?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className="flex flex-col">
      <div className="flex items-center justify-between">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
          className="flex flex-1 items-center gap-1 rounded px-1.5 py-1 text-xs font-semibold text-muted-foreground hover:text-foreground"
        >
          {open ? (
            <ChevronDownIcon className="size-3.5" />
          ) : (
            <ChevronRightIcon className="size-3.5" />
          )}
          {title}
        </button>
        {action}
      </div>
      {open && <div className="flex flex-col">{children}</div>}
    </section>
  );
}

function SubGroup({
  title,
  defaultOpen = false,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="flex flex-col">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1 rounded py-1 pr-1.5 pl-4 text-xs font-medium text-muted-foreground hover:text-foreground"
      >
        {open ? <ChevronDownIcon className="size-3" /> : <ChevronRightIcon className="size-3" />}
        {title}
      </button>
      {open && <div className="flex flex-col">{children}</div>}
    </div>
  );
}

const INDENT_REM = 0.75;

function Item({
  label,
  count,
  active,
  onClick,
  depth = 0,
  leading,
  menu,
  color,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  depth?: number;
  /** ラベルの前に置く要素（ツリーの開閉ボタンなど）。 */
  leading?: ReactNode;
  /** 「…」メニュー（と右クリック）の操作。なければメニューを出さない。 */
  menu?: MenuEntry[];
  /** タグの色（CSS の色）。 */
  color?: string;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  return (
    <div
      className={cn(
        "group/item flex items-center rounded-md pr-1.5 hover:bg-sidebar-accent",
        active && "bg-sidebar-accent font-medium text-sidebar-accent-foreground",
        count === 0 && !active && "opacity-50",
      )}
      style={{ paddingLeft: `${0.5 + depth * INDENT_REM}rem` }}
      onContextMenu={
        menu
          ? (event) => {
              event.preventDefault();
              setMenuOpen(true);
            }
          : undefined
      }
    >
      {leading ?? <span className="w-5 shrink-0" aria-hidden />}
      <button
        type="button"
        aria-pressed={active}
        onClick={onClick}
        className="flex min-w-0 flex-1 items-center justify-between gap-2 py-1 text-left"
      >
        <span className="flex min-w-0 items-center gap-1.5">
          {color !== undefined && (
            <span
              className="size-2 shrink-0 rounded-full"
              style={{ backgroundColor: color }}
              aria-hidden
            />
          )}
          <span className="truncate">{label}</span>
        </span>
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
          {formatInteger(count)}
        </span>
      </button>
      {menu && (
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={ja.sidebar.manage.menuLabel(label)}
              className="ml-0.5 flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground opacity-0 group-hover/item:opacity-100 hover:bg-sidebar-accent hover:text-foreground focus-visible:opacity-100 data-[state=open]:opacity-100"
            >
              <MoreHorizontalIcon className="size-3.5" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {menu.map((entry) => (
              <DropdownMenuItem
                key={entry.label}
                variant={entry.destructive ? "destructive" : "default"}
                onSelect={entry.onSelect}
              >
                {entry.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

/** 開閉できる項目。子は children で渡す。 */
function CollapsibleItem({
  label,
  count,
  active,
  onClick,
  hasChildren,
  depth,
  menu,
  color,
  children,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
  hasChildren: boolean;
  depth: number;
  menu?: MenuEntry[];
  color?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Item
        label={label}
        count={count}
        active={active}
        onClick={onClick}
        depth={depth}
        menu={menu}
        color={color}
        leading={
          hasChildren ? (
            <button
              type="button"
              aria-expanded={open}
              aria-label={open ? ja.sidebar.collapse : ja.sidebar.expand}
              onClick={() => setOpen((v) => !v)}
              className="flex size-5 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
            >
              {open ? (
                <ChevronDownIcon className="size-3.5" />
              ) : (
                <ChevronRightIcon className="size-3.5" />
              )}
            </button>
          ) : undefined
        }
      />
      {open && children}
    </>
  );
}

function LocationItem({
  node,
  depth,
  selectedId,
  onSelect,
  onManage,
}: {
  node: LocationNode;
  depth: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onManage: (dialog: ManageDialog) => void;
}) {
  const text = ja.sidebar.manage.locations;
  const menu: MenuEntry[] = [
    {
      label: text.addChild,
      onSelect: () => onManage({ kind: "createLocation", parentId: node.id }),
    },
    {
      label: text.rename,
      onSelect: () => onManage({ kind: "renameLocation", id: node.id, name: node.name }),
    },
    {
      label: text.move,
      onSelect: () => onManage({ kind: "moveLocation", id: node.id, name: node.name }),
    },
    {
      label: text.delete,
      destructive: true,
      onSelect: () => onManage({ kind: "deleteLocation", id: node.id, name: node.name }),
    },
  ];
  return (
    <CollapsibleItem
      label={node.name}
      count={node.count}
      active={selectedId === node.id}
      onClick={() => onSelect(node.id)}
      hasChildren={node.children.length > 0}
      depth={depth}
      menu={menu}
    >
      {node.children.map((child) => (
        <LocationItem
          key={child.id}
          node={child}
          depth={depth + 1}
          selectedId={selectedId}
          onSelect={onSelect}
          onManage={onManage}
        />
      ))}
    </CollapsibleItem>
  );
}

function TagItem({
  node,
  depth,
  selectedPath,
  onSelect,
  colors,
  onManage,
}: {
  node: TagNode;
  depth: number;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  /** タグ名（パス）→ 色。 */
  colors: ReadonlyMap<string, string | null>;
  onManage: (dialog: ManageDialog) => void;
}) {
  const text = ja.sidebar.manage.tags;
  const color = colors.get(node.path) ?? null;
  const menu: MenuEntry[] = [
    {
      label: text.rename,
      onSelect: () =>
        onManage({ kind: "renameTag", name: node.path, hasChildren: node.children.length > 0 }),
    },
    { label: text.color, onSelect: () => onManage({ kind: "tagColor", name: node.path, color }) },
  ];
  return (
    <CollapsibleItem
      label={node.name}
      count={node.count}
      active={selectedPath === node.path}
      onClick={() => onSelect(node.path)}
      hasChildren={node.children.length > 0}
      depth={depth}
      menu={menu}
      color={tagColorVar(color)}
    >
      {node.children.map((child) => (
        <TagItem
          key={child.path}
          node={child}
          depth={depth + 1}
          selectedPath={selectedPath}
          onSelect={onSelect}
          colors={colors}
          onManage={onManage}
        />
      ))}
    </CollapsibleItem>
  );
}

/** 件数の多い順に上位だけを出す分類（出版社・著者）。選んでいる項目は必ず残す。 */
function RankedGroup({
  title,
  counts,
  selected,
  onSelect,
}: {
  title: string;
  counts: ReadonlyMap<string, number>;
  selected: string | null;
  onSelect: (value: string) => void;
}) {
  const entries = useMemo(() => topEntries(counts, AUTO_LIST_LIMIT, selected), [counts, selected]);
  const total = useMemo(() => [...counts.keys()].filter((key) => key !== UNSET).length, [counts]);
  return (
    <SubGroup title={title}>
      {entries.map((entry) => (
        <Item
          key={entry.key}
          label={entry.key}
          count={entry.count}
          active={selected === entry.key}
          onClick={() => onSelect(entry.key)}
        />
      ))}
      {total > entries.length && (
        <p className="px-2 py-1 pl-7 text-xs text-muted-foreground">
          {ja.sidebar.auto.showingTop(entries.length, total)}
        </p>
      )}
    </SubGroup>
  );
}
