// メイン画面（9.1）。上部バー・サイドバー・一覧・詳細ペイン・ステータスバーの 3 ペイン構成。
// データの取得は useLibraryView（IPC。Tauri の外ではモック）。変更の反映（library-changed）は App が購読する。
//
// 操作のつなぎ:
//   手動登録・編集 … useEditorStore（フォームは features/editor。EditorHost が開く）
//   スキャン登録・設定 … useScreenStore（画面の切り替えは App.tsx）
//   検索欄の ISBN（FR-SCAN-09）・メイン画面でのスキャン（FR-SCAN-10）… 登録済みなら選ぶ、未登録ならスキャン登録へ引き渡す
//   一括変更・再取得・書き出し・ゴミ箱 … useLibraryActions
import { useCallback, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { StatusBar } from "@/components/layout/StatusBar";
import { TopBar } from "@/components/layout/TopBar";
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
import { resolveIsbnSearch } from "@/features/library/isbnSearch";
import { useLibraryView } from "@/features/library/useLibraryView";
import { BookTable, type BulkChange, type ListAction } from "@/features/list/BookTable";
import { BulkEditDialog, type BulkDialogState } from "@/features/list/BulkEditDialog";
import { tagsOfRows } from "@/features/list/bulk";
import { ColumnPicker } from "@/features/list/ColumnPicker";
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
  const books = useBooks();
  const tags = useTags();
  const cCodes = useCCodeTable();
  const price = usePriceSettings();
  const actions = useLibraryActions();
  const inTrash = useViewStore((s) => s.filter.library === "trash");

  const sidebarOpen = useLayoutStore((s) => s.sidebarOpen);
  const detailOpen = useLayoutStore((s) => s.detailOpen);
  const toggleDetail = useLayoutStore((s) => s.toggleDetail);
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

  // 詳細ペインに出す本（focus している行。絞り込みで消えていれば出さない）
  const focusRow = useMemo(
    () => (focus === null ? null : (view.rows.find((row) => row.id === focus) ?? null)),
    [view.rows, focus],
  );
  const detail = useBookDetail(detailOpen && focusRow !== null ? focusRow.id : null);
  const selectedVisibleCount = useMemo(
    () => view.rows.reduce((count, row) => count + (selected.has(row.id) ? 1 : 0), 0),
    [view.rows, selected],
  );

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
      useSelectionStore.setState({
        selected: new Set([result.id]),
        anchor: result.id,
        focus: result.id,
      });
    },
    [books.data],
  );

  useShortcuts(
    modalOpen
      ? {}
      : {
          newBook: () => openNew(),
          scanRegister: () => showScan(),
          focusSearch,
          toggleDetail,
          openSettings: () => showSettings(),
        },
  );

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
          <div className="flex h-8 shrink-0 items-center justify-end border-b px-2">
            <ColumnPicker />
          </div>
          <BookTable
            rows={view.rows}
            cCodes={cCodes}
            locationPaths={view.locationPaths}
            price={price}
            isLoading={view.isLoading}
            onOpenEdit={openEdit}
            onAction={handleAction}
            onBulkChange={handleBulkChange}
            inTrash={inTrash}
          />
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
