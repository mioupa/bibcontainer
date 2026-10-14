import { describe, expect, it } from "vitest";
import {
  PAIRING_IDLE,
  pairingReducer,
  remainingSeconds,
  type PairingEffect,
  type PairingEvent,
  type PairingState,
} from "@/features/scan/pairing";
import type { ScanCode } from "@/types/contracts";

const config = { timeoutSec: 10 };

// 付録 B のテスト用データ
const ISBN_A: ScanCode = {
  kind: "isbn13",
  raw: "9784003101117",
  isbn13: "9784003101117",
  isbn10: "4003101111",
};
const ISBN_B: ScanCode = {
  kind: "isbn13",
  raw: "9784780802047",
  isbn13: "9784780802047",
  isbn10: "4780802040",
};
const SECOND_193: ScanCode = {
  kind: "secondTier",
  raw: "1920193007008",
  cCode: "0193",
  listPrice: 700,
};
const SECOND_077: ScanCode = {
  kind: "secondTier",
  raw: "1920077013002",
  cCode: "0077",
  listPrice: 1300,
};
const SECOND_NO_PRICE: ScanCode = {
  kind: "secondTier",
  raw: "1920193000009",
  cCode: "0193",
  listPrice: null,
};

const scan = (code: ScanCode, now: number): PairingEvent => ({ type: "scan", code, now });
const tick = (now: number): PairingEvent => ({ type: "tick", now });
const commit = (now: number): PairingEvent => ({ type: "commit", now });

/** 事象を順に流し、最終状態と、すべての効果（出た順）を返す。 */
function run(events: PairingEvent[], start: PairingState = PAIRING_IDLE) {
  let state = start;
  const effects: PairingEffect[] = [];
  const perStep: PairingEffect[][] = [];
  for (const event of events) {
    const step = pairingReducer(state, event, config);
    state = step.state;
    effects.push(...step.effects);
    perStep.push(step.effects);
  }
  return { state, effects, perStep };
}

const confirms = (effects: PairingEffect[]) =>
  effects.flatMap((e) => (e.type === "confirm" ? [e] : []));
const lookups = (effects: PairingEffect[]) =>
  effects.flatMap((e) => (e.type === "startLookup" ? [e.isbn13] : []));
const sounds = (effects: PairingEffect[]) =>
  effects.flatMap((e) => (e.type === "sound" ? [e.sound] : []));
const messages = (effects: PairingEffect[]) =>
  effects.flatMap((e) => (e.type === "message" ? [e.message] : []));

describe("AT-05: ISBN → 2段目（10 秒以内）", () => {
  it("1 冊として確定し、Cコードと価格が入る。ISBN を読んだ時点で書誌取得を始める", () => {
    const { state, effects, perStep } = run([scan(ISBN_A, 0), scan(SECOND_193, 3000)]);

    expect(state).toEqual(PAIRING_IDLE);
    // ISBN を読んだ時点（1 つ目のステップ）で書誌取得が始まる。2段目を待たない
    expect(perStep[0]).toContainEqual({ type: "startLookup", isbn13: "9784003101117" });
    expect(lookups(effects)).toEqual(["9784003101117"]);
    expect(confirms(effects)).toEqual([
      {
        type: "confirm",
        reason: "paired",
        book: {
          isbn13: "9784003101117",
          isbn10: "4003101111",
          secondTier: { cCode: "0193", listPrice: 700, raw: "1920193007008" },
        },
      },
    ]);
    expect(sounds(effects)).toEqual(["success", "success"]);
    expect(messages(perStep[1] ?? [])).toEqual([
      { kind: "paired", isbn13: "9784003101117", cCode: "0193", listPrice: 700 },
    ]);
  });

  it("2段目の価格が不明（00000）でも、価格 null のまま確定する（AT-04）", () => {
    const { effects } = run([scan(ISBN_A, 0), scan(SECOND_NO_PRICE, 1000)]);
    expect(confirms(effects)[0]?.book.secondTier).toEqual({
      cCode: "0193",
      listPrice: null,
      raw: "1920193000009",
    });
  });
});

