// カメラで読んだ値を、スキャン登録の流れに渡すかどうかの判定（純粋な関数。時刻は引数で渡す）。
//
// カメラは、同じバーコードを毎フレーム読む。リーダーのように「1 回の読み取り＝1 回の入力」にならないので、
// 次の規則で絞る（0029 §3）。
// - 同じ値が、連続する 2 回の読み取りで読めたら採用する（1 フレームだけの読み間違いを渡さない）。
// - 一度渡した値は、映像から消えて `GONE_MS` たつまで渡し直さない（映したままなら 1 回だけ）。
// - 1 回の読み取りに複数の値があれば、ISBN（978・979）→ 2段目（192）→ ほかの順に渡す。
// - 13 桁の数字以外は渡さない。判定そのもの（チェックディジット・雑誌など）は Rust の `classify_code` に任せる。

/** 映像から消えて、この時間（ミリ秒）たつと、同じ値をもう一度渡す。 */
export const GONE_MS = 1500;

export interface DedupeState {
  /** 直前の読み取りで読めた値。 */
  readonly previous: readonly string[];
  /** 渡した値 → 最後に映っていた時刻。映っている間は、この時刻が進み続ける。 */
  readonly delivered: Readonly<Record<string, number>>;
}

export const INITIAL_DEDUPE: DedupeState = { previous: [], delivered: {} };

export interface DedupeResult {
  state: DedupeState;
  /** 渡す値（この順に 1 つずつ渡す）。 */
  emit: string[];
}

/** 渡す順の優先度。ISBN が先、2段目（192）が次、ほかはそのあと。 */
function rank(value: string): number {
  if (value.startsWith("978") || value.startsWith("979")) return 0;
  if (value.startsWith("192")) return 1;
  return 2;
}

/**
 * 1 回の読み取りの結果（`values`）を受けて、渡す値と次の状態を返す。
 *
 * `accepting` が偽のとき（フォームや確認を開いている間）は何も渡さない。その間に映っていた値は
 * 「まだ映っている」として覚え、閉じた直後に、映したままの前のコードが入らないようにする。
 */
export function stepDedupe(
  state: DedupeState,
  values: readonly string[],
  now: number,
  accepting: boolean,
): DedupeResult {
  const seen = [...new Set(values.filter((value) => /^\d{13}$/.test(value)))];

  const delivered: Record<string, number> = {};
  for (const [value, lastSeen] of Object.entries(state.delivered)) {
    // 消えて GONE_MS たった値は、新しく映ったものとして扱えるように忘れる
    if (now - lastSeen < GONE_MS) delivered[value] = lastSeen;
  }

  const emit: string[] = [];
  for (const value of seen) {
    if (value in delivered) {
      // まだ映っている（渡し済み）。映っている時刻だけ進める
      delivered[value] = now;
    } else if (!accepting) {
      // 止めている間に読めた値は捨てる。ただし「映っている」ことは覚える
      delivered[value] = now;
    } else if (state.previous.includes(value)) {
      emit.push(value);
      delivered[value] = now;
    }
  }
  emit.sort((a, b) => rank(a) - rank(b));

  return { state: { previous: seen, delivered }, emit };
}
