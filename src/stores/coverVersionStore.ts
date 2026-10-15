// 書影の版（同期で書影があとから届いたとき、画面が読み込み直すための印。0016）。
//
// 本の行が先に届いて、書影のファイルがあとから届くことがある（同期フォルダにまだなかった）。そのとき、画面は書影の
// 読み込みに失敗して仮の表紙を出している。Rust が `covers-arrived`（届いた書影のハッシュ）を送るので、ハッシュごとに
// 版を進める。`BookCover` は、版を URL に足して（URL が変わるので、ブラウザは取り直す）、画像の要素を作り直す。
import { create } from "zustand";

interface CoverVersionState {
  /** ハッシュ → 版（その書影が届いた回数）。載っていない書影は 0。 */
  versions: Readonly<Record<string, number>>;
  /** 届いた書影の版を進める。 */
  bump: (hashes: readonly string[]) => void;
}

export const useCoverVersionStore = create<CoverVersionState>()((set) => ({
  versions: {},
  bump: (hashes) =>
    set((state) => {
      if (hashes.length === 0) return state;
      const versions = { ...state.versions };
      for (const hash of hashes) versions[hash] = (versions[hash] ?? 0) + 1;
      return { versions };
    }),
}));

/** ある書影の版。そのハッシュの版が進んだときだけ、使っている部品が描き直される。 */
export function useCoverVersion(hash: string | null): number {
  return useCoverVersionStore((state) => (hash === null ? 0 : (state.versions[hash] ?? 0)));
}
