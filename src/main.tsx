import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/App";
import { whenSettingsHydrated } from "@/lib/hydration";
import { inTauri } from "@/lib/runtime";
import { applyDisplay, applyTheme } from "@/lib/theme";
import { useSettingsStore } from "@/stores/settingsStore";
import "@/index.css";

// バックエンドはローカルのプロセスなので、再試行やフォーカス時の再取得は既定では行わない。
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false },
  },
});

const root = document.getElementById("root");
if (!root) {
  throw new Error("#root が見つかりません");
}

async function start(container: HTMLElement) {
  // Tauri の中では、設定を Rust から読み込み終えてから描く（ライトの画面が一瞬見えるのを避ける）。
  if (inTauri()) await whenSettingsHydrated();
  const { theme, fontFamily, fontSize } = useSettingsStore.getState();
  applyTheme(theme);
  applyDisplay({ fontFamily, fontSize });

  createRoot(container).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void start(root);
