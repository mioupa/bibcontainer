import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { ViewSwitch } from "@/features/list/ViewSwitch";
import { ja } from "@/i18n/ja";
import { useLayoutStore } from "@/stores/layoutStore";

const button = (name: string) => screen.getByRole("button", { name });

describe("表示の形の切り替え（FR-LIST-07）", () => {
  beforeEach(() => {
    useLayoutStore.setState({ viewMode: "table" });
  });

  it("表・グリッドのアイコンのボタンを出し、いまの形が押された状態になる", () => {
    render(<ViewSwitch />);
    expect(screen.getByRole("group", { name: ja.list.viewSwitch.ariaLabel })).toBeInTheDocument();
    expect(button(ja.list.viewSwitch.table)).toHaveAttribute("aria-pressed", "true");
    expect(button(ja.list.viewSwitch.grid)).toHaveAttribute("aria-pressed", "false");
    expect(button(ja.list.viewSwitch.grid)).toHaveAttribute("title", ja.list.viewSwitch.gridHint);
  });

  it("ボタンを押すと、layoutStore の表示の形が切り替わる", () => {
    render(<ViewSwitch />);
    fireEvent.click(button(ja.list.viewSwitch.grid));
    expect(useLayoutStore.getState().viewMode).toBe("grid");
    expect(button(ja.list.viewSwitch.grid)).toHaveAttribute("aria-pressed", "true");
    expect(button(ja.list.viewSwitch.table)).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(button(ja.list.viewSwitch.table));
    expect(useLayoutStore.getState().viewMode).toBe("table");
  });
});
