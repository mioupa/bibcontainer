import { useEffect, useState } from "react";

/** 値が変わってから `delayMs` ミリ秒、変わらなかったときの値を返す（入力のたびに検索しないため）。 */
export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}
