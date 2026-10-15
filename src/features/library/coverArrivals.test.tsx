import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCoverArrivalSync } from "@/features/library/coverArrivals";
import { useCoverVersionStore } from "@/stores/coverVersionStore";

const isTauri = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

function Probe() {
  useCoverArrivalSync();
  return null;
}

/** 購読を受け取る関数と、解除の関数。 */
function captureListener() {
  const unlisten = vi.fn();
  let emit: (payload: { hashes: string[] }) => void = () => {};
  listen.mockImplementation((name: string, callback: (event: { payload: unknown }) => void) => {
    expect(name).toBe("covers-arrived");
    emit = (payload) => callback({ payload });
    return Promise.resolve(unlisten);
  });
  return { unlisten, emit: (payload: { hashes: string[] }) => emit(payload) };
}

beforeEach(() => {
  listen.mockReset();
  isTauri.mockReset();
  isTauri.mockReturnValue(true);
  useCoverVersionStore.setState({ versions: {} });
});

describe("useCoverArrivalSync", () => {
  it("covers-arrived を受けて、届いた書影の版を進める。届くたびに 1 ずつ", async () => {
    const { emit } = captureListener();
    render(<Probe />);
    await vi.waitFor(() => expect(listen).toHaveBeenCalledTimes(1));

    emit({ hashes: ["a", "b"] });
    expect(useCoverVersionStore.getState().versions).toEqual({ a: 1, b: 1 });
    emit({ hashes: ["a"] });
    expect(useCoverVersionStore.getState().versions).toEqual({ a: 2, b: 1 });
    emit({ hashes: [] });
    expect(useCoverVersionStore.getState().versions).toEqual({ a: 2, b: 1 });
  });

  it("画面が外れたら、購読をやめる", async () => {
    const { unlisten } = captureListener();
    const view = render(<Probe />);
    await vi.waitFor(() => expect(listen).toHaveBeenCalledTimes(1));
    // 購読の確立（Promise）が終わるのを待ってから外す
    await Promise.resolve();
    view.unmount();
    await vi.waitFor(() => expect(unlisten).toHaveBeenCalledOnce());
  });

  it("購読の確立より先に外れても、あとで解除する", async () => {
    const unlisten = vi.fn();
    let resolveListen: (stop: () => void) => void = () => {};
    listen.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          resolveListen = resolve;
        }),
    );
    const view = render(<Probe />);
    view.unmount();
    resolveListen(unlisten);
    await vi.waitFor(() => expect(unlisten).toHaveBeenCalledOnce());
  });

  it("Tauri の外（ブラウザ）では、何も購読しない", () => {
    isTauri.mockReturnValue(false);
    render(<Probe />);
    expect(listen).not.toHaveBeenCalled();
  });
});
