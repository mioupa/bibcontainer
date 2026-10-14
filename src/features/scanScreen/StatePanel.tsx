// 画面上部の、読み取りの状態の表示と確定ボタン（9.3）。
// 「待機中」「ISBN 保留中（残り N 秒）」「2段目 保留中（残り N 秒）」を大きく出す。
import { Button } from "@/components/ui/button";
import type { PairingState } from "@/features/scan/pairing";
import { ja } from "@/i18n/ja";
import { cn } from "@/lib/utils";

interface StatePanelProps {
  state: PairingState;
  /** 残り秒数（切り上げ）。待機中は null。 */
  remainingSec: number | null;
  /** 読み取りを受け付けているか。止めているときは、その旨を出す。 */
  listening: boolean;
  /** 手入力欄以外の入力欄にフォーカスがある（リーダーの数字が、その欄に入ってしまう）。 */
  editing: boolean;
  /** 確定キー（画面上の「確定」ボタン）。 */
  onCommit: () => void;
}

function stateText(state: PairingState, remainingSec: number | null): string {
  switch (state.phase) {
    case "idle":
      return ja.scan.state.idle;
    case "isbnHeld":
      return ja.scan.state.isbnHeld(remainingSec ?? 0);
    case "secondHeld":
      return ja.scan.state.secondHeld(remainingSec ?? 0);
  }
}

const PHASE_STYLE: Record<PairingState["phase"], string> = {
  idle: "border-border bg-muted text-muted-foreground",
  isbnHeld: "border-info bg-info/15 text-foreground",
  secondHeld: "border-warning bg-warning/15 text-foreground",
};

export function StatePanel({ state, remainingSec, listening, editing, onCommit }: StatePanelProps) {
  const idle = state.phase === "idle";
  return (
    <section
      className={cn(
        "flex shrink-0 flex-wrap items-center justify-between gap-4 border-b-2 px-6 py-4",
        PHASE_STYLE[state.phase],
      )}
      data-phase={state.phase}
    >
      <div className="min-w-0">
        <p
          role="status"
          aria-label={ja.scan.screen.stateLabel}
          className="text-3xl leading-tight font-bold tracking-tight"
        >
          {stateText(state, remainingSec)}
        </p>
        {!listening && <p className="mt-1 text-sm">{ja.scan.screen.paused}</p>}
        {listening && editing && <p className="mt-1 text-sm">{ja.scan.screen.editing}</p>}
      </div>
      <div className="flex items-center gap-3">
        <span className="hidden text-sm text-muted-foreground sm:inline">
          {ja.scan.screen.commit.description}
        </span>
        <Button size="lg" onClick={onCommit} disabled={idle} className="px-4">
          {ja.scan.screen.commit.button}
          <kbd className="ml-1 rounded border bg-background/60 px-1.5 py-0.5 font-sans text-xs text-muted-foreground">
            {ja.scan.screen.commit.hint}
          </kbd>
        </Button>
      </div>
    </section>
  );
}
