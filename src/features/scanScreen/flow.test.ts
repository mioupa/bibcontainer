import { describe, expect, it, vi } from "vitest";
import type { IsbnMatch } from "@/bindings/IsbnMatch";
import type { SoundKind } from "@/features/scan/config";
import type { ConfirmedBook } from "@/features/scan/pairing";
import type { LookupOutcome, RowDuplicate } from "@/features/scanScreen/batch";
import { processConfirmed, type FlowDeps } from "@/features/scanScreen/flow";
import type { LastRead, ScanResult } from "@/features/scanScreen/sessionStore";
import { ISBN_A, done } from "@/features/scanScreen/testData";
import { ja } from "@/i18n/ja";
import type { NewBookSource } from "@/stores/editorStore";

const SECOND = { cCode: "0193", listPrice: 700, raw: "1920193007008" };
const book = (secondTier: ConfirmedBook["secondTier"] = null): ConfirmedBook => ({
  isbn13: ISBN_A,
  isbn10: "4003101111",
  secondTier,
});

function match(overrides: Partial<IsbnMatch> = {}): IsbnMatch {
  return {
    id: "b1",
    title: "こころ",
    ownership: "owned",
    locationId: "loc",
    locationPath: "自宅 > 書斎の本棚 > 3段目",
    deleted: false,
    ...overrides,
  };
}

interface Fakes {
  deps: FlowDeps;
  sounds: SoundKind[];
  results: (ScanResult | null)[];
  lasts: LastRead[];
  forms: NewBookSource[];
  rows: { book: ConfirmedBook; duplicate: RowDuplicate | null }[];
  startedLookups: {
    isbn13: string;
    serial: boolean | undefined;
    reuseKnown: boolean | undefined;
  }[];
  updated: string[];
  errors: unknown[];
}

function setup(
  options: {
    matches?: IsbnMatch[] | Error;
    lookup?: LookupOutcome;
    move?: boolean;
    moveFails?: boolean;
    existingRows?: string[];
    active?: boolean;
  } = {},
): Fakes {
  const fakes: Fakes = {
    sounds: [],
    results: [],
    lasts: [],
    forms: [],
    rows: [],
    startedLookups: [],
    updated: [],
    errors: [],
    deps: undefined as unknown as FlowDeps,
  };
  fakes.deps = {
    findByIsbn: vi.fn(() =>
      options.matches instanceof Error
        ? Promise.reject(options.matches)
        : Promise.resolve(options.matches ?? []),
    ),
    startLookup: (isbn13, o) => {
      fakes.startedLookups.push({ isbn13, serial: o?.serial, reuseKnown: o?.reuseKnown });
    },
    awaitLookup: vi.fn(() => Promise.resolve(options.lookup ?? done(ISBN_A, { title: "こころ" }))),
    openNewForm: (source) => {
      fakes.forms.push(source);
      return Promise.resolve();
    },
    confirmMoveToOwned: vi.fn(() => Promise.resolve(options.move ?? true)),
    moveToOwned: (id) => {
      if (options.moveFails) return Promise.reject(new Error("失敗"));
      fakes.updated.push(id);
      return Promise.resolve();
    },
    addRow: (b, duplicate) => {
      fakes.rows.push({ book: b, duplicate });
    },
    hasRow: (isbn13) => (options.existingRows ?? []).includes(isbn13),
    playSound: (kind) => {
      fakes.sounds.push(kind);
    },
    setResult: (r) => {
      fakes.results.push(r);
    },
    setLast: (l) => {
      fakes.lasts.push(l);
    },
    reportError: (e) => {
      fakes.errors.push(e);
    },
    isActive: () => options.active ?? true,
  };
  return fakes;
}

const lastResult = (f: Fakes) => f.results.at(-1);

