// ISBN 一覧の取り込み（FR-IO-03）。`parse_isbn_list` が返した判定済みのコードを、連続登録と同じペアリングに通す。
//
// 行の間でタイムアウトしないよう、すべてのコードを同じ合成時刻（0）で reducer に渡す。reducer は時計を持たず、
// 時刻は呼び出し側が渡すので、ファイルを読み込む実際の時間とは関係なく、1 つの ISBN が次の行まで保留され続ける。
// 最後に確定キー（commit）を送って、保留中の本を 2段目なしで確定する。
import {
  PAIRING_IDLE,
  pairingReducer,
  type ConfirmedBook,
  type PairingConfig,
  type PairingMessage,
  type PairingEffect,
  type PairingState,
} from "@/features/scan/pairing";
import type { ScanCode } from "@/types/contracts";

/** 合成時刻。すべてのイベントに同じ値を渡すので、期限（時刻 + T）に達することはない。 */
export const LIST_CLOCK = 0;

export interface PairedList {
  /** 確定した本（ファイルの順）。 */
  books: ConfirmedBook[];
  /** 読み取れなかった・無視した行についての知らせ（雑誌・一般 JAN・不正・二度読み・捨てた 2段目）。 */
  skipped: PairingMessage[];
}

/** 知らせのうち、行が使われなかったことを表すもの。 */
function isSkipNotice(message: PairingMessage): boolean {
  return (
    message.kind === "rejected" ||
    message.kind === "duplicateIsbn" ||
    message.kind === "secondTierReplaced" ||
    message.kind === "secondTierDiscarded"
  );
}

export function pairIsbnList(codes: readonly ScanCode[], config: PairingConfig): PairedList {
  const books: ConfirmedBook[] = [];
  const skipped: PairingMessage[] = [];
  let state: PairingState = PAIRING_IDLE;

  const collect = (effects: readonly PairingEffect[]) => {
    for (const effect of effects) {
      if (effect.type === "confirm") books.push(effect.book);
      else if (effect.type === "message" && isSkipNotice(effect.message)) {
        skipped.push(effect.message);
      }
    }
  };

  for (const code of codes) {
    const step = pairingReducer(state, { type: "scan", code, now: LIST_CLOCK }, config);
    state = step.state;
    collect(step.effects);
  }
  // 最後に保留中の本を確定する（2段目だけが残っていれば捨てる）
  const last = pairingReducer(state, { type: "commit", now: LIST_CLOCK }, config);
  collect(last.effects);
  return { books, skipped };
}
