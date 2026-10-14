import { describe, expect, it } from "vitest";
import type { BookDraft } from "@/bindings/BookDraft";
import type { FetchMetadataResult } from "@/bindings/FetchMetadataResult";
import { buildEditPlan, buildNewBookDraft } from "@/features/editor/changes";
import { buildDraftBase, fetchStateOf, valuesFromBase } from "@/features/editor/draftBase";
import {
  emptyFormValues,
  valuesFromDetail,
  type BookFormValues,
} from "@/features/editor/formValues";
import { planToEditRequest } from "@/features/editor/save";
import type { NewBookSource } from "@/stores/editorStore";
import { makeDetail } from "@/test/detail";

const MANUAL = { kind: "manual" } as const;
const openbd = { kind: "provider", name: "openbd" } as const;
const ndl = { kind: "provider", name: "ndl" } as const;

describe("edit: 変わった項目だけの FieldChange（手入力の由来）", () => {
  const detail = makeDetail({
    title: "吾輩は猫である",
    publisher: "岩波書店",
    rating: 3,
    tags: ["小説"],
    isbn13: "9784003101117",
  });
  const initial = valuesFromDetail(detail, null);

  it("何も変えなければ、変更は空（何も送らない）", () => {
    const plan = buildEditPlan(initial, { ...initial });
    expect(plan.changes).toEqual([]);
    expect(plan.series).toBeNull();
    expect(plan.overrideManual).toBe(false);
  });

  it("直した項目だけを、origin: manual で作る。Op 上の項目名（snake_case）で送る", () => {
    const current: BookFormValues = {
      ...initial,
      title: "我輩は猫である",
      publishedOn: "1905",
      rating: 5,
    };
    const plan = buildEditPlan(initial, current);
    expect(plan.changes).toEqual([
      { field: "title", value: "我輩は猫である", origin: MANUAL },
      { field: "published_on", value: "1905", origin: MANUAL },
      { field: "rating", value: 5, origin: MANUAL },
    ]);
    expect(plan.overrideManual).toBe(false);
  });

  it("書式だけの違い（ISBN のハイフン、前後の空白）は送らない", () => {
    const plan = buildEditPlan(initial, {
      ...initial,
      isbn13: "978-4-00-310111-7",
      publisher: " 岩波書店 ",
    });
    expect(plan.changes).toEqual([]);
  });

  it("消した項目は null で送る。タグは正規化した配列", () => {
    const plan = buildEditPlan(initial, {
      ...initial,
      publisher: "",
      rating: null,
      tags: ["小説", "名作"],
    });
    expect(plan.changes).toEqual([
      { field: "publisher", value: null, origin: MANUAL },
      { field: "tags", value: ["小説", "名作"], origin: MANUAL },
      { field: "rating", value: null, origin: MANUAL },
    ]);
  });

  it("シリーズは名前で受け、保存のとき ID にする（外すときは null）", () => {
    const plan = buildEditPlan(initial, { ...initial, seriesName: " 猫シリーズ " });
    expect(plan.changes).toEqual([]);
    expect(plan.series).toEqual({ name: "猫シリーズ", origin: MANUAL });

    const removed = buildEditPlan(valuesFromDetail(detail, "猫シリーズ"), {
      ...valuesFromDetail(detail, "猫シリーズ"),
      seriesName: "",
    });
    expect(removed.series).toEqual({ name: null, origin: MANUAL });
  });

  it("取得元の候補から選んで、そのまま直していない項目は、その取得元の由来にし、手修正の上書きを許す", () => {
    const current = { ...initial, publisher: "NDL 出版" };
    const plan = buildEditPlan(initial, current, {
      publisher: { source: "ndl", value: "NDL 出版" },
    });
    expect(plan.changes).toEqual([{ field: "publisher", value: "NDL 出版", origin: ndl }]);
    expect(plan.overrideManual).toBe(true);
  });

  it("候補を選んだあとで直した項目は、手入力になる", () => {
    const current = { ...initial, publisher: "NDL 出版（改）" };
    const plan = buildEditPlan(initial, current, {
      publisher: { source: "ndl", value: "NDL 出版" },
    });
    expect(plan.changes).toEqual([{ field: "publisher", value: "NDL 出版（改）", origin: MANUAL }]);
    expect(plan.overrideManual).toBe(false);
  });

  it("planToEditRequest: シリーズを find_or_create_series で ID にして series_id を足す", async () => {
    const plan = buildEditPlan(initial, { ...initial, title: "新", seriesName: "猫シリーズ" });
    const resolved: string[] = [];
    const request = await planToEditRequest(plan, (name) => {
      resolved.push(name);
      return Promise.resolve("series-id-1");
    });
    expect(resolved).toEqual(["猫シリーズ"]);
    expect(request?.changes).toEqual([
      { field: "title", value: "新", origin: MANUAL },
      { field: "series_id", value: "series-id-1", origin: MANUAL },
    ]);
    expect(request?.options).toBeUndefined();
  });

  it("planToEditRequest: 変更がなければ null。候補を選んだときは overrideManual を付ける", async () => {
    expect(await planToEditRequest(buildEditPlan(initial, { ...initial }))).toBeNull();
    const plan = buildEditPlan(
      initial,
      { ...initial, publisher: "X" },
      {
        publisher: { source: "openbd", value: "X" },
      },
    );
    expect((await planToEditRequest(plan))?.options).toEqual({ overrideManual: true });
  });
});

