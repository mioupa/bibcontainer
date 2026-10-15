import { act, fireEvent, render } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { BookCover } from "@/components/BookCover";
import { useCoverVersionStore } from "@/stores/coverVersionStore";

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

describe("BookCover: 同期であとから届いた書影の読み込み直し（0016）", () => {
  const OTHER = "f".repeat(64);

  beforeEach(() => {
    useCoverVersionStore.setState({ versions: {} });
  });

  const arrive = (...hashes: string[]) => act(() => useCoverVersionStore.getState().bump(hashes));

  it("読み込めず仮の表紙を出していても、書影が届いたら、URL を変えて読み込み直す", () => {
    const { container } = render(
      <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={HASH} variant="sm" />,
    );
    fireEvent.error(container.querySelector("img") as HTMLImageElement);
    expect(container.querySelector("img")).toBeNull();

    arrive(HASH);
    const img = container.querySelector("img");
    expect(img?.getAttribute("src")).toBe(`cover://localhost/${HASH}?size=80&v=1`);
    expect(container.querySelector('[role="img"]')).toBeNull();

    // また失敗したら仮の表紙（同じ版のあいだは読み直さない）。次に届いたら、もう一度
    fireEvent.error(img as HTMLImageElement);
    expect(container.querySelector("img")).toBeNull();
    arrive(HASH);
    expect(container.querySelector("img")?.getAttribute("src")).toBe(
      `cover://localhost/${HASH}?size=80&v=2`,
    );
  });

  it("詳細ペイン・フォーム（lg）の 320px も同じ", () => {
    const { container } = render(
      <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={HASH} variant="lg" />,
    );
    fireEvent.error(container.querySelector("img") as HTMLImageElement);
    expect(container.querySelector("img")).toBeNull();
    arrive(HASH);
    expect(container.querySelector("img")?.getAttribute("src")).toBe(
      `cover://localhost/${HASH}?size=320&v=1`,
    );
  });

  it("画像の要素を作り直す（同じ要素の src の書き換えに頼らない）", () => {
    const { container } = render(
      <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={HASH} variant="sm" />,
    );
    const before = container.querySelector("img");
    arrive(HASH);
    const after = container.querySelector("img");
    expect(after).not.toBeNull();
    expect(after).not.toBe(before);
  });

  it("ほかの本の書影が届いても、この書影は何もしない（要素もそのまま）", () => {
    const { container } = render(
      <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={HASH} variant="sm" />,
    );
    const before = container.querySelector("img");
    arrive(OTHER);
    expect(container.querySelector("img")).toBe(before);
    expect(before?.getAttribute("src")).toBe(`cover://localhost/${HASH}?size=80`);
  });

  it("書影がない本（coverHash が null）は、届いたあとも仮の表紙のまま", () => {
    const { container } = render(
      <BookCover id="b1" title="おにぎり" authors="佐藤" coverHash={null} variant="sm" />,
    );
    arrive(HASH);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector('[role="img"]')).not.toBeNull();
  });

  it("届く前に版がある（先に届いていた）書影は、最初から版つきの URL で読む", () => {
    arrive(HASH);
    arrive(HASH);
    const img = renderCover("sm");
    expect(img?.getAttribute("src")).toBe(`cover://localhost/${HASH}?size=80&v=2`);
  });
});
