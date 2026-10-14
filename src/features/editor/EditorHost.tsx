// 登録・編集フォームの置き場所。App.tsx が画面の外側に 1 つだけ置き、useEditorStore の request を見てフォームを開く。
// フォームの本体は BookEditorDialog（データの読み込みと、閉じる確認）と BookEditorForm（タブ・検証・保存）。
import { BookEditorDialog } from "@/features/editor/BookEditorDialog";
import { useEditorStore, type EditorRequest } from "@/stores/editorStore";

// 開くたびに別のフォームとして作り直す（前の入力が残らないように）ための、要求ごとの番号
const requestKeys = new WeakMap<EditorRequest, number>();
let nextKey = 0;

function keyOf(request: EditorRequest): number {
  let key = requestKeys.get(request);
  if (key === undefined) {
    key = nextKey++;
    requestKeys.set(request, key);
  }
  return key;
}

export function EditorHost() {
  const request = useEditorStore((s) => s.request);
  const close = useEditorStore((s) => s.close);
  if (request === null) return null;
  return <BookEditorDialog key={keyOf(request)} request={request} onClose={close} />;
}
