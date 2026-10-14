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
});
