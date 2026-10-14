// 日付の検証（コアの `is_valid_partial_date` と同じ規則）。フォームの検証と「今日」ボタンで使う。

/** `YYYY`、`YYYY-MM`、`YYYY-MM-DD` のどれかで、実在する日付か。 */
export function isValidPartialDate(value: string): boolean {
  const match = /^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?$/.exec(value);
  if (!match) return false;
  const [, , month, day] = match;
  if (month === undefined) return true;
  const monthNumber = Number(month);
  if (monthNumber < 1 || monthNumber > 12) return false;
  if (day === undefined) return true;
  const year = Number(match[1]);
  // Date.UTC は 0〜99 年を 1900 年代に読み替えるので、setUTCFullYear で年月日をまとめて入れる
  const date = new Date(0);
  date.setUTCFullYear(year, monthNumber - 1, Number(day));
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() === monthNumber - 1 &&
    date.getUTCDate() === Number(day)
  );
}

/** ローカルの今日の日付（`YYYY-MM-DD`）。 */
export function todayLocal(now: Date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
