import { beforeEach, describe, expect, it } from "vitest";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";

const KEY = "bibcontainer.scanMode";

describe("スキャン登録の端末ごとの保存", () => {
  beforeEach(() => {
    localStorage.clear();
    useScanSessionStore.setState({ mode: "single", cameraDeviceId: null });
  });

  it("選んだカメラを保存する（オン・オフは保存しない）", () => {
    useScanSessionStore.getState().setCameraDeviceId("cam-b");
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as {
      state: Record<string, unknown>;
    };
    expect(saved.state).toEqual({ mode: "single", cameraDeviceId: "cam-b" });
  });

  it("カメラの項目がない古い保存（mode だけ）も読める", async () => {
    localStorage.setItem(KEY, JSON.stringify({ state: { mode: "batch" }, version: 1 }));
    await useScanSessionStore.persist.rehydrate();
    expect(useScanSessionStore.getState().mode).toBe("batch");
    expect(useScanSessionStore.getState().cameraDeviceId).toBeNull();
  });

  it("保存したカメラを読み戻す", async () => {
    localStorage.setItem(
      KEY,
      JSON.stringify({ state: { mode: "single", cameraDeviceId: "cam-c" }, version: 1 }),
    );
    await useScanSessionStore.persist.rehydrate();
    expect(useScanSessionStore.getState().cameraDeviceId).toBe("cam-c");
  });
});
