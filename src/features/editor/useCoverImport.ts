// 書影の取り込み操作（ファイル選択・ドロップ・貼り付け・URL・候補）をまとめたフック。
// 取り込んだ書影のハッシュを `onImported` に渡す（本への設定は、フォームの保存が行う）。
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  COVER_EXTENSIONS,
  firstImagePath,
  imageBytesFromClipboardItems,
  imageBytesFromFiles,
  normalizeImageUrl,
} from "@/features/editor/coverImport";
import { libraryApi } from "@/features/library/api";
import { ja } from "@/i18n/ja";
import { chooseOpenFile } from "@/lib/dialog";
import { errorMessage } from "@/lib/errors";
import { inTauri } from "@/lib/runtime";

export interface CoverImporter {
  busy: boolean;
  /** ファイル選択ダイアログから。 */
  chooseFile: () => Promise<void>;
  /** ドロップされたファイルのパス（Tauri のウィンドウのドロップ）から。 */
  fromPaths: (paths: readonly string[]) => Promise<void>;
  /** HTML のドロップ・貼り付けの `File` から。 */
  fromFiles: (files: ArrayLike<File> | null | undefined) => Promise<boolean>;
  /** クリップボードの画像から。 */
  fromClipboard: () => Promise<void>;
  fromUrl: (url: string) => Promise<boolean>;
}

export function useCoverImport(onImported: (hash: string) => void): CoverImporter {
  const [busy, setBusy] = useState(false);
  const onImportedRef = useRef(onImported);
  useEffect(() => {
    onImportedRef.current = onImported;
  });

  /** 取り込み 1 回分。失敗したら理由を通知して false。 */
  const run = useCallback(async (task: () => Promise<string>): Promise<boolean> => {
    setBusy(true);
    try {
      const hash = await task();
      onImportedRef.current(hash);
      toast.success(ja.edit.cover.imported);
      return true;
    } catch (error) {
      toast.error(errorMessage(error));
      return false;
    } finally {
      setBusy(false);
    }
  }, []);

  const chooseFile = useCallback(async () => {
    if (!inTauri()) {
      toast.info(ja.toast.mockUnavailable);
      return;
    }
    try {
      const path = await chooseOpenFile({
        title: ja.edit.cover.chooseTitle,
        filters: [{ name: ja.edit.cover.imageFilter, extensions: [...COVER_EXTENSIONS] }],
      });
      if (path !== null) await run(() => libraryApi.importCoverFile(path));
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }, [run]);

  const fromPaths = useCallback(
    async (paths: readonly string[]) => {
      const path = firstImagePath(paths);
      if (path === null) {
        toast.error(ja.edit.cover.unsupportedFile);
        return;
      }
      await run(() => libraryApi.importCoverFile(path));
    },
    [run],
  );

  const fromFiles = useCallback(
    async (files: ArrayLike<File> | null | undefined) => {
      const bytes = await imageBytesFromFiles(files);
      if (bytes === null) return false;
      return run(() => libraryApi.importCoverBytes(bytes));
    },
    [run],
  );

  const fromClipboard = useCallback(async () => {
    try {
      const items = await navigator.clipboard.read();
      const bytes = await imageBytesFromClipboardItems(items);
      if (bytes === null) {
        toast.info(ja.edit.cover.clipboardEmpty);
        return;
      }
      await run(() => libraryApi.importCoverBytes(bytes));
    } catch {
      // 読み取りの許可がない・クリップボードが空など。⌘V（貼り付け）なら許可なしで読める
      toast.info(ja.edit.cover.clipboardUnavailable);
    }
  }, [run]);

  const fromUrl = useCallback(
    async (input: string) => {
      const url = normalizeImageUrl(input);
      if (url === null) {
        toast.error(ja.edit.cover.invalidUrl);
        return false;
      }
      return run(() => libraryApi.importCoverUrl(url));
    },
    [run],
  );

  return { busy, chooseFile, fromPaths, fromFiles, fromClipboard, fromUrl };
}