describe("processConfirmed: 所有済み（FR-SCAN-17、AT-12）", () => {
  it("1 冊ずつ: 注意音を鳴らし、「所有済み: 書名 ／ 保管場所」を出す。フォームは開かない", async () => {
    const f = setup({ matches: [match()] });
    const outcome = await processConfirmed(book(), { mode: "single" }, f.deps);

    expect(outcome).toBe("duplicate");
    expect(f.sounds).toEqual(["attention"]);
    expect(lastResult(f)).toMatchObject({
      level: "warning",
      text: "所有済み: こころ ／ 自宅 > 書斎の本棚 > 3段目",
    });
    // 「2 冊目として登録」で使う情報を持つ
    expect(lastResult(f)?.duplicate?.match.id).toBe("b1");
    expect(f.forms).toEqual([]);
    expect(f.rows).toEqual([]);
  });

  it("保管場所がなければ「保管場所なし」", async () => {
    const f = setup({ matches: [match({ locationId: null, locationPath: null })] });
    await processConfirmed(book(), { mode: "single" }, f.deps);
    expect(lastResult(f)?.text).toBe("所有済み: こころ ／ 保管場所なし");
  });

  it("連続: 行の状態を「重複」にして足す（既定では保存の対象外は batch 側）", async () => {
    const f = setup({ matches: [match()] });
    await processConfirmed(book(SECOND), { mode: "batch" }, f.deps);

    expect(f.sounds).toEqual(["attention"]);
    expect(f.rows).toEqual([
      {
        book: book(SECOND),
        duplicate: { kind: "owned", title: "こころ", locationPath: "自宅 > 書斎の本棚 > 3段目" },
      },
    ]);
    expect(f.startedLookups).toEqual([]);
    expect(lastResult(f)?.level).toBe("warning");
    expect(lastResult(f)?.duplicate).toBeUndefined();
  });

  it("ゴミ箱にある本だけなら、重複にならず新しい本として登録する", async () => {
    const f = setup({ matches: [match({ deleted: true })] });
    expect(await processConfirmed(book(), { mode: "single" }, f.deps)).toBe("form");
    expect(f.forms).toHaveLength(1);
  });
});

describe("processConfirmed: 欲しい本（FR-SCAN-18、AT-13）", () => {
  const wanted = () => match({ ownership: "wanted", id: "w1", title: "欲しかった本" });

  it("確認して移すなら、所有状態を owned にする。新しい本は作らない", async () => {
    const f = setup({ matches: [wanted()], move: true });
    const outcome = await processConfirmed(book(), { mode: "single" }, f.deps);

    expect(outcome).toBe("moved");
    expect(f.deps.confirmMoveToOwned).toHaveBeenCalledWith(expect.objectContaining({ id: "w1" }));
    expect(f.updated).toEqual(["w1"]);
    expect(f.sounds).toEqual(["attention", "success"]);
    expect(lastResult(f)).toMatchObject({
      level: "success",
      text: "所有に移しました: 欲しかった本",
    });
    expect(f.forms).toEqual([]);
    expect(f.rows).toEqual([]);
  });

  it("移さないなら、何も変えず、欲しい本のままにする", async () => {
    const f = setup({ matches: [wanted()], move: false });
    const outcome = await processConfirmed(book(), { mode: "batch" }, f.deps);

    expect(outcome).toBe("kept");
    expect(f.updated).toEqual([]);
    expect(f.rows).toEqual([]);
    expect(f.forms).toEqual([]);
    expect(lastResult(f)?.text).toBe("欲しい本のままにしました: 欲しかった本");
  });

  it("移すのに失敗したら、エラー音とエラーを出す", async () => {
    const f = setup({ matches: [wanted()], move: true, moveFails: true });
    const outcome = await processConfirmed(book(), { mode: "single" }, f.deps);

    expect(outcome).toBe("failed");
    expect(f.sounds).toEqual(["attention", "error"]);
    expect(f.errors).toHaveLength(1);
    expect(lastResult(f)?.level).toBe("error");
  });

  it("画面を離れたあとは、確認を出さない", async () => {
    const f = setup({ matches: [wanted()], active: false });
    expect(await processConfirmed(book(), { mode: "single" }, f.deps)).toBe("skipped");
    expect(f.deps.confirmMoveToOwned).not.toHaveBeenCalled();
  });
});

