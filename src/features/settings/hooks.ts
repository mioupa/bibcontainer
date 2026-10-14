// 設定画面のデータの取得と保存（TanStack Query）。取得も保存も IPC なので、Tauri の外（ブラウザ）では動かさない。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { BackupSettings } from "@/bindings/BackupSettings";
import type { PriceSettings } from "@/bindings/PriceSettings";
import type { ProviderSettings } from "@/bindings/ProviderSettings";
import { libraryKeys } from "@/features/library/hooks";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";

export const settingsKeys = {
  appInfo: ["app_info"] as const,
  /** メイン画面が読む価格の設定（features/library/hooks.ts の usePriceSettings）と同じキー。保存すると、そちらも更新される。 */
  price: libraryKeys.priceSettings,
  backup: ["backup_settings"] as const,
  providerSettings: ["provider_settings"] as const,
  providers: ["providers"] as const,
  backups: ["backups"] as const,
};

/** 設定 1 セクションの読み込みと保存。保存した値（Rust が範囲に収めたもの）で、手元の値を置き換える。 */
function useSection<T>(
  key: readonly unknown[],
  load: () => Promise<T>,
  save: (value: T) => Promise<T>,
  onSaved?: () => void,
) {
  const client = useQueryClient();
  const query = useQuery({ queryKey: key, queryFn: load, enabled: inTauri(), staleTime: Infinity });
  const mutation = useMutation({
    mutationFn: save,
    onSuccess: (saved) => {
      client.setQueryData(key, saved);
      onSaved?.();
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  return {
    value: query.data,
    isLoading: query.isLoading,
    isError: query.isError,
    error: query.error,
    save: mutation.mutate,
    saveAsync: mutation.mutateAsync,
    saving: mutation.isPending,
  };
}

/** 価格表示の設定の読み込みと保存（FR-CONF-05）。メイン画面の一覧・詳細ペインは、同じキーの設定を読んで税込価格を併記する。 */
export function useEditablePriceSettings() {
  return useSection<PriceSettings>(settingsKeys.price, ipc.getPriceSettings, ipc.setPriceSettings);
}

export function useBackupSettings() {
  return useSection<BackupSettings>(
    settingsKeys.backup,
    ipc.getBackupSettings,
    ipc.setBackupSettings,
  );
}

/** 取得元の設定。保存したら、取得元の一覧（実際に使うか、など）も取り直す。 */
export function useProviderSettings() {
  const client = useQueryClient();
  return useSection<ProviderSettings>(
    settingsKeys.providerSettings,
    ipc.getProviderSettings,
    ipc.setProviderSettings,
    () => void client.invalidateQueries({ queryKey: settingsKeys.providers }),
  );
}
