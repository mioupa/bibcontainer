// フロントエンドから Rust のコマンドを呼ぶ唯一の入口。
// コンポーネントは `invoke` を直接呼ばず、ここの `ipc` を使う。
// コマンドを足すときは、Rust 側の #[tauri::command] と CommandMap の両方に追記する。
import { invoke } from "@tauri-apps/api/core";
import type { AppError } from "@/bindings/AppError";
import type { AppInfo } from "@/bindings/AppInfo";
import type { ErrorKind } from "@/bindings/ErrorKind";

/**
 * コマンドが失敗したときに投げられるエラー。
 * `kind` で種類を判別し、`message` に Rust 側が用意した原因の説明が入る。
 */
export class IpcError extends Error {
  readonly kind: ErrorKind;

  constructor(payload: AppError) {
    super(payload.message);
    this.name = "IpcError";
    this.kind = payload.kind;
  }
}

function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    typeof value.kind === "string" &&
    "message" in value &&
    typeof value.message === "string"
  );
}

/** invoke が拒否した値を IpcError にそろえる。Rust 側の AppError 以外（未登録のコマンドなど）は internal にする。 */
function toIpcError(reason: unknown): IpcError {
  if (reason instanceof IpcError) return reason;
  if (isAppError(reason)) return new IpcError(reason);
  const message = reason instanceof Error ? reason.message : String(reason);
  return new IpcError({ kind: "internal", message });
}

/** コマンド名ごとの引数と戻り値の型。引数がないコマンドは args を省く。 */
interface CommandMap {
  app_info: { result: AppInfo };
}

type CommandName = keyof CommandMap;
type ArgsOf<C extends CommandName> = CommandMap[C] extends { args: infer A } ? A : never;
type ResultOf<C extends CommandName> = CommandMap[C]["result"];

async function call<C extends CommandName>(
  command: C,
  ...args: [ArgsOf<C>] extends [never] ? [] : [args: ArgsOf<C>]
): Promise<ResultOf<C>> {
  try {
    return await invoke<ResultOf<C>>(command, args[0] as Record<string, unknown> | undefined);
  } catch (reason) {
    throw toIpcError(reason);
  }
}

export const ipc = {
  /** アプリ名・バージョン・DB の状態を返す（動作確認と診断用）。 */
  appInfo: () => call("app_info"),
} as const;
