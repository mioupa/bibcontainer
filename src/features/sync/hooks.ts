// 同期の状態の取得・購読と、同期の操作（TanStack Query）。呼び出しは api.ts（いまはモック）の裏に隠れている。
//
// 状態は TanStack Query のキャッシュ（`syncKeys.status`）に置く。`sync-status` イベント（状態が変わるたびに届く）と、
// 操作の戻り値（状態）の両方で更新する。本が取り込みで変わったときの一覧の更新は、既存の `library-changed` が行うので、
// ここでは扱わない。
import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { toast } from "sonner";
import { reloadLibrary } from "@/features/library/sync";
import * as api from "@/features/sync/api";
import type { SetupMode, SyncStatus } from "@/features/sync/types";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { formatInteger } from "@/lib/format";

export const syncKeys = {
  status: ["sync_status"] as const,
};

// 取得の途中で `sync-status` が届いたとき、あとから戻ってきた古い取得結果で上書きしないための印
let eventsSeen = 0;

/** 状態をキャッシュに入れる。 */
function putStatus(client: QueryClient, status: SyncStatus): void {
  eventsSeen += 1;
  client.setQueryData<SyncStatus>(syncKeys.status, status);
}

/** 同期の状態。未取得の間は undefined。 */
export function useSyncStatus() {
  return useQuery({
    queryKey: syncKeys.status,
    queryFn: async ({ client }) => {
      const seen = eventsSeen;
      const status = await api.getSyncStatus();
      // 取得している間に sync-status が届いていたら、そちらが新しい
      return eventsSeen === seen
        ? status
        : (client.getQueryData<SyncStatus>(syncKeys.status) ?? status);
    },
    staleTime: Infinity,
  });
}

/**
 * `sync-status` を購読して、状態のキャッシュを更新し続ける。画面の最上位（AppShell）で 1 回だけ呼ぶ。
 * 状態の取得そのものは、表示する側（上部バー、設定の節）の `useSyncStatus` が行う。
 */
export function useSyncStatusSubscription(): void {
  const client = useQueryClient();
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    api
      .onSyncStatus((status) => putStatus(client, status))
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch((error: unknown) => {
        console.error("sync-status を購読できませんでした", error);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [client]);
}

/** 設定の節が使う操作。成功したら状態を更新し、失敗したら理由を通知する。 */
export function useSyncActions() {
  const client = useQueryClient();
  const onError = (error: unknown) => toast.error(errorMessage(error));

  const setup = useMutation({
    mutationFn: ({ path, mode }: { path: string; mode: SetupMode }) => api.setupSync(path, mode),
    onSuccess: async (status, { mode }) => {
      putStatus(client, status);
      // 置き換えは、DB を作り直す。一覧などを全部取り直す（復元と同じ。0009 §3）
      if (mode === "joinReplace") await reloadLibrary(client);
      toast.success(ja.sync.setup.done);
    },
    onError,
  });

  const leave = useMutation({
    mutationFn: api.leaveSync,
    onSuccess: (status) => {
      putStatus(client, status);
      toast.success(ja.sync.leaveConfirm.done);
    },
    onError,
  });

  const syncNow = useMutation({
    mutationFn: api.syncNow,
    onSuccess: (status) => {
      putStatus(client, status);
      toast.success(ja.sync.actions.syncNowDone);
    },
    onError,
  });

  const rename = useMutation({
    mutationFn: (name: string) => api.setDeviceName(name),
    onSuccess: (status) => {
      putStatus(client, status);
      toast.success(ja.sync.deviceName.saved);
    },
    onError,
  });

  const gcCovers = useMutation({
    mutationFn: api.gcCovers,
    onSuccess: (removed) => toast.success(ja.sync.covers.result(formatInteger(removed))),
    onError,
  });

  return { setup, leave, syncNow, rename, gcCovers };
}
