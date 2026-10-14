// フォームの検証（zod）。規則はコアの検証（0006 §2、`library/validate.rs`）に合わせる:
// 日付は YYYY・YYYY-MM・YYYY-MM-DD の実在する日付、ISBN は 13 桁でチェックデジットが正しいこと、
// Cコードは 4 桁の数字、金額とページ数は 0 以上の整数、評価は 1〜5。
// コアは書名の空を許すが、フォームでは、ISBN がない本は書名を必須にする
// （ISBN があれば、書名が空でも取得待ちとして保存できる。FR-SCAN-16）。
import { z } from "zod";
import { isValidPartialDate } from "@/features/editor/dates";
import {
  fieldDefByOp,
  opValue,
  parseIntegerInput,
  type BookFormValues,
  type FormKey,
} from "@/features/editor/formValues";
import { ja } from "@/i18n/ja";
import { isValidIsbn13 } from "@/lib/isbn";

/** コアが受け付ける整数の上限（`i32::MAX`）。 */
const MAX_INTEGER = 2_147_483_647;

const authorRow = z.object({ name: z.string(), kana: z.string(), role: z.string() });

export const bookFormSchema = z
  .object({
    title: z.string(),
    titleKana: z.string(),
    subtitle: z.string(),
    authors: z.array(authorRow),
    publisher: z.string(),
    publishedOn: z.string(),
    seriesName: z.string(),
    volume: z.string(),
    isbn13: z.string(),
    cCode: z.string(),
    listPrice: z.string(),
    pages: z.string(),
    sizeText: z.string(),
    ndc: z.string(),
    language: z.string(),
    description: z.string(),
    coverHash: z.string().nullable(),
    locationId: z.string().nullable(),
    tags: z.array(z.string()),
    ownership: z.enum(["owned", "wanted", "released"]),
    wishPriority: z.enum(["high", "mid", "low"]).nullable(),
    readStatus: z.enum(["unread", "reading", "read", "paused"]),
    readStartedOn: z.string(),
    readFinishedOn: z.string(),
    rating: z.number().int().min(1).max(5).nullable(),
    review: z.string(),
    purchasedOn: z.string(),
    purchaseStore: z.string(),
    purchasePrice: z.string(),
    purchaseCond: z.enum(["new", "used"]).nullable(),
    purchaseMemo: z.string(),
    memo: z.string(),
  })
  .superRefine((values, ctx) => {
    const fail = (path: (string | number)[], message: string) =>
      ctx.addIssue({ code: "custom", path, message });

    const isbn = opValue("isbn13", values);
    if (typeof isbn === "string" && !isValidIsbn13(isbn)) fail(["isbn13"], ja.edit.errors.isbn);
    if (isbn === null && values.title.trim() === "") fail(["title"], ja.edit.errors.titleRequired);

    const cCode = opValue("cCode", values);
    if (typeof cCode === "string" && !/^\d{4}$/.test(cCode)) fail(["cCode"], ja.edit.errors.cCode);

    for (const key of ["publishedOn", "readStartedOn", "readFinishedOn", "purchasedOn"] as const) {
      const date = opValue(key, values);
      if (typeof date === "string" && !isValidPartialDate(date)) fail([key], ja.edit.errors.date);
    }

    for (const key of ["listPrice", "pages", "purchasePrice"] as const) {
      const parsed = parseIntegerInput(values[key]);
      if (typeof parsed === "string" || (typeof parsed === "number" && parsed > MAX_INTEGER)) {
        fail([key], ja.edit.errors.integer);
      }
    }

    values.authors.forEach((author, index) => {
      const filled = author.kana.trim() !== "" || author.role.trim() !== "";
      if (author.name.trim() === "" && filled)
        fail(["authors", index, "name"], ja.edit.errors.authorName);
    });
  });

/** 検証して、エラーを項目（`authors.0.name` のようなパス）→ 文言の形で返す。エラーがなければ空。 */
export function validateBookForm(values: BookFormValues): Record<string, string> {
  const result = bookFormSchema.safeParse(values);
  if (result.success) return {};
  const errors: Record<string, string> = {};
  for (const issue of result.error.issues) {
    const path = issue.path.join(".");
    errors[path] ??= issue.message;
  }
  return errors;
}

/** サーバーの検証エラー（`入力が不正です（rating）: …`）の、項目と文言。 */
export interface ServerFieldError {
  /** フォームの項目。フォームにない項目なら null。 */
  key: FormKey | null;
  /** Op 上の項目名。 */
  field: string;
  message: string;
}

const SERVER_ERROR = /^入力が不正です（([^）]*)）:\s*([\s\S]*)$/;

/** `validation` エラーの message から、項目と文言を取り出す。形が違えば null。 */
export function parseServerValidation(message: string): ServerFieldError | null {
  const match = SERVER_ERROR.exec(message);
  if (!match) return null;
  const field = match[1] ?? "";
  const def = fieldDefByOp(field);
  return { key: def?.key ?? null, field, message: match[2] ?? message };
}