describe("AT-06: 2段目 → ISBN（10 秒以内）", () => {
  it("AT-05 と同じ結果になる（逆順）。ISBN を読んだ時点で書誌取得を始める", () => {
    const forward = run([scan(ISBN_A, 0), scan(SECOND_193, 3000)]);
    const reverse = run([scan(SECOND_193, 0), scan(ISBN_A, 3000)]);

    expect(reverse.state).toEqual(PAIRING_IDLE);
    expect(confirms(reverse.effects)).toEqual(confirms(forward.effects));
    expect(lookups(reverse.effects)).toEqual(["9784003101117"]);
    // 最初の 2段目では書誌取得を始めない
    expect(lookups(reverse.perStep[0] ?? [])).toEqual([]);
    expect(reverse.state).toEqual(PAIRING_IDLE);
  });

  it("2段目だけのとき、状態は「2段目 保留」で、期限は読んだ時刻から T 秒後", () => {
    const { state } = run([scan(SECOND_193, 1000)]);
    expect(state).toEqual({
      phase: "secondHeld",
      secondTier: { cCode: "0193", listPrice: 700, raw: "1920193007008" },
      deadline: 11_000,
    });
  });
});

describe("AT-07: ISBN → 別の ISBN", () => {
  it("1 冊目は 2段目なしで確定し、2 冊目は ISBN 保留になる。2 冊とも書誌取得を始める", () => {
    const { state, effects } = run([scan(ISBN_A, 0), scan(ISBN_B, 2000)]);

    expect(confirms(effects)).toEqual([
      {
        type: "confirm",
        reason: "nextIsbn",
        book: { isbn13: "9784003101117", isbn10: "4003101111", secondTier: null },
      },
    ]);
    expect(lookups(effects)).toEqual(["9784003101117", "9784780802047"]);
    expect(state).toEqual({
      phase: "isbnHeld",
      isbn13: "9784780802047",
      isbn10: "4780802040",
      deadline: 12_000,
    });
  });

  it("続けて 2段目を読むと、2 冊目に付く（1 冊目には付かない）", () => {
    const { state, effects } = run([scan(ISBN_A, 0), scan(ISBN_B, 2000), scan(SECOND_077, 4000)]);
    const [first, second] = confirms(effects);
    expect(first?.book.secondTier).toBeNull();
    expect(second?.book).toEqual({
      isbn13: "9784780802047",
      isbn10: "4780802040",
      secondTier: { cCode: "0077", listPrice: 1300, raw: "1920077013002" },
    });
    expect(state).toEqual(PAIRING_IDLE);
  });
});

describe("AT-08: 同じ ISBN を続けて 2 回", () => {
  it("2 回目は無視され、注意音が鳴る。保留（期限も）は変わらない", () => {
    const { state, effects, perStep } = run([scan(ISBN_A, 0), scan(ISBN_A, 1500)]);

    expect(state).toEqual({
      phase: "isbnHeld",
      isbn13: "9784003101117",
      isbn10: "4003101111",
      deadline: 10_000,
    });
    expect(perStep[1]).toEqual([
      { type: "sound", sound: "attention" },
      {
        type: "message",
        level: "warning",
        message: { kind: "duplicateIsbn", isbn13: "9784003101117" },
      },
    ]);
    // 書誌取得は 1 回だけ。確定もしない
    expect(lookups(effects)).toEqual(["9784003101117"]);
    expect(confirms(effects)).toEqual([]);
  });

  it("その後に 2段目を読めば、1 冊として確定できる", () => {
    const { effects } = run([scan(ISBN_A, 0), scan(ISBN_A, 1000), scan(SECOND_193, 2000)]);
    expect(confirms(effects)).toHaveLength(1);
    expect(confirms(effects)[0]?.reason).toBe("paired");
  });
});

