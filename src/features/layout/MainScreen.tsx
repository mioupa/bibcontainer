// メイン画面（9.1）。上部バー・サイドバー・一覧・詳細ペイン・ステータスバーの 3 ペイン構成。
// データの取得は useLibraryView（IPC。Tauri の外ではモック）。変更の反映（library-changed）は App が購読する。
//
// 操作のつなぎ:
//   手動登録・編集 … useEditorStore（フォームは features/editor。EditorHost が開く）
//   スキャン登録・設定 … useScreenStore（画面の切り替えは App.tsx。⌘N・⇧⌘N・⌘, のショートカットも App が受ける）
//   巻をまとめた表示（FR-LIST-08）… useGroupedList。選択はグループの key も含み、操作の対象にするときに本の ID にする
//   検索欄の ISBN（FR-SCAN-09）・メイン画面でのスキャン（FR-SCAN-10）… 登録済みなら選ぶ、未登録ならスキャン登録へ引き渡す
//   一括変更・再取得・書き出し（選んだ本・表示中の絞り込み結果）・ゴミ箱 … useLibraryActions
import { DownloadIcon } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { StatusBar } from "@/components/layout/StatusBar";
import { TopBar } from "@/components/layout/TopBar";
import { Button } from "@/components/ui/button";
import { DetailPane } from "@/features/detail/DetailPane";
import { RefetchDialog } from "@/features/detail/RefetchDialog";
import { IsbnRegisterDialog } from "@/features/layout/IsbnRegisterDialog";
import { PurgeConfirmDialog } from "@/features/layout/PurgeConfirmDialog";
import { usingMockData } from "@/features/library/api";
import { useLibraryActions } from "@/features/library/actions";
import {
  useBookDetail,
  useBooks,
  useCCodeTable,
  usePriceSettings,
  useTags,
} from "@/features/library/hooks";
import { countBooks } from "@/features/library/grouping";
import { resolveIsbnSearch } from "@/features/library/isbnSearch";
import { useGroupedList } from "@/features/library/useGroupedList";
import { useLibraryView } from "@/features/library/useLibraryView";
import { BookGrid } from "@/features/list/BookGrid";
import { BookTable } from "@/features/list/BookTable";
import { BulkEditDialog, type BulkDialogState } from "@/features/list/BulkEditDialog";
import { tagsOfRows } from "@/features/list/bulk";
import { ColumnPicker } from "@/features/list/ColumnPicker";
import { GroupToggle } from "@/features/list/GroupToggle";
import type { BulkChange, ListAction, ListViewProps } from "@/features/list/listActions";
import { SortMenu } from "@/features/list/SortMenu";
import { ViewSwitch } from "@/features/list/ViewSwitch";
import { useScanCapture } from "@/features/scan/useScanCapture";
import { useShortcuts } from "@/features/shortcuts/useShortcuts";
import { Sidebar } from "@/features/sidebar/Sidebar";
import { ja } from "@/i18n/ja";
import { useAnyModalOpen } from "@/lib/useModalGuard";
import { useEditorStore } from "@/stores/editorStore";
import { useLayoutStore } from "@/stores/layoutStore";
import { useScreenStore } from "@/stores/screenStore";
import { useSelectionStore } from "@/stores/selectionStore";
import { useViewStore } from "@/stores/viewStore";

