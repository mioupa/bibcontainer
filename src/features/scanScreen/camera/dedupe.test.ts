import { describe, expect, it } from "vitest";
import { GONE_MS, INITIAL_DEDUPE, stepDedupe, type DedupeState } from "./dedupe";

const ISBN = "9784101001012";
const SECOND = "1920193007008";
const OTHER = "4901234567894";

/** 読み取りを順に与えて、渡した値を（時刻つきで）集める。 */
function run(
  reads: { at: number; values: string[]; accepting?: boolean }[],
  from: DedupeState = INITIAL_DEDUPE,
) {
  let state = from;
  const out: { at: number; value: string }[] = [];
  for (const read of reads) {
    const result = stepDedupe(state, read.values, read.at, read.accepting ?? true);
    state = result.state;
    for (const value of result.emit) out.push({ at: read.at, value });
  }
  return { state, out };
}

describe("stepDedupe", () => {
  it("1 回だけ読めた値は渡さない", () => {
    const { out } = run([
      { at: 0, values: [ISBN] },
      { at: 150, values: [] },
    ]);
    expect(out).toEqual([]);
  });

  it("連続する 2 回の読み取りで読めたら、2 回目に渡す", () => {
    const { out } = run([
      { at: 0, values: [ISBN] },
      { at: 150, values: [ISBN] },
    ]);
    expect(out).toEqual([{ at: 150, value: ISBN }]);
  });

  it("間に読めない回をはさむと、連続とは数えない", () => {
    const { out } = run([
      { at: 0, values: [ISBN] },
      { at: 150, values: [] },
      { at: 300, values: [ISBN] },
    ]);
    expect(out).toEqual([]);
  });

  it("映したままでは 1 回だけ渡す", () => {
    const reads = Array.from({ length: 30 }, (_, i) => ({ at: i * 150, values: [ISBN] }));
    expect(run(reads).out).toEqual([{ at: 150, value: ISBN }]);
  });

  it("消えて 1.5 秒たってから映すと、もう一度渡す", () => {
    const { out } = run([
      { at: 0, values: [ISBN] },
      { at: 150, values: [ISBN] },
      { at: 300, values: [] },
      { at: 300 + GONE_MS, values: [] },
      { at: 300 + GONE_MS + 150, values: [ISBN] },
      { at: 300 + GONE_MS + 300, values: [ISBN] },
    ]);
    expect(out.map((o) => o.at)).toEqual([150, 300 + GONE_MS + 300]);
  });

  it("消えて 1.5 秒たたないうちに映し直しても、渡さない", () => {
    const { out } = run([
      { at: 0, values: [ISBN] },
      { at: 150, values: [ISBN] },
      { at: 300, values: [] },
      { at: 600, values: [ISBN] },
      { at: 750, values: [ISBN] },
    ]);
    expect(out).toEqual([{ at: 150, value: ISBN }]);
  });

  it("2 段同時に映ると、ISBN、2段目の順に渡す（映る順によらない）", () => {
    const { out } = run([
      { at: 0, values: [SECOND, OTHER, ISBN] },
      { at: 150, values: [SECOND, OTHER, ISBN] },
    ]);
    expect(out.map((o) => o.value)).toEqual([ISBN, SECOND, OTHER]);
  });

  it("2段目だけが先に採用されることもある（組み合わせは Rust 側のペアリングが行う）", () => {
    const { out } = run([
      { at: 0, values: [SECOND] },
      { at: 150, values: [SECOND] },
      { at: 300, values: [SECOND, ISBN] },
      { at: 450, values: [SECOND, ISBN] },
    ]);
    expect(out.map((o) => o.value)).toEqual([SECOND, ISBN]);
  });

  it("止めている間に読めた値は捨て、再開した直後に入れない", () => {
    const { out, state } = run([
      { at: 0, values: [ISBN], accepting: false },
      { at: 150, values: [ISBN], accepting: false },
      { at: 300, values: [ISBN], accepting: false },
      { at: 450, values: [ISBN] },
      { at: 600, values: [ISBN] },
    ]);
    expect(out).toEqual([]);
    expect(state.delivered[ISBN]).toBe(600);
  });

  it("止めている間に消えた本は、再開後に映せば渡す", () => {
    const { out } = run([
      { at: 0, values: [ISBN], accepting: false },
      { at: 150, values: [], accepting: false },
      { at: 150 + GONE_MS, values: [], accepting: false },
      { at: 3000, values: [ISBN] },
      { at: 3150, values: [ISBN] },
    ]);
    expect(out).toEqual([{ at: 3150, value: ISBN }]);
  });

  it("13 桁の数字以外は渡さない", () => {
    const bad = ["978410100101", "97841010010120", "978410100101X", "", " 9784101001012"];
    const { out } = run([
      { at: 0, values: bad },
      { at: 150, values: bad },
    ]);
    expect(out).toEqual([]);
  });

  it("1 回の読み取りに同じ値が重なっても 1 つにする", () => {
    const { out } = run([
      { at: 0, values: [ISBN, ISBN] },
      { at: 150, values: [ISBN, ISBN] },
    ]);
    expect(out).toEqual([{ at: 150, value: ISBN }]);
  });

  it("入力の状態は書き換えない", () => {
    const before = INITIAL_DEDUPE;
    stepDedupe(before, [ISBN], 0, true);
    expect(before).toEqual({ previous: [], delivered: {} });
  });
});
