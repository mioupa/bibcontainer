// 書誌取得（`fetch_metadata`）の開始と、ISBN ごとの結果の記憶。
//
// ISBN を読んだ時点で取得を始め（2段目を待たない。FR-SCAN-05）、結果はストア（`lookups`）に ISBN ごとに入れる。
// 確定したあとの処理（フォームを開く、連続登録の行にする）は、この結果を引く。
// 同じ ISBN の取得が進行中なら、新しく始めずにその結果を待つ。
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import type { LookupOutcome } from "@/features/scanScreen/batch";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";

/** 進行中の取得（ISBN → 完了を待つ Promise）。 */
const inflight = new Map<string, Promise<LookupOutcome>>();
/** 順に取得するための列（ISBN 一覧の取り込み）。 */
let serialTail: Promise<unknown> = Promise.resolve();

async function fetchOnce(isbn13: string): Promise<LookupOutcome> {
  let outcome: LookupOutcome;
  try {
    outcome = { status: "done", result: await ipc.fetchMetadata(isbn13) };
  } catch (error) {
    outcome = { status: "failed", message: errorMessage(error) };
  }
  useScanSessionStore.getState().setLookup(isbn13, outcome);
  return outcome;
}

/** 取り直す価値のある結果か（取得に失敗した、またはオフラインで取得待ちの下書きになった）。 */
function isRetryable(outcome: LookupOutcome): boolean {
  return outcome.status === "failed" || outcome.result.offline;
}

export interface StartLookupOptions {
  /** 前の取得が終わってから始める（ISBN 一覧の取り込み。取得元のレート制限は Rust 側が守る）。 */
  serial?: boolean;
}

/**
 * 取得を始めて、結果を待つ Promise を返す。
 * - 進行中なら、その結果を待つ。
 * - 取得済みなら、その結果をすぐ返す。ただし、失敗やオフラインだったものは、改めて取得する。
 */
export function startLookup(
  isbn13: string,
  options: StartLookupOptions = {},
): Promise<LookupOutcome> {
  const running = inflight.get(isbn13);
  if (running) return running;

  const store = useScanSessionStore.getState();
  const known = store.lookups[isbn13];
  if (known !== undefined) {
    if (!isRetryable(known)) return Promise.resolve(known);
    store.dropLookup(isbn13);
  }

  const run = () => fetchOnce(isbn13);
  let promise: Promise<LookupOutcome>;
  if (options.serial) {
    promise = serialTail.then(run);
    serialTail = promise;
  } else {
    promise = run();
  }
  const tracked = promise.finally(() => {
    if (inflight.get(isbn13) === tracked) inflight.delete(isbn13);
  });
  inflight.set(isbn13, tracked);
  return tracked;
}

/** 取得の結果を待つ。始めていなければ、始める。 */
export function awaitLookup(isbn13: string): Promise<LookupOutcome> {
  return startLookup(isbn13);
}

/** 進行中の取得の記録を捨てる（テスト用）。 */
export function resetLookupsForTest(): void {
  inflight.clear();
  serialTail = Promise.resolve();
}
