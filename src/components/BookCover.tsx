import { useState } from "react";
import { coverAccentVar } from "@/lib/coverColor";
import { DETAIL_COVER_SIZE, LIST_COVER_SIZE, coverUrl } from "@/lib/cover";
import { cn } from "@/lib/utils";
import { useCoverVersion } from "@/stores/coverVersionStore";

export type CoverVariant = "sm" | "lg";

interface BookCoverProps {
  /** 本の ID（仮の表紙の色を決める）。 */
  id: string;
  title: string;
  /** 著者の表示用の文字列。 */
  authors: string;
  coverHash: string | null;
  /** sm: 一覧の小さな表紙。lg: 詳細ペインの大きな表紙。 */
  variant?: CoverVariant;
  className?: string;
}

function frameClass(variant: CoverVariant, className?: string) {
  return cn(
    "relative shrink-0 overflow-hidden ring-1 ring-foreground/10",
    variant === "sm"
      ? "aspect-[5/7] w-7 rounded-[3px]"
      : "aspect-[5/7] w-full max-w-60 rounded-md shadow-md",
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
          src={coverUrl(
            coverHash,
            variant === "sm" ? LIST_COVER_SIZE : DETAIL_COVER_SIZE,
            undefined,
            version,
          )}
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
        borderLeft: `${variant === "sm" ? 3 : 8}px solid ${accent}`,
      }}
    >
      {variant === "sm" ? (
        <span className="absolute inset-0 flex items-center justify-center text-[11px] leading-none font-bold text-foreground/80">
          {Array.from(title)[0] ?? ""}
        </span>
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
