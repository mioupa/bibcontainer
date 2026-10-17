// 「巻をまとめる」の切替ボタン（FR-LIST-08）。表・グリッドのどちらでも効く。オンのとき、同じ作品の巻を 1 つの行（カード）にまとめる。
// 設定は端末ごとに保存する（layoutStore の groupVolumes）。まとめ方は features/library/grouping.ts。
import { LayersIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ja } from "@/i18n/ja";
import { useLayoutStore } from "@/stores/layoutStore";

export function GroupToggle() {
  const groupVolumes = useLayoutStore((s) => s.groupVolumes);
  const setGroupVolumes = useLayoutStore((s) => s.setGroupVolumes);

  return (
    <Button
      variant={groupVolumes ? "secondary" : "ghost"}
      size="xs"
      aria-pressed={groupVolumes}
      title={ja.list.group.toggleHint}
      onClick={() => setGroupVolumes(!groupVolumes)}
    >
      <LayersIcon />
      {ja.list.group.toggle}
    </Button>
  );
}
