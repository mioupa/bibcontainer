// スキャン登録画面（9.3）。上部に読み取りの状態、直前に読んだ本と結果、「1 冊ずつ」と「連続」の切り替え、
// 連続のときは共通値と、溜まった本の一覧。読み取りの流れは useScanFlow、確定した本の処理は flow.ts。
import { ArrowLeftIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { LastReadCard } from "@/features/scanScreen/LastReadCard";
import { ManualInput } from "@/features/scanScreen/ManualInput";
import { useManualFocusHome } from "@/features/scanScreen/manualFocus";
import { ModeToggle } from "@/features/scanScreen/ModeToggle";
import { BatchPanel } from "@/features/scanScreen/BatchPanel";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { StatePanel } from "@/features/scanScreen/StatePanel";
import { useEditingFocus } from "@/features/scanScreen/useEditingFocus";
import { useScanFlow } from "@/features/scanScreen/useScanFlow";
import { WantedDialog } from "@/features/scanScreen/WantedDialog";
import { ja } from "@/i18n/ja";
import { inTauri } from "@/lib/runtime";
import { useScreenStore } from "@/stores/screenStore";

export function ScanScreen() {
  const flow = useScanFlow();
  const editing = useEditingFocus();
  useManualFocusHome();
  const mode = useScanSessionStore((s) => s.mode);
  const setMode = useScanSessionStore((s) => s.setMode);
  const batchCount = useScanSessionStore((s) => s.rows.length);
  const showMain = useScreenStore((s) => s.showMain);

  return (
    <div className="flex h-full flex-col bg-background text-foreground" data-testid="scan-screen">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b bg-card px-3">
        <Button variant="ghost" size="sm" onClick={showMain}>
          <ArrowLeftIcon />
          {ja.scan.screen.back}
        </Button>
        <h1 className="text-base font-semibold">{ja.scan.screen.title}</h1>
        <div className="ml-auto">
          <ModeToggle mode={mode} onChange={setMode} batchCount={batchCount} />
        </div>
      </header>

      {!inTauri() && (
        <p className="shrink-0 bg-warning/15 px-4 py-2 text-sm">{ja.scan.screen.browserNotice}</p>
      )}

      <StatePanel
        state={flow.pairing.state}
        remainingSec={flow.pairing.remainingSec}
        listening={flow.listening}
        editing={editing}
        onCommit={flow.pairing.commit}
      />

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <div className="mx-auto flex max-w-5xl flex-col gap-4">
          <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_minmax(0,22rem)]">
            <LastReadCard onRegisterSecond={flow.registerSecondCopy} />
            <div className="flex flex-col gap-4">
              <ManualInput
                onSubmit={flow.submitDigits}
                // フォームやダイアログを開いている間は、確定キーと同じく止める
                onCommit={() => {
                  if (flow.listening) flow.pairing.commit();
                }}
              />
            </div>
          </div>
          {mode === "batch" && <BatchPanel onImport={() => void flow.importIsbnList()} />}
        </div>
      </div>

      <WantedDialog />
    </div>
  );
}
