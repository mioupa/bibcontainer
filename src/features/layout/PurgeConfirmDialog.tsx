// 完全に削除する前の確認。完全削除は取り消せず、他の端末にも伝わるので、必ず確認する。
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ja } from "@/i18n/ja";
import { useModalGuard } from "@/lib/useModalGuard";

interface PurgeConfirmDialogProps {
  /** 削除する冊数。 */
  count: number;
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function PurgeConfirmDialog({ count, open, onCancel, onConfirm }: PurgeConfirmDialogProps) {
  useModalGuard(open);
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{ja.list.purgeConfirm.title}</DialogTitle>
          <DialogDescription>{ja.list.purgeConfirm.description(count)}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            {ja.list.purgeConfirm.cancel}
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            {ja.list.purgeConfirm.confirm}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
