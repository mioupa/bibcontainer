// 確定した本 1 冊の処理（スキャン登録の流れ）。React にも画面にも依存せず、必要な操作は `FlowDeps` で受け取る。
//
//   find_by_isbn（重複チェック）
//     ├─ 所有済み       → 注意音と「所有済み: 書名 ／ 保管場所」。連続なら重複の行（既定で保存しない）。1 冊ずつなら表示だけ
//     ├─ 欲しい本       → 「所有に移しますか？」→ 移すなら update_book で所有状態を owned に（新しい本は作らない）
//     └─ それ以外       → 1 冊ずつ: 取得を待って登録フォームを開く ／ 連続: 一覧に行を足す
//
// 2段目は ISBN の後に読まれることがある（取得結果の下書きには入っていない）ので、フォームや一覧の下書きには、
// ここで 2段目を重ねて渡す（applySecondTier）。
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import type { LookupOutcome, RowDuplicate } from "@/features/scanScreen/batch";
import { decideConfirmed } from "@/features/scanScreen/decision";
import { toMetaSecondTier, withSecondTier } from "@/features/scanScreen/secondTier";
import type { LastRead, ScanMode, ScanResult } from "@/features/scanScreen/sessionStore";
import type { SoundKind } from "@/features/scan/config";
import type { ConfirmedBook } from "@/features/scan/pairing";
import type { NewBookSource } from "@/stores/editorStore";
import { ja } from "@/i18n/ja";

/** 処理が使う操作。画面（フック）が本物をつなぎ、テストは偽物を渡す。 */
export interface FlowDeps {
  findByIsbn: (isbn13: string) => Promise<IsbnMatch[]>;
  /** 書誌取得を始める（進行中・取得済みならそのまま。`reuseKnown` なら、失敗・オフラインの結果も取り直さない）。 */
  startLookup: (isbn13: string, options?: { serial?: boolean; reuseKnown?: boolean }) => void;
  /** 書誌取得の完了を待つ。 */
  awaitLookup: (isbn13: string) => Promise<LookupOutcome>;
  /** 新しい本の登録フォームを開き、閉じるまで待つ。 */
  openNewForm: (source: NewBookSource) => Promise<void>;
  /** 「所有に移しますか？」と確認する。 */
  confirmMoveToOwned: (match: IsbnMatch) => Promise<boolean>;
  /** 本の所有状態を owned にする（update_book。購入日はコアが今日を入れる）。 */
  moveToOwned: (bookId: string) => Promise<void>;
  /** 連続登録の一覧に行を足す。 */
  addRow: (book: ConfirmedBook, duplicate: RowDuplicate | null) => void;
  /** 一覧に同じ ISBN の行がすでにあるか。 */
  hasRow: (isbn13: string) => boolean;
  playSound: (kind: SoundKind) => void;
  setResult: (result: ScanResult | null) => void;
  setLast: (last: LastRead) => void;
  reportError: (error: unknown) => void;
  /** 画面がまだ表示されているか。離れたあとは、フォームや確認を出さない。 */
  isActive: () => boolean;
}

export interface ProcessOptions {
  mode: ScanMode;
  /** 音と「直前に読んだ本」の表示を出さない（ISBN 一覧の取り込み）。 */
  quiet?: boolean;
  /** 書誌取得を順に行う（ISBN 一覧の取り込み）。 */
  serialLookup?: boolean;
}

/** 処理の結果。ISBN 一覧の取り込みの集計に使う。 */
export type ProcessOutcome =
  "duplicate" | "moved" | "kept" | "failed" | "form" | "added" | "skipped";

export async function processConfirmed(
  book: ConfirmedBook,
  options: ProcessOptions,
  deps: FlowDeps,
): Promise<ProcessOutcome> {
  const { mode, quiet = false } = options;
  const sound = (kind: SoundKind) => {
    if (!quiet) deps.playSound(kind);
  };
  const show = (result: ScanResult) => {
    if (!quiet) deps.setResult(result);
  };
  if (!quiet) {
    deps.setLast({
      isbn13: book.isbn13,
      secondTier: book.secondTier
        ? { cCode: book.secondTier.cCode, listPrice: book.secondTier.listPrice }
        : null,
    });
  }

  let matches: IsbnMatch[] = [];
  try {
    matches = await deps.findByIsbn(book.isbn13);
  } catch (error) {
    // 重複を確かめられなくても、登録は続けられる
    deps.reportError(error);
  }
  const decision = decideConfirmed(matches);

  if (decision.kind === "owned") {
    const { match } = decision;
    sound("attention");
    if (mode === "batch") {
      deps.addRow(book, {
        kind: "owned",
        title: match.title,
        locationPath: match.locationPath,
      });
      show({
        level: "warning",
        text: ja.scan.result.ownedListed(match.title, match.locationPath),
      });
    } else {
      show({
        level: "warning",
        text: ja.scan.result.owned(match.title, match.locationPath),
        duplicate: { match, book },
      });
    }
    return "duplicate";
  }

  if (decision.kind === "wanted") {
    const { match } = decision;
    sound("attention");
    show({ level: "warning", text: ja.scan.result.wanted(match.title) });
    if (!deps.isActive()) return "skipped";
    const move = await deps.confirmMoveToOwned(match);
    if (!move) {
      show({ level: "info", text: ja.scan.result.wantedKept(match.title) });
      return "kept";
    }
    try {
      await deps.moveToOwned(match.id);
    } catch (error) {
      sound("error");
      deps.reportError(error);
      show({ level: "error", text: ja.scan.result.moveFailed(match.title) });
      return "failed";
    }
    sound("success");
    show({ level: "success", text: ja.scan.result.moved(match.title) });
    return "moved";
  }

  // 新しい本
  if (mode === "single") {
    show({ level: "info", text: ja.scan.result.fetching });
    const outcome = await deps.awaitLookup(book.isbn13);
    if (!deps.isActive()) return "skipped";
    show({ level: "success", text: ja.scan.result.opened });
    await deps.openNewForm({
      isbn13: book.isbn13,
      secondTier: toMetaSecondTier(book.secondTier),
      fetched: outcome.status === "done" ? withSecondTier(outcome.result, book.secondTier) : null,
    });
    return "form";
  }

  // 一覧に足す。取得は、行を足す前に始める（画面が、取得を始めていない行を見つけて、順番を待たない取得を始めないように）
  const inBatch = deps.hasRow(book.isbn13);
  // 読んだときに始めた取得の結果は、オフラインでも取り直さない（取り直すのは、次に読んだとき）。
  // ISBN 一覧の取り込みは、読んだときの取得がないので、取り直す
  if (!inBatch) {
    deps.startLookup(book.isbn13, {
      serial: options.serialLookup,
      reuseKnown: !options.serialLookup,
    });
  }
  deps.addRow(book, inBatch ? { kind: "inBatch" } : null);
  if (inBatch) {
    sound("attention");
    show({ level: "warning", text: ja.scan.result.inBatch });
  } else {
    show({ level: "success", text: ja.scan.result.added });
  }
  return "added";
}
