// 新しい本のフォームの上に出す、重複の警告。
//   - 同じ ISBN の本が登録済み（FR-BOOK-10）: 警告し、確認（チェック）したら別の本として登録できる。
//   - 書名・著者が似た本（FR-SCAN-19。手動登録のとき）: 候補として出すだけ。
import { useQuery } from "@tanstack/react-query";
import { AlertTriangleIcon, InfoIcon } from "lucide-react";
import { useEffect } from "react";
import { useFormContext, useWatch } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { parseIsbnInput, type BookFormValues } from "@/features/editor/formValues";
import { canSearchSimilar, pickSimilarBooks } from "@/features/editor/similar";
import { useDebouncedValue } from "@/features/editor/useDebouncedValue";
import { libraryApi } from "@/features/library/api";
import { libraryKeys, useBooks } from "@/features/library/hooks";
import { ja } from "@/i18n/ja";
import { isValidIsbn13 } from "@/lib/isbn";

interface DuplicatePanelProps {
  /** 手動登録（書誌を取得していない）か。そうなら、似た本も探す。 */
  searchSimilar: boolean;
  /** 同じ ISBN の本があっても登録する、と確認したか。 */
  confirmed: boolean;
  onConfirmedChange: (confirmed: boolean) => void;
  /** 同じ ISBN の本があるか（保存ボタンを止めるために、親へ伝える）。 */
  onDuplicateChange: (hasDuplicate: boolean) => void;
}

export function DuplicatePanel({
  searchSimilar,
  confirmed,
  onConfirmedChange,
  onDuplicateChange,
}: DuplicatePanelProps) {
  const { control } = useFormContext<BookFormValues>();
  const isbnText = useWatch({ control, name: "isbn13" });
  const title = useWatch({ control, name: "title" });
  const authors = useWatch({ control, name: "authors" });

  const isbn = useDebouncedValue(parseIsbnInput(isbnText), 300);
  const validIsbn = isbn !== null && isValidIsbn13(isbn) ? isbn : null;
  const matchesQuery = useQuery({
    queryKey: ["find_by_isbn", validIsbn],
    queryFn: () => libraryApi.findByIsbn(validIsbn ?? ""),
    enabled: validIsbn !== null,
  });
  // ゴミ箱の本は重複とはみなさない（登録し直せる）
  const matches = (validIsbn === null ? [] : (matchesQuery.data ?? [])).filter((m) => !m.deleted);
  const hasDuplicate = matches.length > 0;
  useEffect(() => {
    onDuplicateChange(hasDuplicate);
  }, [hasDuplicate, onDuplicateChange]);

  const debouncedTitle = useDebouncedValue(title.trim(), 400);
  const similarEnabled = searchSimilar && canSearchSimilar(debouncedTitle);
  const similarQuery = useQuery({
    queryKey: libraryKeys.search(debouncedTitle),
    queryFn: () => libraryApi.searchBooks(debouncedTitle),
    enabled: similarEnabled,
  });
  const books = useBooks().data;
  const similar =
    similarEnabled && books !== undefined && similarQuery.data !== undefined
      ? pickSimilarBooks(
          books,
          new Set(similarQuery.data),
          authors.map((author) => author.name),
        ).filter((row) => !matches.some((match) => match.id === row.id))
      : [];

  return (
    <>
      {hasDuplicate && (
        <div
          role="alert"
          className="flex flex-col gap-2 rounded-md border border-warning/50 bg-warning/10 p-3 text-sm"
        >
          <p className="flex items-center gap-1.5 font-medium">
            <AlertTriangleIcon className="size-4 shrink-0 text-warning" aria-hidden />
            {ja.edit.duplicate.title}
          </p>
          <ul className="list-disc pl-6 text-xs">
            {matches.map((match) => (
              <li key={match.id}>
                {ja.edit.duplicate.item(
                  match.title,
                  ja.labels.ownership[match.ownership],
                  match.locationPath,
                )}
              </li>
            ))}
          </ul>
          <div className="flex items-center gap-2">
            <Checkbox
              id="edit-duplicate-confirm"
              checked={confirmed}
              onCheckedChange={(value) => onConfirmedChange(value === true)}
            />
            <Label htmlFor="edit-duplicate-confirm" className="text-xs">
              {ja.edit.duplicate.confirm}
            </Label>
          </div>
        </div>
      )}
      {similar.length > 0 && (
        <div className="flex flex-col gap-1 rounded-md border bg-muted/40 p-3 text-xs">
          <p className="flex items-center gap-1.5 font-medium">
            <InfoIcon className="size-4 shrink-0 text-info" aria-hidden />
            {ja.edit.similar.title}
          </p>
          <ul className="list-disc pl-6">
            {similar.map((row) => (
              <li key={row.id}>
                {ja.edit.similar.item(
                  row.title,
                  row.authors.map((author) => author.name).join(ja.format.listSeparator),
                  row.publisher,
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}
