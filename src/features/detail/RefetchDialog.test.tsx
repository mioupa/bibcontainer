import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_OVERRIDE_MANUAL, RefetchDialog } from "@/features/detail/RefetchDialog";
import { ja } from "@/i18n/ja";

describe("書誌の再取得の確認（AT-16）", () => {
  it("既定では、手で直した項目を上書きしない（false で再取得する）", () => {
    expect(DEFAULT_OVERRIDE_MANUAL).toBe(false);
    const onConfirm = vi.fn();
    render(<RefetchDialog ids={["a", "b"]} onCancel={vi.fn()} onConfirm={onConfirm} />);

    expect(screen.getByText(ja.list.refetchDialog.description(2))).toBeInTheDocument();
    const checkbox = screen.getByRole("checkbox", { name: ja.list.refetchDialog.overrideLabel });
    expect(checkbox).not.toBeChecked();
    expect(screen.getByText(ja.list.refetchDialog.keepHint)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: ja.list.refetchDialog.confirm }));
    expect(onConfirm).toHaveBeenCalledExactlyOnceWith(["a", "b"], false);
  });

  it("チェックを入れると、上書きして再取得する（そのことを説明する）", () => {
    const onConfirm = vi.fn();
    render(<RefetchDialog ids={["a"]} onCancel={vi.fn()} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("checkbox", { name: ja.list.refetchDialog.overrideLabel }));
    expect(screen.getByText(ja.list.refetchDialog.overrideHint)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: ja.list.refetchDialog.confirm }));
    expect(onConfirm).toHaveBeenCalledExactlyOnceWith(["a"], true);
  });

  it("キャンセルでは再取得しない。開き直すと、チェックは既定（オフ）に戻る", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    const { rerender } = render(
      <RefetchDialog ids={["a"]} onCancel={onCancel} onConfirm={onConfirm} />,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: ja.list.refetchDialog.overrideLabel }));
    fireEvent.click(screen.getByRole("button", { name: ja.list.refetchDialog.cancel }));
    expect(onCancel).toHaveBeenCalledOnce();
    expect(onConfirm).not.toHaveBeenCalled();

    rerender(<RefetchDialog ids={null} onCancel={onCancel} onConfirm={onConfirm} />);
    rerender(<RefetchDialog ids={["a"]} onCancel={onCancel} onConfirm={onConfirm} />);
    expect(
      screen.getByRole("checkbox", { name: ja.list.refetchDialog.overrideLabel }),
    ).not.toBeChecked();
  });

  describe("件数に応じた案内（0027 §1）", () => {
    const manyIds = (count: number) => Array.from({ length: count }, (_, i) => `b${i}`);
    const text = ja.list.refetchDialog;

    it("少ない冊数では、ISBN のない本以外の案内を出さない", () => {
      render(<RefetchDialog ids={manyIds(9)} onCancel={vi.fn()} onConfirm={vi.fn()} />);
      expect(screen.queryByText(/ISBN のない/)).not.toBeInTheDocument();
      expect(screen.queryByText(/約 \d+ 分/)).not.toBeInTheDocument();
      expect(screen.queryByText(text.limitNote)).not.toBeInTheDocument();
    });

    it("ISBN のない本の数を出す", () => {
      render(
        <RefetchDialog ids={manyIds(5)} noIsbnCount={2} onCancel={vi.fn()} onConfirm={vi.fn()} />,
      );
      expect(screen.getByText(text.noIsbn(2))).toBeInTheDocument();
    });

    it("10 冊以上は、時間の目安とステータスバーの案内を出す（300 冊未満では上限の注意なし）", () => {
      render(<RefetchDialog ids={manyIds(100)} onCancel={vi.fn()} onConfirm={vi.fn()} />);
      expect(screen.getByText("約 4 分（1 冊 2 秒ほど）")).toBeInTheDocument();
      expect(screen.getByText(text.progressHint)).toBeInTheDocument();
      expect(screen.queryByText(text.limitNote)).not.toBeInTheDocument();
    });

    it("300 冊以上は、取得元の 1 日の上限の注意を出す", () => {
      render(<RefetchDialog ids={manyIds(300)} onCancel={vi.fn()} onConfirm={vi.fn()} />);
      expect(screen.getByText("約 10 分（1 冊 2 秒ほど）")).toBeInTheDocument();
      expect(screen.getByText(text.limitNote)).toBeInTheDocument();
    });
  });
});
