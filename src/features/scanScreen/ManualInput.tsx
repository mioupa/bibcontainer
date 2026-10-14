// ISBN・2段目の手入力欄（FR-SCAN-08）。全角の数字は半角にして、リーダーの入力と同じ流れ（classify_code）に渡す。
// リーダーの入力は KeyboardEvent.code で拾うので、この欄（data-scan-target なし）にフォーカスがあるときは拾わず、
// 文字として入る（数字と Enter で確定できる）。確定したら、リーダーが使えるよう、フォーカスを外す。
import { useId, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { toHalfWidthDigits } from "@/features/scan/normalize";
import { ja } from "@/i18n/ja";

interface ManualInputProps {
  onSubmit: (digits: string) => void;
}

export function ManualInput({ onSubmit }: ManualInputProps) {
  const [value, setValue] = useState("");
  const id = useId();

  const submit = () => {
    const digits = toHalfWidthDigits(value).trim();
    if (digits === "") return;
    onSubmit(digits);
    setValue("");
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  };

  return (
    <form
      data-manual-input
      className="flex flex-col gap-2 rounded-xl border bg-card p-4"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <label htmlFor={id} className="text-sm font-semibold text-muted-foreground">
        {ja.scan.manualInput.label}
      </label>
      <div className="flex gap-2">
        <Input
          id={id}
          value={value}
          inputMode="numeric"
          autoComplete="off"
          placeholder={ja.scan.manualInput.placeholder}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            // 日本語入力の変換を確定する Enter では、送らない
            if (event.key === "Enter" && event.nativeEvent.isComposing) event.preventDefault();
          }}
        />
        <Button type="submit" variant="outline" disabled={value.trim() === ""}>
          {ja.scan.manualInput.submit}
        </Button>
      </div>
    </form>
  );
}
