// ISBN・2段目の手入力欄（FR-SCAN-08）。スキャン登録画面でフォーカスが居る定位置（manualFocus.ts）。
// 全角の数字は半角にして、リーダーの入力と同じ流れ（classify_code）に渡す。
//
// - リーダーの入力は KeyboardEvent.code で拾うので、この欄（data-scan-target なし）にフォーカスがあるときは
//   拾わず、文字として入る。数字と Enter で確定できる。確定してもフォーカスは外さず、続けて打てる。
// - 欄が空のとき、Enter・スペースは確定キーと同じ（保留中の本の確定）。欄に値があるとき、スペースは入れない。
// - 日本語入力がオンだと、リーダーの数字は変換中の文字（全角のことが多い）として入り、リーダーの Enter は
//   変換の確定にしかならない。変換が確定（compositionend）したとき、欄の値が完全なコード（13 桁、または 10 桁）なら、
//   Enter と同じに送る。変換中の Enter では送らない（二重に送らない）。
//   Chromium は Enter（変換中）→ compositionend、WebKit は compositionend → Enter（keyCode 229）の順で来る。
import { useId, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { isImeKeyEvent } from "@/features/scan/dom";
import { isCompleteCode, normalizeManualInput } from "@/features/scan/normalize";
import { focusManualInput } from "@/features/scanScreen/manualFocus";
import { ja } from "@/i18n/ja";

interface ManualInputProps {
  onSubmit: (digits: string) => void;
  /** 欄が空のときの Enter・スペース（確定キーと同じ）。 */
  onCommit: () => void;
}

export function ManualInput({ onSubmit, onCommit }: ManualInputProps) {
  const [value, setValue] = useState("");
  // 日本語入力の変換中か（compositionstart〜compositionend）
  const composing = useRef(false);
  const id = useId();

  const submitValue = (raw: string) => {
    const digits = normalizeManualInput(raw);
    if (digits === "") return;
    onSubmit(digits);
    setValue("");
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    const native = event.nativeEvent;
    const isEnter = event.key === "Enter" || event.code === "Enter" || event.code === "NumpadEnter";
    const isSpace = event.code === "Space" || event.key === " ";
    const empty = normalizeManualInput(event.currentTarget.value) === "";

    if (isEnter) {
      // フォームの送信にはしない（変換中の Enter も含めて、ここで扱う）
      event.preventDefault();
      // 変換の確定は、入力の確定ではない。値は compositionend で受ける
      if (isImeKeyEvent(native) || composing.current) return;
      if (empty) onCommit();
      else submitValue(event.currentTarget.value);
      return;
    }

    if (isSpace) {
      // 変換中のスペースは、変換のキー（日本語入力に任せる）
      if (native.isComposing || composing.current) return;
      // ISBN に空白はないので、入れない。空のときだけ確定キー（押しっぱなしの繰り返しは数えない）
      event.preventDefault();
      if (empty && !event.repeat) onCommit();
    }
  };

  return (
    <form
      data-manual-input
      className="flex flex-col gap-2 rounded-xl border bg-card p-4"
      onSubmit={(event) => {
        event.preventDefault();
        submitValue(value);
        // ボタンを押したあとも、続けて打てるように
        focusManualInput();
      }}
    >
      <label htmlFor={id} className="text-sm font-semibold text-muted-foreground">
        {ja.scan.manualInput.label}
      </label>
      <div className="flex gap-2">
        <Input
          data-manual-field=""
          id={id}
          value={value}
          inputMode="numeric"
          autoComplete="off"
          placeholder={ja.scan.manualInput.placeholder}
          onChange={(event) => {
            const raw = event.target.value;
            // 変換中の文字列には触らない（空白は変換のキーが入れることがあるので、確定してから落とす）
            const composingNow = composing.current || (event.nativeEvent as InputEvent).isComposing;
            setValue(composingNow ? raw : raw.replace(/\s/g, ""));
          }}
          onKeyDown={handleKeyDown}
          onCompositionStart={() => {
            composing.current = true;
          }}
          onCompositionEnd={(event) => {
            composing.current = false;
            const normalized = normalizeManualInput(event.currentTarget.value);
            if (isCompleteCode(normalized)) submitValue(normalized);
            else setValue(normalized);
          }}
          onBlur={() => {
            composing.current = false;
          }}
        />
        <Button type="submit" variant="outline" disabled={normalizeManualInput(value) === ""}>
          {ja.scan.manualInput.submit}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">{ja.scan.manualInput.hint}</p>
    </form>
  );
}
