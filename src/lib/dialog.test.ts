import { beforeEach, describe, expect, it, vi } from "vitest";
import { chooseDirectory, chooseOpenFile, chooseSavePath } from "@/lib/dialog";

const open = vi.hoisted(() => vi.fn());
const save = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open, save }));

describe("dialog（tauri-plugin-dialog の薄いラッパー）", () => {
  beforeEach(() => {
    open.mockReset();
    save.mockReset();
  });

  it("保存先: 選んだパスを返し、キャンセルなら null", async () => {
    save.mockResolvedValueOnce("/tmp/a.csv").mockResolvedValueOnce(null);
    const options = {
      title: "書き出す",
      defaultPath: "a.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    };
    expect(await chooseSavePath(options)).toBe("/tmp/a.csv");
    expect(save).toHaveBeenCalledWith(options);
    expect(await chooseSavePath()).toBeNull();
  });

  it("ファイル: 1 つだけ選ばせる。キャンセル・文字列以外は null", async () => {
    open
      .mockResolvedValueOnce("/tmp/isbn.txt")
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(["x"]);
    expect(await chooseOpenFile({ title: "選ぶ" })).toBe("/tmp/isbn.txt");
    expect(open).toHaveBeenCalledWith({ title: "選ぶ", multiple: false, directory: false });
    expect(await chooseOpenFile()).toBeNull();
    expect(await chooseOpenFile()).toBeNull();
  });

  it("フォルダ: directory を指定して選ばせる", async () => {
    open.mockResolvedValueOnce("/Volumes/Backup");
    expect(await chooseDirectory({ title: "フォルダ" })).toBe("/Volumes/Backup");
    expect(open).toHaveBeenCalledWith({ title: "フォルダ", multiple: false, directory: true });
  });
});