describe("タイムアウト T（FR-SCAN-06）", () => {
  it("ISBN 保留: T 秒たつと、2段目なしで確定して待機中に戻る。音は鳴らさない", () => {
    const held = run([scan(ISBN_A, 0)]).state;
    const before = pairingReducer(held, tick(9999), config);
    expect(before.state).toEqual(held);
    expect(before.effects).toEqual([]);

    const after = pairingReducer(held, tick(10_000), config);
    expect(after.state).toEqual(PAIRING_IDLE);
    expect(confirms(after.effects)).toEqual([
      {
        type: "confirm",
        reason: "timeout",
        book: { isbn13: "9784003101117", isbn10: "4003101111", secondTier: null },
      },
    ]);
    expect(sounds(after.effects)).toEqual([]);
    expect(messages(after.effects)).toEqual([
      { kind: "confirmedWithoutSecondTier", isbn13: "9784003101117", reason: "timeout" },
    ]);
  });

  it("2段目 保留: T 秒たつと、2段目を捨てて（エラー表示とエラー音）待機中に戻る。何も確定しない", () => {
    const held = run([scan(SECOND_193, 0)]).state;
    const after = pairingReducer(held, tick(10_000), config);
    expect(after.state).toEqual(PAIRING_IDLE);
    expect(confirms(after.effects)).toEqual([]);
    expect(sounds(after.effects)).toEqual(["error"]);
    expect(messages(after.effects)).toEqual([{ kind: "secondTierDiscarded", reason: "timeout" }]);
  });

  it("待機中の tick は何もしない", () => {
    expect(pairingReducer(PAIRING_IDLE, tick(999_999), config)).toEqual({
      state: PAIRING_IDLE,
      effects: [],
    });
  });

  it("T は設定で変えられる（3〜30 秒に収まる）", () => {
    const at = (timeoutSec: number) =>
      pairingReducer(PAIRING_IDLE, scan(ISBN_A, 0), { timeoutSec }).state;
    expect(at(3)).toMatchObject({ deadline: 3000 });
    expect(at(30)).toMatchObject({ deadline: 30_000 });
    expect(at(1)).toMatchObject({ deadline: 3000 });
    expect(at(99)).toMatchObject({ deadline: 30_000 });
    expect(at(Number.NaN)).toMatchObject({ deadline: 10_000 });
  });

  it("tick が遅れても、期限後のスキャンは先にタイムアウトとして片づいてから処理される", () => {
    // ISBN A を保留したまま 15 秒後に、ISBN B を読む。A は時間切れで確定（nextIsbn ではなく timeout）
    const { state, effects } = run([scan(ISBN_A, 0), scan(ISBN_B, 15_000)]);
    const [first] = confirms(effects);
    expect(first?.reason).toBe("timeout");
    expect(state).toMatchObject({ phase: "isbnHeld", isbn13: "9784780802047", deadline: 25_000 });
    expect(confirms(effects)).toHaveLength(1);
  });

  it("期限後に届いた 2段目は、ペアにならず、新しい保留になる", () => {
    const { state, effects } = run([scan(ISBN_A, 0), scan(SECOND_193, 12_000)]);
    expect(confirms(effects).map((c) => [c.reason, c.book.secondTier])).toEqual([
      ["timeout", null],
    ]);
    expect(state).toMatchObject({ phase: "secondHeld", deadline: 22_000 });
  });

  it("残り秒数は切り上げで出る", () => {
    const held = run([scan(ISBN_A, 0)]).state;
    expect(remainingSeconds(held, 0)).toBe(10);
    expect(remainingSeconds(held, 2500)).toBe(8);
    expect(remainingSeconds(held, 9001)).toBe(1);
    expect(remainingSeconds(held, 10_000)).toBe(0);
    expect(remainingSeconds(held, 50_000)).toBe(0);
    expect(remainingSeconds(PAIRING_IDLE, 0)).toBeNull();
  });
});

describe("2段目の連続", () => {
  it("古い 2段目を捨てて、新しいものを保留する（注意音）。期限は新しい 2段目から数え直す", () => {
    const { state, perStep } = run([scan(SECOND_193, 0), scan(SECOND_077, 4000)]);
    expect(state).toEqual({
      phase: "secondHeld",
      secondTier: { cCode: "0077", listPrice: 1300, raw: "1920077013002" },
      deadline: 14_000,
    });
    expect(perStep[1]).toEqual([
      { type: "sound", sound: "attention" },
      { type: "message", level: "warning", message: { kind: "secondTierReplaced", cCode: "0077" } },
    ]);
  });

  it("その後に ISBN を読むと、新しいほうの 2段目とペアになる", () => {
    const { effects } = run([scan(SECOND_193, 0), scan(SECOND_077, 1000), scan(ISBN_A, 2000)]);
    expect(confirms(effects)).toEqual([
      {
        type: "confirm",
        reason: "paired",
        book: {
          isbn13: "9784003101117",
          isbn10: "4003101111",
          secondTier: { cCode: "0077", listPrice: 1300, raw: "1920077013002" },
        },
      },
    ]);
  });

  it("同じ 2段目をもう一度読んでも、置き換えとして扱う（注意音）", () => {
    const { perStep } = run([scan(SECOND_193, 0), scan(SECOND_193, 1000)]);
    expect(sounds(perStep[1] ?? [])).toEqual(["attention"]);
  });
});

