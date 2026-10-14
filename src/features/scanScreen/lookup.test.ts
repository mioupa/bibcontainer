import { beforeEach, describe, expect, it, vi } from "vitest";
import type * as IpcModule from "@/lib/ipc";
import { awaitLookup, startLookup, resetLookupsForTest } from "@/features/scanScreen/lookup";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ISBN_A, ISBN_B, ISBN_C, makeResult } from "@/features/scanScreen/testData";
import type { FetchMetadataResult } from "@/types/contracts";

const fetchMetadata = vi.hoisted(() => vi.fn());
vi.mock("@/lib/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return { ...actual, ipc: { ...actual.ipc, fetchMetadata } };
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("startLookup（書誌取得の開始と、ISBN ごとの結果の記憶）", () => {
  beforeEach(() => {
    fetchMetadata.mockReset();
    resetLookupsForTest();
    useScanSessionStore.getState().reset();
  });

  it("結果を ISBN ごとに覚える", async () => {
    fetchMetadata.mockResolvedValue(makeResult(ISBN_A));
    await startLookup(ISBN_A);
    expect(useScanSessionStore.getState().lookups[ISBN_A]).toMatchObject({ status: "done" });
    expect(fetchMetadata).toHaveBeenCalledWith(ISBN_A);
  });

  it("進行中の同じ ISBN は、新しく取得せず、その結果を待つ", async () => {
    const d = deferred<FetchMetadataResult>();
    fetchMetadata.mockReturnValue(d.promise);
    const first = startLookup(ISBN_A);
    const second = startLookup(ISBN_A);
    d.resolve(makeResult(ISBN_A));
    await Promise.all([first, second]);
    expect(fetchMetadata).toHaveBeenCalledTimes(1);
  });

  it("取得済みの結果があれば、通信せずに返す", async () => {
    fetchMetadata.mockResolvedValue(makeResult(ISBN_A));
    await startLookup(ISBN_A);
    await startLookup(ISBN_A);
    expect(fetchMetadata).toHaveBeenCalledTimes(1);
  });

  it("失敗した結果は、理由を覚え、次に読んだときに取り直す", async () => {
    fetchMetadata.mockRejectedValueOnce(new Error("x"));
    const failed = await startLookup(ISBN_A);
    expect(failed.status).toBe("failed");

    fetchMetadata.mockResolvedValueOnce(makeResult(ISBN_A));
    const retried = await startLookup(ISBN_A);
    expect(retried.status).toBe("done");
    expect(fetchMetadata).toHaveBeenCalledTimes(2);
  });

  it("オフラインで取得待ちになった結果も、次に読んだときに取り直す", async () => {
    fetchMetadata.mockResolvedValueOnce(makeResult(ISBN_A, { offline: true }));
    await startLookup(ISBN_A);
    fetchMetadata.mockResolvedValueOnce(makeResult(ISBN_A));
    const retried = await startLookup(ISBN_A);
    expect(retried).toMatchObject({ status: "done", result: { offline: false } });
  });

  it("reuseKnown（確定のとき）: オフライン・失敗の結果も、取り直さずにそのまま使う（同じ読み取りで 2 回取得しない）", async () => {
    fetchMetadata.mockResolvedValueOnce(makeResult(ISBN_A, { offline: true }));
    const read = await startLookup(ISBN_A); // ISBN を読んだとき
    expect(read).toMatchObject({ status: "done", result: { offline: true } });

    const confirmed = await startLookup(ISBN_A, { reuseKnown: true }); // 確定したとき
    expect(confirmed).toBe(read);
    expect(fetchMetadata).toHaveBeenCalledTimes(1);

    fetchMetadata.mockRejectedValueOnce(new Error("x"));
    await startLookup(ISBN_B);
    const failed = await startLookup(ISBN_B, { reuseKnown: true });
    expect(failed.status).toBe("failed");
    expect(fetchMetadata).toHaveBeenCalledTimes(2);

    // 次に読んだとき（reuseKnown なし）は、取り直す
    fetchMetadata.mockResolvedValueOnce(makeResult(ISBN_A));
    const again = await startLookup(ISBN_A);
    expect(again).toMatchObject({ status: "done", result: { offline: false } });
    expect(fetchMetadata).toHaveBeenCalledTimes(3);
  });

  it("reuseKnown でも、取得済みの結果がなければ始める", async () => {
    fetchMetadata.mockResolvedValueOnce(makeResult(ISBN_C));
    await startLookup(ISBN_C, { reuseKnown: true });
    expect(fetchMetadata).toHaveBeenCalledTimes(1);
  });

  it("awaitLookup（1 冊ずつの確定）: 読んだときに始めた取得の結果を待つ。オフラインでも取り直さない", async () => {
    const d = deferred<FetchMetadataResult>();
    fetchMetadata.mockReturnValueOnce(d.promise);
    void startLookup(ISBN_A); // 読んだとき（進行中）
    const waiting = awaitLookup(ISBN_A);
    d.resolve(makeResult(ISBN_A, { offline: true }));
    expect(await waiting).toMatchObject({ result: { offline: true } });
    // 終わったあとも、オフラインの結果をそのまま返す
    expect(await awaitLookup(ISBN_A)).toMatchObject({ result: { offline: true } });
    expect(fetchMetadata).toHaveBeenCalledTimes(1);
  });

  it("serial: 前の取得が終わってから次を始める（ISBN 一覧の取り込み）", async () => {
    const order: string[] = [];
    const first = deferred<FetchMetadataResult>();
    fetchMetadata.mockImplementation((isbn: string) => {
      order.push(`start ${isbn}`);
      if (isbn === ISBN_A) return first.promise;
      return Promise.resolve(makeResult(isbn));
    });

    const a = startLookup(ISBN_A, { serial: true });
    const b = startLookup(ISBN_B, { serial: true });
    const c = startLookup(ISBN_C, { serial: true });
    await Promise.resolve();
    await Promise.resolve();
    expect(order).toEqual([`start ${ISBN_A}`]);

    first.resolve(makeResult(ISBN_A));
    await Promise.all([a, b, c]);
    expect(order).toEqual([`start ${ISBN_A}`, `start ${ISBN_B}`, `start ${ISBN_C}`]);
  });

  it("serial でない取得は、順番を待たない", async () => {
    const slow = deferred<FetchMetadataResult>();
    fetchMetadata.mockImplementation((isbn: string) =>
      isbn === ISBN_A ? slow.promise : Promise.resolve(makeResult(isbn)),
    );
    void startLookup(ISBN_A);
    await startLookup(ISBN_B);
    expect(useScanSessionStore.getState().lookups[ISBN_B]).toBeDefined();
    expect(useScanSessionStore.getState().lookups[ISBN_A]).toBeUndefined();
    slow.resolve(makeResult(ISBN_A));
  });
});
