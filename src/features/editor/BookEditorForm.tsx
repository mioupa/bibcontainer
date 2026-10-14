// 登録・編集フォームの本体（9.2）。タブ、検証、保存。
//
// 値は react-hook-form が持つ（検証は schema.ts の zod）。保存で何を送るかは changes.ts の純粋関数が決める:
//   edit  … 変わった項目だけを `update_book`（手で直した項目は origin: manual）
//   new   … 下書き・先に入れる値・フォームの内容から `NewBookDraft` を作り `create_books_from_drafts`
//   draft … 保存せず、`onDone(NewBookDraft)` を呼んで閉じる（連続登録の一覧の 1 行を直す）
import { zodResolver } from "@hookform/resolvers/zod";
import { useQueryClient } from "@tanstack/react-query";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { FormProvider, useForm, useWatch, type FieldErrors } from "react-hook-form";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { FetchMetadataResult } from "@/bindings/FetchMetadataResult";
import type { MetaMerged } from "@/bindings/MetaMerged";
import type { PriceSettings } from "@/bindings/PriceSettings";
import { buildNewBookDraft, buildEditPlan } from "@/features/editor/changes";
import type { FieldCandidate } from "@/features/editor/candidates";
import {
  buildDraftBase,
  fetchStateOf,
  valuesFromBase,
  type DraftBase,
} from "@/features/editor/draftBase";
import { DuplicatePanel } from "@/features/editor/DuplicatePanel";
import { EditorContext, type EditorContextValue } from "@/features/editor/editorContext";
import {
  EDITOR_TABS,
  FIELDS,
  fieldDef,
  isFieldChanged,
  parseIsbnInput,
  type BookFormValues,
  type EditorTab,
  type FormKey,
} from "@/features/editor/formValues";
import { bookFormSchema, parseServerValidation } from "@/features/editor/schema";
import { saveEdit, saveNew } from "@/features/editor/save";
import { originSourceName, type Picks } from "@/features/editor/sources";
import {
  BasicTab,
  ClassificationTab,
  MemoTab,
  PurchaseTab,
  ReadingTab,
} from "@/features/editor/tabs";
import { useCoverDrop } from "@/features/editor/useCoverDrop";
import { useCoverImport } from "@/features/editor/useCoverImport";
import { libraryApi, usingMockData } from "@/features/library/api";
import { applyLibraryChange } from "@/features/library/sync";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { IpcError } from "@/lib/ipc";
import { toIsbn13 } from "@/lib/isbn";
import { cn } from "@/lib/utils";
import type { EditorRequest } from "@/stores/editorStore";
import { useSelectionStore } from "@/stores/selectionStore";
import type { CCodeTable, Location } from "@/types/contracts";

export interface BookEditorFormProps {
  request: EditorRequest;
  /** フォームを開いたときの値。 */
  initial: BookFormValues;
  /** Op 上の項目名 → 元の取得元。 */
  initialSources: Record<string, string | null>;
  /** new・draft のときの元のデータ。 */
  draftBase: DraftBase | null;
  merged: MetaMerged | null;
  /** 仮の表紙の色に使う ID。 */
  coverId: string;
  cCodes: CCodeTable;
  price: PriceSettings;
  locations: readonly Location[];
  suggestions: { tags: string[]; series: string[]; stores: string[] };
  /** 変更があるか（閉じる前の確認に使う）。 */
  dirtyRef: { current: boolean };
  /** 保存したあと・draft の OK のあとに、確認なしで閉じる。 */
  onClose: () => void;
  /** 「キャンセル」。変更があれば、破棄の確認を出す。 */
  onCancel: () => void;
}

/** 著者の欄が空のままにならないよう、空の 1 行を足す（空の行は保存のとき無視される）。 */
function withBlankAuthor(values: BookFormValues): BookFormValues {
  return values.authors.length > 0
    ? values
    : { ...values, authors: [{ name: "", kana: "", role: "" }] };
}

/** エラーのある項目のうち、画面の順で最初のものと、エラーのあるタブの集合。 */
function errorLocations(errors: FieldErrors<BookFormValues>) {
  const keys = FIELDS.map((def) => def.key).filter((key) => errors[key] !== undefined);
  return { first: keys[0], tabs: new Set<EditorTab>(keys.map((key) => fieldDef(key).tab)) };
}

