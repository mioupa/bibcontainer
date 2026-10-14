// 書誌の再取得の確認（FR-META-13、AT-16）。手で直した項目も上書きするかを選ぶ。既定は、上書きしない。
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { ja } from "@/i18n/ja";
import { useModalGuard } from "@/lib/useModalGuard";

/** 「手で直した項目も上書きする」の既定。手で直した項目は、再取得で上書きしない（FR-META-07）。 */
export const DEFAULT_OVERRIDE_MANUAL = false;

interface RefetchDialogProps {
  /** 再取得する本の ID。null なら閉じている。 */
  ids: string[] | null;
  onCancel: () => void;
  onConfirm: (ids: string[], overrideManual: boolean) => void;
}

export function RefetchDialog({ ids, onCancel, onConfirm }: RefetchDialogProps) {
  useModalGuard(ids !== null);
  return (
    <Dialog open={ids !== null} onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="max-w-md">
        {ids !== null && <RefetchForm ids={ids} onCancel={onCancel} onConfirm={onConfirm} />}
      </DialogContent>
    </Dialog>
  );
}

/** 開くたびに作り直されるので、チェックは毎回既定（オフ）から始まる。 */
function RefetchForm({
  ids,
  onCancel,
  onConfirm,
}: {
  ids: string[];
  onCancel: () => void;
  onConfirm: RefetchDialogProps["onConfirm"];
}) {
  const [overrideManual, setOverrideManual] = useState(DEFAULT_OVERRIDE_MANUAL);
  return (
    <>
      <DialogHeader>
        <DialogTitle>{ja.list.refetchDialog.title}</DialogTitle>
        <DialogDescription>{ja.list.refetchDialog.description(ids.length)}</DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <Checkbox
            id="refetch-override-manual"
            checked={overrideManual}
            onCheckedChange={(value) => setOverrideManual(value === true)}
          />
          <Label htmlFor="refetch-override-manual">{ja.list.refetchDialog.overrideLabel}</Label>
        </div>
        <p className="text-xs text-muted-foreground">
          {overrideManual ? ja.list.refetchDialog.overrideHint : ja.list.refetchDialog.keepHint}
        </p>
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={onCancel}>
          {ja.list.refetchDialog.cancel}
        </Button>
        <Button onClick={() => onConfirm(ids, overrideManual)}>
          {ja.list.refetchDialog.confirm}
        </Button>
      </DialogFooter>
    </>
  );
}
