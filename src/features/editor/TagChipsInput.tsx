// タグの複数入力。候補（`list_tags`）付き。Enter かカンマで足し、× で外す。
// フォーム（TagsField）と、一括変更の「タグを追加」で使う。
import { XIcon } from "lucide-react";
import { useId, useState, type KeyboardEvent } from "react";
import { Input } from "@/components/ui/input";
import { normalizeTags } from "@/features/editor/formValues";
import { ja } from "@/i18n/ja";

interface TagChipsInputProps {
  value: readonly string[];
  onChange: (tags: string[]) => void;
  onBlur?: () => void;
  suggestions: readonly string[];
  id?: string;
  /** 入力欄に付ける aria-label（ラベルが別にないとき）。 */
  label?: string;
}

export function TagChipsInput({
  value,
  onChange,
  onBlur,
  suggestions,
  id,
  label,
}: TagChipsInputProps) {
  const [draft, setDraft] = useState("");
  const listId = useId();

  const add = (text: string) => {
    const next = normalizeTags([...value, ...text.split(/[,、，]/)]);
    if (next.length !== value.length) onChange(next);
    setDraft("");
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      if (draft.trim() !== "") add(draft);
    } else if (event.key === "Backspace" && draft === "" && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  };

  return (
    <div className="flex flex-col gap-2">
      {value.length > 0 && (
        <ul className="flex flex-wrap gap-1.5" aria-label={ja.edit.fields.tags}>
          {value.map((tag) => (
            <li
              key={tag}
              className="flex items-center gap-1 rounded-full bg-secondary py-0.5 pr-1 pl-2.5 text-xs"
            >
              <span>{tag}</span>
              <button
                type="button"
                aria-label={ja.edit.removeTag(tag)}
                className="rounded-full p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                onClick={() => onChange(value.filter((other) => other !== tag))}
              >
                <XIcon className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <Input
        id={id}
        aria-label={label}
        value={draft}
        list={listId}
        autoComplete="off"
        placeholder={ja.edit.tagPlaceholder}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={onKeyDown}
        onBlur={() => {
          if (draft.trim() !== "") add(draft);
          onBlur?.();
        }}
      />
      <datalist id={listId}>
        {suggestions
          .filter((tag) => !value.includes(tag))
          .map((tag) => (
            <option key={tag} value={tag} />
          ))}
      </datalist>
    </div>
  );
}