describe("processConfirmed: 新しい本", () => {
  it("1 冊ずつ: 取得の完了を待って、ISBN・2段目・取得結果を渡してフォームを開く（FR-SCAN-13）", async () => {
    const f = setup({ lookup: done(ISBN_A, { title: "こころ", cCode: "0093", price: 600 }) });
    const outcome = await processConfirmed(book(SECOND), { mode: "single" }, f.deps);

    expect(outcome).toBe("form");
    expect(f.deps.awaitLookup).toHaveBeenCalledWith(ISBN_A);
    expect(f.forms).toHaveLength(1);
    const source = f.forms[0]!;
    expect(source.isbn13).toBe(ISBN_A);
    expect(source.secondTier).toEqual({ cCode: "0193", listPrice: 700 });
    // 取得結果の下書きには、2段目の値が由来 barcode で入っている
    expect(source.fetched?.draft.fields.c_code).toEqual({
      value: "0193",
      origin: { kind: "barcode" },
    });
    expect(source.fetched?.draft.fields.list_price?.value).toBe(700);
    expect(source.fetched?.draft.fields.title?.value).toBe("こころ");
  });

  it("2段目がなければ secondTier は渡さない", async () => {
    const f = setup();
    await processConfirmed(book(), { mode: "single" }, f.deps);
    expect(f.forms[0]?.secondTier).toBeUndefined();
  });

  it("取得に失敗しても、ISBN と 2段目でフォームを開く（fetched は null。FR-SCAN-16）", async () => {
    const f = setup({ lookup: { status: "failed", message: "x" } });
    await processConfirmed(book(SECOND), { mode: "single" }, f.deps);
    expect(f.forms[0]?.fetched).toBeNull();
    expect(f.forms[0]?.isbn13).toBe(ISBN_A);
  });

  it("オフラインの結果（取得待ちの下書き）は、そのまま渡す（フォームが取得待ちで保存する）", async () => {
    const f = setup({ lookup: done(ISBN_A, { offline: true }) });
    await processConfirmed(book(), { mode: "single" }, f.deps);
    expect(f.forms[0]?.fetched?.offline).toBe(true);
    expect(f.forms[0]?.fetched?.draft.fields.meta_status.value).toBe("pending");
  });

  it("連続: 一覧に行を足し、取得を始める（フォームは開かない）", async () => {
    const f = setup();
    const outcome = await processConfirmed(book(SECOND), { mode: "batch" }, f.deps);

    expect(outcome).toBe("added");
    expect(f.rows).toEqual([{ book: book(SECOND), duplicate: null }]);
    // 読んだときに始めた取得の結果を使う（オフラインでも取り直さない。取り直すのは、次に読んだとき）
    expect(f.startedLookups).toEqual([{ isbn13: ISBN_A, serial: undefined, reuseKnown: true }]);
    expect(f.forms).toEqual([]);
    expect(f.deps.awaitLookup).not.toHaveBeenCalled();
    expect(lastResult(f)?.level).toBe("success");
  });

  it("連続: 一覧にすでにある ISBN は、重複の行として足す（既定では保存しない）。注意音", async () => {
    const f = setup({ existingRows: [ISBN_A] });
    await processConfirmed(book(), { mode: "batch" }, f.deps);
    expect(f.rows[0]?.duplicate).toEqual({ kind: "inBatch" });
    expect(f.sounds).toEqual(["attention"]);
    expect(f.startedLookups).toEqual([]);
  });

  it("ISBN 一覧の取り込み（quiet・serialLookup）: 音と直前の表示を出さず、取得は順に行う", async () => {
    const f = setup();
    await processConfirmed(book(), { mode: "batch", quiet: true, serialLookup: true }, f.deps);
    expect(f.sounds).toEqual([]);
    expect(f.results).toEqual([]);
    expect(f.lasts).toEqual([]);
    // 読んだときの取得がないので、オフラインで取得待ちだった結果は取り直す
    expect(f.startedLookups).toEqual([{ isbn13: ISBN_A, serial: true, reuseKnown: false }]);
  });
});

describe("processConfirmed: そのほか", () => {
  it("直前に読んだ本として、ISBN と 2段目を表示に渡す", async () => {
    const f = setup();
    await processConfirmed(book(SECOND), { mode: "batch" }, f.deps);
    expect(f.lasts[0]).toEqual({
      isbn13: ISBN_A,
      secondTier: { cCode: "0193", listPrice: 700 },
    });
  });

  it("重複の確認（find_by_isbn）が失敗しても、登録は続ける", async () => {
    const f = setup({ matches: new Error("DB") });
    const outcome = await processConfirmed(book(), { mode: "batch" }, f.deps);
    expect(outcome).toBe("added");
    expect(f.errors).toHaveLength(1);
  });

  it("文言: 2 冊目として登録のボタン名がある", () => {
    expect(ja.scan.result.registerSecond).toBe("2 冊目として登録");
  });
});
