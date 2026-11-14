// CSV 取り込みのダイアログ（0028）。ipc と dialog を差し替え、列の対応 → 確認 → 取り込み → 結果の流れを確かめる。
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CsvImportPlan } from "@/bindings/CsvImportPlan";
import type { CsvImportResult } from "@/bindings/CsvImportResult";
import type { CsvPreview } from "@/bindings/CsvPreview";
import { ja } from "@/i18n/ja";
import type * as ipcModule from "@/lib/ipc";
import { IpcError } from "@/lib/ipc";
import { useModalStore } from "@/lib/useModalGuard";
import { CsvImportDialog } from "./CsvImportDialog";

const ipc = vi.hoisted(() => ({
  csvImportOpen: vi.fn(),
  csvImportCheck: vi.fn(),
  csvImportRun: vi.fn(),
  csvImportSaveRejected: vi.fn(),
}));
vi.mock("@/lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof ipcModule>()),
  ipc,
}));
const chooseSavePath = vi.hoisted(() => vi.fn());
vi.mock("@/lib/dialog", () => ({ chooseSavePath }));
const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn() }));
vi.mock("sonner", () => ({ toast }));

const t = ja.csvImport;

function preview(overrides: Partial<CsvPreview> = {}): CsvPreview {
  return {
    encoding: "utf8",
    autoDetected: true,
    columns: [
      { index: 0, header: "ISBN", samples: ["9784780802047", "9784101010014"] },
      { index: 1, header: "書名", samples: ["おにぎりレシピ101"] },
      { index: 2, header: "メモ欄", samples: [] },
    ],
    rowCount: 12,
    suggested: [
      { column: 0, target: "isbn13" },
      { column: 1, target: "title" },
    ],
    ...overrides,
  };
}

function plan(overrides: Partial<CsvImportPlan> = {}): CsvImportPlan {
  return {
    totalRows: 12,
    toImport: 8,
    toFetch: 6,
    skipped: { registeredIsbn: 2, sameId: 0, duplicateInFile: 1, trashedRow: 0 },
    errors: 1,
    warnedRows: 1,
    issues: [],
    newLocations: ["自宅 > 書斎"],
    newSeries: ["シリーズA"],
    ...overrides,
  };
}

const result: CsvImportResult = {
  imported: 8,
  fetchQueued: 6,
  skipped: 3,
  errors: 1,
  bookIds: [],
  createdLocations: 1,
  createdSeries: 1,
};

const onClose = vi.fn();
const PATH = "/home/u/蔵書.csv";

const targetSelect = (column: string) =>
  screen.getByRole<HTMLSelectElement>("combobox", { name: t.mapping.targetOf(column) });
const nextButton = () => screen.getByRole("button", { name: t.next });

async function openDialog() {
  render(<CsvImportDialog path={PATH} onClose={onClose} />);
  await screen.findByTestId("csv-row-count");
}

async function goToConfirm() {
  await openDialog();
  fireEvent.click(nextButton());
  await screen.findByTestId("csv-summary");
}

beforeEach(() => {
  vi.clearAllMocks();
  useModalStore.setState({ count: 0 });
  ipc.csvImportOpen.mockResolvedValue(preview());
  ipc.csvImportCheck.mockResolvedValue(plan());
  ipc.csvImportRun.mockResolvedValue(result);
  ipc.csvImportSaveRejected.mockResolvedValue(1);
});

