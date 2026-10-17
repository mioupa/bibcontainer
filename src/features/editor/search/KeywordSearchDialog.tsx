// 書名・著者のキーワード検索ダイアログ（登録フォームの書名の横の「探す」。FR-META-12）。
//
// 取得元（NDL サーチ・Google Books・Open Library のうち、有効なもの）ごとに別々に検索して、答えが届いた順に
// 見出しごとの結果を埋める。結果は、取得元ごとにまとめて出し、並べ替えも絞り込みもしない（Google Books の規約）。
// 選べるのは、全取得元の結果を通して 1 行（上から下の表示順）。選んだ 1 件は `onChoose` で親に渡す
// （書誌の取得とフォームへの反映は、BookEditorForm）。
//
// このダイアログは、フォームの `<form>` の外に置く（React のイベントは、ポータルを越えて親へ伝わるので、
// 中に置くと、入力欄の Enter がフォームの「保存しない」処理に取られる）。
import { SearchIcon } from "lucide-react";
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { toast } from "sonner";
import type { ProviderId } from "@/bindings/ProviderId";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { SearchHit } from "@/bindings/SearchHit";
import poweredByGoogle from "@/assets/google/powered-by-google.png";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  authorsText,
  hitDomId,
  hitKey,
  isFailure,
  movedIndex,
  publicationText,
  searchTargets,
  statusText,
  viewOnLabel,
  type Move,
  type SectionState,
} from "@/features/editor/search/searchModel";
import { isEnterKey, isImeCommitKey } from "@/features/library/useSearchEnter";
import { libraryApi } from "@/features/library/api";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { useModalGuard } from "@/lib/useModalGuard";
import { cn } from "@/lib/utils";

export interface KeywordSearchDialogProps {
  /** 開いたときの書名・著者（フォームの今の値）。どちらかがあれば、開いてすぐ探す。 */
  initialTitle: string;
  initialAuthor: string;
  /** 結果の 1 件を選んだ（行のダブルクリック、「入れる」、リストで Enter）。ダイアログは親が閉じる。 */
  onChoose: (hit: SearchHit) => void;
  onClose: () => void;
}

/** 取得元 1 つ分の見出しと結果。 */
interface Section {
  id: ProviderId;
  displayName: string;
  state: SectionState;
}

/** 結果の 1 行（全取得元を通した表示順）。 */
interface Row {
  key: string;
  hit: SearchHit;
}

/** 日本語入力の変換中・確定の Enter か（検索を始めない）。 */
function isComposingKey(event: KeyboardEvent): boolean {
  return isImeCommitKey({
    key: event.key,
    isComposing: event.nativeEvent.isComposing,
    keyCode: event.nativeEvent.keyCode,
  });
}

