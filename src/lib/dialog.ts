// ファイルの選択と保存先の指定（tauri-plugin-dialog）の薄いラッパー。
// コンポーネントから `@tauri-apps/plugin-dialog` を直接呼ばず、ここを使う。
// 選んだパスは、そのまま IPC（export_csv・restore_backup など）に渡す。キャンセルしたときは null。
import { open, save } from "@tauri-apps/plugin-dialog";

/** ファイルの種類の絞り込み。`name` は画面に出す名前（i18n から渡す）、`extensions` は拡張子（ドットなし）。 */
export interface FileFilter {
  name: string;
  extensions: string[];
}

interface SaveOptions {
  /** ダイアログの題。 */
  title?: string;
  /** 初めに入れておくファイル名（またはパス）。 */
  defaultPath?: string;
  filters?: FileFilter[];
}

interface OpenOptions {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
}

/** 保存先のファイルを選ぶ。キャンセルしたら null。 */
export async function chooseSavePath(options: SaveOptions = {}): Promise<string | null> {
  return save(options);
}

/** 読み込むファイルを 1 つ選ぶ。キャンセルしたら null。 */
export async function chooseOpenFile(options: OpenOptions = {}): Promise<string | null> {
  const path = await open({ ...options, multiple: false, directory: false });
  return typeof path === "string" ? path : null;
}

/** フォルダを選ぶ（バックアップの保存先など）。キャンセルしたら null。 */
export async function chooseDirectory(
  options: Pick<OpenOptions, "title" | "defaultPath"> = {},
): Promise<string | null> {
  const path = await open({ ...options, multiple: false, directory: true });
  return typeof path === "string" ? path : null;
}