export function MainScreen() {
  const view = useLibraryView();
  // 巻をまとめた並び（FR-LIST-08）。表・グリッドに渡す。件数・書き出しは本の数なので、`view.rows` のまま
  const grouped = useGroupedList(view.rows);
  const books = useBooks();
  const tags = useTags();
  const cCodes = useCCodeTable();
  const price = usePriceSettings();
  const actions = useLibraryActions();
  const inTrash = useViewStore((s) => s.filter.library === "trash");

  const sidebarOpen = useLayoutStore((s) => s.sidebarOpen);
  const detailOpen = useLayoutStore((s) => s.detailOpen);
  const toggleDetail = useLayoutStore((s) => s.toggleDetail);
  const setDetailOpen = useLayoutStore((s) => s.setDetailOpen);
  const viewMode = useLayoutStore((s) => s.viewMode);
  const selected = useSelectionStore((s) => s.selected);
  const focus = useSelectionStore((s) => s.focus);

  const showScan = useScreenStore((s) => s.showScan);
  const showSettings = useScreenStore((s) => s.showSettings);
  const openNew = useEditorStore((s) => s.openNew);
  const openEdit = useEditorStore((s) => s.openEdit);

  // フォームやダイアログを開いている間は、画面全体のショートカットとスキャンの検出を止める（9.5）
  const modalOpen = useAnyModalOpen();

  // 完全に削除する前の確認（対象の本の ID。null なら閉じている）
  const [purgeTarget, setPurgeTarget] = useState<string[] | null>(null);
  // 書誌の再取得の確認（対象の本の ID）
  const [refetchTarget, setRefetchTarget] = useState<string[] | null>(null);
  // 保管場所・タグの一括変更のダイアログ
  const [bulkDialog, setBulkDialog] = useState<BulkDialogState>(null);
  // 検索欄で確定した、未登録の ISBN
  const [unregisteredIsbn, setUnregisteredIsbn] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // 詳細ペインに出す本（focus している行。絞り込みで消えていれば出さない。focus がグループの行なら、本ではないので出さない）
  const focusRow = focus === null ? null : (grouped.rowById.get(focus) ?? null);
  const detail = useBookDetail(detailOpen && focusRow !== null ? focusRow.id : null);
  // 選んでいる冊数（グループを選んでいれば、その全巻。重なりは数えない）
  const selectedVisibleCount = useMemo(() => countBooks(selected, grouped), [selected, grouped]);

  const handleAction = useCallback(
    (action: Exclude<ListAction, "edit">, ids: string[]) => {
      switch (action) {
        case "trash":
          void actions.trash(ids);
          break;
        case "restore":
          void actions.restore(ids);
          break;
        case "purge":
          setPurgeTarget(ids);
          break;
        case "export":
          void actions.exportCsv(ids);
          break;
        case "refetch":
          setRefetchTarget(ids);
          break;
        case "location":
        case "addTags":
        case "removeTags":
          setBulkDialog({ kind: action, ids });
          break;
        case "markPurchased":
          void actions.markPurchased(ids);
          break;
      }
    },
    [actions],
  );

  const handleBulkChange = useCallback(
    (change: BulkChange, ids: string[]) => void actions.bulk(change, ids),
    [actions],
  );

  const focusSearch = useCallback(() => {
    searchRef.current?.focus();
    searchRef.current?.select();
  }, []);

  // 検索欄で ISBN を確定したとき（FR-SCAN-09）: 登録済みならその本を選び、未登録なら登録を確かめる
  const handleSearchSubmit = useCallback(
    (text: string) => {
      const result = resolveIsbnSearch(books.data ?? [], text);
      switch (result.kind) {
        case "none":
          return;
        case "unregistered":
          setUnregisteredIsbn(result.isbn13);
          return;
        case "trashed":
          toast.info(ja.list.isbnSearch.foundInTrash(result.title));
          useViewStore.getState().clearFilters();
          useViewStore.getState().setLibrary("trash");
          break;
        case "found":
          useViewStore.getState().clearFilters();
          break;
      }
      // 検索語は消し、全冊（ゴミ箱の本ならゴミ箱）から選ぶ。検索結果が届くのを待たずに選べる
      useViewStore.getState().setSearchText("");
      useSelectionStore.getState().reveal(result.id);
      // 選んだ本の詳細が見えるよう、詳細ペインが閉じていれば開く（FR-SCAN-09）
      setDetailOpen(true);
    },
    [books.data, setDetailOpen],
  );

  // 表示中の絞り込み結果（今、一覧に出ている本すべて）を CSV に書き出す（FR-IO-01）
  // （巻をまとめていても、グループの全巻を、表示の順に書き出す）
  const shownCount = view.rows.length;
  const exportShown = useCallback(() => {
    void actions.exportCsv(grouped.bookIds);
  }, [actions, grouped.bookIds]);

  // ⌘F（検索欄）と ⌘I（詳細ペイン）は、メイン画面だけで効く。⌘N・⇧⌘N・⌘, は、どの画面でも効くので App が受ける
  // （features/shortcuts/useGlobalShortcuts.ts）
  useShortcuts(modalOpen ? {} : { focusSearch, toggleDetail });

  // メイン画面でバーコードリーダーの入力を拾い、スキャン登録に引き渡す（FR-SCAN-10）
  useScanCapture({
    mode: "mainScreen",
    enabled: !modalOpen,
    onScan: (digits) => showScan(digits),
  });

  const removableTags = useMemo(() => {
    if (bulkDialog?.kind !== "removeTags") return [];
    const wanted = new Set(bulkDialog.ids);
    return tagsOfRows((books.data ?? []).filter((row) => wanted.has(row.id)));
  }, [bulkDialog, books.data]);
  const tagNames = useMemo(() => (tags.data ?? []).map((tag) => tag.name), [tags.data]);

  // 表とグリッドは、同じ props を受け取る
  const listProps: ListViewProps = {
    rows: view.rows,
    grouped,
    cCodes,
    locationPaths: view.locationPaths,
    price,
    isLoading: view.isLoading,
    onOpenEdit: openEdit,
    onAction: handleAction,
    onBulkChange: handleBulkChange,
    inTrash,
  };

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <TopBar
        searchRef={searchRef}
        onNewBook={() => openNew()}
        onScanRegister={() => showScan()}
        onOpenSettings={() => showSettings()}
        onSearchSubmit={handleSearchSubmit}
      />

      <div className="flex min-h-0 flex-1">
        {sidebarOpen && (
          <aside className="w-64 shrink-0 border-r bg-sidebar text-sidebar-foreground">
            <Sidebar
              facets={view.facets}
              locations={view.locations}
              allTagPaths={view.allTagPaths}
              cCodes={cCodes}
              tags={tags.data}
            />
          </aside>
        )}

        <main className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-8 shrink-0 items-center gap-1 border-b px-2">
            <ViewSwitch />
            <GroupToggle />
            <div className="ml-auto flex items-center gap-1">
              <Button variant="ghost" size="xs" disabled={shownCount === 0} onClick={exportShown}>
                <DownloadIcon />
                {ja.list.exportShown.label(shownCount)}
              </Button>
              {/* 表は列の見出しで並べ替えるので、列のメニュー。グリッドには見出しがないので、並べ替えのメニュー */}
              {viewMode === "grid" ? <SortMenu /> : <ColumnPicker />}
            </div>
          </div>
          {/* 選択はどちらの形でも同じ（selectionStore）。形を切り替えると、選んでいる本までスクロールする */}
          {viewMode === "grid" ? <BookGrid {...listProps} /> : <BookTable {...listProps} />}
        </main>

        {detailOpen && (
          <aside className="w-80 shrink-0 border-l bg-card xl:w-96">
            <DetailPane
              row={focusRow}
              detail={detail.data ?? null}
              selectedCount={selectedVisibleCount}
              cCodes={cCodes}
              locationPaths={view.locationPaths}
              price={price}
              onEdit={openEdit}
              onRefetch={(id) => setRefetchTarget([id])}
              onMarkPurchased={(id) => void actions.markPurchased([id])}
            />
          </aside>
        )}
      </div>

      <StatusBar
        total={view.facets.total}
        shown={view.facets.visible}
        selectedCount={selectedVisibleCount}
        mock={usingMockData()}
      />

      <PurgeConfirmDialog
        count={purgeTarget?.length ?? 0}
        open={purgeTarget !== null}
        onCancel={() => setPurgeTarget(null)}
        onConfirm={() => {
          const ids = purgeTarget;
          setPurgeTarget(null);
          if (ids !== null) void actions.purge(ids);
        }}
      />
      <RefetchDialog
        ids={refetchTarget}
        onCancel={() => setRefetchTarget(null)}
        onConfirm={(ids, overrideManual) => {
          setRefetchTarget(null);
          void actions.refetch(ids, overrideManual);
        }}
      />
      <BulkEditDialog
        state={bulkDialog}
        locations={view.locations}
        tagSuggestions={tagNames}
        currentTags={removableTags}
        onCancel={() => setBulkDialog(null)}
        onApply={(change, ids) => {
          setBulkDialog(null);
          void actions.bulk(change, ids);
        }}
      />
      <IsbnRegisterDialog
        isbn13={unregisteredIsbn}
        onCancel={() => setUnregisteredIsbn(null)}
        onConfirm={(isbn13) => {
          setUnregisteredIsbn(null);
          useViewStore.getState().setSearchText("");
          showScan(isbn13);
        }}
      />
    </div>
  );
}