// ---- new: NewBookDraft の組み立て ----

function draftWith(fields: BookDraft["fields"], seriesName: BookDraft["seriesName"] = null) {
  return { fields, seriesName } satisfies BookDraft;
}

function fetchedOf(draft: BookDraft, offline = false): FetchMetadataResult {
  return {
    merged: { isbn13: "9784780802047", fields: [] },
    draft,
    reports: [],
    offline,
    coverHash: null,
  };
}

const fetchedDraft = draftWith(
  {
    isbn13: { value: "9784780802047", origin: { kind: "barcode" } },
    title: { value: "おにぎりレシピ101", origin: openbd },
    publisher: { value: "ポット出版", origin: openbd },
    c_code: { value: "0077", origin: { kind: "barcode" } },
    list_price: { value: 1300, origin: { kind: "barcode" } },
    volume: { value: "1", origin: openbd },
    volume_sort: { value: 1, origin: openbd },
    meta_status: { value: "ok", origin: { kind: "provider", name: "bibcontainer" } },
  },
  { name: "おにぎり", origin: openbd },
);

function build(source: NewBookSource, edit: (values: BookFormValues) => BookFormValues = (v) => v) {
  const base = buildDraftBase(source);
  const initial = valuesFromBase(base);
  return buildNewBookDraft({
    base,
    initial,
    current: edit({ ...initial }),
    fetchState: fetchStateOf(source),
  });
}

const changeOf = (draft: ReturnType<typeof build>, field: string) =>
  draft.changes.find((change) => change.field === field);

