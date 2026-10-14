// 登録・編集フォーム（ダイアログ）を開く要求。保存しない。
//
// フォームの実装は src/features/editor/ にあり、このストアの request を見て開く。
// ほかの画面（メイン画面、スキャン登録画面）は open* を呼ぶだけで、フォームの中身には関わらない。
//
// - edit:  登録済みの本を編集する。保存は update_book（手で直した項目は origin: manual）。
// - new:   新しい本を登録する。保存は create_books_from_drafts。
//          手動登録（ISBN なし）、1 冊ずつ登録（FR-SCAN-13）、欲しい本の追加などで使う。
// - draft: 連続登録（FR-SCAN-14）の一覧の 1 行を、保存せずに直す。
//          「OK」で onDone に直した下書きを渡し、呼び出し側が一覧の行を置き換える。
import { create } from "zustand";
import type {
  FetchMetadataResult,
  FieldChange,
  MetaSecondTier,
  NewBookDraft,
} from "@/types/contracts";

/** 新しい本のフォームに入れる元のデータ。どれも省略できる（全部無ければ空のフォーム）。 */
export interface NewBookSource {
  /** 読んだ ISBN-13。手動登録では null。 */
  isbn13?: string | null;
  /** 読んだ 2段目の値。 */
  secondTier?: MetaSecondTier | null;
  /** fetch_metadata の結果。取得前・取得失敗・手動登録では null。offline なら取得待ちとして保存する。 */
  fetched?: FetchMetadataResult | null;
  /** 先に入れておく値（連続登録の共通値、欲しい本としての登録など）。下書きの値より優先する。 */
  presets?: FieldChange[];
  /** draft モードで、既に一度直した下書き。あればこれを元に開く。 */
  draft?: NewBookDraft | null;
}

export type EditorRequest =
  | { mode: "edit"; bookId: string }
  | { mode: "new"; source: NewBookSource }
  | { mode: "draft"; source: NewBookSource; onDone: (draft: NewBookDraft) => void };

interface EditorState {
  request: EditorRequest | null;
  openEdit: (bookId: string) => void;
  openNew: (source?: NewBookSource) => void;
  openDraft: (source: NewBookSource, onDone: (draft: NewBookDraft) => void) => void;
  close: () => void;
}

export const useEditorStore = create<EditorState>()((set) => ({
  request: null,
  openEdit: (bookId) => set({ request: { mode: "edit", bookId } }),
  openNew: (source) => set({ request: { mode: "new", source: source ?? {} } }),
  openDraft: (source, onDone) => set({ request: { mode: "draft", source, onDone } }),
  close: () => set({ request: null }),
}));
