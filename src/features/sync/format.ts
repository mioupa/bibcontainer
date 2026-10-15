// 同期の表示用の整形（時刻、時計のずれの長さ、OS の名前）。時刻は端末のローカル時間で出す。

const pad = (value: number) => String(value).padStart(2, "0");

function parse(iso: string | null | undefined): Date | null {
  if (iso === null || iso === undefined) return null;
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** `2026-10-15 12:04`（ローカル時間）。読めなければ null。 */
export function formatDateTime(iso: string | null | undefined): string | null {
  const date = parse(iso);
  if (date === null) return null;
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 上部バー用の短い時刻。今日なら `12:04`、それ以前なら `10/14 12:04`。読めなければ null。 */
export function formatSyncTime(iso: string | null | undefined, now: Date): string | null {
  const date = parse(iso);
  if (date === null) return null;
  const time = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  return sameDay ? time : `${date.getMonth() + 1}/${date.getDate()} ${time}`;
}

/** 2 つの時刻のうち、新しい方（片方しかなければその方）。どちらも読めなければ null。 */
export function latestOf(
  a: string | null | undefined,
  b: string | null | undefined,
): string | null {
  const da = parse(a);
  const db = parse(b);
  if (da === null && db === null) return null;
  if (da === null) return b ?? null;
  if (db === null) return a ?? null;
  return da.getTime() >= db.getTime() ? (a ?? null) : (b ?? null);
}

/** 時計の進みの長さ。60 分未満は `12 分`、1 日未満は `2 時間 30 分`、それ以上は `1 日 3 時間`。 */
export function formatAhead(aheadMs: number): { minutes: number; text: string } {
  const minutes = Math.max(0, Math.floor(aheadMs / 60_000));
  if (minutes < 60) return { minutes, text: `${minutes} 分` };
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const rest = minutes % 60;
    return { minutes, text: rest === 0 ? `${hours} 時間` : `${hours} 時間 ${rest} 分` };
  }
  const days = Math.floor(hours / 24);
  const restHours = hours % 24;
  return { minutes, text: restHours === 0 ? `${days} 日` : `${days} 日 ${restHours} 時間` };
}

/** `std::env::consts::OS` の値を、画面に出す名前にする。知らない値はそのまま。 */
export function osLabel(os: string): string {
  switch (os.toLowerCase()) {
    case "macos":
    case "darwin":
      return "macOS";
    case "windows":
      return "Windows";
    case "linux":
      return "Linux";
    default:
      return os;
  }
}