export function KeywordSearchDialog({
  initialTitle,
  initialAuthor,
  onChoose,
  onClose,
}: KeywordSearchDialogProps) {
  useModalGuard(true);
  const [initial] = useState({ title: initialTitle, author: initialAuthor });
  const [title, setTitle] = useState(initial.title);
  const [author, setAuthor] = useState(initial.author);
  const [providers, setProviders] = useState<ProviderStatus[] | null>(null);
  const [providersError, setProvidersError] = useState<string | null>(null);
  /** null は、まだ探していない。 */
  const [sections, setSections] = useState<Section[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  /** 探すたびに増やす。前の検索の遅い答えを捨てるため。 */
  const generation = useRef(0);
  const titleRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const authorId = useId();

  const runSearch = useCallback(
    (list: readonly ProviderStatus[], titleText: string, authorText: string) => {
      const titleQuery = titleText.trim();
      const authorQuery = authorText.trim();
      if (titleQuery === "" && authorQuery === "") return;
      const run = ++generation.current;
      const targets = searchTargets(list, titleQuery, authorQuery);
      setSelected(null);
      setSections(
        targets.map((provider) => ({
          id: provider.id,
          displayName: provider.displayName,
          // API キーがなくて使えない取得元は、探さずに理由を出す
          state: provider.active ? { kind: "loading" } : { kind: "noKey" },
        })),
      );
      const settle = (id: ProviderId, state: SectionState) => {
        if (run !== generation.current) return;
        setSections(
          (current) =>
            current?.map((section) => (section.id === id ? { ...section, state } : section)) ??
            null,
        );
      };
      for (const provider of targets) {
        if (!provider.active) continue;
        libraryApi.searchMetadata(provider.id, titleQuery || null, authorQuery || null).then(
          (result) => settle(provider.id, { kind: "result", result }),
          (error: unknown) => settle(provider.id, { kind: "error", message: errorMessage(error) }),
        );
      }
    },
    [],
  );

  // 取得元の一覧を読んで、書名か著者が入っていれば、すぐ探す
  useEffect(() => {
    let cancelled = false;
    libraryApi.listProviders().then(
      (list) => {
        if (cancelled) return;
        setProviders(list);
        runSearch(list, initial.title, initial.author);
      },
      (error: unknown) => {
        if (!cancelled) setProvidersError(errorMessage(error));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [initial, runSearch]);

  const rows = useMemo<Row[]>(
    () =>
      (sections ?? []).flatMap((section) =>
        section.state.kind === "result"
          ? section.state.result.hits.map((hit, index) => ({ key: hitKey(section.id, index), hit }))
          : [],
      ),
    [sections],
  );

  // 選んだ行が見えるように、リストをスクロールする
  useEffect(() => {
    if (selected !== null) {
      document.getElementById(hitDomId(selected))?.scrollIntoView({ block: "nearest" });
    }
  }, [selected]);

  // Google Books を実際に探すか（有効で、検索に対応し、API キーがある）
  const googleSearched =
    providers?.some((p) => p.id === "google" && p.enabled && p.supportsSearch && p.active) ?? false;
  const canSearch = providers !== null && (title.trim() !== "" || author.trim() !== "");
  const search = () => {
    if (providers !== null) runSearch(providers, title, author);
  };

  const moveSelection = (move: Move) => {
    const index = movedIndex(
      rows.findIndex((row) => row.key === selected),
      rows.length,
      move,
    );
    if (index !== null) setSelected(rows[index]?.key ?? null);
  };

  const chooseSelected = () => {
    const row = rows.find((candidate) => candidate.key === selected);
    if (row) onChoose(row.hit);
  };

  const openLink = (url: string) => {
    libraryApi.openLink(url).catch((error: unknown) => toast.error(errorMessage(error)));
  };

  // 入力欄: Enter で探す。↑↓ で結果の選択に移る（そのままリストに移して、Enter で選べるようにする）
  const onInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (isComposingKey(event)) return;
    if (isEnterKey(event)) {
      event.preventDefault();
      search();
    } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && rows.length > 0) {
      event.preventDefault();
      moveSelection(event.key === "ArrowDown" ? "next" : "prev");
      listRef.current?.focus();
    }
  };

  // リスト: ↑↓・Home・End で選択を動かし、リスト自体にフォーカスがあるときの Enter で選ぶ
  const onListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const moves: Record<string, Move> = {
      ArrowDown: "next",
      ArrowUp: "prev",
      Home: "first",
      End: "last",
    };
    const move = moves[event.key];
    if (move !== undefined) {
      event.preventDefault();
      moveSelection(move);
    } else if (isEnterKey(event) && event.target === event.currentTarget) {
      event.preventDefault();
      chooseSelected();
    }
  };

  const text = ja.edit.search;
  let results: ReactNode;
  if (providersError !== null) {
    results = (
      <p role="alert" className="p-3 text-sm text-destructive">
        {providersError}
      </p>
    );
  } else if (providers === null) {
    results = <p className="p-3 text-sm text-muted-foreground">{text.loadingProviders}</p>;
  } else if (sections === null) {
    results = <p className="p-3 text-sm text-muted-foreground">{text.idle}</p>;
  } else if (sections.length === 0) {
    results = <p className="p-3 text-sm text-muted-foreground">{text.noProviders}</p>;
  } else {
    results = sections.map((section) => (
      <SectionView
        key={section.id}
        section={section}
        selected={selected}
        onSelect={setSelected}
        onChoose={onChoose}
        onOpenLink={openLink}
      />
    ));
  }
  const hasSections = providers !== null && sections !== null && sections.length > 0;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        // 外側のクリックでは閉じない（閉じるのは Esc・×・「閉じる」）。Esc は、このダイアログだけを閉じる
        onInteractOutside={(event) => event.preventDefault()}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          titleRef.current?.focus();
        }}
        className="flex h-[min(40rem,92vh)] w-full flex-col gap-3 sm:max-w-3xl"
      >
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>{text.description}</DialogDescription>
        </DialogHeader>

        <div role="search" aria-label={text.searchLabel} className="flex flex-col gap-1.5">
          <div className="grid grid-cols-[minmax(0,3fr)_minmax(0,2fr)_auto] items-end gap-2">
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={titleId}>{text.titleLabel}</Label>
              <Input
                ref={titleRef}
                id={titleId}
                autoComplete="off"
                spellCheck={false}
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                onKeyDown={onInputKeyDown}
              />
            </div>
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={authorId}>{text.authorLabel}</Label>
              <Input
                id={authorId}
                autoComplete="off"
                spellCheck={false}
                value={author}
                onChange={(event) => setAuthor(event.target.value)}
                onKeyDown={onInputKeyDown}
              />
            </div>
            <Button type="button" disabled={!canSearch} onClick={search}>
              <SearchIcon />
              {text.submit}
            </Button>
          </div>
          {googleSearched && (
            // Google Books を探すときは、検索欄のそばにも「Powered by Google」を置く（Google のブランドの規約）
            <PoweredByGoogle className="self-end" />
          )}
        </div>

        <div
          ref={listRef}
          role={hasSections ? "listbox" : undefined}
          aria-label={hasSections ? text.resultsLabel : undefined}
          aria-activedescendant={selected === null ? undefined : hitDomId(selected)}
          tabIndex={hasSections ? 0 : undefined}
          onKeyDown={hasSections ? onListKeyDown : undefined}
          className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto rounded-md border p-2 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          {results}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            {text.close}
          </Button>
          <Button type="button" disabled={selected === null} onClick={chooseSelected}>
            {text.choose}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Google が配る「Powered by Google」の画像。そのまま使う（色・大きさを変えない）。透明な背景に暗い文字なので、ダークテーマでも読めるよう、白の地に置く。 */
