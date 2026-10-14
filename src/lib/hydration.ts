// 設定のストア（theme・table・layout）が、保存済みの値を読み込み終えるのを待つ。
// Tauri の中では設定の読み込みが非同期（IPC）なので、最初の描画の前にテーマを合わせたいときに使う（src/main.tsx）。
import { useLayoutStore } from "@/stores/layoutStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTableStore } from "@/stores/tableStore";

interface Persisted {
  persist: {
    hasHydrated: () => boolean;
    onFinishHydration: (listener: () => void) => () => void;
  };
}

function whenHydrated(store: Persisted): Promise<void> {
  if (store.persist.hasHydrated()) return Promise.resolve();
  return new Promise((resolve) => {
    const stop = store.persist.onFinishHydration(() => {
      stop();
      resolve();
    });
  });
}

/**
 * 設定のストアがすべて読み込み終えたら解決する。読み込みに失敗したとき（DB を開けないなど）は完了の通知が
 * 来ないので、`timeoutMs` で打ち切る。打ち切ったときは、既定の設定のまま進む。
 */
export async function whenSettingsHydrated(timeoutMs = 1000): Promise<void> {
  const hydrated = Promise.all(
    [useSettingsStore, useTableStore, useLayoutStore].map(whenHydrated),
  ).then(() => undefined);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, timeoutMs);
  });
  await Promise.race([hydrated, timeout]);
  clearTimeout(timer);
}
