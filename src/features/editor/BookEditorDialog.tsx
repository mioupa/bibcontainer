// 登録・編集フォームのダイアログ。データ（本の詳細、シリーズ、保管場所、タグ、購入店、取得元の候補）を読み込んで、
// 揃ったらフォーム（BookEditorForm）を出す。閉じる操作（Esc、キャンセル）で、変更があれば破棄の確認を出す。
import { useQuery } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { BookEditorForm } from "@/features/editor/BookEditorForm";
import { buildDraftBase, valuesFromBase, type DraftBase } from "@/features/editor/draftBase";
import { valuesFromDetail, type BookFormValues } from "@/features/editor/formValues";
import { originSourceName } from "@/features/editor/sources";
import { libraryApi, usingMockData } from "@/features/library/api";
import {
  useBooks,
  useCCodeTable,
  useLocations,
  usePriceSettings,
  usePurchaseStores,
  useSeries,
  useTags,
} from "@/features/library/hooks";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { isValidIsbn13 } from "@/lib/isbn";
import { useModalGuard } from "@/lib/useModalGuard";
import type { EditorRequest } from "@/stores/editorStore";
import type { BookDetail } from "@/types/contracts";

interface BookEditorDialogProps {
  request: EditorRequest;
  onClose: () => void;
}

export function BookEditorDialog({ request, onClose }: BookEditorDialogProps) {
  const dirtyRef = useRef(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  useModalGuard(true);

  const requestClose = () => {
    if (dirtyRef.current) setConfirmDiscard(true);
    else onClose();
  };

  const mode = request.mode;
  const title = ja.edit.titles[mode];

  return (
    <>
      <Dialog open onOpenChange={(open) => !open && requestClose()}>
        <DialogContent
          // 外側のクリック（通知の操作を含む）では閉じない。閉じるのは Esc・×・キャンセル
          onInteractOutside={(event) => event.preventDefault()}
          className="flex h-[min(46rem,92vh)] w-full flex-col gap-3 sm:max-w-5xl"
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{ja.edit.descriptions[mode]}</DialogDescription>
          </DialogHeader>
          <EditorBody
            request={request}
            dirtyRef={dirtyRef}
            onClose={onClose}
            onCancel={requestClose}
          />
        </DialogContent>
      </Dialog>

      <Dialog open={confirmDiscard} onOpenChange={setConfirmDiscard}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{ja.edit.discard.title}</DialogTitle>
            <DialogDescription>{ja.edit.discard.description}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDiscard(false)}>
              {ja.edit.discard.keep}
            </Button>
            <Button variant="destructive" onClick={onClose}>
              {ja.edit.discard.discard}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

interface EditorBodyProps {
  request: EditorRequest;
  dirtyRef: { current: boolean };
  onClose: () => void;
  onCancel: () => void;
}

/** データを読み込み、揃ったらフォームを出す。edit は本の詳細を待つ。new・draft は元のデータがすでにある。 */
function EditorBody({ request, dirtyRef, onClose, onCancel }: EditorBodyProps) {
  const bookId = request.mode === "edit" ? request.bookId : null;

  // 編集のための詳細は、開くたびに最新を読み、あとから変わっても差し替えない（入力中の値を壊さないため）。
  // 詳細ペインのキャッシュ（book_detail）とは別のキーにして、保存したいときの古い値を避ける
  const detail = useQuery({
    queryKey: ["book_detail_for_edit", bookId],
    queryFn: () => libraryApi.getBookDetail(bookId ?? ""),
    enabled: bookId !== null,
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  const series = useSeries();

  if (detail.isError) {
    return (
      <div className="flex flex-1 flex-col items-start gap-3 text-sm">
        <p role="alert" className="text-destructive">
          {errorMessage(detail.error)}
        </p>
        <Button variant="outline" onClick={onCancel}>
          {ja.edit.actions.close}
        </Button>
      </div>
    );
  }
  if (request.mode === "edit" && (!detail.data || series.isPending)) {
    return <p className="text-sm text-muted-foreground">{ja.edit.loading}</p>;
  }
  return (
    <LoadedEditor
      request={request}
      detail={detail.data ?? null}
      dirtyRef={dirtyRef}
      onClose={onClose}
      onCancel={onCancel}
    />
  );
}

interface Frozen {
  initial: BookFormValues;
  initialSources: Record<string, string | null>;
  draftBase: DraftBase | null;
  coverId: string;
}

/** フォームを開いた時点の値。あとからデータが変わっても、入力中のフォームは差し替えない。 */
function freeze(
  request: EditorRequest,
  detail: BookDetail | null,
  seriesName: string | null,
): Frozen {
  if (request.mode === "edit") {
    if (detail === null) throw new Error("本の詳細がありません");
    return {
      initial: valuesFromDetail(detail, seriesName),
      initialSources: detail.fieldSources,
      draftBase: null,
      coverId: detail.id,
    };
  }
  const base = buildDraftBase(request.source);
  const sources: Record<string, string | null> = {};
  for (const [op, field] of base.fields) sources[op] = originSourceName(field.origin);
  if (base.seriesName) sources.series_id = originSourceName(base.seriesName.origin);
  return {
    initial: valuesFromBase(base),
    initialSources: sources,
    draftBase: base,
    coverId: "new-book",
  };
}

interface LoadedEditorProps extends Omit<EditorBodyProps, never> {
  detail: BookDetail | null;
}

function LoadedEditor({ request, detail, dirtyRef, onClose, onCancel }: LoadedEditorProps) {
  const series = useSeries();
  const books = useBooks();
  const locations = useLocations();
  const tags = useTags();
  const stores = usePurchaseStores();
  const cCodes = useCCodeTable();
  const price = usePriceSettings();

  const [frozen] = useState(() => {
    const seriesName =
      detail === null || detail.seriesId === null
        ? null
        : (series.data?.find((item) => item.id === detail.seriesId)?.name ??
          books.data?.find((row) => row.id === detail.id)?.seriesName ??
          null);
    return freeze(request, detail, seriesName);
  });

  // edit: ISBN があれば、取得元の候補を取る（キャッシュが効くので、通信は通常起きない）。取れなくても編集はできる
  const detailIsbn = detail?.isbn13 ?? null;
  const candidateIsbn = detailIsbn !== null && isValidIsbn13(detailIsbn) ? detailIsbn : null;
  const candidates = useQuery({
    queryKey: ["fetch_candidates", candidateIsbn],
    queryFn: () => libraryApi.fetchMetadata(candidateIsbn ?? ""),
    enabled: request.mode === "edit" && candidateIsbn !== null && !usingMockData(),
    staleTime: 10 * 60_000,
    retry: false,
  });
  const merged =
    request.mode === "edit"
      ? (candidates.data?.merged ?? null)
      : (request.source.fetched?.merged ?? null);

  const suggestions = useMemo(
    () => ({
      tags: (tags.data ?? []).map((tag) => tag.name),
      series: (series.data ?? []).map((item) => item.name),
      stores: (stores.data ?? []).map((store) => store.name),
    }),
    [tags.data, series.data, stores.data],
  );

  return (
    <BookEditorForm
      request={request}
      initial={frozen.initial}
      initialSources={frozen.initialSources}
      draftBase={frozen.draftBase}
      merged={merged}
      coverId={frozen.coverId}
      cCodes={cCodes}
      price={price}
      locations={locations.data ?? []}
      suggestions={suggestions}
      dirtyRef={dirtyRef}
      onClose={onClose}
      onCancel={onCancel}
    />
  );
}