describe("確定キー", () => {
  it("ISBN 保留中は、2段目なしで確定して待機中に戻る（成功音）", () => {
    const { state, effects } = run([scan(ISBN_A, 0), commit(2000)]);
    expect(state).toEqual(PAIRING_IDLE);
    expect(confirms(effects)).toEqual([
      {
        type: "confirm",
        reason: "commitKey",
        book: { isbn13: "9784003101117", isbn10: "4003101111", secondTier: null },
      },
    ]);
    expect(sounds(effects)).toEqual(["success", "success"]);
  });

  it("2段目 保留中は、2段目を捨てる（エラー表示）。1 冊にならないので確定しない", () => {
    const { state, effects } = run([scan(SECOND_193, 0), commit(2000)]);
    expect(state).toEqual(PAIRING_IDLE);
    expect(confirms(effects)).toEqual([]);
    expect(messages(effects).at(-1)).toEqual({ kind: "secondTierDiscarded", reason: "commitKey" });
  });

  it("待機中は何もしない", () => {
    expect(pairingReducer(PAIRING_IDLE, commit(0), config)).toEqual({
      state: PAIRING_IDLE,
      effects: [],
    });
  });

  it("確定キーの後は、同じ ISBN をもう一度読んでも二度読みにならない（新しい本として保留する）", () => {
    const { state, effects } = run([scan(ISBN_A, 0), commit(1000), scan(ISBN_A, 2000)]);
    expect(state).toMatchObject({ phase: "isbnHeld", isbn13: "9784003101117" });
    expect(lookups(effects)).toEqual(["9784003101117", "9784003101117"]);
  });
});

describe("受け付けないバーコード", () => {
  const magazine: ScanCode = { kind: "magazine", raw: "4910123456789" };
  const jan: ScanCode = { kind: "otherJan", raw: "4901234567894" };
  const checksum: ScanCode = { kind: "invalid", raw: "1920193007009", reason: "checksum" };
  const format: ScanCode = { kind: "invalid", raw: "12345", reason: "format" };

  it.each([
    [magazine, { kind: "rejected", reason: "magazine", raw: "4910123456789" }],
    [jan, { kind: "rejected", reason: "otherJan", raw: "4901234567894" }],
    [checksum, { kind: "rejected", reason: "checksum", raw: "1920193007009" }],
    [format, { kind: "rejected", reason: "format", raw: "12345" }],
  ] as const)("%j はエラー音とエラー表示で、状態は変わらない（AT-03）", (code, message) => {
    for (const start of [
      PAIRING_IDLE,
      run([scan(ISBN_A, 0)]).state,
      run([scan(SECOND_193, 0)]).state,
    ]) {
      const step = pairingReducer(start, scan(code, 1000), config);
      expect(step.state).toEqual(start);
      expect(step.effects).toEqual([
        { type: "sound", sound: "error" },
        { type: "message", level: "error", message },
      ]);
    }
  });

  it("受け付けないバーコードでは、保留の期限が延びない", () => {
    const { state } = run([scan(ISBN_A, 0), scan(magazine, 9000)]);
    expect(state).toMatchObject({ phase: "isbnHeld", deadline: 10_000 });
  });
});

describe("書誌取得の効果", () => {
  it("ISBN を読むたびに 1 回だけ出る（同じ ISBN の二度読みでは出ない）", () => {
    const { effects } = run([
      scan(ISBN_A, 0),
      scan(ISBN_A, 100),
      scan(SECOND_193, 200),
      scan(SECOND_077, 300),
      scan(ISBN_B, 400),
    ]);
    expect(lookups(effects)).toEqual(["9784003101117", "9784780802047"]);
  });
});
