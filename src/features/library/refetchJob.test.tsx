import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { libraryApi } from "@/features/library/api";
import {
  applyRefetchStatus,
  resetRefetchJobSync,
  useRefetchJobSync,
} from "@/features/library/refetchJob";
import { useRefetchJobStore } from "@/stores/refetchJobStore";
import { makeJob } from "@/test/refetchJob";

const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { success: vi.fn(), info: vi.fn(), error: vi.fn() }),
);
vi.mock("sonner", () => ({ toast }));
vi.mock("@/features/library/api", () => ({
  libraryApi: { onRefetchProgress: vi.fn(), getRefetchJob: vi.fn() },
}));

const api = vi.mocked(libraryApi);
let emit: (status: ReturnType<typeof makeJob>) => void = () => undefined;

function Host() {
  useRefetchJobSync();
  return null;
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("再取得の作業の購読（0027）", () => {
  beforeEach(() => {
    for (const fn of [toast.success, toast.info, toast.error]) fn.mockReset();
    resetRefetchJobSync();
    api.getRefetchJob.mockReset().mockResolvedValue(null);
    api.onRefetchProgress.mockReset().mockImplementation((handler) => {
      emit = handler;
      return Promise.resolve(() => undefined);
    });
  });

  it("開いたとき、今の作業を読み込む", async () => {
    api.getRefetchJob.mockResolvedValue(makeJob({ done: 5, remaining: 95 }));
    render(<Host />);
    await flush();
    expect(useRefetchJobStore.getState().job?.done).toBe(5);
  });

  it("動いているのを見た作業は、終わったとき 1 回だけ通知する（同じ状態が重ねて届いても）", async () => {
    render(<Host />);
    await flush();
    emit(makeJob({ done: 10 }));
    const done = makeJob({ state: "finished", done: 100, remaining: 0, updated: 100 });
    emit(done);
    emit(done);
    expect(toast.success).toHaveBeenCalledOnce();
  });

  it("開いたときにすでに終わっていた作業は通知しない。つけ直しても通知し直さない", async () => {
    api.getRefetchJob.mockResolvedValue(makeJob({ state: "finished", remaining: 0, done: 100 }));
    const first = render(<Host />);
    await flush();
    first.unmount();
    render(<Host />);
    await flush();
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("通知したあとにつけ直しても、同じ作業を通知し直さない", async () => {
    const first = render(<Host />);
    await flush();
    emit(makeJob());
    emit(makeJob({ state: "finished", remaining: 0, done: 100 }));
    first.unmount();
    api.getRefetchJob.mockResolvedValue(makeJob({ state: "finished", remaining: 0, done: 100 }));
    render(<Host />);
    await flush();
    expect(toast.success).toHaveBeenCalledOnce();
  });

  it("止まった作業の通知は、自動で消えない", () => {
    applyRefetchStatus(makeJob());
    applyRefetchStatus(
      makeJob({ state: "stopped", stopReason: { kind: "offline" }, done: 10, remaining: 90 }),
    );
    expect(toast.error).toHaveBeenCalledWith(expect.any(String), {
      duration: Infinity,
      closeButton: true,
    });
  });

  it("終わった作業を、遅れて届いた「動いている」で戻さない", () => {
    applyRefetchStatus(makeJob());
    applyRefetchStatus(makeJob({ state: "finished", remaining: 0, done: 100 }));
    applyRefetchStatus(makeJob({ done: 50 }));
    expect(useRefetchJobStore.getState().job?.state).toBe("finished");
  });

  it("始めた作業がすぐ終わった（使える取得元がない）ときも、通知する", () => {
    applyRefetchStatus(
      makeJob({ state: "stopped", stopReason: { kind: "noActiveProviders" }, done: 0 }),
      { started: true },
    );
    expect(toast.error).toHaveBeenCalledOnce();
  });

  it("始めた作業の戻り値より先に終わりが届いても、1 回だけ通知する", () => {
    applyRefetchStatus(makeJob({ state: "finished", remaining: 0, done: 100 }));
    expect(toast.success).not.toHaveBeenCalled();
    applyRefetchStatus(makeJob(), { started: true });
    expect(toast.success).toHaveBeenCalledOnce();
    expect(useRefetchJobStore.getState().job?.state).toBe("finished");
    applyRefetchStatus(makeJob({ state: "finished", remaining: 0, done: 100 }));
    expect(toast.success).toHaveBeenCalledOnce();
  });
});
