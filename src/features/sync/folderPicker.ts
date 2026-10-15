// 同期フォルダを選ぶ。Tauri の中ではフォルダ選択のダイアログ、外（ブラウザの `pnpm dev`）にはダイアログがないので、
// モックの固定のパスを選んだことにする。キャンセルしたら null。
import { MOCK_FOLDER_PATH } from "@/dev/mockSync";
import { ja } from "@/i18n/ja";
import { chooseDirectory } from "@/lib/dialog";
import { inTauri } from "@/lib/runtime";

export async function pickSyncFolder(): Promise<string | null> {
  if (!inTauri()) return MOCK_FOLDER_PATH;
  return chooseDirectory({ title: ja.sync.setup.dialogTitle });
}
