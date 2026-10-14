// ウィンドウへのファイルのドロップ（Tauri 2 の `onDragDropEvent`）を受ける。
// Tauri 2 は既定でウィンドウのドロップを OS 側で受けるので、HTML の drop イベントでは `File` が届かず、
// ファイルのパスがこのイベントで届く。ドロップの間は `dragging` が true（ドロップ先の強調に使う）。
import { useEffect, useRef, useState } from "react";
import { inTauri } from "@/lib/runtime";

export function useCoverDrop(onDropPaths: (paths: string[]) => void, enabled = true): boolean {
  const [dragging, setDragging] = useState(false);
  const handlerRef = useRef(onDropPaths);
  useEffect(() => {
    handlerRef.current = onDropPaths;
  });

  useEffect(() => {
    if (!enabled || !inTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    // Tauri の外・テストでは使えないので、読み込みと購読の失敗は無視する（ドロップできないだけ）
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          switch (event.payload.type) {
            case "enter":
            case "over":
              setDragging(true);
              break;
            case "leave":
              setDragging(false);
              break;
            case "drop":
              setDragging(false);
              handlerRef.current(event.payload.paths);
              break;
          }
        }),
      )
      .then((stop) => {
        if (cancelled) void stop();
        else unlisten = () => void stop();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
      setDragging(false);
    };
  }, [enabled]);

  return dragging;
}
