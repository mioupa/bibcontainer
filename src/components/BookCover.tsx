import { useState } from "react";
import { coverAccentVar } from "@/lib/coverColor";
import {
  DETAIL_COVER_SIZE,
  GRID_COVER_SIZE,
  LIST_COVER_SIZE,
  coverUrl,
  type CoverSize,
} from "@/lib/cover";
import { cn } from "@/lib/utils";
import { useCoverVersion } from "@/stores/coverVersionStore";

export type CoverVariant = "sm" | "lg" | "grid";

/** 変種ごとの、取り寄せるサムネイルの大きさ。 */
const COVER_SIZE: Record<CoverVariant, CoverSize> = {
  sm: LIST_COVER_SIZE,
  lg: DETAIL_COVER_SIZE,
  grid: GRID_COVER_SIZE,
};

/** 仮の表紙の左端の線の太さ（px）。 */
const SPINE_WIDTH: Record<CoverVariant, number> = { sm: 3, lg: 8, grid: 5 };

interface BookCoverProps {
  /** 本の ID（仮の表紙の色を決める）。 */
  id: string;
  title: string;
  /** 著者の表示用の文字列。 */
  authors: string;
  coverHash: string | null;
  /** sm: 一覧の小さな表紙。lg: 詳細ペインの大きな表紙。grid: グリッドのカードの表紙（置き場所の幅いっぱい）。 */
  variant?: CoverVariant;
  className?: string;
}

const FRAME_CLASS: Record<CoverVariant, string> = {
  sm: "aspect-[5/7] w-7 rounded-[3px]",
  lg: "aspect-[5/7] w-full max-w-60 rounded-md shadow-md",
  grid: "aspect-[5/7] w-full rounded-md shadow-sm",
};

function frameClass(variant: CoverVariant, className?: string) {
  return cn(
    "relative shrink-0 overflow-hidden ring-1 ring-foreground/10",
    FRAME_CLASS[variant],
    className,
  );
}

/** 書影を出す。書影がない、または読み込めないときは、書名と著者を描いた仮の表紙にする。 */
export function BookCover({
  id,
  title,
  authors,
  coverHash,
  variant = "sm",
  className,
}: BookCoverProps) {
  // 同期であとから届いた回数。届くたびに、URL を変えて画像の要素を作り直し、読み込み直す
  const version = useCoverVersion(coverHash);
  // 読み込みに失敗した書影（hash と版）。同じ hash・同じ版のあいだは仮の表紙を出す。版が進めば、また読み込む
  const [failed, setFailed] = useState<{ hash: string; version: number } | null>(null);
  const showImage =
    coverHash !== null && !(failed?.hash === coverHash && failed.version === version);

  if (showImage) {
    return (
      <div className={frameClass(variant, className)}>
        <img
          key={version}
          src={coverUrl(coverHash, COVER_SIZE[variant], undefined, version)}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          className="size-full object-cover"
          onError={() => setFailed({ hash: coverHash, version })}
        />
      </div>
    );
  }
  return (
    <PlaceholderCover
      id={id}
      title={title}
      authors={authors}
      variant={variant}
      className={className}
    />
  );
}

interface PlaceholderCoverProps extends Omit<BookCoverProps, "coverHash"> {
  variant: CoverVariant;
}

/** 書影がないときの仮の表紙。色は ID から決める。 */
export function PlaceholderCover({
  id,
  title,
  authors,
  variant,
  className,
}: PlaceholderCoverProps) {
  const accent = coverAccentVar(id);
  return (
    <div
      role="img"
      aria-label={title}
      className={frameClass(variant, className)}
      style={{
        // 背景に色を薄く混ぜ、文字は本文色にして、ライト・ダークのどちらでも読めるようにする
        backgroundColor: `color-mix(in srgb, ${accent} 28%, var(--ctp-base))`,
        borderLeft: `${SPINE_WIDTH[variant]}px solid ${accent}`,
      }}
    >
      {variant === "sm" ? (
        <span className="absolute inset-0 flex items-center justify-center text-[11px] leading-none font-bold text-foreground/80">
          {Array.from(title)[0] ?? ""}
        </span>
      ) : variant === "grid" ? (
        // グリッドのカードは幅が狭いので、lg より字を小さくし、行数を絞る
        <div className="flex size-full flex-col justify-between gap-1 p-2 text-foreground">
          <p className="line-clamp-4 text-xs leading-snug font-bold break-words">{title}</p>
          {authors !== "" && (
            <p className="line-clamp-2 text-[10px] leading-snug break-words text-foreground/70">
              {authors}
            </p>
          )}
        </div>
      ) : (
        <div className="flex size-full flex-col justify-between p-3 text-foreground">
          <p className="line-clamp-6 text-base leading-snug font-bold break-words">{title}</p>
          {authors !== "" && (
            <p className="line-clamp-2 text-xs leading-snug break-words text-foreground/70">
              {authors}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
