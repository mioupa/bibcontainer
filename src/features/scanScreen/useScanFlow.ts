// スキャン登録画面の読み取りの流れ（9.3）。
//
//   useScanCapture（キー入力 → 数字列）→ classify_code（Rust が判定）→ usePairing（2 段のペアリング）
//     → 効果: startLookup（書誌取得を始める）／sound（音）／message（画面の表示）／confirm（確定した本の処理）
//
// 確定した本の処理（flow.ts の processConfirmed）は、列に並べて 1 冊ずつ行う（フォームや確認を順に出すため）。
// フォームやダイアログを開いている間は、読み取りと確定キーを止める。
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import type { PairingEffect } from "@/features/scan/pairing";
import { playSound } from "@/features/scan/playSound";
import { useCommitKey } from "@/features/scan/useCommitKey";
import { usePairing, type UsePairingResult } from "@/features/scan/usePairing";
import { useScanCapture } from "@/features/scan/useScanCapture";
import { processConfirmed } from "@/features/scanScreen/flow";
import { pairIsbnList } from "@/features/scanScreen/isbnList";
import { createLiveDeps, trackScreenActive } from "@/features/scanScreen/liveDeps";
import { startLookup } from "@/features/scanScreen/lookup";
import { createSerialQueue } from "@/features/scanScreen/serialQueue";
import { withSecondTier, toMetaSecondTier } from "@/features/scanScreen/secondTier";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import { chooseOpenFile } from "@/lib/dialog";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { useEditorStore } from "@/stores/editorStore";
import { useScreenStore } from "@/stores/screenStore";
import { useSettingsStore } from "@/stores/settingsStore";

export interface ScanFlow {
  pairing: UsePairingResult;
  /** 読み取りを受け付けているか（フォームやダイアログを開いている間は止める）。 */
  listening: boolean;
  /** 数字列（リーダーや手入力）を判定して、ペアリングに渡す。 */
  submitDigits: (digits: string) => void;
  /** ISBN 一覧のファイルを選んで、連続登録の一覧に足す（FR-IO-03）。 */
  importIsbnList: () => Promise<void>;
  /** 所有済みの本を、2 冊目として登録するフォームを開く（FR-BOOK-10）。 */
  registerSecondCopy: () => void;
}

export function useScanFlow(): ScanFlow {
  const timeoutSec = useSettingsStore((s) => s.scan.timeoutSec);
  const editorOpen = useEditorStore((s) => s.request !== null);
  const promptOpen = useScanSessionStore((s) => s.prompt !== null);
  const listening = !editorOpen && !promptOpen;

  // 画面がまだ表示されているか。離れたあとは、フォームや確認を新しく出さない
  // （StrictMode は、マウント → 後片付け → マウントを行うが、数えているので釣り合う）
  useEffect(() => {
    const stop = trackScreenActive();
    return () => {
      stop();
      // 答えを待っている確認があれば、「移さない」として閉じる
      useScanSessionStore.getState().prompt?.resolve(false);
    };
  }, []);

  const deps = useMemo(() => createLiveDeps(), []);
  const [classifyQueue] = useState(createSerialQueue);
  const [confirmQueue] = useState(createSerialQueue);

  const handleEffect = useCallback(
    (effect: PairingEffect) => {
      const session = useScanSessionStore.getState();
      switch (effect.type) {
        case "startLookup": {
          // 2段目を先に読んでいたら（逆順）、その値を引き継ぐ
          const held = session.last !== null && session.last.isbn13 === null ? session.last : null;
          session.setLast({ isbn13: effect.isbn13, secondTier: held?.secondTier ?? null });
          void startLookup(effect.isbn13);
          break;
        }
        case "confirm": {
          const mode = session.mode;
          confirmQueue(() => processConfirmed(effect.book, { mode }, deps)).catch(
            (error: unknown) => toast.error(errorMessage(error)),
          );
          break;
        }
        case "sound":
          playSound(effect.sound);
          break;
        case "message": {
          session.setResult({ level: effect.level, text: ja.scan.message(effect.message) });
          if (effect.message.kind === "secondTierHeld") {
            session.setLast({
              isbn13: null,
              secondTier: { cCode: effect.message.cCode, listPrice: effect.message.listPrice },
            });
          }
          break;
        }
      }
    },
    [confirmQueue, deps],
  );

  const pairing = usePairing({ timeoutSec, onEffect: handleEffect });
  const { submit, commit } = pairing;

  // 連続登録のとき、画面を離れる時点で保留中の本があれば、確定して一覧に残す（読んだ本を失わない）
  useEffect(
    () => () => {
      if (useScanSessionStore.getState().mode === "batch") commit();
    },
    [commit],
  );

  const submitDigits = useCallback(
    (digits: string) => {
      // 判定は非同期（IPC）。読んだ順に判定して、ペアリングに渡す
      void classifyQueue(async () => {
        try {
          submit(await ipc.classifyCode(digits));
        } catch (error) {
          playSound("error");
          toast.error(errorMessage(error));
        }
      });
    },
    [classifyQueue, submit],
  );

  useScanCapture({ mode: "scanScreen", enabled: listening, onScan: submitDigits });
  useCommitKey({ enabled: listening, onCommit: pairing.commit });

  // メイン画面で読んだバーコードを、読んだものとして処理する（FR-SCAN-10）
  useEffect(() => {
    const handoff = useScreenStore.getState().takeHandoff();
    if (handoff !== null) submitDigits(handoff);
  }, [submitDigits]);

  const importIsbnList = useCallback(async () => {
    try {
      const path = await chooseOpenFile({
        title: ja.dialog.isbnList.title,
        filters: [{ name: ja.dialog.isbnList.filter, extensions: ["txt", "text", "csv"] }],
      });
      if (path === null) return;
      const codes = await ipc.parseIsbnList(path);
      const { books, skipped } = pairIsbnList(codes, { timeoutSec });
      toast.info(ja.scan.import.started(books.length));
      await confirmQueue(async () => {
        let added = 0;
        let duplicates = 0;
        for (const book of books) {
          const outcome = await processConfirmed(
            book,
            { mode: "batch", quiet: true, serialLookup: true },
            deps,
          );
          if (outcome === "added") added += 1;
          else if (outcome === "duplicate") duplicates += 1;
        }
        playSound("success");
        toast.success(ja.scan.import.done({ added, duplicates, skipped: skipped.length }));
      });
    } catch (error) {
      playSound("error");
      toast.error(errorMessage(error));
    }
  }, [confirmQueue, deps, timeoutSec]);

  const registerSecondCopy = useCallback(() => {
    const duplicate = useScanSessionStore.getState().result?.duplicate;
    if (duplicate === undefined) return;
    const { book } = duplicate;
    void confirmQueue(async () => {
      const outcome = await deps.awaitLookup(book.isbn13);
      await deps.openNewForm({
        isbn13: book.isbn13,
        secondTier: toMetaSecondTier(book.secondTier),
        fetched: outcome.status === "done" ? withSecondTier(outcome.result, book.secondTier) : null,
      });
    }).catch((error: unknown) => toast.error(errorMessage(error)));
  }, [confirmQueue, deps]);

  return { pairing, listening, submitDigits, importIsbnList, registerSecondCopy };
}
