// スキャン登録画面の状態。画面を離れても、連続登録の一覧と共通値、取得結果を失わないよう、ストアに置く。
// 「1 冊ずつ／連続」の選択だけを端末に保存する（連続登録の一覧は、保存するまでの一時的なもの）。
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import type { NewBookDraft } from "@/bindings/NewBookDraft";
import {
  EMPTY_PRESETS,
  createRow,
  type BatchPresets,
  type BatchRow,
  type LookupOutcome,
  type RowDuplicate,
} from "@/features/scanScreen/batch";
import type { ConfirmedBook, MessageLevel } from "@/features/scan/pairing";

export type ScanMode = "single" | "batch";

/** 直前に読んだ本として表示する内容（書名・書影は、取得結果から引く）。 */
export interface LastRead {
  isbn13: string | null;
  /** 読んだ 2段目の値。 */
  secondTier: { cCode: string | null; listPrice: number | null } | null;
}

/** 直前の結果。色（成功・注意・エラー）と文言。 */
export interface ScanResult {
  level: MessageLevel;
  text: string;
  /** 所有済みのとき。「2 冊目として登録」で、フォームを開くのに使う（FR-BOOK-10）。 */
  duplicate?: { match: IsbnMatch; book: ConfirmedBook };
}

/** 「所有に移しますか？」の確認（FR-SCAN-18）。答えを待っている処理がある間だけ入っている。 */
export interface WantedPrompt {
  match: IsbnMatch;
  resolve: (move: boolean) => void;
}

interface ScanSessionState {
  mode: ScanMode;
  presets: BatchPresets;
  rows: BatchRow[];
  /** 次の行の識別子。 */
  nextKey: number;
  /** ISBN → 書誌取得の結果。取得中のものは入っていない。 */
  lookups: Record<string, LookupOutcome>;
  last: LastRead | null;
  result: ScanResult | null;
  prompt: WantedPrompt | null;

  setMode: (mode: ScanMode) => void;
  setPresets: (patch: Partial<BatchPresets>) => void;
  /** 行を足して、足した行を返す。 */
  addRow: (book: ConfirmedBook, duplicate: RowDuplicate | null) => BatchRow;
  removeRow: (key: string) => void;
  setIncluded: (key: string, included: boolean) => void;
  setEdited: (key: string, edited: NewBookDraft) => void;
  /** 保存した行を一覧から外す（keys を省くと全行）。 */
  clearRows: (keys?: readonly string[]) => void;
  setLookup: (isbn13: string, outcome: LookupOutcome) => void;
  dropLookup: (isbn13: string) => void;
  clearLookups: (isbn13s?: readonly string[]) => void;
  setLast: (last: LastRead | null) => void;
  setResult: (result: ScanResult | null) => void;
  setPrompt: (prompt: WantedPrompt | null) => void;
  /** 一時的な状態をすべて初期に戻す（テスト用。選んだ方法は変えない）。 */
  reset: () => void;
}

const INITIAL = {
  presets: EMPTY_PRESETS,
  rows: [] as BatchRow[],
  nextKey: 1,
  lookups: {} as Record<string, LookupOutcome>,
  last: null,
  result: null,
  prompt: null,
};

export const useScanSessionStore = create<ScanSessionState>()(
  persist(
    (set, get) => ({
      mode: "single",
      ...INITIAL,
      setMode: (mode) => set({ mode }),
      setPresets: (patch) => set((s) => ({ presets: { ...s.presets, ...patch } })),
      addRow: (book, duplicate) => {
        const row = createRow(`row-${get().nextKey}`, book, duplicate);
        set((s) => ({ rows: [...s.rows, row], nextKey: s.nextKey + 1 }));
        return row;
      },
      removeRow: (key) => set((s) => ({ rows: s.rows.filter((row) => row.key !== key) })),
      setIncluded: (key, included) =>
        set((s) => ({ rows: s.rows.map((row) => (row.key === key ? { ...row, included } : row)) })),
      setEdited: (key, edited) =>
        set((s) => ({ rows: s.rows.map((row) => (row.key === key ? { ...row, edited } : row)) })),
      clearRows: (keys) =>
        set((s) => ({
          rows: keys === undefined ? [] : s.rows.filter((row) => !keys.includes(row.key)),
        })),
      setLookup: (isbn13, outcome) =>
        set((s) => ({ lookups: { ...s.lookups, [isbn13]: outcome } })),
      dropLookup: (isbn13) =>
        set((s) => {
          const rest = { ...s.lookups };
          delete rest[isbn13];
          return { lookups: rest };
        }),
      clearLookups: (isbn13s) =>
        set((s) => {
          if (isbn13s === undefined) return { lookups: {} };
          const rest = { ...s.lookups };
          for (const isbn13 of isbn13s) delete rest[isbn13];
          return { lookups: rest };
        }),
      setLast: (last) => set({ last }),
      setResult: (result) => set({ result }),
      setPrompt: (prompt) => set({ prompt }),
      reset: () => set({ ...INITIAL }),
    }),
    {
      name: "bibcontainer.scanMode",
      version: 1,
      // localStorage を使えない環境（プライベートウィンドウなど）では、何も保存しない
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ mode: s.mode }),
      merge: (persisted, current) => {
        const mode = (persisted as { mode?: unknown } | null | undefined)?.mode;
        return { ...current, mode: mode === "batch" || mode === "single" ? mode : current.mode };
      },
    },
  ),
);