describe("列の対応", () => {
  it("開くと CSV を読み、列・見本・推測した対応を出す", async () => {
    await openDialog();
    expect(ipc.csvImportOpen).toHaveBeenCalledWith(PATH, null, true);
    expect(screen.getByTestId("csv-file-name")).toHaveTextContent("蔵書.csv");
    expect(screen.getByTestId("csv-row-count")).toHaveTextContent(t.mapping.rowCount(12));
    expect(screen.getByText("9784780802047")).toBeInTheDocument();
    expect(targetSelect("ISBN").value).toBe("isbn13");
    expect(targetSelect("書名").value).toBe("title");
    expect(targetSelect("メモ欄").value).toBe("");
    expect(nextButton()).toBeEnabled();
    expect(
      screen.getByRole("option", { name: t.mapping.encodingAuto("utf8") }),
    ).toBeInTheDocument();
  });

  it("文字コードを選ぶと、その文字コードで読み直し、推測を取り直す", async () => {
    await openDialog();
    ipc.csvImportOpen.mockResolvedValue(
      preview({
        encoding: "shiftJis",
        autoDetected: false,
        suggested: [{ column: 2, target: "memo" }],
      }),
    );
    fireEvent.change(screen.getByRole("combobox", { name: t.mapping.encoding }), {
      target: { value: "shiftJis" },
    });
    await waitFor(() => expect(ipc.csvImportOpen).toHaveBeenLastCalledWith(PATH, "shiftJis", true));
    await waitFor(() => expect(targetSelect("メモ欄").value).toBe("memo"));
    // 自動の選択肢は、最初に判定した文字コードのまま
    expect(
      screen.getByRole("option", { name: t.mapping.encodingAuto("utf8") }),
    ).toBeInTheDocument();
  });

  it("「1 行目は見出し」を外すと、見出しなしで読み直し「N 列目」と出す", async () => {
    await openDialog();
    ipc.csvImportOpen.mockResolvedValue(
      preview({
        columns: [
          { index: 0, header: null, samples: ["ISBN"] },
          { index: 1, header: null, samples: ["書名"] },
        ],
        suggested: [],
      }),
    );
    fireEvent.click(screen.getByRole("checkbox", { name: t.mapping.hasHeader }));
    await waitFor(() => expect(ipc.csvImportOpen).toHaveBeenLastCalledWith(PATH, null, false));
    expect(await screen.findByText("1 列目")).toBeInTheDocument();
    expect(screen.getByText("2 列目")).toBeInTheDocument();
    expect(nextButton()).toBeDisabled();
    expect(screen.getByTestId("csv-need-target")).toHaveTextContent(t.mapping.needTarget);
  });

  it("ISBN も書名も対応づけていないと、次へは進めない。選べば進める", async () => {
    await openDialog();
    fireEvent.change(targetSelect("ISBN"), { target: { value: "" } });
    fireEvent.change(targetSelect("書名"), { target: { value: "authors" } });
    expect(nextButton()).toBeDisabled();
    fireEvent.change(targetSelect("メモ欄"), { target: { value: "title" } });
    expect(nextButton()).toBeEnabled();
  });

  it("同じ項目を別の列に選ぶと、前の列は「取り込まない」に戻る", async () => {
    await openDialog();
    fireEvent.change(targetSelect("メモ欄"), { target: { value: "title" } });
    expect(targetSelect("書名").value).toBe("");
    expect(targetSelect("メモ欄").value).toBe("title");
  });

  it("読み込みに失敗したら、理由をダイアログに出す", async () => {
    ipc.csvImportOpen.mockRejectedValue(
      new IpcError({ kind: "internal", message: "CSV を読めません" }),
    );
    render(<CsvImportDialog path={PATH} onClose={onClose} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("CSV を読めません");
    expect(nextButton()).toBeDisabled();
  });
});

describe("確認", () => {
  it("件数と、新しく作るものを出し、取り込む冊数をボタンに出す", async () => {
    await goToConfirm();
    expect(ipc.csvImportCheck).toHaveBeenCalledWith(PATH, {
      encoding: null,
      hasHeader: true,
      mapping: [
        { column: 0, target: "isbn13" },
        { column: 1, target: "title" },
      ],
    });
    const summary = screen.getByTestId("csv-summary");
    expect(summary).toHaveTextContent(t.confirm.willImport(8));
    expect(summary).toHaveTextContent(t.confirm.willFetch(6));
    expect(summary).toHaveTextContent(t.confirm.skipped(3));
    expect(summary).toHaveTextContent(t.confirm.skipBreakdown.registeredIsbn(2));
    expect(summary).toHaveTextContent(t.confirm.skipBreakdown.duplicateInFile(1));
    expect(summary).not.toHaveTextContent("同じ ID の本がある:");
    expect(summary).not.toHaveTextContent("ゴミ箱の行:");
    expect(summary).toHaveTextContent(t.confirm.errors(1));
    expect(summary).toHaveTextContent(t.confirm.warned(1));
    expect(screen.getByText("自宅 > 書斎")).toBeInTheDocument();
    expect(screen.getByText("シリーズA")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t.confirm.run(8) })).toBeEnabled();
  });

  it("長い保管場所の一覧は 20 件で切り、「ほか N」と出す", async () => {
    ipc.csvImportCheck.mockResolvedValue(
      plan({ newSeries: Array.from({ length: 25 }, (_, i) => `S${i}`) }),
    );
    await goToConfirm();
    expect(screen.getByText(/ほか 5/)).toBeInTheDocument();
    expect(screen.queryByText(/S24/)).not.toBeInTheDocument();
  });

  it("問題の行は、種類ごとの理由で出し、絞り込める。最初は件数のある最初の絞り込み", async () => {
    ipc.csvImportCheck.mockResolvedValue(
      plan({
        issues: [
          {
            line: 3,
            status: "error",
            isbn13: null,
            title: "壊れた ISBN の本",
            reasons: [{ kind: "invalidIsbn", value: "9.78E+12", excelNumber: true }],
          },
          {
            line: 4,
            status: "error",
            isbn13: null,
            title: null,
            reasons: [{ kind: "noTitleOrIsbn" }],
          },
          {
            line: 5,
            status: "skip",
            isbn13: "9784780802047",
            title: "CSV の書名",
            reasons: [{ kind: "registeredIsbn", existingTitle: "登録済みの書名" }],
          },
          {
            line: 6,
            status: "skip",
            isbn13: "9784101010014",
            title: null,
            reasons: [{ kind: "duplicateInFile", firstLine: 2 }],
          },
          {
            line: 7,
            status: "skip",
            isbn13: null,
            title: "x",
            reasons: [{ kind: "sameId", trashed: true }],
          },
          { line: 8, status: "skip", isbn13: null, title: "y", reasons: [{ kind: "trashedRow" }] },
          {
            line: 9,
            status: "import",
            isbn13: "9784780802047",
            title: "z",
            reasons: [{ kind: "unreadableValue", target: "published_on", value: "昨日" }],
          },
        ],
      }),
    );
    await goToConfirm();
    const table = () => screen.getByTestId("csv-issues");
    // 最初は「取り込めない」
    expect(within(table()).getByText("3 行目")).toBeInTheDocument();
    expect(table()).toHaveTextContent("Excel");
    expect(table()).toHaveTextContent("9.78E+12");
    expect(table()).toHaveTextContent(t.reason.noTitleOrIsbn);
    expect(within(table()).queryByText("5 行目")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: t.confirm.filterCount("飛ばす", 4) }));
    expect(table()).toHaveTextContent("登録済みの書名");
    expect(table()).toHaveTextContent("2 行目と同じ");
    expect(table()).toHaveTextContent(t.reason.sameId(true));
    expect(table()).toHaveTextContent(t.reason.trashedRow);
    expect(table()).toHaveTextContent("9784780802047");

    fireEvent.click(screen.getByRole("button", { name: t.confirm.filterCount("値を読めない", 1) }));
    expect(table()).toHaveTextContent("発行日 を読めません（昨日）");
  });

  it("取り込めない行がなければ、最初は「飛ばす」。500 行までで切り、ほかの件数を出す", async () => {
    ipc.csvImportCheck.mockResolvedValue(
      plan({
        issues: Array.from({ length: 503 }, (_, i) => ({
          line: i + 2,
          status: "skip" as const,
          isbn13: null,
          title: `本${i}`,
          reasons: [{ kind: "trashedRow" as const }],
        })),
      }),
    );
    await goToConfirm();
    expect(within(screen.getByTestId("csv-issues")).getAllByRole("row")).toHaveLength(501);
    expect(screen.getByText(t.confirm.moreRows(3))).toBeInTheDocument();
  });

  it("取り込む本が 0 冊なら、取り込むボタンは押せない", async () => {
    ipc.csvImportCheck.mockResolvedValue(plan({ toImport: 0, toFetch: 0 }));
    await goToConfirm();
    expect(screen.getByRole("button", { name: t.confirm.run(0) })).toBeDisabled();
  });

  it("戻ると、対応づけはそのまま", async () => {
    await openDialog();
    fireEvent.change(targetSelect("メモ欄"), { target: { value: "memo" } });
    fireEvent.click(nextButton());
    await screen.findByTestId("csv-summary");
    fireEvent.click(screen.getByRole("button", { name: t.back }));
    expect(targetSelect("メモ欄").value).toBe("memo");
    expect(targetSelect("ISBN").value).toBe("isbn13");
    expect(ipc.csvImportOpen).toHaveBeenCalledTimes(1);
  });

  it("確認に失敗したら、列の対応に戻って理由を出す", async () => {
    ipc.csvImportCheck.mockRejectedValue(new IpcError({ kind: "internal", message: "壊れた CSV" }));
    await openDialog();
    fireEvent.click(nextButton());
    expect(await screen.findByRole("alert")).toHaveTextContent("壊れた CSV");
    expect(nextButton()).toBeEnabled();
  });
});

