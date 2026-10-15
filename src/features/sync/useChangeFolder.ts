// 設定済みの「同期」の節: 同期フォルダの選び直しの操作（0016）。表示は ChangeFolder.tsx。
import { useState } from "react";
import { toast } from "sonner";
import * as api from "@/features/sync/api";
import { pickSyncFolder } from "@/features/sync/folderPicker";
import { useSyncActions } from "@/features/sync/hooks";
import type { FolderInspection } from "@/features/sync/types";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";

type Step =
  | { kind: "idle" }
  | { kind: "inspecting" }
  | { kind: "ready"; path: string; inspection: FolderInspection };

/**
 * 選び直しの操作。ボタン（`ChangeFolderButton`。何か所に置いてもよい）と、結果の表示（`ChangeFolderPanel`。1 か所）で共有する。
 */
export function useChangeFolder() {
  const t = ja.sync.changeFolder;
  const actions = useSyncActions();
  const [step, setStep] = useState<Step>({ kind: "idle" });

  const choose = async () => {
    let path: string | null;
    try {
      path = await pickSyncFolder();
    } catch (error) {
      toast.error(errorMessage(error));
      return;
    }
    if (path === null) return;
    setStep({ kind: "inspecting" });
    try {
      const inspection = await api.inspectSyncFolder(path);
      setStep({ kind: "ready", path, inspection });
    } catch (error) {
      toast.error(`${t.inspectFailed}: ${errorMessage(error)}`);
      setStep({ kind: "idle" });
    }
  };

  const apply = () => {
    if (step.kind !== "ready") return;
    actions.changeFolder.mutate(step.path, { onSuccess: () => setStep({ kind: "idle" }) });
  };

  return {
    step,
    /** フォルダを調べている、または付け替えている間。 */
    busy: step.kind === "inspecting" || actions.changeFolder.isPending,
    applying: actions.changeFolder.isPending,
    choose: () => void choose(),
    apply,
    cancel: () => setStep({ kind: "idle" }),
  };
}

export type ChangeFolderControl = ReturnType<typeof useChangeFolder>;
