import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BookCover } from "@/components/BookCover";

const HASH = "a1b2c3d4e5f60718293a4b5c6d7e8f901234567890abcdef1234567890abcdef";

function renderCover(variant: "sm" | "lg", coverHash: string | null = HASH) {
  const { container } = render(
    <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={coverHash} variant={variant} />,
  );
  return container.querySelector("img");
}

describe("BookCover の書影の大きさ（0010 §12）", () => {
  it("一覧（sm）は、枠（幅 28px）に合わせた長辺 80px のサムネイルを取り寄せる", () => {
    const img = renderCover("sm");
    expect(img?.getAttribute("src")).toBe(`cover://localhost/${HASH}?size=80`);
  });

  it("詳細ペイン・フォーム（lg）は、長辺 320px のサムネイルを取り寄せる", () => {
    const img = renderCover("lg");
    expect(img?.getAttribute("src")).toBe(`cover://localhost/${HASH}?size=320`);
  });

  it("画像はメインスレッドの外でデコードし、画面の外では読み込まない", () => {
    const img = renderCover("sm");
    expect(img?.getAttribute("decoding")).toBe("async");
    expect(img?.getAttribute("loading")).toBe("lazy");
  });

  it("書影がないとき、読み込めないときは、画像を出さず仮の表紙にする", () => {
    expect(renderCover("sm", null)).toBeNull();

    const { container } = render(
      <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={HASH} variant="sm" />,
    );
    fireEvent.error(container.querySelector("img") as HTMLImageElement);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe("おにぎり");
  });
});
