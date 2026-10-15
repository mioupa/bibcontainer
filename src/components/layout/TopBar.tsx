// 上部バー（9.1）: 検索欄、スキャン登録・手動登録、同期状態、サイドバーと詳細ペインの開閉。
import {
  PanelLeftIcon,
  PanelRightIcon,
  PlusIcon,
  ScanBarcodeIcon,
  SearchIcon,
  SettingsIcon,
} from "lucide-react";
import type { RefObject } from "react";
import { ShortcutTip } from "@/components/layout/ShortcutTip";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useSearchEnter } from "@/features/library/useSearchEnter";
import { SyncIndicator } from "@/features/sync/SyncIndicator";
import { ja } from "@/i18n/ja";
import { useLayoutStore } from "@/stores/layoutStore";
import { useViewStore } from "@/stores/viewStore";

interface TopBarProps {
  searchRef: RefObject<HTMLInputElement | null>;
  onNewBook: () => void;
  onScanRegister: () => void;
  onOpenSettings: () => void;
  /**
   * 検索欄で Enter を押したとき（ISBN の確定。FR-SCAN-09）。日本語入力の変換を確定する Enter は、
   * 確定した値が ISBN の形のときだけ呼ぶ（日本語入力がオンのままスキャンしても、Enter 1 回で確定する。useSearchEnter）。
   */
  onSearchSubmit?: (text: string) => void;
}

export function TopBar({
  searchRef,
  onNewBook,
  onScanRegister,
  onOpenSettings,
  onSearchSubmit,
}: TopBarProps) {
  const searchText = useViewStore((s) => s.searchText);
  const setSearchText = useViewStore((s) => s.setSearchText);
  const sidebarOpen = useLayoutStore((s) => s.sidebarOpen);
  const detailOpen = useLayoutStore((s) => s.detailOpen);
  const toggleSidebar = useLayoutStore((s) => s.toggleSidebar);
  const toggleDetail = useLayoutStore((s) => s.toggleDetail);
  const searchEnter = useSearchEnter(searchRef, onSearchSubmit);

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b bg-card px-2">
      <ShortcutTip label={ja.topBar.toggleSidebar}>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={ja.topBar.toggleSidebar}
          aria-pressed={sidebarOpen}
          onClick={toggleSidebar}
        >
          <PanelLeftIcon />
        </Button>
      </ShortcutTip>

      <div className="relative max-w-xl min-w-40 flex-1">
        <SearchIcon
          className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden
        />
        <Input
          ref={searchRef}
          type="search"
          value={searchText}
          onChange={(event) => setSearchText(event.target.value)}
          onKeyDown={searchEnter.onKeyDown}
          onCompositionStart={searchEnter.onCompositionStart}
          onCompositionEnd={searchEnter.onCompositionEnd}
          aria-label={ja.topBar.searchLabel}
          placeholder={ja.topBar.searchPlaceholder}
          className="h-8 pl-8"
        />
      </div>

      <ShortcutTip label={ja.topBar.scanRegister} shortcut="scanRegister">
        <Button variant="outline" size="sm" onClick={onScanRegister}>
          <ScanBarcodeIcon />
          {ja.topBar.scanRegister}
        </Button>
      </ShortcutTip>
      <ShortcutTip label={ja.topBar.newBook} shortcut="newBook">
        <Button size="sm" onClick={onNewBook}>
          <PlusIcon />
          {ja.topBar.newBook}
        </Button>
      </ShortcutTip>

      <div className="ml-auto flex items-center gap-1">
        {/* 同期の状態（9.1）。押すと設定の同期の節を開く */}
        <SyncIndicator />
        <ShortcutTip label={ja.topBar.settings} shortcut="openSettings">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={ja.topBar.settings}
            onClick={onOpenSettings}
          >
            <SettingsIcon />
          </Button>
        </ShortcutTip>
        <ShortcutTip label={ja.topBar.toggleDetail} shortcut="toggleDetail">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={ja.topBar.toggleDetail}
            aria-pressed={detailOpen}
            onClick={toggleDetail}
          >
            <PanelRightIcon />
          </Button>
        </ShortcutTip>
      </div>
    </header>
  );
}