describe("new: NewBookDraft の組み立て", () => {
  it("取得結果の下書きを、直していなければ、由来ごとそのまま渡す（取得状態も入る）", () => {
    const draft = build({ fetched: fetchedOf(fetchedDraft) });
    expect(changeOf(draft, "title")).toEqual({
      field: "title",
      value: "おにぎりレシピ101",
      origin: openbd,
    });
    expect(changeOf(draft, "c_code")?.origin).toEqual({ kind: "barcode" });
    expect(changeOf(draft, "meta_status")?.value).toBe("ok");
    expect(draft.seriesName).toEqual({ name: "おにぎり", origin: openbd });
  });

  it("優先順位は、フォームで直した値 > 既に直した下書き > 先に入れておく値 > 取得結果", () => {
    const source: NewBookSource = {
      fetched: fetchedOf(fetchedDraft),
      presets: [
        { field: "title", value: "共通のタイトル", origin: MANUAL },
        { field: "publisher", value: "共通の出版社", origin: MANUAL },
        { field: "ownership", value: "wanted", origin: MANUAL },
      ],
      draft: {
        changes: [{ field: "publisher", value: "直した出版社", origin: MANUAL }],
        seriesName: null,
      },
    };
    const untouched = build(source);
    expect(changeOf(untouched, "title")?.value).toBe("共通のタイトル"); // 先に入れておく値 > 取得結果
    expect(changeOf(untouched, "publisher")?.value).toBe("直した出版社"); // 下書き > 先に入れておく値
    expect(changeOf(untouched, "ownership")?.value).toBe("wanted");
    expect(untouched.seriesName).toBeNull(); // 下書きが持つ seriesName が優先

    const edited = build(source, (values) => ({ ...values, publisher: "フォームの出版社" }));
    expect(changeOf(edited, "publisher")).toEqual({
      field: "publisher",
      value: "フォームの出版社",
      origin: MANUAL,
    });
    // 直していない項目の由来は変わらない
    expect(changeOf(edited, "ownership")?.origin).toEqual(MANUAL);
  });

  it("フォームで直した項目だけが manual になる。直していない取得項目は取得元のまま", () => {
    const draft = build({ fetched: fetchedOf(fetchedDraft) }, (values) => ({
      ...values,
      title: "書名を直した",
    }));
    expect(changeOf(draft, "title")).toEqual({
      field: "title",
      value: "書名を直した",
      origin: MANUAL,
    });
    expect(changeOf(draft, "publisher")?.origin).toEqual(openbd);
  });

  it("巻次を直したら、元の volume_sort を捨てる（コアが作り直す）", () => {
    const kept = build({ fetched: fetchedOf(fetchedDraft) });
    expect(changeOf(kept, "volume_sort")?.value).toBe(1);
    const edited = build({ fetched: fetchedOf(fetchedDraft) }, (v) => ({ ...v, volume: "2" }));
    expect(changeOf(edited, "volume")?.value).toBe("2");
    expect(changeOf(edited, "volume_sort")).toBeUndefined();
  });

  it("手動登録（取得なし）: ISBN がなければ取得状態を付けない。ISBN を入れたら取得待ち", () => {
    const manual = build({}, (v) => ({ ...v, title: "手で入れた本" }));
    expect(changeOf(manual, "meta_status")).toBeUndefined();
    expect(changeOf(manual, "title")).toEqual({
      field: "title",
      value: "手で入れた本",
      origin: MANUAL,
    });

    const withIsbn = build({}, (v) => ({ ...v, title: "本", isbn13: "978-4-00-310111-7" }));
    expect(changeOf(withIsbn, "isbn13")).toEqual({
      field: "isbn13",
      value: "9784003101117",
      origin: MANUAL,
    });
    expect(changeOf(withIsbn, "meta_status")?.value).toBe("pending");
  });

  it("オフライン（offline）の取得結果は、取得待ちのまま保存する（AT-11）", () => {
    const offlineDraft = draftWith({
      isbn13: { value: "9784780802047", origin: { kind: "barcode" } },
      meta_status: { value: "pending", origin: { kind: "provider", name: "bibcontainer" } },
    });
    const draft = build({ fetched: fetchedOf(offlineDraft, true) });
    expect(changeOf(draft, "meta_status")?.value).toBe("pending");
    expect(changeOf(draft, "isbn13")?.origin).toEqual({ kind: "barcode" });
  });

  it("スキャンした ISBN と 2段目の値は、取得結果がなくても barcode の由来で入る。取得待ちにする", () => {
    const draft = build({
      isbn13: "9784003101117",
      secondTier: { cCode: "0193", listPrice: 700 },
    });
    expect(changeOf(draft, "isbn13")).toEqual({
      field: "isbn13",
      value: "9784003101117",
      origin: { kind: "barcode" },
    });
    expect(changeOf(draft, "c_code")?.origin).toEqual({ kind: "barcode" });
    expect(changeOf(draft, "list_price")).toEqual({
      field: "list_price",
      value: 700,
      origin: { kind: "barcode" },
    });
    expect(changeOf(draft, "meta_status")?.value).toBe("pending");
  });

  it("取得済みでも、ISBN を直したら取得待ちにする（取得結果は別の ISBN のものになるため）", () => {
    const draft = build({ fetched: fetchedOf(fetchedDraft) }, (v) => ({
      ...v,
      isbn13: "9784003101117",
    }));
    expect(changeOf(draft, "meta_status")?.value).toBe("pending");
    expect(changeOf(draft, "isbn13")?.origin).toEqual(MANUAL);
  });

  it("シリーズを直すと、名前と手入力の由来に置き換わる。空にすると null", () => {
    const edited = build({ fetched: fetchedOf(fetchedDraft) }, (v) => ({
      ...v,
      seriesName: "別のシリーズ",
    }));
    expect(edited.seriesName).toEqual({ name: "別のシリーズ", origin: MANUAL });
    const cleared = build({ fetched: fetchedOf(fetchedDraft) }, (v) => ({ ...v, seriesName: "" }));
    expect(cleared.seriesName).toBeNull();
  });

  it("フォームに空のままの項目は、変更に含まれない", () => {
    const draft = build({}, (v) => ({ ...emptyFormValues(), ...v, title: "本" }));
    expect(draft.changes.map((change) => change.field)).toEqual(["title"]);
  });
});
