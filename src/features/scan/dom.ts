// キー入力のイベントの対象が「文字を入力する場所」かどうかの判定。DOM に依存する。

/** 文字を入力しない種類の <input>。これらにフォーカスがあるときは、数字キーを奪ってよい。 */
const NON_TEXT_INPUT_TYPES = new Set([
  "checkbox",
  "radio",
  "button",
  "submit",
  "reset",
  "image",
  "file",
  "range",
  "color",
]);

const EDITABLE_ROLES =
  '[role="textbox"], [role="combobox"], [role="searchbox"], [role="spinbutton"]';
const CONTENT_EDITABLE =
  '[contenteditable=""], [contenteditable="true"], [contenteditable="plaintext-only"]';

/** テキスト入力欄（input・textarea・select・contenteditable など）か。 */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target.closest(CONTENT_EDITABLE) || target.closest(EDITABLE_ROLES)) return true;
  const field = target.closest("input, textarea, select");
  if (!field) return false;
  if (field instanceof HTMLInputElement) return !NON_TEXT_INPUT_TYPES.has(field.type);
  return true;
}

/**
 * スキャン専用の入力欄の印（`data-scan-target`）。スキャン登録画面は、この欄にフォーカスがあるときも
 * リーダーの入力を拾う（ほかの入力欄にフォーカスがあるときは拾わない）。
 */
export function isScanTarget(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("[data-scan-target]") !== null;
}

/**
 * 日本語入力（IME）の変換中、またはその確定のキーイベントか。
 * Chromium（WebView2）は変換中の Enter を isComposing が true で送り、そのあとに compositionend が来る。
 * WebKit（WKWebView）は compositionend が先で、確定の Enter は isComposing が false・keyCode が 229 で来る。
 * どちらも「変換の確定であって、入力の確定ではない」ので、Enter として扱ってはいけない。
 */
export function isImeKeyEvent(event: KeyboardEvent): boolean {
  return event.isComposing || event.keyCode === 229;
}