function PoweredByGoogle({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 rounded-md bg-white px-1 py-0.5 ring-1 ring-black/10",
        className,
      )}
    >
      <img src={poweredByGoogle} alt={ja.edit.search.poweredByGoogle} width={62} height={30} />
    </span>
  );
}

interface SectionViewProps {
  section: Section;
  selected: string | null;
  onSelect: (key: string) => void;
  onChoose: (hit: SearchHit) => void;
  onOpenLink: (url: string) => void;
}

/** 取得元 1 つ分: 見出し（取得元の名前・状態）と、その取得元が返した順の結果。 */
function SectionView({ section, selected, onSelect, onChoose, onOpenLink }: SectionViewProps) {
  const { id, displayName, state } = section;
  const headingId = `kw-section-${id}`;
  const hits = state.kind === "result" ? state.result.hits : [];
  // Google Books の結果には、公式の「Powered by Google」の画像を添える
  const showGoogleBrand = id === "google" && hits.length > 0;
  return (
    <div role="group" aria-labelledby={headingId} className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1">
        <span id={headingId} className="text-sm font-medium">
          {id === "google" ? ja.edit.search.googleHeading : displayName}
        </span>
        <span
          role="status"
          className={cn("text-xs", isFailure(state) ? "text-destructive" : "text-muted-foreground")}
        >
          {statusText(state)}
        </span>
        {showGoogleBrand && <PoweredByGoogle className="ml-auto" />}
      </div>
      {hits.map((hit, index) => {
        const key = hitKey(id, index);
        return (
          <HitRow
            key={key}
            domId={hitDomId(key)}
            hit={hit}
            selected={selected === key}
            onSelect={() => onSelect(key)}
            onChoose={() => onChoose(hit)}
            onOpenLink={onOpenLink}
          />
        );
      })}
    </div>
  );
}

interface HitRowProps {
  domId: string;
  hit: SearchHit;
  selected: boolean;
  onSelect: () => void;
  onChoose: () => void;
  onOpenLink: (url: string) => void;
}

/** 結果の 1 行。文字だけ（書影は出さない）。クリックで選び、ダブルクリックで入れる。 */
function HitRow({ domId, hit, selected, onSelect, onChoose, onOpenLink }: HitRowProps) {
  const text = ja.edit.search;
  const { candidate } = hit;
  const authors = authorsText(candidate.authors);
  const publication = publicationText(hit);
  const link = hit.link;
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      onDoubleClick={onChoose}
      className={cn(
        "flex cursor-default flex-col gap-0.5 rounded-md border px-3 py-2",
        selected
          ? "border-ring bg-accent text-accent-foreground"
          : "border-transparent hover:bg-muted",
      )}
    >
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span className="font-medium break-words">{candidate.title ?? text.untitled}</span>
        {candidate.subtitle && (
          <span className="text-sm text-muted-foreground">{candidate.subtitle}</span>
        )}
        {candidate.volume && (
          <span className="rounded border px-1 text-xs text-muted-foreground">
            {text.volume(candidate.volume)}
          </span>
        )}
      </div>
      {authors !== "" && <div className="text-sm break-words">{authors}</div>}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
        {publication !== "" && <span>{publication}</span>}
        <span className="font-mono">{hit.isbn13 ?? text.noIsbn}</span>
        {link !== null && (
          // 行を選んだことにしない。キーボードでは、選んでいる行のリンクだけ Tab で届く
          <button
            type="button"
            tabIndex={selected ? 0 : -1}
            className="text-primary underline underline-offset-2 hover:opacity-80"
            onClick={(event) => {
              event.stopPropagation();
              onOpenLink(link);
            }}
            onDoubleClick={(event) => event.stopPropagation()}
          >
            {viewOnLabel(hit.provider)}
          </button>
        )}
      </div>
    </div>
  );
}
