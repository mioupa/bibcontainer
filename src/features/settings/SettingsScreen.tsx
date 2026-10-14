// 設定画面（9.4、FR-CONF）。左に節の一覧、右に内容。表示する節は useScreenStore の settingsSection で決まり、
// 切り替えは showSettings(section) で行う（メイン画面のショートカット ⌘, や、スキャン登録画面からも同じ）。
import { ArrowLeftIcon } from "lucide-react";
import type { ComponentType } from "react";
import { Button } from "@/components/ui/button";
import { AboutSection } from "@/features/settings/sections/AboutSection";
import { BackupSection } from "@/features/settings/sections/BackupSection";
import { DisplaySection } from "@/features/settings/sections/DisplaySection";
import { PriceSection } from "@/features/settings/sections/PriceSection";
import { ProvidersSection } from "@/features/settings/sections/ProvidersSection";
import { ScanSection } from "@/features/settings/sections/ScanSection";
import { SyncSection } from "@/features/settings/sections/SyncSection";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";
import { SETTINGS_SECTIONS } from "@/features/settings/sectionList";
import { useScreenStore, type SettingsSection } from "@/stores/screenStore";

const SECTION_BODY: Record<SettingsSection, ComponentType> = {
  sync: SyncSection,
  providers: ProvidersSection,
  scan: ScanSection,
  display: DisplaySection,
  price: PriceSection,
  backup: BackupSection,
  about: AboutSection,
};

export function SettingsScreen() {
  const section = useScreenStore((s) => s.settingsSection);
  const showSettings = useScreenStore((s) => s.showSettings);
  const showMain = useScreenStore((s) => s.showMain);
  const Body = SECTION_BODY[section];

  return (
    <div
      className="flex h-full flex-col bg-background text-foreground"
      data-testid="settings-screen"
    >
      <header className="flex h-12 shrink-0 items-center gap-3 border-b bg-card px-3">
        <Button variant="ghost" size="sm" onClick={showMain}>
          <ArrowLeftIcon />
          {ja.settings.back}
        </Button>
        <h1 className="text-base font-semibold">{ja.settings.title}</h1>
        <p className="hidden text-xs text-muted-foreground sm:block">{ja.settings.description}</p>
      </header>

      <div className="flex min-h-0 flex-1">
        <nav
          aria-label={ja.settings.navLabel}
          className="w-52 shrink-0 overflow-y-auto border-r bg-sidebar p-2 text-sidebar-foreground"
        >
          <ul className="flex flex-col gap-0.5">
            {SETTINGS_SECTIONS.map((id) => (
              <li key={id}>
                <button
                  type="button"
                  aria-current={id === section ? "page" : undefined}
                  onClick={() => showSettings(id)}
                  className={cn(
                    "w-full rounded-md px-3 py-2 text-left text-sm transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
                    id === section
                      ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                      : "hover:bg-sidebar-accent/60",
                  )}
                >
                  {ja.settings.sections[id]}
                </button>
              </li>
            ))}
          </ul>
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto p-6">
          <div className="mx-auto flex max-w-3xl flex-col gap-4">
            <h2 className="text-lg font-semibold">{ja.settings.sections[section]}</h2>
            <Body />
          </div>
        </main>
      </div>
    </div>
  );
}
