// 処理を 1 つずつ順に実行する列。前の処理が終わる（失敗しても）まで、次の処理を始めない。
// スキャンの判定の順序を保つ、確定した本を 1 冊ずつ処理する（フォームやダイアログを順に出す）ために使う。
export type SerialQueue = <T>(task: () => Promise<T>) => Promise<T>;

export function createSerialQueue(): SerialQueue {
  let tail: Promise<unknown> = Promise.resolve();
  return <T>(task: () => Promise<T>): Promise<T> => {
    const next = tail.then(task);
    tail = next.catch(() => undefined);
    return next;
  };
}