describe("取り込みと結果", () => {
  it("取り込むと、確認したときと同じ指定で実行し、結果を出す", async () => {
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    const shown = await screen.findByTestId("csv-result");
    expect(ipc.csvImportRun).toHaveBeenCalledWith(PATH, ipc.csvImportCheck.mock.calls[0]![1]);
    expect(shown).toHaveTextContent(t.result.imported(8));
    expect(shown).toHaveTextContent(t.result.fetchNote(6));
    expect(shown).toHaveTextContent(t.result.skipped(3));
    expect(shown).toHaveTextContent(t.result.errors(1));
    fireEvent.click(screen.getByRole("button", { name: t.close }));
    expect(onClose).toHaveBeenCalled();
  });

  it("実行中は進み具合の文を出し、Esc でも閉じない", async () => {
    let finish: (value: CsvImportResult) => void = () => {};
    ipc.csvImportRun.mockReturnValue(new Promise<CsvImportResult>((resolve) => (finish = resolve)));
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    expect(await screen.findByText(t.running)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await act(async () => {
      finish(result);
      await Promise.resolve();
    });
    expect(await screen.findByTestId("csv-result")).toBeInTheDocument();
  });

  it("実行に失敗したら、確認に戻って理由を出す", async () => {
    ipc.csvImportRun.mockRejectedValue(
      new IpcError({ kind: "database", message: "書き込めません" }),
    );
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    expect(await screen.findByRole("alert")).toHaveTextContent("書き込めません");
    expect(screen.getByRole("button", { name: t.confirm.run(8) })).toBeEnabled();
  });

  it("取り込めなかった行があれば、CSV に保存できる。名前は元のファイル名から付く", async () => {
    chooseSavePath.mockResolvedValue("/out/r.csv");
    ipc.csvImportSaveRejected.mockResolvedValue(1);
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    await screen.findByTestId("csv-result");
    fireEvent.click(screen.getByRole("button", { name: t.result.saveRejected }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(t.result.saved(1)));
    expect(chooseSavePath).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: "蔵書-取り込めなかった行.csv",
        filters: [{ name: t.result.csvFilter, extensions: ["csv"] }],
      }),
    );
    expect(ipc.csvImportSaveRejected).toHaveBeenCalledWith(
      PATH,
      ipc.csvImportRun.mock.calls[0]![1],
      "/out/r.csv",
    );
  });

  it("保存先を選ばなければ、何も書かない。取り込めない行がなければボタンは出ない", async () => {
    chooseSavePath.mockResolvedValue(null);
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    await screen.findByTestId("csv-result");
    fireEvent.click(screen.getByRole("button", { name: t.result.saveRejected }));
    await waitFor(() => expect(chooseSavePath).toHaveBeenCalled());
    expect(ipc.csvImportSaveRejected).not.toHaveBeenCalled();
  });

  it("エラーが 0 行なら、保存のボタンは出ない", async () => {
    ipc.csvImportRun.mockResolvedValue({ ...result, errors: 0 });
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    await screen.findByTestId("csv-result");
    expect(screen.queryByRole("button", { name: t.result.saveRejected })).not.toBeInTheDocument();
  });

  it("保存の IPC が失敗したら、理由を出す", async () => {
    chooseSavePath.mockResolvedValue("/out/r.csv");
    ipc.csvImportSaveRejected.mockRejectedValue(
      new IpcError({ kind: "io", message: "書き込めません（保存）" }),
    );
    await goToConfirm();
    fireEvent.click(screen.getByRole("button", { name: t.confirm.run(8) }));
    await screen.findByTestId("csv-result");
    fireEvent.click(screen.getByRole("button", { name: t.result.saveRejected }));
    expect(await screen.findByRole("alert")).toHaveTextContent("書き込めません（保存）");
  });
});

describe("開閉", () => {
  it("path が null なら何も描かない。Esc で閉じる（実行中でなければ）", async () => {
    const { rerender } = render(<CsvImportDialog path={null} onClose={onClose} />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    rerender(<CsvImportDialog path={PATH} onClose={onClose} />);
    await screen.findByTestId("csv-row-count");
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
