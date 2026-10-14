// 欲しい本に登録済みの ISBN を読んだときの確認「所有に移しますか？」（FR-SCAN-18）。
// 答えを待っている処理（flow.ts の confirmMoveToOwned）が、ストアの prompt に入れた確認を出す。
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";

export function WantedDialog() {
  const prompt = useScanSessionStore((s) => s.prompt);
  const d = ja.scan.wantedDialog;
  return (
    <Dialog
      open={prompt !== null}
      // ×ボタン・Esc・外側のクリックは「移さない」
      onOpenChange={(open) => {
        if (!open) prompt?.resolve(false);
      }}
    >
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{d.title}</DialogTitle>
          <DialogDescription>{prompt ? d.description(prompt.match.title) : ""}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => prompt?.resolve(false)}>
            {d.cancel}
          </Button>
          <Button onClick={() => prompt?.resolve(true)}>{d.confirm}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
