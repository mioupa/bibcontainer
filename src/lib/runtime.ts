// 実行環境の判定。`pnpm dev` をブラウザで開いたとき（Tauri の IPC がないとき）は、モックのデータで動かす。
import { isTauri } from "@tauri-apps/api/core";

/** Tauri のウィンドウの中で動いているか。false のときは IPC を呼べない。 */
export function inTauri(): boolean {
  return isTauri();
}
