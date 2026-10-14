// 検索欄の Enter の扱い（FR-SCAN-08／09）。ISBN の確定（バーコードリーダーが打つ数字の連続と Enter、または手入力）を、
// 日本語入力（IME）がオンのままでも、Enter 1 回で受ける。
//
// 日本語入力がオンのとき、リーダーの数字は変換中の文字（全角の数字）として入り、Enter は「変換の確定」に使われる。
// 変換中の Enter は、`isComposing` が true（Chromium・WebView2）で、`key` が "Enter" ではなく "Process"、`keyCode` が 229 になる。
// 以前はこれを無視していたので、確定のあとにもう 1 回 Enter が要った（WebView2）。Safari（WKWebView）は、確定の Enter が
// `compositionend` のあとに、`isComposing` が false・`keyCode` が 229 で届く。
//
// 方法: 変換を確定する Enter を見たら、確定が反映されたあとの欄の値を見る。ISBN の形（`detectIsbnSearch`）なら、
// 通常の Enter と同じ確定として扱う。ISBN の形でなければ何もしない（普通の日本語入力では、変換を確定するだけ）。
// 数字キーを奪って半角で入れる方法は取らない: 日本語入力に渡さずに済むかは OS ごとに違い（keydown の preventDefault が
// 変換に効かない環境がある）、効かないと全角と半角が二重に入るため。
import { useCallback, useEffect, useRef, type KeyboardEvent, type RefObject } from "react";
import { detectIsbnSearch } from "@/features/library/isbnSearch";

/** 変換を確定する Enter を見てから、確定の反映（compositionend・入力）を待つ上限（ミリ秒）。 */
const COMMIT_WINDOW_MS = 500;

/** Enter キーか（変換中は `key` が "Process" になるので、物理キー（`code`）でも見る）。 */
export function isEnterKey(event: { key: string; code: string }): boolean {
  return event.key === "Enter" || event.code === "Enter" || event.code === "NumpadEnter";
}

/** 日本語入力の変換を確定する Enter か（変換中、または変換の確定の直後のキー）。 */
export function isImeCommitKey(event: { key: string; isComposing: boolean; keyCode: number }) {
  return event.isComposing || event.key === "Process" || event.keyCode === 229;
}

export interface SearchEnterHandlers {
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
  onCompositionStart: () => void;
  onCompositionEnd: () => void;
}

/**
 * 検索欄に付けるイベントの受け手を返す。
 * `onSubmit` は、Enter で確定したとき（日本語入力の変換を確定する Enter の場合は、確定後の値が ISBN の形のときだけ）に呼ぶ。
 */
export function useSearchEnter(
  inputRef: RefObject<HTMLInputElement | null>,
  onSubmit?: (text: string) => void,
): SearchEnterHandlers {
  const onSubmitRef = useRef(onSubmit);
  useEffect(() => {
    onSubmitRef.current = onSubmit;
  });

  const composing = useRef(false);
  /** 変換を確定する Enter を見た時刻。確定の反映を待っている間だけ入る。 */
  const commitAt = useRef<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => clearTimeout(timer.current), []);

  // 確定が欄の値に反映されたあと（次のタスク）に、値を見る
  const checkAfterCommit = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      commitAt.current = null;
      const text = inputRef.current?.value ?? "";
      if (detectIsbnSearch(text) !== null) onSubmitRef.current?.(text);
    }, 0);
  }, [inputRef]);

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLInputElement>) => {
      if (!isEnterKey(event)) {
        // ほかのキーが続いたら、変換の確定の待ちは終わり
        commitAt.current = null;
        return;
      }
      if (!isImeCommitKey(event.nativeEvent)) {
        commitAt.current = null;
        onSubmitRef.current?.(event.currentTarget.value);
        return;
      }
      commitAt.current = Date.now();
      if (event.nativeEvent.isComposing) composing.current = true;
      // Safari は compositionend が先に来ているので、ここで確かめる。Chromium は compositionend が来てから確かめる
      if (!composing.current) checkAfterCommit();
    },
    [checkAfterCommit],
  );

  const onCompositionStart = useCallback(() => {
    composing.current = true;
  }, []);

  const onCompositionEnd = useCallback(() => {
    composing.current = false;
    if (commitAt.current !== null && Date.now() - commitAt.current <= COMMIT_WINDOW_MS) {
      checkAfterCommit();
    }
  }, [checkAfterCommit]);

  return { onKeyDown, onCompositionStart, onCompositionEnd };
}
