// 検索欄で確定した ISBN が未登録のときの確認（FR-SCAN-09）。「登録する」でスキャン登録の画面に ISBN を引き渡す。
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

interface IsbnRegisterDialogProps {
  /** 未登録の ISBN-13。null なら閉じている。 */
  isbn13: string | null;
  onCancel: () => void;
  onConfirm: (isbn13: string) => void;
}

export function IsbnRegisterDialog({ isbn13, onCancel, onConfirm }: IsbnRegisterDialogProps) {
  useModalGuard(isbn13 !== null);
  return (
    <Dialog open={isbn13 !== null} onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{ja.list.isbnSearch.registerTitle}</DialogTitle>
          <DialogDescription>
            {isbn13 !== null && ja.list.isbnSearch.registerDescription(isbn13)}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            {ja.list.isbnSearch.cancel}
          </Button>
          <Button autoFocus onClick={() => isbn13 !== null && onConfirm(isbn13)}>
            {ja.list.isbnSearch.register}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