export function BookEditorForm(props: BookEditorFormProps) {
  const { request, dirtyRef, onClose, onCancel } = props;
  const mode = request.mode;
  const client = useQueryClient();

  const form = useForm<BookFormValues>({
    resolver: zodResolver(bookFormSchema),
    defaultValues: withBlankAuthor(props.initial),
    mode: "onBlur",
  });
  const { formState } = form;
  const isbnText = useWatch({ control: form.control, name: "isbn13" });

  const [tab, setTab] = useState<EditorTab>("basic");
  const [picks, setPicks] = useState<Picks>({});
  const [saving, setSaving] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [hasDuplicate, setHasDuplicate] = useState(false);
  const [confirmedDuplicate, setConfirmedDuplicate] = useState(false);
  const [fetching, setFetching] = useState(false);

  // 新しい本は、書誌を取得し直すと（ISBN の横のボタン）、元のデータが差し替わる
  const [basis, setBasis] = useState(() => ({
    initial: props.initial,
    sources: props.initialSources,
    base: props.draftBase,
    fetchState: request.mode === "edit" ? "none" : fetchStateOf(request.source),
  }));
  // フォームの中で取得し直した統合結果。なければ、開いたときの候補（edit は、あとから届くことがある）
  const [fetchedMerged, setFetchedMerged] = useState<MetaMerged | null>(null);
  const merged = fetchedMerged ?? props.merged;

  useEffect(() => {
    dirtyRef.current = formState.isDirty;
  }, [dirtyRef, formState.isDirty]);

  // 開いたら、すぐ書名を入力できるようにする
  const { setFocus } = form;
  useEffect(() => {
    setFocus("title");
  }, [setFocus]);

  // ---- 書影の取り込み。ドロップ（Tauri のウィンドウ）と貼り付けは、どのタブでも受ける ----
  const importer = useCoverImport(
    useCallback(
      (hash: string) =>
        form.setValue("coverHash", hash, { shouldDirty: true, shouldValidate: true }),
      [form],
    ),
  );
  const dragging = useCoverDrop((paths) => void importer.fromPaths(paths));

  const onPaste = (event: React.ClipboardEvent) => {
    const data = event.clipboardData;
    // 文字の貼り付け（欄への入力）は邪魔しない。画像だけのときに書影にする
    if (data.types.includes("text/plain") || data.files.length === 0) return;
    event.preventDefault();
    void importer.fromFiles(data.files);
  };

  // ---- 取得元の候補 ----
  const pickCandidate = useCallback(
    (key: FormKey, candidate: FieldCandidate) => {
      form.setValue(key, candidate.formValue, { shouldDirty: true, shouldValidate: true });
      setPicks((current) => ({
        ...current,
        [key]: { source: candidate.source, value: candidate.opValue },
      }));
    },
    [form],
  );

  const context = useMemo<EditorContextValue>(
    () => ({
      mode,
      initial: basis.initial,
      initialSources: basis.sources,
      picks,
      pickCandidate,
      merged,
      cCodes: props.cCodes,
      price: props.price,
      locations: props.locations,
      tagSuggestions: props.suggestions.tags,
      seriesSuggestions: props.suggestions.series,
      storeSuggestions: props.suggestions.stores,
      cover: { importer, dragging },
      coverId: props.coverId,
    }),
    [
      mode,
      basis,
      merged,
      picks,
      pickCandidate,
      props.cCodes,
      props.price,
      props.locations,
      props.suggestions,
      props.coverId,
      importer,
      dragging,
    ],
  );

  // ---- 書誌を取得して、まだ直していない項目に入れる（new のみ） ----
  const fetchMetadata = async () => {
    const isbn = toIsbn13(form.getValues("isbn13"));
    if (isbn === null || request.mode === "edit") {
      toast.error(ja.edit.errors.isbn);
      return;
    }
    setFetching(true);
    try {
      const fetched: FetchMetadataResult = await libraryApi.fetchMetadata(isbn);
      const base = buildDraftBase({
        ...request.source,
        isbn13: isbn,
        fetched,
        // 取得した値を土台に、先に入れておく値・下書きが重なる
      });
      const next = valuesFromBase(base);
      const current = form.getValues();
      for (const def of FIELDS) {
        if (def.key === "isbn13") continue;
        // 直していない項目（空のまま、または前の取得のまま）だけを、新しい値にする
        if (isFieldChanged(def.key, basis.initial, current)) continue;
        if (!isFieldChanged(def.key, basis.initial, next)) continue;
        form.setValue(def.key, next[def.key], { shouldDirty: true, shouldValidate: true });
      }
      const sources: Record<string, string | null> = {};
      for (const [op, field] of base.fields) sources[op] = originSourceName(field.origin);
      if (base.seriesName) sources.series_id = originSourceName(base.seriesName.origin);
      setBasis({
        initial: next,
        sources,
        base,
        fetchState: fetchStateOf({ fetched }),
      });
      setFetchedMerged(fetched.merged);
      setPicks({});
      toast.success(fetched.offline ? ja.edit.fetchOffline : ja.edit.fetched);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setFetching(false);
    }
  };

  // ---- 保存 ----
  const handleSaveError = (error: unknown) => {
    if (error instanceof IpcError && error.kind === "validation") {
      const parsed = parseServerValidation(error.message);
      if (parsed?.key) {
        form.setError(parsed.key, { type: "server", message: parsed.message });
        setTab(fieldDef(parsed.key).tab);
        focusFirstError();
        return;
      }
      setServerError(parsed?.message ?? error.message);
      return;
    }
    setServerError(errorMessage(error));
  };

  const submit = async (values: BookFormValues) => {
    setServerError(null);
    setSaving(true);
    try {
      if (request.mode === "edit") {
        const plan = buildEditPlan(basis.initial, values, picks);
        const report = await saveEdit(request.bookId, plan);
        if (report === null) {
          toast.info(ja.edit.toast.noChanges);
        } else {
          if (usingMockData()) {
            await applyLibraryChange(client, { books: [request.bookId], others: true });
          }
          toast.success(ja.edit.toast.saved);
        }
        onClose();
        return;
      }
      const draft = buildNewBookDraft({
        base: basis.base ?? buildDraftBase({}),
        initial: basis.initial,
        current: values,
        picks,
        fetchState: basis.fetchState,
      });
      if (request.mode === "draft") {
        request.onDone(draft);
        onClose();
        return;
      }
      const id = await saveNew(draft);
      if (usingMockData()) await applyLibraryChange(client, { books: [id], others: true });
      useSelectionStore.setState({ selected: new Set([id]), anchor: id, focus: id });
      toast.success(ja.edit.toast.created(values.title.trim()));
      onClose();
    } catch (error) {
      handleSaveError(error);
    } finally {
      setSaving(false);
    }
  };

  // エラーのある項目があるタブを開き、最初のエラーの欄にフォーカスする
  // （隠れているタブの欄には、react-hook-form はフォーカスできないので、タブを替えてから自分で動かす）
  const focusFirstError = () => {
    requestAnimationFrame(() => {
      document.querySelector<HTMLElement>('[role="dialog"] [aria-invalid="true"]')?.focus();
    });
  };
  const onInvalid = (errors: FieldErrors<BookFormValues>) => {
    const { first } = errorLocations(errors);
    if (first === undefined) return;
    setTab(fieldDef(first).tab);
    focusFirstError();
  };

  const onSubmit = form.handleSubmit(submit, onInvalid);

  const onFormKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void onSubmit();
      return;
    }
    // 一行の入力欄の Enter で、意図せず保存しない（保存は、ボタンか ⌘Enter・Ctrl+Enter）
    if (event.key === "Enter" && event.target instanceof HTMLInputElement) event.preventDefault();
  };

  const { tabs: invalidTabs } = errorLocations(formState.errors);
  const isbnPresent = parseIsbnInput(isbnText) !== null;
  const canFetch = request.mode === "new" || request.mode === "draft";
  const duplicateBlocks = mode === "new" && hasDuplicate && !confirmedDuplicate;
  const submitLabel =
    mode === "draft"
      ? ja.edit.actions.apply
      : mode === "new"
        ? ja.edit.actions.create
        : ja.edit.actions.save;

  return (
    <EditorContext.Provider value={context}>
      <FormProvider {...form}>
        <form
          onSubmit={(event: FormEvent<HTMLFormElement>) => void onSubmit(event)}
          onKeyDown={onFormKeyDown}
          onPaste={onPaste}
          noValidate
          className="flex min-h-0 flex-1 flex-col gap-3"
          aria-label={ja.edit.formLabel}
        >
          {mode === "new" && (
            <DuplicatePanel
              searchSimilar={basis.fetchState === "none" && !isbnPresent}
              confirmed={confirmedDuplicate}
              onConfirmedChange={setConfirmedDuplicate}
              onDuplicateChange={setHasDuplicate}
            />
          )}

          <Tabs
            value={tab}
            onValueChange={(value) => setTab(value as EditorTab)}
            className="min-h-0 flex-1"
          >
            <TabsList className="w-full">
              {EDITOR_TABS.map((id) => (
                <TabsTrigger key={id} value={id}>
                  {ja.edit.tabs[id]}
                  {invalidTabs.has(id) && (
                    <span
                      className="size-1.5 rounded-full bg-destructive"
                      role="img"
                      aria-label={ja.edit.tabHasError}
                    />
                  )}
                </TabsTrigger>
              ))}
            </TabsList>
            {/* 非表示のタブも描いておく（エラーの項目にフォーカスできるように） */}
            {EDITOR_TABS.map((id) => (
              <TabsContent
                key={id}
                value={id}
                forceMount
                className={cn(
                  "min-h-0 overflow-y-auto pr-2 data-[state=inactive]:hidden",
                  "flex-1",
                )}
              >
                {id === "basic" && (
                  <BasicTab
                    onFetch={canFetch ? () => void fetchMetadata() : null}
                    fetching={fetching}
                  />
                )}
                {id === "classification" && <ClassificationTab />}
                {id === "reading" && <ReadingTab />}
                {id === "purchase" && <PurchaseTab />}
                {id === "memo" && <MemoTab />}
              </TabsContent>
            ))}
          </Tabs>

          <DialogFooter className="items-center sm:justify-between">
            <p role="alert" className="min-w-0 text-xs break-words text-destructive">
              {serverError}
            </p>
            <div className="flex shrink-0 items-center gap-2">
              <Button type="button" variant="outline" onClick={onCancel} disabled={saving}>
                {ja.edit.actions.cancel}
              </Button>
              <Button
                type="submit"
                disabled={saving || duplicateBlocks}
                title={duplicateBlocks ? ja.edit.duplicate.blocked : undefined}
              >
                {saving ? ja.edit.actions.saving : submitLabel}
              </Button>
            </div>
          </DialogFooter>
        </form>
      </FormProvider>
    </EditorContext.Provider>
  );
}
