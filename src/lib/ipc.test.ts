import { beforeEach, describe, expect, it, vi } from "vitest";
import { IpcError, ipc } from "@/lib/ipc";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("ipc", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("appInfo は app_info コマンドを引数なしで呼ぶ", async () => {
    const info = {
      name: "bibcontainer",
      version: "0.1.0",
      deviceId: "d-0123456789ab",
      schemaVersion: 1,
      dbPath: "/data/library.sqlite3",
      hlcLast: null,
      pendingOps: 0,
    };
    invoke.mockResolvedValue(info);

    await expect(ipc.appInfo()).resolves.toEqual(info);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("app_info", undefined);
  });

  it("Rust の AppError（{ kind, message }）は IpcError として投げ直す", async () => {
    invoke.mockRejectedValue({ kind: "schema_too_new", message: "新しすぎる" });

    const error = await ipc.appInfo().catch((e: unknown) => e);

    expect(error).toBeInstanceOf(IpcError);
    expect(error).toMatchObject({ kind: "schema_too_new", message: "新しすぎる" });
  });

  it("AppError の形でない失敗は internal の IpcError にする", async () => {
    invoke.mockRejectedValue("command app_info not found");

    const error = await ipc.appInfo().catch((e: unknown) => e);

    expect(error).toBeInstanceOf(IpcError);
    expect(error).toMatchObject({ kind: "internal", message: "command app_info not found" });
  });
});
