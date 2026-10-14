// 操作の失敗を、利用者に見せる文言にする。
import { ja } from "@/i18n/ja";
import { IpcError } from "@/lib/ipc";

/** 失敗の理由。Rust が日本語で用意した原因（IpcError の message）があればそれを、なければ汎用の文言。 */
export function errorMessage(error: unknown): string {
  if (error instanceof IpcError && error.message.trim() !== "") return error.message;
  return ja.toast.failed;
}
